//! The shell: shows the watch face, the launcher and the open app through the app host.
//!
//! The face is shown at start. A swipe up on the face opens the launcher, a tap there opens an app,
//! and a swipe right that the app does not use goes back a step. The side button's gestures act as
//! `neotime_app_host::side_button` says: a click goes to the face and from the face to sleep. The
//! backlight and display turn off, or dim for an always-on screen, after the settings' timeout
//! without input, unless the app keeps the screen on, and the next touch or button gesture turns
//! them on without acting on it. Each app's persistent state is saved to
//! `/apps/<id>/state` when it closes or the screen turns off, and changed settings at the same times.
//! The watch buzzes on the local hour or half hour when the chimes setting asks, and for each notification
//! from the phone, which also goes to the open app and into the kept list. A notification that
//! finds the screen dark turns it on and opens the notifications app, when one is installed. Do not
//! disturb silences all of it, keeps the screen dark, and silences every app vibration but the
//! alarm pattern. The accelerometer is read only while the screen is on and the open app
//! subscribes to acceleration or shakes. Plugging the charger in or out, or the charge going low,
//! lights the screen on the battery app, which goes back to what was open when the screen sleeps
//! or the wearer goes back. The phone's music and weather are kept for apps to read, an incoming
//! call buzzes and lights the screen, and the phone can ring the watch until the wearer touches it.
//! While an update runs, the status page holds the screen lit; once the update ends or
//! fails, the next touch or press puts the shell back.

extern crate alloc;

use core::{
    cell::Cell,
    sync::atomic::{AtomicBool, Ordering},
};

use alloc::vec;
use core::fmt::Write;

use embassy_futures::{
    select::{select, select4, Either, Either4},
    yield_now,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal};
use embassy_time::{Delay, Duration, Instant, Timer};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use neotime_app_host::{
    haptics::{self, Vibration},
    motion as app_motion, phone as app_phone, settings as app_settings,
    side_button::{self, ButtonAction, ButtonSituation, OpenScreen},
    AppEffect, AppFailure, AppHost, BatteryChange, EventResult, MotionKind, OpenError, PushedEvent, System,
    Touch, Transition, STRIP_PIXELS,
};
use neotime_battery::BatteryEvent;
use neotime_button::Gesture;
use neotime_companion::{PhoneData, PhoneUpdates, SettingsChange, NOTIFICATION_SOURCE_BYTES};
use neotime_crash_record::screen::Choice;
use neotime_display::SLEEP_SWITCH_GAP_IN_MILLISECONDS;
use neotime_notifications::NotificationList;
use neotime_runtime::{
    events::SwipeDirection, resources::ImageReference, AppImage, AppKind, EffectKind, SystemValue,
    SystemValues, Theme, Value,
};
use neotime_settings::{chimes, Setting};
use neotime_steps::{StepHistory, DAYS_REMEMBERED, HOURS_PER_DAY};

use crate::{
    app_events::{self, AppEvent},
    apps::{self, InstalledApps, LauncherIcons, Lent},
    backlight::{self, Backlight},
    battery, clock,
    crash_screen::{FirmwarePage, ShownPage},
    diagnostics::{self, Counter},
    foreground, help, motion, packages, phone, restart, settings, storage, system_values,
    touch::{self, TOUCHES},
    vibration, SharedDisplay,
};

/// The side button's gestures, from the task that watches it.
pub static BUTTON_GESTURES: Channel<CriticalSectionRawMutex, Gesture, 2> = Channel::new();

/// While set, the screen shows the start-up or update status page and the shell draws nothing.
static SHOWING_STATUS: AtomicBool = AtomicBool::new(true);

/// Set when the status page goes away, so the shell paints the whole screen again.
static REDRAW_EVERYTHING: AtomicBool = AtomicBool::new(false);

/// While set, the status page gives way to the next touch or press and lets the screen time out; the start-up page begins so.
static STATUS_DISMISSABLE: AtomicBool = AtomicBool::new(true);

/// Wakes the shell when the status page takes the screen or lets it go, to light it or restart the timeout.
static STATUS_CHANGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

const MILLISECONDS_PER_MINUTE: Value = 60_000;

/// A chime is skipped when the shell notices it later than this, such as after the phone moved the clock on.
const LATEST_CHIME_IN_MILLISECONDS: Value = MILLISECONDS_PER_MINUTE;

fn is_do_not_disturb() -> bool {
    settings::current().is_on(Setting::DoNotDisturb)
}

