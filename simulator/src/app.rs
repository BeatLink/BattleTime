//! Runs an app image in the simulator window through `neotime-app-host`, with the mouse as the
//! touch panel, the computer's clock as the watch's, and effects written to the terminal.

use std::{
    cell::Cell,
    error::Error,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use embedded_graphics_simulator::{sdl2::Keycode, OutputSettings, SimulatorDisplay, SimulatorEvent, Window};
use neotime_app_host::{AppEffect, AppFailure, AppHost, EventResult, System, Touch};
use neotime_runtime::{
    events::{ButtonPress, SwipeDirection},
    EffectKind, SystemValue, SystemValues, Theme, Value, DEFAULT_THEME, NONE,
};
use neotime_vibrator::{Step, TAP};

use super::{copy_panel_to_screen, start_vibrator, WatchDisplay, SWIPE_DISTANCE_IN_PIXELS};
use crate::panel::SimulatedPanel;

// ============================================================================
// Settings
// ============================================================================

const FRAME_PAUSE: Duration = Duration::from_millis(16);
const LONG_PRESS: Duration = Duration::from_millis(500);
const DRAG_DISTANCE_IN_PIXELS: i32 = 4;
const MINUTE_IN_MILLISECONDS: Value = 60_000;
const HOUR_IN_MILLISECONDS: Value = 60 * MINUTE_IN_MILLISECONDS;

/// A light theme, for trying `set_theme` with the T key.
const LIGHT_THEME: Theme = Theme::new([0xFFFF, 0xDEFB, 0x0000, 0x6B4D, 0x04B3, 0x2589, 0xFC40, 0xD8A7]);

/// Where to touch before a screenshot, so it can show a screen after a click.
#[derive(Clone, Copy, Debug)]
pub struct Click {
    pub x: i32,
    pub y: i32,
}

pub fn parse_click(text: &str) -> Result<Click, String> {
    let (x, y) = text.split_once(',').ok_or(format!("{text} is not X,Y"))?;
    let number = |text: &str| text.trim().parse().map_err(|_| format!("{text} is not a number"));
    Ok(Click {
        x: number(x)?,
        y: number(y)?,
    })
}

fn wall_clock_in_milliseconds() -> Value {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as Value)
}

// ============================================================================
// The simulated system
// ============================================================================

/// Fixed system values, the real clock, and effects printed to the terminal.
pub struct SimulatedSystem {
    vibrator_requests: Option<mpsc::Sender<&'static [Step]>>,
    random_state: Cell<u32>,
}

impl SystemValues for SimulatedSystem {
    fn read(&self, value: SystemValue) -> Value {
        use SystemValue::*;
        let now = wall_clock_in_milliseconds();
        match value {
            Now => now,
            BatteryPercent => 87,
            StepsToday => 4321,
            StepGoal => 10_000,
            Brightness => 2,
            HeartRate => 64,
            HeartRateMeasuredAt => now - 5 * MINUTE_IN_MILLISECONDS,
            Uses24HourClock | UsesMetricUnits | IsBluetoothEnabled | IsPhoneConnected | MusicIsPlaying => 1,
            WeatherTemperature => 18,
            WeatherLow => 11,
            WeatherHigh => 21,
            WeatherReceivedAt => now - 20 * MINUTE_IN_MILLISECONDS,
            WeatherSunrise => now - 6 * HOUR_IN_MILLISECONDS,
            WeatherSunset => now + 6 * HOUR_IN_MILLISECONDS,
            MusicPosition => 72_000,
            MusicLength => 215_000,
            NavigationDistance => 350,
            NavigationProgressPercent => 40,
            NextAlarm => NONE,
            _ => 0,
        }
    }

    fn read_text(&self, value: SystemValue) -> &str {
        use SystemValue::*;
        match value {
            WeatherLocation => "Simulated City",
            MusicArtist => "The Simulators",
            MusicTrack => "Test Pattern",
            NavigationInstruction => "Turn left onto Main Street",
            _ => "",
        }
    }

    fn random_number(&self) -> u32 {
        let mut state = self.random_state.get();
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        self.random_state.set(state);
        state
    }
}

/// Which of an effect's arguments are text, so they print as text rather than as numbers.
fn text_arguments(kind: EffectKind) -> &'static [usize] {
    use EffectKind::*;
    match kind {
        Go | OpenApp | ShowToast | Log | SendToPhone | SetSetting => &[0],
        Notify => &[0, 1],
        _ => &[],
    }
}

