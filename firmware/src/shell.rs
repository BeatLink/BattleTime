//! The shell: shows the watch face, the launcher and the open app through the app host.
//!
//! The face is shown at start and after the button from the launcher. A swipe up on the face opens
//! the launcher, a tap there opens an app, and the button or a swipe right that the app does not
//! use goes back a step. The backlight and display turn off after a while without input, unless the
//! app keeps the screen on, and the next touch or press turns them on without acting on it. Each
//! app's persistent state is saved to `/apps/<id>/state` when it closes or the screen turns off.

extern crate alloc;

use alloc::boxed::Box;
use core::{
    cell::Cell,
    sync::atomic::{AtomicBool, Ordering},
};

use alloc::vec;
use core::fmt::Write;

use embassy_futures::select::{select4, Either4};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Delay, Duration, Instant, Timer};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use neotime_app_host::{
    launcher::launcher_image, AppEffect, AppFailure, AppHost, EventResult, PushedEvent, System, Touch,
    LARGEST_SAVED_STATE_BYTES,
};
use neotime_runtime::{
    events::{ButtonPress, SwipeDirection},
    AppImage, EffectKind, SystemValue, SystemValues, Value, DEFAULT_THEME,
};

use crate::{
    backlight::{self, Backlight},
    battery, built_in_apps, clock,
    diagnostics::{self, Counter},
    motion, storage, system_values,
    touch::TOUCHES,
    vibration, SharedDisplay,
};

/// Presses of the side button, from the task that watches it.
pub static BUTTON_PRESSES: Channel<CriticalSectionRawMutex, ButtonPress, 2> = Channel::new();

/// While set, the screen shows the start-up or update status page and the shell draws nothing.
static SHOWING_STATUS: AtomicBool = AtomicBool::new(true);

/// Set when the status page goes away, so the shell paints the whole screen again.
static REDRAW_EVERYTHING: AtomicBool = AtomicBool::new(false);

const SCREEN_TIMEOUT: Duration = Duration::from_secs(15);

/// How long the start-up page stays up, so its lines can be read.
const START_PAGE_TIME: Duration = Duration::from_secs(2);


pub fn status_is_shown() -> bool {
    SHOWING_STATUS.load(Ordering::Relaxed)
}

/// Puts the status page up; true when it was not already showing, so the caller clears the screen.
pub fn show_status() -> bool {
    !SHOWING_STATUS.swap(true, Ordering::Relaxed)
}

/// Takes the status page down, and has the shell paint its screen again.
pub fn hide_status() {
    if SHOWING_STATUS.swap(false, Ordering::Relaxed) {
        REDRAW_EVERYTHING.store(true, Ordering::Relaxed);
    }
}

// ============================================================================
// What is open
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
enum Showing {
    Face,
    Launcher,
    App(usize),
}

/// What a handler asked the shell to do, carried out once the host call returns.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Request {
    Nothing,
    Open(Showing),
    Back,
}

/// The system values apps read, and the effects the shell carries out.
struct WatchSystem {
    request: Request,
    random_state: Cell<u32>,
    brightness: u8,
    /// A brightness the open app asked for, applied once the host call returns.
    brightness_request: Option<u8>,
    keep_screen_on: bool,
}

impl SystemValues for WatchSystem {
    fn read(&self, value: SystemValue) -> Value {
        match value {
            SystemValue::Now => clock::now(),
            SystemValue::BatteryPercent => battery::percent() as Value,
            SystemValue::IsCharging => battery::is_charging() as Value,
            SystemValue::StepsToday => motion::steps_today() as Value,
            SystemValue::Uses24HourClock | SystemValue::UsesMetricUnits | SystemValue::IsBluetoothEnabled => 1,
            SystemValue::StepGoal => 10_000,
            SystemValue::Brightness => self.brightness as Value,
            SystemValue::NextAlarm => neotime_runtime::NONE,
            _ => 0,
        }
    }

    /// Xorshift, seeded from the clock when the shell starts; enough for dice and games.
    fn random_number(&self) -> u32 {
        let mut state = self.random_state.get();
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        self.random_state.set(state);
        state
    }
}