/// What the touch panel listens for while the screen is dark.
fn listening_while_dark() -> touch::Listening {
    touch::Listening::WhileDark {
        wake_on_touch: settings::current().get(Setting::WakeOnTouch),
    }
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

/// Puts the status page up and holds the screen lit; true when it was not already showing, so the caller clears the screen.
pub fn show_status() -> bool {
    let was_dismissable = STATUS_DISMISSABLE.swap(false, Ordering::Relaxed);
    let newly_shown = !SHOWING_STATUS.swap(true, Ordering::Relaxed);
    if newly_shown || was_dismissable {
        STATUS_CHANGED.signal(());
    }
    newly_shown
}

/// Leaves the status page up until the next touch or press, and lets the screen time out from now.
pub fn release_status() {
    if !STATUS_DISMISSABLE.swap(true, Ordering::Relaxed) {
        STATUS_CHANGED.signal(());
    }
}

/// True while the status page keeps the screen from timing out.
fn status_holds_screen() -> bool {
    status_is_shown() && !STATUS_DISMISSABLE.load(Ordering::Relaxed)
}

/// Takes the status page down, and has the shell paint its screen again.
fn hide_status() {
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
    /// The music and weather the phone last sent.
    phone: PhoneData,
    notifications: NotificationList,
    /// Set when the kept notifications changed, so the open app is told once the host call returns.
    notifications_changed: bool,
    /// The open app's name, which a notification it makes shows as its source.
    open_app_name: heapless::String<NOTIFICATION_SOURCE_BYTES>,
    /// The open app's id, which the Software app cannot turn off while it is open.
    open_app_id: heapless::String<{ neotime_packages::MOST_ID_BYTES }>,
    /// Set when an app turned another on or off, so the open app and the shell catch up once the host call returns.
    apps_toggled: bool,
    files: &'static storage::Files,
    /// Counted once after an app opens and first reads it, since counting walks the file system.
    free_storage_kilobytes: Cell<Option<Value>>,
    /// Whether an app read the free storage before it was counted, so the loop counts it.
    free_storage_wanted: Cell<bool>,
}

impl SystemValues for WatchSystem {
    fn read(&self, value: SystemValue) -> Value {
        if let Some(setting) = app_settings::read_setting(&settings::current(), value) {
            return setting;
        }
        if value == SystemValue::ThemePreset {
            return self.theme_preset;
        }
        if let Some(phone_value) = self.phone.read(value) {
            return phone_value;
        }
        if let Some(number) = self.notifications.read(value) {
            return number;
        }
        match value {
            SystemValue::Now => clock::utc_now(),
            SystemValue::UtcOffset => clock::utc_offset(),
            SystemValue::BatteryPercent => battery::percent() as Value,
            SystemValue::IsCharging => battery::is_charging() as Value,
            SystemValue::BatteryMillivolts => battery::millivolts() as Value,
            SystemValue::StepsToday => motion::steps_today() as Value,
            SystemValue::StepTrip => motion::read_history(StepHistory::trip_steps) as Value,
            SystemValue::StepDayCount => DAYS_REMEMBERED as Value,
            SystemValue::StepHourCount => (DAYS_REMEMBERED * HOURS_PER_DAY) as Value,
            SystemValue::IsPhoneConnected => phone::is_connected() as Value,
            SystemValue::Brightness => self.brightness as Value,
            SystemValue::NextAlarm => neotime_runtime::NONE,
            SystemValue::InstalledAppCount => self.installed.listed_count() as Value,
            SystemValue::PackageCount => self.installed.packages.len() as Value,
            SystemValue::AppMemoryBudget => neotime_runtime::APP_HEAP_BUDGET_BYTES as Value,
            SystemValue::StartedAt => clock::started_at(),
            SystemValue::FreeStorageKilobytes => self.free_storage_kilobytes.get().unwrap_or_else(|| {
                self.free_storage_wanted.set(true);
                0
            }),
            _ => 0,
        }
    }

    fn read_text(&self, value: SystemValue) -> &str {
        match value {
            SystemValue::FirmwareVersion => crate::FIRMWARE_VERSION,
            SystemValue::ResetReason => crate::restart::restart_reason(),
            _ => self.phone.read_text(value).unwrap_or(""),
        }
    }

    /// Each listed app's icon, which the host draws from the icons `apps` keeps, the kept notifications and the step history.
    fn read_element(&self, value: SystemValue, index: Value) -> Value {
        if is_notification_value(value) {
            return self.notifications.read_element(value, index);
        }
        let place = usize::try_from(index).unwrap_or(usize::MAX);
        match value {
            SystemValue::StepDayTotal => motion::read_history(|history| history.steps_on_day(place)) as Value,
            SystemValue::StepHourSteps => motion::read_history(|history| {
                history.steps_in_hour(place / HOURS_PER_DAY, place % HOURS_PER_DAY)
            }) as Value,
            SystemValue::InstalledAppIcon => match u16::try_from(index) {
                Ok(place) if (place as usize) < self.installed.listed_count() => {
                    ImageReference::InstalledApp(place).to_value() as Value
                }
                _ => neotime_runtime::NONE,
            },
            SystemValue::PackageEnabled => self
                .installed
                .packages
                .get(place)
                .map_or(0, |app| app.enabled as Value),
            SystemValue::PackageIsFace => self
                .installed
                .packages
                .get(place)
                .map_or(0, |app| (app.kind == AppKind::Face) as Value),
            SystemValue::PackageMemory => self.installed.memory_of(place).map_or(0, Value::from),
            SystemValue::PackageRefusal => self.installed.refusal_of(place, &self.open_app_id) as Value,
            _ => neotime_runtime::NONE,
        }
    }

    /// Each listed app's name and id, and each kept notification's text.
    fn read_element_text(&self, value: SystemValue, index: Value) -> &str {
        if is_notification_value(value) {
            return self.notifications.read_element_text(value, index);
        }
        let place = usize::try_from(index).unwrap_or(usize::MAX);
        let entry = match value {
            SystemValue::InstalledAppName | SystemValue::InstalledAppId => self.installed.listed(place),
            _ => self.installed.packages.get(place),
        };
        let Some(entry) = entry else {
            return "";
        };
        match value {
            SystemValue::InstalledAppName | SystemValue::PackageName => &entry.name,
            SystemValue::InstalledAppId | SystemValue::PackageId => &entry.id,
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

    fn read_file(&self, path: &str, offset: u32, out: &mut [u8]) -> Option<usize> {
        self.files.read_part(path, offset, out)
    }
}

/// Whether the value is one of the kept notifications' fields, which all share one bit.
fn is_notification_value(value: SystemValue) -> bool {
    value.bit() == SystemValue::UnreadNotifications.bit()
}

impl WatchSystem {
    /// Walks the file system from the loop rather than deep inside a redraw, so its frames stay off the peak stack.
    fn count_free_storage_if_wanted(&self) {
        if !self.free_storage_wanted.replace(false) {
            return;
        }
        let counted = self
            .files
            .file_system()
            .and_then(neotime_storage::free_kilobytes)
            .map_or(0, Value::from);
        self.free_storage_kilobytes.set(Some(counted));
        system_values::changed(&[SystemValue::FreeStorageKilobytes]);
    }
}

impl System for WatchSystem {
    fn carry_out(&mut self, effect: &AppEffect) {
        if self
            .notifications
            .carry_out(effect, &self.open_app_name, clock::utc_now())
        {
            self.notifications_changed = true;
        }
        let current = settings::current();
        let silence = haptics::Silence::from_settings(
            current.is_on(Setting::DoNotDisturb),
            current.is_on(Setting::AlarmsInDoNotDisturb),
        );
        let vibration = haptics::vibration_for_effect(effect.kind, effect.number(0), silence);
        if vibration.is_some() {
            vibration::request(vibration);
            return;
        }
        if let Some(line) = app_phone::line_for_effect(effect.kind, effect.number(0)) {
            settings::send_reply(&line);
            return;
        }
        if let Some(line) = app_phone::app_line_for_effect(effect) {
            settings::send_reply(&line);
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
            EffectKind::ResetStepTrip => motion::reset_trip(),
            EffectKind::SetBluetooth => settings::set_by_name(Setting::Bluetooth.name(), effect.number(0)),
            EffectKind::SetAppEnabled => {
                let id = effect.text(0).unwrap_or("");
                let allowed = !(id == self.open_app_id.as_str() && effect.number(1) == 0);
                if allowed && self.installed.set_enabled(self.files, id, effect.number(1) != 0) {
                    self.apps_toggled = true;
                }
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
    /// When the panel last went to sleep or woke, which it cannot do again too soon after.
    panel_switched_at: Instant,
    /// Settings changed since they were last written to the flash.
    settings_unsaved: bool,
    /// The local time the chimes were last checked at.
    chimes_checked_at: Value,
    /// The open app's image, which its host borrows; freed only once that host is gone.
    image: Option<Lent<[u8]>>,
    /// The listed apps' icons, held only while the launcher is open.
    launcher_icons: Option<LauncherIcons>,
    library: Option<&'static [u8]>,
    /// While the battery app shows because of a battery event, what was open before it.
    battery_screen_returns_to: Option<Showing>,
    /// Until when the phone's find ring plays, unless the wearer touches the watch first.
    finding_until: Option<Instant>,
    /// A touch taken from the queue while merging drags, handled next.
    waiting_touch: Option<Touch>,
    /// Set when a new screen opens, so the rest of a touch begun on the old one goes nowhere until the finger lifts.
    finger_left_on_old_screen: bool,
    /// The crash or not-enough-memory page, shown in place of an app.
    firmware_page: Option<ShownPage>,
}

impl Shell {
    /// Closes what is open and opens another, coming in by `transition`; the face is the fallback when an image will not open.
    fn open(&mut self, showing: Showing, transition: Transition) {
        self.battery_screen_returns_to = None;
        let now = clock::utc_now();
        self.finger_left_on_old_screen = true;
        if let Some(mut host) = self.host.take() {
            host.close(now, &mut self.system);
            self.save_state_of(&mut host);
        }
        self.save_settings();
        vibration::request(Some(Vibration::Stop));
        self.finding_until = None;
        self.firmware_page = None;
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
            Showing::App(place) => installed.listed(place).map(|entry| entry.id.clone()),
        };
        restart::set_open_app(id.as_deref().unwrap_or(""));
        let Some(image) = id.and_then(|id| apps::read_image(self.files, &id)) else {
            self.open_failed(showing);
            return;
        };
        if let Some(page) = crate::crash_screen::refusal_for(image.get(), showing == Showing::Face) {
            image.release();
            restart::set_open_app("");
            self.firmware_page = Some(ShownPage::new(page));
            return;
        }
        self.system.free_storage_kilobytes.set(None);
        self.system.free_storage_wanted.set(false);
        let bytes = image.get();
        let saved = AppImage::parse(bytes)
            .ok()
            .and_then(|parsed| parsed.string(parsed.id_string))
            .and_then(|id| self.files.read(&state_path(id)))
            .unwrap_or_default();
        if self.start_host(bytes, &saved, now) {
            self.image = Some(image);
            if let Some(host) = &mut self.host {
                host.set_next_transition(transition);
            }
        } else {
            image.release();
            self.open_failed(showing);
        }
    }

    /// Opens a host on the image into `self.host`.
    fn start_host(&mut self, bytes: &'static [u8], saved: &[u8], now: Value) -> bool {
        self.system.open_app_name = app_name_of(bytes);
        self.system.open_app_id = AppImage::parse(bytes)
            .ok()
            .and_then(|image| image.string(image.id_string))
            .and_then(|id| heapless::String::try_from(id).ok())
            .unwrap_or_default();
        let Ok(host) = AppHost::open_into(&mut self.host, bytes, self.theme, saved, now, &mut self.system)
        else {
            return false;
        };
        if let Some(library) = self.library {
            // An app built for a newer library would fail wherever it calls into it, so it is not opened.
            if let Err(OpenError::LibraryTooOld { .. }) = host.set_library(library, now, &mut self.system) {
                self.host = None;
                return false;
            }
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
            self.open(Showing::Face, Transition::None);
        }
    }

    /// Reads the installed apps again, such as after the companion app installed one, and shows the face.
    fn reload_apps(&mut self) {
        if let Some(mut host) = self.host.take() {
            host.close(clock::utc_now(), &mut self.system);
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
        self.show_first_screen();
    }

    /// Shows the crash page while the previous run's crash waits for the wearer, or else the face; nothing is open before.
    fn show_first_screen(&mut self) {
        if restart::pending_crash().is_some() {
            self.showing = Showing::Face;
            self.firmware_page = Some(ShownPage::new(FirmwarePage::Crash));
        } else {
            self.open(Showing::Face, Transition::None);
        }
    }

    /// Does what the wearer picked on the crash or not-enough-memory page.
    fn follow_page_choice(&mut self, choice: Choice) {
        let refused_face = matches!(
            self.firmware_page,
            Some(ShownPage {
                page: FirmwarePage::NotEnoughMemory {
                    refused_face: true,
                    ..
                },
                ..
            })
        );
        match choice {
            Choice::Restart => {
                restart::dismiss_crash();
                self.open(Showing::Face, Transition::None);
            }
            Choice::SendDebugInfo => {
                self.firmware_page = Some(ShownPage::new(FirmwarePage::WaitingForCompanion));
            }
            Choice::Dismiss if refused_face => {
                self.firmware_page = None;
                self.help_drawn_at = None;
            }
            Choice::Dismiss => self.open(Showing::Face, Transition::None),
        }
    }

    /// Leaves the crash page for the face once the companion app has saved the crash.
    fn leave_crash_page_once_dealt_with(&mut self) {
        if self
            .firmware_page
            .as_ref()
            .is_some_and(ShownPage::crash_is_dealt_with)
        {
            self.open(Showing::Face, Transition::None);
        }
    }

    /// After the open app turned another on or off: keeps the open app's place in the launcher's list and tells it.
    fn apps_toggled(&mut self) {
        if let Showing::App(_) = self.showing {
            if let Some(place) = self.system.installed.place_of(&self.system.open_app_id) {
                self.showing = Showing::App(place);
            }
        }
        if let Some(host) = &mut self.host {
            let changed = SystemValue::InstalledAppCount.bit();
            host.report_system_value_changes(changed, clock::utc_now(), &mut self.system);
        }
    }

    /// Writes the app's persistent state when any of it changed since the last save.
    fn save_state_of(&self, host: &mut AppHost<'static>) {
        let Some(id) = host.image().string(host.image().id_string) else {
            return;
        };
        // Only as much as this app's persistent state can take, so closing a large app never runs out of heap.
        let size = neotime_runtime::memory_need::saved_state_bytes(host.image());
        if size == 0 {
            return;
        }
        let mut buffer = vec![0; size];
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
                clock::utc_now(),
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
            app_settings::tell_app_about_changes(host, changed, clock::utc_now(), &mut self.system);
        }
        if !self.lit {
            if changed & Setting::WakeOnTouch.bit() != 0 {
                touch::listen_for(listening_while_dark());
            }
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

    /// Goes back a step, the new screen coming in by `transition`.
    fn go_back(&mut self, transition: Transition) {
        if let Some(showing) = self.battery_screen_returns_to {
            self.open(showing, transition);
            return;
        }
        match self.showing {
            Showing::Face => {}
            Showing::Launcher => self.open(Showing::Face, transition),
            Showing::App(_) => self.open(Showing::Launcher, transition),
        }
    }

    /// Merges a drag with the drags queued behind it, so the screen follows the finger however far drawing fell behind.
    fn latest_of(&mut self, touch: Touch) -> Touch {
        let Touch::Drag { .. } = touch else {
            return touch;
        };
        let mut latest = touch;
        while let Ok(next) = TOUCHES.try_receive() {
            if let Touch::Drag { .. } = next {
                latest = next;
            } else {
                self.waiting_touch = Some(next);
                break;
            }
        }
        latest
    }

    fn touch(&mut self, touch: Touch) {
        if let Some(page) = &self.firmware_page {
            if let Touch::Click { x, y } = touch {
                if let Some(choice) = page.choice_at(x, y, &self.system.installed) {
                    self.follow_page_choice(choice);
                }
            }
            return;
        }
        let Some(host) = &mut self.host else {
            return;
        };
        let result = host.touch(touch, clock::utc_now(), &mut self.system);
        if result == EventResult::NotHandled {
            match (touch, self.showing) {
                (Touch::Swipe(SwipeDirection::Up), Showing::Face) => {
                    self.open(Showing::Launcher, Transition::Up)
                }
                (Touch::Swipe(SwipeDirection::Right), _) => self.go_back(Transition::Right),
                _ => {}
            }
        }
    }

    /// Offers the gesture to the open app first, and otherwise acts on it as `side_button` says.
    fn button_gesture(&mut self, gesture: Gesture) {
        if let (true, Some(page)) = (self.lit, &self.firmware_page) {
            if gesture == Gesture::Click {
                self.follow_page_choice(page.button_press_choice(&self.system.installed));
            }
            return;
        }
        let situation = self.button_situation();
        if let (Some(press), Some(host)) = (side_button::press_for_app(gesture, &situation), &mut self.host) {
            if host.press_button(press, clock::utc_now(), &mut self.system) != EventResult::NotHandled {
                return;
            }
        }
        match side_button::action_for(gesture, &situation) {
            ButtonAction::Nothing => {}
            ButtonAction::WakeScreen => self.turn_screen_on(),
            ButtonAction::PutToSleep => self.turn_screen_off(),
            ButtonAction::GoBack => self.go_back(Transition::Down),
            ButtonAction::GoToFace => self.open(Showing::Face, Transition::Down),
            ButtonAction::OpenNotifications => {
                if let Some(place) = self.system.installed.place_of(side_button::NOTIFICATIONS_APP_ID) {
                    self.open(Showing::App(place), Transition::Up);
                }
            }
        }
    }

    fn button_situation(&self) -> ButtonSituation {
        let installed = &self.system.installed;
        let notifications_place = installed.place_of(side_button::NOTIFICATIONS_APP_ID);
        let open_screen = match self.showing {
            Showing::Face => OpenScreen::Face,
            Showing::Launcher => OpenScreen::Launcher,
            Showing::App(place) if Some(place) == notifications_place => OpenScreen::NotificationsApp,
            Showing::App(_) => OpenScreen::App,
        };
        ButtonSituation {
            screen_is_lit: self.lit,
            open_screen,
            notifications_app_is_installed: notifications_place.is_some(),
            covers_an_earlier_screen: self.battery_screen_returns_to.is_some(),
        }
    }

    fn system_values_changed(&mut self, changed: u32) {
        self.settings_changed();
        if let Some(host) = &mut self.host {
            host.report_system_value_changes(changed, clock::utc_now(), &mut self.system);
        }
        self.take_battery_events();
    }

    /// Keeps and buzzes for a phone notification, takes the phone's other updates, and passes readings and notifications to the open app.
    fn app_event(&mut self, event: AppEvent) {
        let now = clock::utc_now();
        match event {
            AppEvent::Phone(updates) => self.phone_updated(updates),
            AppEvent::Notification(notification) => {
                if let Some(host) = &mut self.host {
                    let pushed = PushedEvent::Notification {
                        id: notification.id,
                        app: &notification.source,
                        title: &notification.title,
                        body: &notification.body,
                        time: now,
                    };
                    host.deliver(pushed, now, &mut self.system);
                }
                self.system.notifications.add(notification, now);
                self.system.notifications_changed = true;
                // A ringing find goes on, so the notification neither buzzes over it nor opens an app.
                if !self.is_finding() {
                    self.wake_for_notification();
                    vibration::request(haptics::notification_vibration(is_do_not_disturb()));
                }
            }
            AppEvent::NotificationDismissed(id) => {
                self.system.notifications_changed |= self.system.notifications.dismiss(id);
            }
            AppEvent::Acceleration(reading) => self.deliver(PushedEvent::Acceleration {
                x: reading.x_in_milli_g as Value,
                y: reading.y_in_milli_g as Value,
                z: reading.z_in_milli_g as Value,
            }),
            AppEvent::Shake => self.deliver(PushedEvent::Motion(MotionKind::Shake)),
        }
    }

    fn deliver(&mut self, event: PushedEvent) {
        if let Some(host) = &mut self.host {
            host.deliver(event, clock::utc_now(), &mut self.system);
        }
    }

    /// Asks for a dark screen to turn on outside do not disturb, showing the notifications app when one is installed.
    ///
    /// The screen turns on through `follow_requests`, as for the battery screen, so it wakes once and counts as input.
    fn wake_for_notification(&mut self) {
        if self.lit || is_do_not_disturb() {
            return;
        }
        let notifications_app = self.system.installed.place_of(side_button::NOTIFICATIONS_APP_ID);
        let over_a_page = self.firmware_page.is_some();
        if let Some(place) =
            notifications_app.filter(|&place| self.showing != Showing::App(place) && !over_a_page)
        {
            self.open(Showing::App(place), Transition::None);
        }
        self.system.screen_request = Some(true);
    }

    /// Buzzes and rings as the phone asks, keeps its music and weather, and tells the open app.
    fn phone_updated(&mut self, mut updates: PhoneUpdates) {
        let now = clock::utc_now();
        let is_do_not_disturb = is_do_not_disturb();
        if let Some(count) = updates.buzzes {
            vibration::request(haptics::phone_buzzes_vibration(count, is_do_not_disturb));
        }
        if let Some(call) = &updates.call {
            let buzz = haptics::call_vibration(call.kind, is_do_not_disturb);
            if buzz.is_some() {
                vibration::request(buzz);
                self.system.screen_request = Some(true);
            }
        }
        if let Some(is_on) = updates.find {
            vibration::request(Some(haptics::find_vibration(is_on)));
            self.finding_until = is_on.then(|| Instant::now() + vibration::RING_DURATION);
        }
        let changes = self.system.phone.apply(&mut updates, now);
        if let Some(host) = &mut self.host {
            app_phone::tell_app(host, updates.call.as_deref(), changes, now, &mut self.system);
        }
    }

    fn is_finding(&self) -> bool {
        self.finding_until.is_some_and(|until| Instant::now() < until)
    }

    /// Stops the phone's find ring if it still plays, and says whether it did, so the touch does nothing else.
    fn stop_finding(&mut self) -> bool {
        match self.finding_until.take() {
            Some(until) if Instant::now() < until => {
                vibration::request(Some(Vibration::Stop));
                true
            }
            _ => false,
        }
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
        if let Some(showing) = self.battery_screen_returns_to {
            self.open(showing, Transition::None);
        }
        // A finished update's page has been seen, so the next wake shows the shell.
        if status_is_shown() {
            hide_status();
        }
        self.lit = false;
        // A finger still down as the panel stops reporting would otherwise never be seen to lift.
        foreground::set_finger_down(false);
        touch::listen_for(listening_while_dark());
        self.dimmed = settings::current().is_on(Setting::AlwaysOnScreen);
        if self.dimmed {
            self.backlight.dim();
            self.display.lock(|display| {
                let _ = display.borrow_mut().enter_low_power_mode();
            });
        } else {
            self.backlight.turn_off();
            wait_until_panel_can_switch(self.panel_switched_at);
            self.display.lock(|display| {
                let _ = display.borrow_mut().sleep(&mut Delay);
            });
            self.panel_switched_at = Instant::now();
        }
        if let Some(mut host) = self.host.take() {
            host.screen_turned_off(clock::utc_now(), &mut self.system);
            self.save_state_of(&mut host);
            self.host = Some(host);
        }
        self.save_settings();
    }

    fn turn_screen_on(&mut self) {
        self.lit = true;
        touch::listen_for(touch::Listening::Everything);
        self.display.lock(|display| {
            let mut display = display.borrow_mut();
            if self.dimmed {
                let _ = display.leave_low_power_mode();
            } else {
                wait_until_panel_can_switch(self.panel_switched_at);
                let _ = display.wake(&mut Delay);
                self.panel_switched_at = Instant::now();
            }
        });
        self.dimmed = false;
        self.backlight.turn_on();
        if let Some(host) = &mut self.host {
            let now = clock::utc_now();
            host.screen_turned_on(now, &mut self.system);
            // The clock was not redrawn while the screen was dark, so this catches it up.
            host.tick(now, &mut self.system);
        }
    }

    /// Carries out what the last host call's handlers asked for, and says whether an app woke the screen.
    fn follow_requests(&mut self) -> bool {
        if core::mem::take(&mut self.system.apps_toggled) {
            self.apps_toggled();
        }
        if core::mem::take(&mut self.system.notifications_changed) {
            if let Some(host) = &mut self.host {
                let changed = SystemValue::UnreadNotifications.bit();
                host.report_system_value_changes(changed, clock::utc_now(), &mut self.system);
            }
        }
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
            Request::Open(showing) => self.open(showing, Transition::Up),
            Request::Back => self.go_back(Transition::Down),
        }
        screen_request == Some(true)
    }

    /// Whether the open app is part way through a transition, which the shell finishes before anything else.
    /// Whether a transition or a redraw is part way through, with more slices to paint before the next input.
    fn is_drawing_under_way(&self) -> bool {
        self.lit && !status_is_shown() && self.host.as_ref().is_some_and(|host| host.needs_drawing())
    }

    fn draw(&mut self) {
        if REDRAW_EVERYTHING.swap(false, Ordering::Relaxed) {
            self.display.lock(|display| {
                let _ = display.borrow_mut().clear(Rgb565::BLACK);
            });
            if let Some(host) = &mut self.host {
                host.set_theme(*host.theme());
            }
            if let Some(page) = &mut self.firmware_page {
                page.drawn = false;
            }
        }
        if (!self.lit && !self.dimmed) || status_is_shown() {
            return;
        }
        if let Some(page) = &mut self.firmware_page {
            let installed = &self.system.installed;
            self.display
                .lock(|display| page.draw_if_needed(&mut display.borrow_mut(), installed));
            return;
        }
        match &mut self.host {
            Some(host) if host.needs_drawing() => draw_host(self.display, host),
            Some(_) => {}
            None if self.help_drawn_at != Some(help::next_change(clock::utc_now())) => {
                let now = clock::utc_now();
                let local_now = clock::now();
                self.display
                    .lock(|display| help::draw(&mut display.borrow_mut(), local_now, &self.theme));
                self.help_drawn_at = Some(help::next_change(now));
            }
            None => {}
        }
    }
}

// =====================================================================}

// The battery screen
// ============================================================================

/// The app shown when the charger is plugged in or out, or the charge goes low.
const BATTERY_APP_ID: &str = "battery";

/// The app `battery` subscription's name for a battery event.
fn battery_change(event: BatteryEvent) -> BatteryChange {
    match event {
        BatteryEvent::Low => BatteryChange::Low,
        BatteryEvent::ChargingStarts => BatteryChange::ChargingStarts,
        BatteryEvent::ChargingStops => BatteryChange::ChargingStops,
        BatteryEvent::ChargingCompletes => BatteryChange::ChargingCompletes,
    }
}

impl Shell {
    /// Hands the battery's events to the open app, and lights the battery app for those that wake the screen.
    fn take_battery_events(&mut self) {
        while let Ok(event) = battery::EVENTS.try_receive() {
            if let Some(host) = &mut self.host {
                let change = battery_change(event);
                let percent = battery::percent() as Value;
                host.deliver(
                    PushedEvent::Battery { change, percent },
                    clock::utc_now(),
                    &mut self.system,
                );
            }
            if event.wakes_screen(is_do_not_disturb()) {
                self.show_battery_screen();
            }
        }
    }

    /// Opens the battery app over what is open, unless that app keeps the screen on, and turns the screen on.
    fn show_battery_screen(&mut self) {
        let place = self.system.installed.place_of(BATTERY_APP_ID);
        // The crash and memory pages stay up until the wearer answers them.
        let over_a_page = self.firmware_page.is_some();
        if let Some(place) = place.filter(|_| !self.system.keep_screen_on && !over_a_page) {
            let battery_app = Showing::App(place);
            let returns_to = self.battery_screen_returns_to.unwrap_or(self.showing);
            if self.showing != battery_app {
                self.open(battery_app, Transition::Up);
            }
            if self.showing == battery_app && returns_to != battery_app {
                self.battery_screen_returns_to = Some(returns_to);
            }
        }
        self.system.screen_request = Some(true);
    }
}

/// Waits out what is left of the panel's gap between going to sleep and waking, which is rarely any time at all.
fn wait_until_panel_can_switch(switched_at: Instant) {
    let can_switch_at = switched_at + Duration::from_millis(SLEEP_SWITCH_GAP_IN_MILLISECONDS);
    embassy_time::block_for(can_switch_at.saturating_duration_since(Instant::now()));
}

/// Draws what changed through rows of the screen held on the stack, free again once they are sent.
// Kept out of the shell task, so the rows are on the stack only while drawing, never in the task's state.
#[inline(never)]
fn draw_host(display: &SharedDisplay, host: &mut AppHost<'static>) {
    let mut strip = [Rgb565::BLACK; STRIP_PIXELS];
    display.lock(|display| {
        let now_in_milliseconds = Instant::now().as_millis();
        let _ = host.draw_step_at(&mut *display.borrow_mut(), &mut strip, now_in_milliseconds);
    });
}

/// The name an app image gives itself, cut to fit a notification's source.
// Kept out of the shell task, so the image's parse never adds to the task's stack frame.
#[inline(never)]
fn app_name_of(bytes: &[u8]) -> heapless::String<NOTIFICATION_SOURCE_BYTES> {
    AppImage::parse(bytes)
        .ok()
        .and_then(|parsed| parsed.string(parsed.name_string))
        .map(neotime_notifications::cut_to_fit)
        .unwrap_or_default()
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
    // An update that began during the start-up page keeps it.
    if !status_holds_screen() {
        hide_status();
    }
    settings::load(files);
    clock::load_time_zone(files);
    install_fonts(apps::shared_library());

    let theme = settings::load_theme(files);
    let mut shell = Shell {
        showing: Showing::Face,
        host: None,
        system: WatchSystem {
            request: Request::Nothing,
            random_state: Cell::new(clock::utc_now() as u32 | 1),
            brightness: backlight::DEFAULT_LEVEL,
            brightness_request: None,
            keep_screen_on: false,
            theme_preset: app_settings::preset_number(&theme),
            theme_request: None,
            installed: InstalledApps::read(files),
            screen_request: None,
            phone: PhoneData::default(),
            notifications: NotificationList::new(),
            notifications_changed: false,
            open_app_name: heapless::String::new(),
            open_app_id: heapless::String::new(),
            apps_toggled: false,
            files,
            free_storage_kilobytes: Cell::new(None),
            free_storage_wanted: Cell::new(false),
        },
        theme,
        app_set_brightness: false,
        files,
        backlight,
        display,
        lit: true,
        help_drawn_at: None,
        dimmed: false,
        panel_switched_at: Instant::from_ticks(0),
        settings_unsaved: false,
        chimes_checked_at: clock::now(),
        image: None,
        launcher_icons: None,
        library: apps::shared_library(),
        battery_screen_returns_to: None,
        finding_until: None,
        waiting_touch: None,
        finger_left_on_old_screen: false,
        firmware_page: None,
    };
    shell.show_first_screen();
    let mut last_input = Instant::now();

    loop {
        diagnostics::add_one(Counter::ShellLoops);
        diagnostics::set(Counter::ScreenLit, shell.lit as u32);
        shell.leave_crash_page_once_dealt_with();
        shell.draw();
        foreground::set_drawing(shell.is_drawing_under_way());
        while shell.is_drawing_under_way() {
            yield_now().await;
            shell.draw();
        }
        foreground::set_drawing(false);
        shell.system.count_free_storage_if_wanted();
        shell.match_accelerometer_to_app();

        let now = clock::utc_now();
        let local_now = clock::now();
        shell.chime_if_due(local_now);
        let mut wake_at = match &shell.host {
            // A dark screen needs no redraws, so only the app's subscriptions wake the shell.
            Some(host) if !shell.lit && !shell.dimmed => host
                .next_subscription_due(now)
                .map_or(Instant::MAX, clock::instant_at),
            Some(host) => host.next_wakeup(now).map_or(Instant::MAX, clock::instant_at),
            None => clock::instant_at(help::next_change(now)),
        };
        if shell.lit && !shell.system.keep_screen_on && !status_holds_screen() {
            wake_at = wake_at.min(last_input + screen_timeout());
        }
        if let Some(period) = chime_period_in_milliseconds() {
            let next_local_chime = local_now - local_now.rem_euclid(period) + period;
            wake_at = wake_at.min(clock::instant_at(next_local_chime - clock::utc_offset()));
        }

        let woke = match shell.waiting_touch.take() {
            Some(touch) => Either4::First(touch),
            None => {
                select4(
                    TOUCHES.receive(),
                    BUTTON_GESTURES.receive(),
                    Timer::at(wake_at),
                    select4(
                        system_values::next_changes(),
                        settings::CHANGES.receive(),
                        packages::CHANGED.wait(),
                        select(app_events::next(), STATUS_CHANGED.wait()),
                    ),
                )
                .await
            }
        };
        // A new library may already lie where the open app's library was, so nothing reads it before the reload.
        if !matches!(woke, Either4::Fourth(Either4::Third(_))) && packages::CHANGED.try_take().is_some() {
            shell.reload_apps();
            continue;
        }
        match woke {
            Either4::First(touch) => {
                let touch = shell.latest_of(touch);
                diagnostics::set(Counter::ShellLastWake, 1);
                last_input = Instant::now();
                if shell.stop_finding() {
                    continue;
                }
                if !shell.lit {
                    if app_settings::touch_wakes_screen(&settings::current(), &touch) {
                        shell.turn_screen_on();
                    }
                    continue;
                }
                if status_is_shown() {
                    if !status_holds_screen() && matches!(touch, Touch::Release { .. }) {
                        hide_status();
                    }
                    continue;
                }
                if shell.finger_left_on_old_screen {
                    match touch {
                        Touch::Press { .. } => shell.finger_left_on_old_screen = false,
                        Touch::Release { .. } => {
                            shell.finger_left_on_old_screen = false;
                            continue;
                        }
                        _ => continue,
                    }
                }
                shell.touch(touch);
            }
            Either4::Second(gesture) => {
                diagnostics::set(Counter::ShellLastWake, 2);
                last_input = Instant::now();
                if shell.stop_finding() {
                    continue;
                }
                if shell.lit && status_is_shown() {
                    if !status_holds_screen() {
                        hide_status();
                    }
                    continue;
                }
                shell.button_gesture(gesture);
            }
            Either4::Third(()) => {
                diagnostics::set(Counter::ShellLastWake, 3);
                let timed_out = Instant::now() >= last_input + screen_timeout();
                if shell.lit && timed_out && !shell.system.keep_screen_on && !status_holds_screen() {
                    shell.turn_screen_off();
                }
                if let Some(host) = &mut shell.host {
                    host.tick(clock::utc_now(), &mut shell.system);
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
            Either4::Fourth(Either4::Fourth(Either::First(event))) => {
                diagnostics::set(Counter::ShellLastWake, 7);
                shell.app_event(event);
            }
            Either4::Fourth(Either4::Fourth(Either::Second(()))) => {
                diagnostics::set(Counter::ShellLastWake, 8);
                last_input = Instant::now();
                if status_holds_screen() && !shell.lit {
                    shell.turn_screen_on();
                }
            }
        }
        if shell.follow_requests() {
            last_input = Instant::now();
        }
    }
}