/// The name an effect's first argument stands for, where the compiler numbers names.
pub fn named_argument(kind: EffectKind, number: Value) -> Option<&'static str> {
    const PATTERNS: [&str; 6] = ["tap", "double_tap", "notification", "alarm", "success", "failure"];
    const MUSIC_COMMANDS: [&str; 6] = ["play", "pause", "next", "previous", "volume_up", "volume_down"];
    const CALL_COMMANDS: [&str; 3] = ["answer", "reject", "mute"];
    let names: &[&str] = match kind {
        EffectKind::VibratePattern => &PATTERNS,
        EffectKind::MusicControl => &MUSIC_COMMANDS,
        EffectKind::CallControl => &CALL_COMMANDS,
        _ => return None,
    };
    usize::try_from(number)
        .ok()
        .and_then(|index| names.get(index).copied())
}

/// An effect written the way an app calls it, such as `ShowToast("Saved")`.
fn describe(effect: &AppEffect) -> String {
    let texts = text_arguments(effect.kind);
    let arguments: Vec<String> = (0..effect.kind.argument_count())
        .map(|position| {
            let number = effect.number(position);
            if texts.contains(&position) {
                if let Some(text) = effect.text(position) {
                    return format!("{text:?}");
                }
            }
            match (position, named_argument(effect.kind, number)) {
                (0, Some(name)) => name.to_string(),
                _ => number.to_string(),
            }
        })
        .collect();
    format!("{:?}({})", effect.kind, arguments.join(", "))
}

impl System for SimulatedSystem {
    fn carry_out(&mut self, effect: &AppEffect) {
        println!("effect: {}", describe(effect));
        if matches!(effect.kind, EffectKind::Vibrate | EffectKind::VibratePattern) {
            if let Some(requests) = &self.vibrator_requests {
                let _ = requests.send(TAP);
            }
        }
        match effect.kind {
            EffectKind::Back | EffectKind::Close => println!("  the watch would leave the app now"),
            EffectKind::RequestNavigationState | EffectKind::RequestMusicState => {
                println!("  the watch would ask the phone; the simulator's values stay as they are")
            }
            _ => {}
        }
    }

    fn app_failed(&mut self, failure: AppFailure) {
        eprintln!("app failed, so the watch would show its crash screen: {failure:?}");
    }
}

/// A simulated system with no motor, for tests.
#[cfg(test)]
pub fn test_system() -> SimulatedSystem {
    SimulatedSystem {
        vibrator_requests: None,
        random_state: Cell::new(1),
    }
}

// ============================================================================
// Running an app
// ============================================================================

/// Opens the app, applies `clicks`, and saves one frame to `screenshot_path`.
pub fn save_screenshot(
    image: &[u8],
    panel: &SimulatedPanel,
    display: &mut WatchDisplay,
    screen: &mut SimulatorDisplay<Rgb565>,
    output_settings: &OutputSettings,
    clicks: &[Click],
    screenshot_path: &str,
) -> Result<(), Box<dyn Error>> {
    let mut system = SimulatedSystem {
        vibrator_requests: None,
        random_state: Cell::new(1),
    };
    let now = wall_clock_in_milliseconds();
    let mut host = AppHost::open(image, DEFAULT_THEME, now, &mut system)
        .map_err(|error| format!("the app did not open: {error:?}"))?;
    for click in clicks {
        host.touch(
            Touch::Click {
                x: click.x,
                y: click.y,
            },
            now,
            &mut system,
        );
    }
    host.draw(display).unwrap_or_else(|never| match never {});

    copy_panel_to_screen(panel, screen, false);
    screen
        .to_rgb_output_image(output_settings)
        .save_png(screenshot_path)?;
    println!("saved {screenshot_path}");
    Ok(())
}

