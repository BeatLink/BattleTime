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
use neotime_app_host::{settings, AppEffect, AppFailure, AppHost, EventResult, System, Touch, STRIP_PIXELS};
use neotime_app_testing::{
    starting_step_hours, HOURS_PER_DAY, STARTING_DAY_STEPS, STARTING_TRIP_STEPS, STEP_HISTORY_DAYS,
};
use neotime_runtime::{
    events::{ButtonPress, SwipeDirection},
    resources::ImageReference,
    AppImage, EffectKind, SystemValue, SystemValues, Theme, Value, DEFAULT_THEME, NONE,
};
use neotime_settings::{Setting, Settings};
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

/// The coming days the simulated phone forecasts: condition number, low and high in °C.
const SIMULATED_FORECAST: [(Value, Value, Value); 3] = [(0, 12, 22), (2, 10, 17), (5, 8, 14)];
const HOUR_IN_MILLISECONDS: Value = 60 * MINUTE_IN_MILLISECONDS;
const SIMULATED_FIRMWARE_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+simulator");
const SIMULATED_FREE_STORAGE_KILOBYTES: Value = 2840;

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

/// Fixed system values, the real clock, settings apps can change, and effects printed to the terminal.
pub struct SimulatedSystem {
    vibrator_requests: Option<mpsc::Sender<&'static [Step]>>,
    random_state: Cell<u32>,
    /// Each installed app's name and id, for a launcher.
    installed_apps: Vec<(String, String)>,
    settings: Settings,
    /// `Setting::bit`s of the settings changed since the app was last told.
    changed_settings: u32,
    /// The preset number of the theme on show, as `settings.theme` reads.
    theme_preset: Value,
    /// A preset theme the app picked, shown once the host call returns.
    theme_request: Option<Theme>,
    /// When the simulator started, as `started_at` reads.
    started_at: Value,
}

impl SimulatedSystem {
    fn new(
        vibrator_requests: Option<mpsc::Sender<&'static [Step]>>,
        random_seed: u32,
        files: &AppFiles,
    ) -> Self {
        let installed_apps = files
            .installed_apps
            .iter()
            .filter_map(|bytes| AppImage::parse(bytes).ok())
            .map(|image| {
                let text = |string| image.string(string).unwrap_or("").to_string();
                (text(image.name_string), text(image.id_string))
            })
            .collect();
        SimulatedSystem {
            vibrator_requests,
            random_state: Cell::new(random_seed),
            installed_apps,
            settings: Settings::default(),
            changed_settings: 0,
            theme_preset: 0,
            theme_request: None,
            started_at: wall_clock_in_milliseconds(),
        }
    }

    fn change_setting(&mut self, name: &str, value: Value) {
        if name == settings::THEME_SETTING {
            self.theme_request = settings::preset_theme(value);
            return;
        }
        match self.settings.set_by_name(name, value) {
            Ok(setting) => self.changed_settings |= setting.bit(),
            Err(error) => println!("  the watch would refuse it: {error:?}"),
        }
    }

    /// Tells the app about settings its own handlers changed, once the host call has returned.
    fn tell_app_about_settings(&mut self, host: &mut AppHost<'_>, now: Value) {
        let changed = core::mem::take(&mut self.changed_settings);
        if changed != 0 {
            settings::tell_app_about_changes(host, changed, now, self);
        }
        if let Some(theme) = self.theme_request.take() {
            self.theme_preset = settings::preset_number(&theme);
            host.set_theme(theme);
            let key = settings::THEME_SETTING;
            host.deliver(neotime_app_host::PushedEvent::SettingsChange { key }, now, self);
        }
    }
}

impl SystemValues for SimulatedSystem {
    fn read(&self, value: SystemValue) -> Value {
        use SystemValue::*;
        let now = wall_clock_in_milliseconds();
        if let Some(setting) = settings::read_setting(&self.settings, value) {
            return setting;
        }
        if value == ThemePreset {
            return self.theme_preset;
        }
        match value {
            Now => now,
            BatteryPercent => 87,
            BatteryMillivolts => 4056,
            StepsToday => 4321,
            Brightness => self.settings.get(Setting::Brightness) as Value,
            HeartRate => 64,
            HeartRateMeasuredAt => now - 5 * MINUTE_IN_MILLISECONDS,
            IsPhoneConnected | MusicIsPlaying => 1,
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
            WeatherForecastLength => SIMULATED_FORECAST.len() as Value,
            InstalledAppCount => self.installed_apps.len() as Value,
            StepTrip => STARTING_TRIP_STEPS,
            StepDayCount => STEP_HISTORY_DAYS as Value,
            StepHourCount => (STEP_HISTORY_DAYS * HOURS_PER_DAY) as Value,
            StartedAt => self.started_at,
            FreeStorageKilobytes => SIMULATED_FREE_STORAGE_KILOBYTES,
            _ => 0,
        }
    }

    fn read_element_text(&self, value: SystemValue, index: Value) -> &str {
        let Some((name, id)) = usize::try_from(index)
            .ok()
            .and_then(|place| self.installed_apps.get(place))
        else {
            return "";
        };
        match value {
            SystemValue::InstalledAppName => name,
            SystemValue::InstalledAppId => id,
            _ => "",
        }
    }

