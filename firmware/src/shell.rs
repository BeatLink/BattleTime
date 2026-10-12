//! The shell: shows the watch face, the launcher and the open app through the app host.
//!
//! The face is shown at start and after the button from the launcher. A swipe up on the face opens
//! the launcher, a tap there opens an app, and the button or a swipe right that the app does not
//! use goes back a step. The backlight and display turn off, or dim for an always-on screen, after
//! the settings' timeout without input, unless the app keeps the screen on, and the next touch or
//! press turns them on without acting on it. Each app's persistent state is saved to
//! `/apps/<id>/state` when it closes or the screen turns off, and changed settings at the same times.
//! The watch buzzes on the hour or half hour when the chimes setting asks, and for each notification
//! from the phone, which also goes to the open app. Do not disturb silences all of it, and every app
//! vibration but the alarm pattern. The accelerometer is read only while the screen is on and the
//! open app subscribes to acceleration or shakes.

extern crate alloc;

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
    haptics::{self, Vibration},
    motion as app_motion, settings as app_settings, AppEffect, AppFailure, AppHost, EventResult, MotionKind,
    PushedEvent, System, Touch, LARGEST_SAVED_STATE_BYTES,
};
use neotime_companion::SettingsChange;
use neotime_runtime::{
    events::{ButtonPress, SwipeDirection},
    resources::ImageReference,
    AppImage, EffectKind, SystemValue, SystemValues, Theme, Value,
};
use neotime_settings::{chimes, Setting};

use crate::{
    app_events::{self, AppEvent},
    apps::{self, InstalledApps, LauncherIcons, Lent},
    backlight::{self, Backlight},
    battery, clock,
    diagnostics::{self, Counter},
    help, motion, packages, settings, storage, system_values,
    touch::TOUCHES,
    vibration, SharedDisplay,
};

/// Presses of the side button, from the task that watches it.
pub static BUTTON_PRESSES: Channel<CriticalSectionRawMutex, ButtonPress, 2> = Channel::new();

/// While set, the screen shows the start-up or update status page and the shell draws nothing.
static SHOWING_STATUS: AtomicBool = AtomicBool::new(true);

/// Set when the status page goes away, so the shell paints the whole screen again.
static REDRAW_EVERYTHING: AtomicBool = AtomicBool::new(false);

const MILLISECONDS_PER_MINUTE: Value = 60_000;

/// A chime is skipped when the shell notices it later than this, such as after the phone moved the clock on.
const LATEST_CHIME_IN_MILLISECONDS: Value = MILLISECONDS_PER_MINUTE;

fn is_do_not_disturb() -> bool {
    settings::current().is_on(Setting::DoNotDisturb)
}

fn screen_timeout() -> Duration {
    Duration::from_secs(settings::current().get(Setting::ScreenTimeoutSeconds) as u64)
}

/// How often the watch chimes, or `None` when the chimes are off.
fn chime_period_in_milliseconds() -> Option<Value> {
    match settings::current().get(Setting::Chimes) {
        chimes::EVERY_HOUR => Some(60 * MILLISECONDS_PER_MINUTE),
        chimes::EVERY_HALF_HOUR => Some(30 * MILLISECONDS_PER_MINUTE),
        _ => None,
    }
}

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
    /// The preset number of the theme in use, or -1, as `settings.theme` reads.
    theme_preset: Value,
    /// A preset theme the open app picked, applied once the host call returns.
    theme_request: Option<Theme>,
    installed: InstalledApps,
    /// Whether the open app asked to turn the screen on (`wake_screen`) or off (`screen_off`).
    screen_request: Option<bool>,
}

impl SystemValues for WatchSystem {
    fn read(&self, value: SystemValue) -> Value {
        if let Some(setting) = app_settings::read_setting(&settings::current(), value) {
            return setting;
        }
        if value == SystemValue::ThemePreset {
            return self.theme_preset;
        }
        match value {
            SystemValue::Now => clock::now(),
            SystemValue::BatteryPercent => battery::percent() as Value,
            SystemValue::IsCharging => battery::is_charging() as Value,
            SystemValue::StepsToday => motion::steps_today() as Value,
            SystemValue::IsBluetoothEnabled => 1,
            SystemValue::Brightness => self.brightness as Value,
            SystemValue::NextAlarm => neotime_runtime::NONE,
            SystemValue::InstalledAppCount => self.installed.listed.len() as Value,
            _ => 0,
        }
    }