/// Runs the app in the window until it is closed.
pub fn run(
    image: &[u8],
    panel: SimulatedPanel,
    mut display: WatchDisplay,
    mut screen: SimulatorDisplay<Rgb565>,
    output_settings: OutputSettings,
) -> Result<(), Box<dyn Error>> {
    let motor_running = Arc::new(AtomicBool::new(false));
    let mut system = SimulatedSystem {
        vibrator_requests: Some(start_vibrator(motor_running.clone())),
        random_state: Cell::new(wall_clock_in_milliseconds() as u32 | 1),
    };
    let mut host = AppHost::open(image, DEFAULT_THEME, wall_clock_in_milliseconds(), &mut system)
        .map_err(|error| format!("the app did not open: {error:?}"))?;
    let mut window = Window::new("NeoTime simulator", &output_settings);
    let mut touch = TouchTracker::default();
    let mut is_light = false;

    print_app_controls(host.image().string(host.image().name_string).unwrap_or("app"));
    loop {
        let now = wall_clock_in_milliseconds();
        host.tick(now, &mut system);
        if host.needs_drawing() {
            host.draw(&mut display).unwrap_or_else(|never| match never {});
        }
        copy_panel_to_screen(&panel, &mut screen, motor_running.load(Ordering::Relaxed));
        window.update(&screen);

        let events: Vec<SimulatorEvent> = window.events().collect();
        for event in events {
            let now = wall_clock_in_milliseconds();
            match event {
                SimulatorEvent::Quit => return Ok(()),
                SimulatorEvent::KeyDown {
                    keycode,
                    repeat: false,
                    ..
                } => match keycode {
                    Keycode::Q | Keycode::Escape => return Ok(()),
                    Keycode::Space => {
                        report(host.press_button(ButtonPress::Press, now, &mut system), "button")
                    }
                    Keycode::L => report(
                        host.press_button(ButtonPress::LongPress, now, &mut system),
                        "long press of the button",
                    ),
                    Keycode::T => {
                        is_light = !is_light;
                        host.set_theme(if is_light { LIGHT_THEME } else { DEFAULT_THEME });
                    }
                    _ => {}
                },
                SimulatorEvent::MouseButtonDown { point, .. } => touch.press(point),
                SimulatorEvent::MouseMove { point } => {
                    if let Some(drag) = touch.drag(point) {
                        host.touch(drag, now, &mut system);
                    }
                }
                SimulatorEvent::MouseButtonUp { point, .. } => {
                    if let Some(gesture) = touch.release(point) {
                        report(host.touch(gesture, now, &mut system), "touch");
                    }
                }
                _ => {}
            }
        }

        thread::sleep(FRAME_PAUSE);
    }
}

fn report(result: EventResult, what: &str) {
    if result == EventResult::NotHandled {
        println!("{what}: the app does not take it, so the system's own meaning would apply");
    }
}

fn print_app_controls(name: &str) {
    println!("NeoTime simulator running {name}");
    println!("  click, hold or drag      touch the screen");
    println!("  swipe with the mouse     swipe");
    println!("  space, l                 press or hold the side button");
    println!("  t                        switch between the default and a light theme");
    println!("  q or Escape              quit");
}

// ============================================================================
// Mouse gestures
// ============================================================================

/// Turns mouse presses and moves into clicks, long presses, drags and swipes.
#[derive(Default)]
pub struct TouchTracker {
    start: Option<(Point, Instant)>,
    has_dragged: bool,
}

impl TouchTracker {
    pub fn press(&mut self, point: Point) {
        self.start = Some((point, Instant::now()));
        self.has_dragged = false;
    }

    fn drag(&mut self, point: Point) -> Option<Touch> {
        let (start, _) = self.start?;
        let distance = (point - start).x.abs().max((point - start).y.abs());
        if distance < DRAG_DISTANCE_IN_PIXELS && !self.has_dragged {
            return None;
        }
        self.has_dragged = true;
        Some(Touch::Drag {
            x: point.x,
            y: point.y,
            start_x: start.x,
            start_y: start.y,
        })
    }

    pub fn release(&mut self, point: Point) -> Option<Touch> {
        let (start, pressed_at) = self.start.take()?;
        let moved = point - start;
        if moved.x.abs().max(moved.y.abs()) >= SWIPE_DISTANCE_IN_PIXELS {
            let direction = match (moved.x.abs() > moved.y.abs(), moved.x > 0, moved.y > 0) {
                (true, true, _) => SwipeDirection::Right,
                (true, false, _) => SwipeDirection::Left,
                (false, _, true) => SwipeDirection::Down,
                (false, _, false) => SwipeDirection::Up,
            };
            return Some(Touch::Swipe(direction));
        }
        if self.has_dragged {
            return None;
        }
        Some(match pressed_at.elapsed() >= LONG_PRESS {
            true => Touch::LongPress {
                x: point.x,
                y: point.y,
            },
            false => Touch::Click {
                x: point.x,
                y: point.y,
            },
        })
    }
}