    /// A three-day forecast: each day's condition, low and high; the installed apps' icons; and the test watch's steps.
    fn read_element(&self, value: SystemValue, index: Value) -> Value {
        if let Some(steps) = simulated_steps(value, index) {
            return steps;
        }
        if value == SystemValue::InstalledAppIcon {
            return match u16::try_from(index) {
                Ok(place) if (place as usize) < self.installed_apps.len() => {
                    ImageReference::InstalledApp(place).to_value() as Value
                }
                _ => NONE,
            };
        }
        let Some(&(condition, low, high)) = usize::try_from(index)
            .ok()
            .and_then(|day| SIMULATED_FORECAST.get(day))
        else {
            return NONE;
        };
        match value {
            SystemValue::WeatherForecastCondition => condition,
            SystemValue::WeatherForecastLow => low,
            SystemValue::WeatherForecastHigh => high,
            _ => NONE,
        }
    }

    fn read_text(&self, value: SystemValue) -> &str {
        use SystemValue::*;
        match value {
            WeatherLocation => "Simulated City",
            MusicArtist => "The Simulators",
            MusicTrack => "Test Pattern",
            NavigationInstruction => "Turn left onto Main Street",
            FirmwareVersion => SIMULATED_FIRMWARE_VERSION,
            ResetReason => "power on",
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

/// The step history the test watch starts with, by day or by hour.
fn simulated_steps(value: SystemValue, index: Value) -> Option<Value> {
    let place = usize::try_from(index).ok();
    match value {
        SystemValue::StepDayTotal => Some(
            place
                .and_then(|day| STARTING_DAY_STEPS.get(day).copied())
                .unwrap_or(NONE),
        ),
        SystemValue::StepHourSteps => Some(
            place
                .and_then(|hour| starting_step_hours().get(hour).copied())
                .unwrap_or(NONE),
        ),
        _ => None,
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
            EffectKind::SetSetting => {
                let name = effect.text(0).unwrap_or("").to_string();
                self.change_setting(&name, effect.number(1));
            }
            EffectKind::SetDoNotDisturb => {
                self.change_setting(Setting::DoNotDisturb.name(), effect.number(0))
            }
            EffectKind::SetBluetooth => self.change_setting(Setting::Bluetooth.name(), effect.number(0)),
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
        installed_apps: Vec::new(),
        settings: Settings::default(),
        changed_settings: 0,
        theme_preset: 0,
        theme_request: None,
        started_at: 0,
    }
}

// ============================================================================
// Running an app
// ============================================================================

/// Opens the app, applies `clicks`, and saves one frame to `screenshot_path`.
pub fn save_screenshot(
    files: AppFiles,
    panel: &SimulatedPanel,
    display: &mut WatchDisplay,
    screen: &mut SimulatorDisplay<Rgb565>,
    output_settings: &OutputSettings,
    clicks: &[Click],
    screenshot_path: &str,
) -> Result<(), Box<dyn Error>> {
    let mut system = SimulatedSystem::new(None, 1, &files);
    let now = wall_clock_in_milliseconds();
    let mut host = open(files, now, &mut system)?;
    for click in clicks {
        host.touch(
            Touch::Click {
                x: click.x,
                y: click.y,
            },
            now,
            &mut system,
        );
        system.tell_app_about_settings(&mut host, now);
    }
    host.draw(display).unwrap_or_else(|never| match never {});

    copy_panel_to_screen(panel, screen, false);
    screen
        .to_rgb_output_image(output_settings)
        .save_png(screenshot_path)?;
    println!("saved {screenshot_path}");
    Ok(())
}

/// An app image, and the shared library's bytes or nothing.
#[derive(Clone, Copy)]
pub struct AppFiles<'image> {
    pub image: &'image [u8],
    pub library: &'image [u8],
    /// Other apps' images, which a launcher lists.
    pub installed_apps: &'image [&'image [u8]],
    /// Each of those apps' icon, or empty for none.
    pub installed_app_icons: &'image [&'image [u8]],
}

/// Opens the app, with the shared library when one was given.
fn open<'image>(
    files: AppFiles<'image>,
    now: Value,
    system: &mut SimulatedSystem,
) -> Result<AppHost<'image>, Box<dyn Error>> {
    let mut host = AppHost::open(files.image, DEFAULT_THEME, now, system)
        .map_err(|error| format!("the app did not open: {error:?}"))?;
    if !files.library.is_empty() {
        host.set_library(files.library)
            .map_err(|error| format!("the shared library did not open: {error:?}"))?;
    }
    host.set_installed_app_icons(files.installed_app_icons);
    Ok(host)
}

/// Runs the app in the window until it is closed.
pub fn run(
    files: AppFiles,
    panel: SimulatedPanel,
    mut display: WatchDisplay,
    mut screen: SimulatorDisplay<Rgb565>,
    output_settings: OutputSettings,
) -> Result<(), Box<dyn Error>> {
    let motor_running = Arc::new(AtomicBool::new(false));
    let mut system = SimulatedSystem::new(
        Some(start_vibrator(motor_running.clone())),
        wall_clock_in_milliseconds() as u32 | 1,
        &files,
    );
    let mut host = open(files, wall_clock_in_milliseconds(), &mut system)?;
    let mut window = Window::new("NeoTime simulator", &output_settings);
    let mut touch = TouchTracker::default();
    let mut is_light = false;

    let mut strip = vec![Rgb565::BLACK; STRIP_PIXELS];

    print_app_controls(host.image().string(host.image().name_string).unwrap_or("app"));
    loop {
        let now = wall_clock_in_milliseconds();
        host.tick(now, &mut system);
        // A transition draws one step a frame, so it moves as it would on the watch.
        if host.needs_drawing() {
            host.draw_step(&mut display, &mut strip)
                .unwrap_or_else(|never| match never {});
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
            system.tell_app_about_settings(&mut host, now);
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