    /// Each listed app's icon, which the host draws from the icons `apps` keeps.
    fn read_element(&self, value: SystemValue, index: Value) -> Value {
        match value {
            SystemValue::InstalledAppIcon => match u16::try_from(index) {
                Ok(place) if (place as usize) < self.installed.listed.len() => {
                    ImageReference::InstalledApp(place).to_value() as Value
                }
                _ => neotime_runtime::NONE,
            },
            _ => neotime_runtime::NONE,
        }
    }

    /// Each listed app's name and id.
    fn read_element_text(&self, value: SystemValue, index: Value) -> &str {
        let Some(entry) = usize::try_from(index)
            .ok()
            .and_then(|place| self.installed.listed.get(place))
        else {
            return "";
        };
        match value {
            SystemValue::InstalledAppName => &entry.name,
            SystemValue::InstalledAppId => &entry.id,
            _ => "",
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
        let vibration = haptics::vibration_for_effect(effect.kind, effect.number(0), is_do_not_disturb());
        if vibration.is_some() {
            vibration::request(vibration);
            return;
        }
        match effect.kind {
            EffectKind::OpenApp => {
                if let Some(index) = effect.text(0).and_then(|id| self.installed.place_of(id)) {
                    self.request = Request::Open(Showing::App(index));
                }
            }
            EffectKind::Back | EffectKind::Close => self.request = Request::Back,
            EffectKind::SetBrightness => self.brightness_request = Some(effect.number(0).clamp(1, 3) as u8),
            EffectKind::WakeScreen => self.screen_request = Some(true),
            EffectKind::ScreenOff => self.screen_request = Some(false),
            EffectKind::KeepScreenOn => self.keep_screen_on = effect.number(0) != 0,
            EffectKind::SetSetting if effect.text(0) == Some(app_settings::THEME_SETTING) => {
                self.theme_request = app_settings::preset_theme(effect.number(1));
            }
            EffectKind::SetSetting => settings::set_by_name(effect.text(0).unwrap_or(""), effect.number(1)),
            EffectKind::SetDoNotDisturb => {
                settings::set_by_name(Setting::DoNotDisturb.name(), effect.number(0))
            }
            _ => {}
        }
    }

    fn app_failed(&mut self, _failure: AppFailure) {}
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
    system: WatchSystem,
    files: &'static storage::Files,
    /// The theme every app opens with, kept in `/settings/theme`.
    theme: Theme,
    /// Whether the open app set its own brightness, which a settings change leaves alone.
    app_set_brightness: bool,
    backlight: Backlight,
    display: &'static SharedDisplay,
    lit: bool,
    /// While the help screen shows, when its time changes next, so it is painted once a minute.
    help_drawn_at: Option<Value>,
    /// Whether the screen went off by dimming, for an always-on screen, rather than by sleeping.
    dimmed: bool,
    /// Settings changed since they were last written to the flash.
    settings_unsaved: bool,
    /// The wall clock time the chimes were last checked at.
    chimes_checked_at: Value,
    /// The open app's image, which its host borrows; freed only once that host is gone.
    image: Option<Lent<[u8]>>,
    /// The listed apps' icons, held only while the launcher is open.
    launcher_icons: Option<LauncherIcons>,
    library: Option<&'static [u8]>,
}

impl Shell {
    /// Closes what is open and opens another; the face is the fallback when an image will not open.
    fn open(&mut self, showing: Showing) {
        let now = clock::now();
        if let Some(mut host) = self.host.take() {
            host.close(now, &mut self.system);
            self.save_state_of(&mut host);
        }
        self.save_settings();
        vibration::request(Some(Vibration::Stop));
        self.system.keep_screen_on = false;
        self.app_set_brightness = false;
        self.set_brightness(settings::current().get(Setting::Brightness) as u8);

        if let Some(image) = self.image.take() {
            image.release();
        }
        if let Some(icons) = self.launcher_icons.take() {
            icons.release();
        }

        self.showing = showing;
        if showing == Showing::Launcher {
            self.launcher_icons = Some(LauncherIcons::read(self.files, &self.system.installed));
        }
        let installed = &self.system.installed;
        let id = match showing {
            Showing::Face => installed.face.clone(),
            Showing::Launcher => installed.has_launcher.then(|| apps::LAUNCHER_ID.into()),
            Showing::App(place) => installed.listed.get(place).map(|entry| entry.id.clone()),
        };
        let Some(image) = id.and_then(|id| apps::read_image(self.files, &id)) else {
            self.open_failed(showing);
            return;
        };
        let bytes = image.get();
        let saved = AppImage::parse(bytes)
            .ok()
            .and_then(|parsed| parsed.string(parsed.id_string))
            .and_then(|id| self.files.read(&state_path(id)))
            .unwrap_or_default();
        if self.start_host(bytes, &saved, now) {
            self.image = Some(image);
        } else {
            image.release();
            self.open_failed(showing);
        }
    }