impl System for WatchSystem {
    fn carry_out(&mut self, effect: &AppEffect) {
        let vibration_request = match effect.kind {
            EffectKind::Vibrate => Some(vibration::buzz(effect.number(0))),
            EffectKind::VibratePattern => vibration::named_pattern(effect.number(0)),
            EffectKind::StopVibrating => Some(vibration::Request::Stop),
            _ => None,
        };
        if let Some(request) = vibration_request {
            let _ = vibration::REQUESTS.try_send(request);
            return;
        }
        match effect.kind {
            EffectKind::OpenApp => {
                if let Some(index) = effect.text(0).and_then(app_with_id) {
                    self.request = Request::Open(Showing::App(index));
                }
            }
            EffectKind::Back | EffectKind::Close => self.request = Request::Back,
            EffectKind::SetBrightness => self.brightness_request = Some(effect.number(0).clamp(1, 3) as u8),
            EffectKind::KeepScreenOn => self.keep_screen_on = effect.number(0) != 0,
            _ => {}
        }
    }

    fn app_failed(&mut self, _failure: AppFailure) {}
}

fn app_with_id(id: &str) -> Option<usize> {
    built_in_apps::APPS.iter().position(|bytes| {
        AppImage::parse(bytes).is_ok_and(|image| image.string(image.id_string) == Some(id))
    })
}

/// Where an app's saved state lives: `/apps/<id>/state`.
fn state_path(id: &str) -> heapless::String<64> {
    let mut path = heapless::String::new();
    let _ = write!(path, "{}/{}/state", neotime_storage::APPS_DIRECTORY, id);
    path
}

// ============================================================================
// The loop
// ============================================================================

struct Shell {
    showing: Showing,
    host: Option<AppHost<'static>>,
    launcher: &'static [u8],
    system: WatchSystem,
    files: storage::Files,
    backlight: Backlight,
    display: &'static SharedDisplay,
    lit: bool,
}

impl Shell {
    /// Closes what is open and opens another; the face is the fallback when an image will not open.
    fn open(&mut self, showing: Showing) {
        let now = clock::now();
        if let Some(mut host) = self.host.take() {
            host.close(now, &mut self.system);
            self.save_state_of(&mut host);
        }
        let _ = vibration::REQUESTS.try_send(vibration::Request::Stop);
        self.system.keep_screen_on = false;
        self.set_brightness(backlight::DEFAULT_LEVEL);

        let bytes = match showing {
            Showing::Face => built_in_apps::FACE,
            Showing::Launcher => self.launcher,
            Showing::App(index) => built_in_apps::APPS[index],
        };
        self.showing = showing;
        let saved = AppImage::parse(bytes)
            .ok()
            .and_then(|image| image.string(image.id_string))
            .and_then(|id| self.files.read(&state_path(id)))
            .unwrap_or_default();
        match AppHost::open_with_saved_state(bytes, DEFAULT_THEME, &saved, now, &mut self.system) {
            Ok(mut host) => {
                let _ = host.set_library(built_in_apps::LIBRARY);
                self.host = Some(host);
            }
            Err(_) if showing != Showing::Face => self.open(Showing::Face),
            Err(_) => {}
        }
    }

    /// Writes the app's persistent state when any of it changed since the last save.
    fn save_state_of(&self, host: &mut AppHost<'static>) {
        let Some(id) = host.image().string(host.image().id_string) else {
            return;
        };
        let mut buffer = vec![0; LARGEST_SAVED_STATE_BYTES];
        if let Ok(Some(length)) = host.save_changed_state(&mut buffer) {
            self.files.write(&state_path(id), &buffer[..length]);
        }
    }

    fn set_brightness(&mut self, level: u8) {
        self.backlight.set_level(level);
        self.system.brightness = self.backlight.level();
    }

    fn go_back(&mut self) {
        match self.showing {
            Showing::Face => {}
            Showing::Launcher => self.open(Showing::Face),
            Showing::App(_) => self.open(Showing::Launcher),
        }
    }

    fn touch(&mut self, touch: Touch) {
        let Some(host) = &mut self.host else {
            return;
        };
        let result = host.touch(touch, clock::now(), &mut self.system);
        if result == EventResult::NotHandled {
            match (touch, self.showing) {
                (Touch::Swipe(SwipeDirection::Up), Showing::Face) => self.open(Showing::Launcher),
                (Touch::Swipe(SwipeDirection::Right), _) => self.go_back(),
                _ => {}
            }
        }
    }

    fn press_button(&mut self, press: ButtonPress) {
        let Some(host) = &mut self.host else {
            return;
        };
        if host.press_button(press, clock::now(), &mut self.system) == EventResult::NotHandled {
            self.go_back();
        }
    }

    fn system_values_changed(&mut self, changed: u32) {
        let Some(host) = &mut self.host else {
            return;
        };
        let now = clock::now();
        host.system_values_changed(changed, now, &mut self.system);
        if changed & SystemValue::StepsToday.bit() != 0 {
            let steps_today = motion::steps_today() as Value;
            host.deliver(PushedEvent::Steps { steps_today }, now, &mut self.system);
        }
    }