    /// Opens a host on the image into `self.host`.
    fn start_host(&mut self, bytes: &'static [u8], saved: &[u8], now: Value) -> bool {
        let Ok(host) = AppHost::open_into(&mut self.host, bytes, self.theme, saved, now, &mut self.system)
        else {
            return false;
        };
        if let Some(library) = self.library {
            let _ = host.set_library(library);
        }
        if let Some(icons) = &self.launcher_icons {
            host.set_installed_app_icons(icons.list());
        }
        true
    }

    /// Falls back to the face when anything else will not open, or to the help screen without one.
    fn open_failed(&mut self, showing: Showing) {
        if showing == Showing::Face {
            self.help_drawn_at = None;
        } else {
            self.open(Showing::Face);
        }
    }

    /// Reads the installed apps again, such as after the companion app installed one, and shows the face.
    fn reload_apps(&mut self) {
        if let Some(mut host) = self.host.take() {
            host.close(clock::now(), &mut self.system);
            self.save_state_of(&mut host);
        }
        if let Some(image) = self.image.take() {
            image.release();
        }
        if let Some(icons) = self.launcher_icons.take() {
            icons.release();
        }
        self.system.installed = InstalledApps::read(self.files);
        self.library = apps::shared_library();
        install_fonts(self.library);
        self.open(Showing::Face);
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

    /// Applies a change from the companion app, then tells it the settings as they now are.
    fn change_settings(&mut self, change: &SettingsChange) {
        self.set_theme(change.applied_to(self.theme));
        settings::set_values(&change.values);
        let report = neotime_companion::settings_report(&self.theme, &settings::current());
        settings::send_reply(&report);
    }

    /// Saves a new theme and shows it, and tells the open app its preset number changed.
    fn set_theme(&mut self, theme: Theme) {
        if theme == self.theme {
            return;
        }
        self.theme = theme;
        self.system.theme_preset = app_settings::preset_number(&theme);
        settings::save_theme(self.files, &theme);
        self.help_drawn_at = None;
        if let Some(host) = &mut self.host {
            host.set_theme(theme);
            let key = app_settings::THEME_SETTING;
            host.deliver(
                PushedEvent::SettingsChange { key },
                clock::now(),
                &mut self.system,
            );
        }
    }

    fn save_settings(&mut self) {
        if core::mem::take(&mut self.settings_unsaved) {
            settings::save(self.files);
        }
    }

    /// Passes changed settings on to the backlight and the open app; saved now only while the screen is off.
    fn settings_changed(&mut self) {
        let changed = settings::take_changes();
        if changed == 0 {
            return;
        }
        self.settings_unsaved = true;
        // An app's own brightness stays until it closes; otherwise the new level shows at once.
        if changed & Setting::Brightness.bit() != 0 && !self.app_set_brightness {
            self.set_brightness(settings::current().get(Setting::Brightness) as u8);
        }
        if let Some(host) = &mut self.host {
            app_settings::tell_app_about_changes(host, changed, clock::now(), &mut self.system);
        }
        if !self.lit {
            self.save_settings();
        }
    }

    /// Buzzes when an hour or half hour, as the chimes setting asks, has passed since the last check.
    fn chime_if_due(&mut self, now: Value) {
        let checked_at = core::mem::replace(&mut self.chimes_checked_at, now);
        let Some(period) = chime_period_in_milliseconds() else {
            return;
        };
        let boundary = now - now.rem_euclid(period);
        let is_due = boundary > checked_at && now - boundary < LATEST_CHIME_IN_MILLISECONDS;
        if is_due {
            let is_on_the_hour = boundary.rem_euclid(60 * MILLISECONDS_PER_MINUTE) == 0;
            vibration::request(haptics::chime_vibration(is_on_the_hour, is_do_not_disturb()));
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
        self.settings_changed();
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

    /// Buzzes for a phone notification, and passes sensor readings and notifications to the open app.
    fn app_event(&mut self, event: AppEvent) {
        let now = clock::now();
        if let AppEvent::Notification(_) = &event {
            vibration::request(haptics::notification_vibration(is_do_not_disturb()));
        }
        let Some(host) = &mut self.host else {
            return;
        };
        let pushed = match &event {
            AppEvent::Acceleration(reading) => PushedEvent::Acceleration {
                x: reading.x_in_milli_g as Value,
                y: reading.y_in_milli_g as Value,
                z: reading.z_in_milli_g as Value,
            },
            AppEvent::Shake => PushedEvent::Motion(MotionKind::Shake),
            AppEvent::Notification(notification) => PushedEvent::Notification {
                id: notification.id,
                app: &notification.source,
                title: &notification.title,
                body: &notification.body,
                time: now,
            },
        };
        host.deliver(pushed, now, &mut self.system);
    }

    /// Reads the accelerometer only as often as the open app needs while the screen is on.
    fn match_accelerometer_to_app(&self) {
        let period = match &self.host {
            Some(host) if self.lit => app_motion::reading_period_in_milliseconds(host),
            _ => None,
        };
        motion::read_acceleration_every(period);
    }

    /// Turns the backlight off and puts the display to sleep, or dims both for an always-on screen.
    fn turn_screen_off(&mut self) {
        self.lit = false;
        self.dimmed = settings::current().is_on(Setting::AlwaysOnScreen);
        if self.dimmed {
            self.backlight.dim();
            self.display.lock(|display| {
                let _ = display.borrow_mut().enter_low_power_mode();
            });
        } else {
            self.backlight.turn_off();
            self.display.lock(|display| {
                let _ = display.borrow_mut().sleep(&mut Delay);
            });
        }
        if let Some(mut host) = self.host.take() {
            host.screen_turned_off(clock::now(), &mut self.system);
            self.save_state_of(&mut host);
            self.host = Some(host);
        }
        self.save_settings();
    }

    fn turn_screen_on(&mut self) {
        self.lit = true;
        self.display.lock(|display| {
            let mut display = display.borrow_mut();
            if self.dimmed {
                let _ = display.leave_low_power_mode();
            } else {
                let _ = display.wake(&mut Delay);
            }
        });
        self.dimmed = false;
        self.backlight.turn_on();
        if let Some(host) = &mut self.host {
            let now = clock::now();
            host.screen_turned_on(now, &mut self.system);
            // The clock was not redrawn while the screen was dark, so this catches it up.
            host.tick(now, &mut self.system);
        }
    }

    /// Carries out what the last host call's handlers asked for, and says whether an app woke the screen.
    fn follow_requests(&mut self) -> bool {
        let screen_request = self.system.screen_request.take();
        match screen_request {
            Some(true) if !self.lit => self.turn_screen_on(),
            Some(false) if self.lit => self.turn_screen_off(),
            _ => {}
        }
        if let Some(level) = self.system.brightness_request.take() {
            self.app_set_brightness = true;
            self.set_brightness(level);
        }
        if let Some(theme) = self.system.theme_request.take() {
            self.set_theme(theme);
        }
        match core::mem::replace(&mut self.system.request, Request::Nothing) {
            Request::Nothing => {}
            Request::Open(showing) => self.open(showing),
            Request::Back => self.go_back(),
        }
        screen_request == Some(true)
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
        if (!self.lit && !self.dimmed) || status_is_shown() {
            return;
        }
        match &mut self.host {
            Some(host) if host.needs_drawing() => {
                self.display.lock(|display| {
                    let _ = host.draw(&mut *display.borrow_mut());
                });
            }
            Some(_) => {}
            None if self.help_drawn_at != Some(help::next_change(clock::now())) => {
                let now = clock::now();
                self.display
                    .lock(|display| help::draw(&mut display.borrow_mut(), now, &self.theme));
                self.help_drawn_at = Some(help::next_change(now));
            }
            None => {}
        }
    }
}

/// Gives the app host its text fonts from the installed shared library, which holds them.
// Kept out of the shell task, so the library's parse never adds to the task's stack frame.
#[inline(never)]
fn install_fonts(library: Option<&'static [u8]>) {
    if let Some(Ok(library)) = library.map(AppImage::parse) {
        neotime_app_host::fonts::install_from_library(&library);
    }
}

#[embassy_executor::task]
pub async fn run_shell(
    display: &'static SharedDisplay,
    backlight: Backlight,
    files: &'static storage::Files,
) {
    Timer::after(START_PAGE_TIME).await;
    hide_status();
    settings::load(files);
    install_fonts(apps::shared_library());

    let theme = settings::load_theme(files);
    let mut shell = Shell {
        showing: Showing::Face,
        host: None,
        system: WatchSystem {
            request: Request::Nothing,
            random_state: Cell::new(clock::now() as u32 | 1),
            brightness: backlight::DEFAULT_LEVEL,
            brightness_request: None,
            keep_screen_on: false,
            theme_preset: app_settings::preset_number(&theme),
            theme_request: None,
            installed: InstalledApps::read(files),
            screen_request: None,
        },
        theme,
        app_set_brightness: false,
        files,
        backlight,
        display,
        lit: true,
        help_drawn_at: None,
        dimmed: false,
        settings_unsaved: false,
        chimes_checked_at: clock::now(),
        image: None,
        launcher_icons: None,
        library: apps::shared_library(),
    };
    shell.open(Showing::Face);
    let mut last_input = Instant::now();

    loop {
        diagnostics::add_one(Counter::ShellLoops);
        diagnostics::set(Counter::ScreenLit, shell.lit as u32);
        shell.draw();
        shell.match_accelerometer_to_app();

        let now = clock::now();
        shell.chime_if_due(now);
        let mut wake_at = match &shell.host {
            // A dark screen needs no redraws, so only the app's subscriptions wake the shell.
            Some(host) if !shell.lit && !shell.dimmed => host
                .next_subscription_due(now)
                .map_or(Instant::MAX, clock::instant_at),
            Some(host) => host.next_wakeup(now).map_or(Instant::MAX, clock::instant_at),
            None => clock::instant_at(help::next_change(now)),
        };
        if shell.lit && !shell.system.keep_screen_on {
            wake_at = wake_at.min(last_input + screen_timeout());
        }
        if let Some(period) = chime_period_in_milliseconds() {
            let next_chime = now - now.rem_euclid(period) + period;
            wake_at = wake_at.min(clock::instant_at(next_chime));
        }

        let woke = select4(
            TOUCHES.receive(),
            BUTTON_PRESSES.receive(),
            Timer::at(wake_at),
            select4(
                system_values::next_changes(),
                settings::CHANGES.receive(),
                packages::CHANGED.wait(),
                app_events::next(),
            ),
        )
        .await;
        // A new library may already lie where the open app's library was, so nothing reads it before the reload.
        if !matches!(woke, Either4::Fourth(Either4::Third(_))) && packages::CHANGED.try_take().is_some() {
            shell.reload_apps();
            continue;
        }
        match woke {
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
                let timed_out = Instant::now() >= last_input + screen_timeout();
                if shell.lit && timed_out && !shell.system.keep_screen_on {
                    shell.turn_screen_off();
                }
                if let Some(host) = &mut shell.host {
                    host.tick(clock::now(), &mut shell.system);
                }
            }
            Either4::Fourth(Either4::First(changed)) => {
                diagnostics::set(Counter::ShellLastWake, 4);
                shell.system_values_changed(changed);
            }
            Either4::Fourth(Either4::Second(change)) => {
                diagnostics::set(Counter::ShellLastWake, 5);
                shell.change_settings(&change);
            }
            Either4::Fourth(Either4::Third(_)) => {
                diagnostics::set(Counter::ShellLastWake, 6);
                shell.reload_apps();
            }
            Either4::Fourth(Either4::Fourth(event)) => {
                diagnostics::set(Counter::ShellLastWake, 7);
                shell.app_event(event);
            }
        }
        if shell.follow_requests() {
            last_input = Instant::now();
        }
    }
}