    /// Turns the backlight off and puts the display to sleep, saving the app's state first.
    fn turn_screen_off(&mut self) {
        self.lit = false;
        self.backlight.turn_off();
        self.display.lock(|display| {
            let _ = display.borrow_mut().sleep(&mut Delay);
        });
        if let Some(mut host) = self.host.take() {
            host.screen_turned_off(clock::now(), &mut self.system);
            self.save_state_of(&mut host);
            self.host = Some(host);
        }
    }

    fn turn_screen_on(&mut self) {
        self.lit = true;
        self.display.lock(|display| {
            let _ = display.borrow_mut().wake(&mut Delay);
        });
        self.backlight.turn_on();
        if let Some(host) = &mut self.host {
            host.screen_turned_on(clock::now(), &mut self.system);
        }
    }

    /// Carries out what the last host call's handlers asked for.
    fn follow_requests(&mut self) {
        if let Some(level) = self.system.brightness_request.take() {
            self.set_brightness(level);
        }
        match core::mem::replace(&mut self.system.request, Request::Nothing) {
            Request::Nothing => {}
            Request::Open(showing) => self.open(showing),
            Request::Back => self.go_back(),
        }
    }

    fn draw(&mut self) {
        if REDRAW_EVERYTHING.swap(false, Ordering::Relaxed) {
            self.display.lock(|display| {
                let _ = display.borrow_mut().clear(Rgb565::BLACK);
            });
            if let Some(host) = &mut self.host {
                host.set_theme(*host.theme());
            }
        }
        if !self.lit || status_is_shown() {
            return;
        }
        if let Some(host) = &mut self.host {
            if host.needs_drawing() {
                self.display.lock(|display| {
                    let _ = host.draw(&mut *display.borrow_mut());
                });
            }
        }
    }
}

#[embassy_executor::task]
pub async fn run_shell(display: &'static SharedDisplay, backlight: Backlight, files: storage::Files) {
    Timer::after(START_PAGE_TIME).await;
    hide_status();

    let mut shell = Shell {
        showing: Showing::Face,
        host: None,
        launcher: Box::leak(launcher_image(&built_in_apps::APPS).into_boxed_slice()),
        system: WatchSystem {
            request: Request::Nothing,
            random_state: Cell::new(clock::now() as u32 | 1),
            brightness: backlight::DEFAULT_LEVEL,
            brightness_request: None,
            keep_screen_on: false,
        },
        files,
        backlight,
        display,
        lit: true,
    };
    shell.open(Showing::Face);
    let mut last_input = Instant::now();

    loop {
        diagnostics::add_one(Counter::ShellLoops);
        diagnostics::set(Counter::ScreenLit, shell.lit as u32);
        shell.draw();

        let now = clock::now();
        let mut wake_at = shell
            .host
            .as_ref()
            .and_then(|host| host.next_wakeup(now))
            .map_or(Instant::MAX, clock::instant_at);
        if shell.lit && !shell.system.keep_screen_on {
            wake_at = wake_at.min(last_input + SCREEN_TIMEOUT);
        }

        match select4(
            TOUCHES.receive(),
            BUTTON_PRESSES.receive(),
            Timer::at(wake_at),
            system_values::next_changes(),
        )
        .await
        {
            Either4::First(touch) => {
                diagnostics::set(Counter::ShellLastWake, 1);
                last_input = Instant::now();
                if !shell.lit {
                    if matches!(touch, Touch::Release { .. }) {
                        shell.turn_screen_on();
                    }
                    continue;
                }
                shell.touch(touch);
            }
            Either4::Second(press) => {
                diagnostics::set(Counter::ShellLastWake, 2);
                last_input = Instant::now();
                if !shell.lit {
                    shell.turn_screen_on();
                    continue;
                }
                shell.press_button(press);
            }
            Either4::Third(()) => {
                diagnostics::set(Counter::ShellLastWake, 3);
                let timed_out = Instant::now() >= last_input + SCREEN_TIMEOUT;
                if shell.lit && timed_out && !shell.system.keep_screen_on {
                    shell.turn_screen_off();
                }
                if let Some(host) = &mut shell.host {
                    host.tick(clock::now(), &mut shell.system);
                }
            }
            Either4::Fourth(changed) => {
                diagnostics::set(Counter::ShellLastWake, 4);
                shell.system_values_changed(changed);
            }
        }
        shell.follow_requests();
    }
}
