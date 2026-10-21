//! The shell: shows the watch face, the launcher and the open app through the app host.
//!
//! The face, the one the watch face setting names, is shown at start and again at once when the
//! setting changes. A swipe up on the face opens the launcher, a tap there opens an app,
//! a swipe down pulls the notifications and then quick settings down, and a swipe left or right
//! steps round the quick apps, as `battletime_app_host::swipes` says. The face chooser, which Settings
//! opens, shows the face itself and hears none of its touches: a swipe left or right changes the
//! `watch_face` setting to the next or previous face turned on and slides that face in, and going
//! back leaves it chosen. The side button's gestures act as
//! `battletime_app_host::side_button` says: a click goes back a step, retracing the way the wearer came,
//! and from the face to sleep. The backlight and display turn off, or dim for an always-on screen, after the settings' timeout
//! without input, unless the app keeps the screen on, and the next touch or button gesture turns
//! them on without acting on it. Two seconds before the timeout the light drops to its low level,
//! and any input in those seconds brings it back and starts the timeout again. While the screen is
//! off the external flash sleeps in deep power-down between store calls. Each app's persistent state is saved to
//! `/apps/<id>/state` when it closes or the screen turns off, and changed settings at the same times.
//! The watch buzzes on the local hour or half hour when the chimes setting asks, and for each notification
//! from the phone, which also goes to the open app and into the kept list. A notification opens the
//! notifications on their preview of it, lighting a dark screen, and the preview goes back by
//! itself after 7 seconds. Do not
//! disturb silences all of it, keeps the screen dark, and silences every app vibration but the
//! alarm pattern. The accelerometer is read while the screen is on and the open app
//! subscribes to acceleration or shakes or shows the shake speed, and while a wake gesture is
//! watched for: a raised or shaken wrist wakes a dark screen and a lowered arm puts a lit one to
//! sleep, as `battletime_app_host::wake` says. A double tap on the face that the face does not take
//! puts the watch to sleep too. Sleep mode, the third state of do not disturb, turns off every
//! wake gesture, touch included, and the always-on screen. The heart rate sensor runs while an app asks for it until
//! that app closes, or as often as the heart rate interval setting says. Plugging the charger in or out, or the charge going low,
//! lights the screen on the battery app, which goes back to what was open when the screen sleeps
//! or the wearer goes back. The phone's music and weather are kept for apps to read, an incoming
//! call opens the call app and rings until the call is answered, rejected, muted or left, and the
//! phone can ring the watch until the wearer touches it.
//! While an update runs, the status page holds the screen lit; once the update ends or
//! fails, the next touch or press puts the shell back.

extern crate alloc;

use core::{
    cell::{Cell, RefCell},
    sync::atomic::{AtomicBool, Ordering},
};

use alloc::{boxed::Box, vec};
use core::fmt::Write;

use battletime_accelerometer::wake_gestures::WakeGesture;
use battletime_app_host::closed_apps::{ClosedApps, ClosedRun, StateHold};
use battletime_app_host::{
    folder::FolderListing,
    haptics::{self, Vibration},
    motion as app_motion, phone as app_phone,
    quick_apps::{self, Destination, Step},
    settings as app_settings,
    side_button::{self, ButtonAction, ButtonSituation, OpenScreen},
    swipes::{self, SwipeAction, SwipeScreen},
    wake::{self as wake_gestures, DoubleTapFinder, ScreenState, WakingTouch},
    way_back::WayBack,
    AppEffect, AppFailure, AppHost, BatteryChange, EventResult, MotionKind, OpenError, PushedEvent, System,
    Touch, Transition,
};
use battletime_battery::BatteryEvent;
use battletime_button::Gesture;
use battletime_companion::{PhoneData, PhoneUpdates, SettingsChange, NOTIFICATION_SOURCE_BYTES};
use battletime_crash_record::screen::Choice;
use battletime_display::SLEEP_SWITCH_GAP_IN_MILLISECONDS;
use battletime_notifications::NotificationList;
use battletime_packages::{
    commands::{run_reply, Change},
    AppId, PackageError, Refusal,
};
use battletime_runtime::library_icons::LIBRARY_ICONS_PATH;
use battletime_runtime::memory_need::{estimate_memory_need, PREFETCH_BYTES};
use battletime_runtime::paging::{pages_code, resident_image};
use battletime_runtime::{
    events::SwipeDirection, resources::ImageReference, AppImage, AppKind, EffectKind, SystemValue,
    SystemValues, Theme, Value,
};
use battletime_settings::{
    chimes, next_screen_timeout_step_in_milliseconds, screen_timeout_stage, ScreenTimeoutStage, Setting,
};
use battletime_steps::{StepHistory, DAYS_REMEMBERED, HOURS_PER_DAY};
use embassy_futures::{
    select::{select3, select4, Either3, Either4},
    yield_now,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal};
use embassy_time::{Delay, Duration, Instant, Timer};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};

use crate::drawing_screen::{DrawingScreen, StripBuffers, STRIP_BUFFER_PIXELS};
use crate::{
    about,
    app_events::{self, AppEvent},
    apps::{self, InstalledApps, LauncherIcons, Lent},
    backlight::{self, Backlight},
    battery, clock,
    crash_screen::{FirmwarePage, ShownPage},
    diagnostics::{self, Counter},
    foreground, heart_rate, help, motion, packages, phone, restart, settings, storage, system_values, tasks,
    touch::{self, TOUCHES},
    vibration, SharedDisplay,
};

#[path = "closed_apps.rs"]
mod closed_apps;

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
        wake_on_touch: app_settings::wake_on_touch(&settings::current()),
    }
}

fn screen_timeout() -> Duration {
    Duration::from_secs(settings::current().get(Setting::ScreenTimeoutSeconds) as u64)
}

/// When a lit screen next moves on toward its timeout: when its light lowers, then when it goes off.
fn next_screen_timeout_step(last_input: Instant) -> Instant {
    let since_input = Instant::now().saturating_duration_since(last_input).as_millis();
    let step = next_screen_timeout_step_in_milliseconds(since_input, screen_timeout().as_millis());
    last_input + Duration::from_millis(step)
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
    /// The notifications, which the firmware carries itself.
    Notifications,
    /// Quick settings, which the firmware carries itself.
    QuickSettings,
    /// The app the phone ran from `/temp`, which is never installed and keeps no state.
    Temporary,
    /// The face, shown to pick with swipes, which hears none of its touches.
    FaceChooser,
}

/// A screen as the way back remembers it: what showed, and whether it was reached round the quick-app ring.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Visit {
    showing: Showing,
    in_quick_ring: bool,
}

/// The notifications screen, compiled from `shell/notifications` when the firmware is built.
static NOTIFICATIONS_IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/notifications.ntapp"));

/// The notifications' screen that previews a notification as it arrives, then goes back by itself.
const NOTIFICATION_PREVIEW_SCREEN: &str = "preview";

/// The quick settings screen, compiled from `shell/quick-settings` when the firmware is built.
static QUICK_SETTINGS_IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/quick-settings.ntapp"));

/// What a handler asked the shell to do, carried out once the host call returns.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Request {
    Nothing,
    Open(Showing),
    Back,
    RollBackFirmware,
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
    /// The music, navigation and weather the phone last sent.
    phone: PhoneData,
    notifications: NotificationList,
    /// Set when the kept notifications changed, so the open app is told once the host call returns.
    notifications_changed: bool,
    /// The open app's name, which a notification it makes shows as its source.
    open_app_name: heapless::String<NOTIFICATION_SOURCE_BYTES>,
    /// The open app's id, which the Software app cannot turn off while it is open.
    open_app_id: heapless::String<{ battletime_packages::MOST_ID_BYTES }>,
    /// Set when an app turned another on or off, so the open app and the shell catch up once the host call returns.
    apps_toggled: bool,
    files: &'static storage::Files,
    /// Counted once after an app opens and first reads it, since counting walks the file system.
    free_storage_kilobytes: Cell<Option<Value>>,
    /// Until when an incoming call rings, unless it is answered, ended or controlled from the watch first.
    call_ringing_until: Option<Instant>,
    /// Whether an app read the free storage before it was counted, so the loop counts it.
    free_storage_wanted: Cell<bool>,
    /// The part of a file the open app read ahead with `prefetch_file`, until a `file_bytes` takes it.
    prefetch: RefCell<Option<Prefetch>>,
    /// The file the open app last read a part of, kept open so its next parts need no look-up.
    open_file: RefCell<Option<Box<storage::OpenFile>>>,
    /// The shared library's icons file, kept open on its own so reading icons never closes the app's file.
    icons_file: RefCell<Option<Box<storage::OpenFile>>>,
    /// A paged app's image file, kept open on its own so reading its code never closes the app's other files.
    image_file: RefCell<Option<Box<storage::OpenFile>>>,
    /// The closed apps with subscriptions still to run, and when each comes due.
    closed_apps: ClosedApps,
    /// While a closed app runs, what it asked of the screen.
    closed_run: Option<ClosedRun>,
    /// The folder the open app last listed with `open_folder`.
    folder: FolderListing,
    /// The folder the open app asked for, which the loop lists once the host call returns.
    folder_wanted: Option<alloc::string::String>,
}

/// A part of a file read ahead into RAM, as the file was when `prefetch_file` asked for it.
struct Prefetch {
    path: heapless::String<{ battletime_flash_store::LONGEST_PATH_BYTES }>,
    offset: u32,
    /// Whether it holds fewer bytes than were asked for because the file ends there.
    reaches_file_end: bool,
    part: storage::FilePart,
}

impl SystemValues for WatchSystem {
    fn read(&self, value: SystemValue) -> Value {
        // Apps see the face shown, which differs from the setting when the chosen face is off or gone.
        if value == SystemValue::WatchFace {
            return self.installed.face_number() as Value;
        }
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
        if let Some(number) = heart_rate::read(value).or_else(|| self.folder.read(value)) {
            return number;
        }
        match value {
            SystemValue::Now => clock::utc_now(),
            SystemValue::UtcOffset => clock::utc_offset(),
            SystemValue::BatteryPercent => battery::percent() as Value,
            SystemValue::IsCharging => battery::is_charging() as Value,
            SystemValue::BatteryMillivolts => battery::millivolts() as Value,
            SystemValue::StepsToday => motion::steps_today() as Value,
            SystemValue::ShakeSpeed => motion::shake_speed() as Value,
            SystemValue::StepTrip => motion::read_history(StepHistory::trip_steps) as Value,
            SystemValue::StepDayCount => DAYS_REMEMBERED as Value,
            SystemValue::StepHourCount => (DAYS_REMEMBERED * HOURS_PER_DAY) as Value,
            SystemValue::IsPhoneConnected => phone::is_connected() as Value,
            SystemValue::Brightness => self.brightness as Value,
            SystemValue::NextAlarm => self.closed_apps.next_alarm(&self.open_app_id),
            SystemValue::InstalledAppCount => self.installed.listed_count() as Value,
            SystemValue::PackageCount => self.installed.packages.len() as Value,
            SystemValue::AppMemoryBudget => battletime_runtime::APP_HEAP_BUDGET_BYTES as Value,
            SystemValue::LauncherStyle => self.installed.look.style as Value,
            SystemValue::LauncherView => self.installed.look.view as Value,
            SystemValue::LauncherGroupCount => self.installed.groups.len() as Value,
            SystemValue::StartedAt => clock::started_at(),
            SystemValue::FreeStorageKilobytes => self.free_storage_kilobytes.get().unwrap_or_else(|| {
                self.free_storage_wanted.set(true);
                0
            }),
            SystemValue::BootloaderCrc => about::bootloader_crc(),
            SystemValue::BluetoothAddress => about::bluetooth_address(),
            SystemValue::FlashId => about::flash_id(),
            SystemValue::TouchChipId => about::touch_chip_id(),
            SystemValue::TouchFirmwareVersion => about::touch_firmware_version(),
            SystemValue::AccelerometerChipId => about::accelerometer_chip_id(),
            SystemValue::FreeHeapBytes => crate::free_heap_bytes() as Value,
            SystemValue::LeastFreeHeapBytes => {
                crate::restart::least_free_heap_bytes(crate::free_heap_bytes()) as Value
            }
            SystemValue::AllocationFailures => crate::restart::allocation_failures() as Value,
            SystemValue::StackOverflows => crate::restart::stack_overflows() as Value,
            SystemValue::TaskCount => tasks::count() as Value,
            SystemValue::FirmwareState => storage::firmware_state() as Value,
            _ => 0,
        }
    }

    fn read_text(&self, value: SystemValue) -> &str {
        match value {
            SystemValue::FirmwareVersion => crate::FIRMWARE_VERSION,
            SystemValue::ResetReason => crate::restart::restart_reason(),
            SystemValue::FirmwareBuildDate => crate::FIRMWARE_BUILD_DATE,
            SystemValue::BootloaderVersion => about::bootloader_version(),
            SystemValue::BootloaderBuildDate => about::bootloader_build_date(),
            SystemValue::WatchFaceName => &self.installed.face_name,
            _ => self
                .phone
                .read_text(value)
                .or_else(|| self.folder.read_text(value))
                .unwrap_or(""),
        }
    }

    /// Each listed app's icon, which the host draws from the icons `apps` keeps, the kept notifications and the step history.
    fn read_element(&self, value: SystemValue, index: Value) -> Value {
        if is_notification_value(value) {
            return self.notifications.read_element(value, index);
        }
        if let Some(number) = self
            .folder
            .read_element(value, index)
            .or_else(|| self.phone.read_element(value, index))
        {
            return number;
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
                _ => battletime_runtime::NONE,
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
            SystemValue::LauncherGroupIcon => self
                .installed
                .group_icon(place)
                .map_or(battletime_runtime::NONE, |icon| icon.to_value() as Value),
            SystemValue::LauncherGroupFirst | SystemValue::LauncherGroupSize => self
                .installed
                .groups
                .get(place)
                .map_or(battletime_runtime::NONE, |group| match value {
                    SystemValue::LauncherGroupFirst => group.first as Value,
                    _ => group.count as Value,
                }),
            SystemValue::PackageRefusal => self.installed.refusal_of(place, &self.open_app_id) as Value,
            SystemValue::TaskRuns => {
                tasks::task(place).map_or(battletime_runtime::NONE, |task| task.runs as Value)
            }
            SystemValue::TaskBusyMilliseconds => tasks::task(place)
                .map_or(battletime_runtime::NONE, |task| {
                    tasks::busy_milliseconds(&task) as Value
                }),
            _ => battletime_runtime::NONE,
        }
    }

    /// Each listed app's name and id, and each kept notification's text.
    fn read_element_text(&self, value: SystemValue, index: Value) -> &str {
        if is_notification_value(value) {
            return self.notifications.read_element_text(value, index);
        }
        if let Some(text) = self.folder.read_element_text(value, index) {
            return text;
        }
        let place = usize::try_from(index).unwrap_or(usize::MAX);
        if value == SystemValue::LauncherGroupName {
            return self.installed.group_name(place);
        }
        if value == SystemValue::TaskName {
            return tasks::task(place).map_or("", |task| task.name());
        }
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
        self.read_prefetched(path, offset, out)
            .or_else(|| self.read_through_open_file(path, offset, out))
    }
}

/// Whether the value is one of the kept notifications' fields, which all share one bit.
fn is_notification_value(value: SystemValue) -> bool {
    value.bit() == SystemValue::UnreadNotifications.bit()
}

impl WatchSystem {
    /// Starts reading part of a file ahead, dropping the part read ahead before so its memory is free first.
    fn prefetch_file(&mut self, path: &str, offset: Value, length: Value) {
        *self.prefetch.get_mut() = None;
        let length = length.clamp(0, PREFETCH_BYTES as Value) as usize;
        let (Ok(offset), Ok(kept_path)) = (u32::try_from(offset), heapless::String::try_from(path)) else {
            return;
        };
        if length == 0 {
            return;
        }
        let Some(part) = self.files.start_reading_part(path, offset, length) else {
            return;
        };
        *self.prefetch.get_mut() = Some(Prefetch {
            path: kept_path,
            offset,
            reaches_file_end: part.len() < length,
            part,
        });
    }

    /// Reads part of a file through the file kept open, opening `path` in its place first if it is another.
    fn read_through_open_file(&self, path: &str, offset: u32, out: &mut [u8]) -> Option<usize> {
        let slot = match path {
            LIBRARY_ICONS_PATH => &self.icons_file,
            _ if path.ends_with(battletime_packages::IMAGE_FILE) => &self.image_file,
            _ => &self.open_file,
        };
        let mut kept = slot.borrow_mut();
        if kept.as_ref().is_none_or(|file| file.path() != path) {
            *kept = self.files.open_now(path).ok().map(Box::new);
        }
        let file = kept.as_mut()?;
        self.files.read_open_now(file, offset, out).ok()
    }

    /// Copies a read the part read ahead covers, waiting for its last bytes if they are still moving.
    ///
    /// The part is dropped once its last byte is taken; a read it does not cover goes to the flash.
    fn read_prefetched(&self, path: &str, offset: u32, out: &mut [u8]) -> Option<usize> {
        let mut held = self.prefetch.borrow_mut();
        let prefetch = held.as_mut().filter(|prefetch| prefetch.path == path)?;
        let start = offset.checked_sub(prefetch.offset)? as usize;
        let reaches_file_end = prefetch.reaches_file_end;
        let bytes = prefetch.part.bytes();
        let available = bytes.len().checked_sub(start)?;
        if available < out.len() && !reaches_file_end {
            return None;
        }
        let count = available.min(out.len());
        out[..count].copy_from_slice(&bytes[start..start + count]);
        if count == available {
            *held = None;
        }
        crate::diagnostics::add_one(crate::diagnostics::Counter::PrefetchedReads);
        Some(count)
    }

    /// Walks the file system from the loop rather than deep inside a redraw, so its frames stay off the peak stack.
    fn count_free_storage_if_wanted(&self) {
        if !self.free_storage_wanted.replace(false) {
            return;
        }
        let counted = self.files.free_kilobytes_now().map_or(0, Value::from);
        self.free_storage_kilobytes.set(Some(counted));
        system_values::changed(&[SystemValue::FreeStorageKilobytes]);
    }

    /// Lists the folder the open app asked for from the loop rather than inside its handler, as the free storage is counted.
    // Out of line, so the listing's frame is on the stack only while it runs and never under a redraw.
    #[inline(never)]
    fn list_folder_if_wanted(&mut self) {
        let Some(path) = self.folder_wanted.take() else {
            return;
        };
        if !self.files.list_folder_now(&path, &mut self.folder) {
            self.folder.clear();
        }
        system_values::changed(&[SystemValue::FolderPath]);
    }

    /// Looks for the bootloader's version record from the loop the first time an app reads it, as the free storage is counted.
    fn find_bootloader_record_if_wanted(&self) {
        if about::find_record_if_wanted() {
            system_values::changed(&[SystemValue::BootloaderVersion]);
        }
    }
}

impl WatchSystem {
    fn is_ringing_for_call(&self) -> bool {
        self.call_ringing_until
            .is_some_and(|until| Instant::now() < until)
    }

    /// Forgets the call ring, and says whether it was still playing.
    fn stop_call_ring(&mut self) -> bool {
        let was_ringing = self.is_ringing_for_call();
        self.call_ringing_until = None;
        was_ringing
    }
}

impl System for WatchSystem {
    fn carry_out(&mut self, effect: &AppEffect) {
        if self.closed_run.as_mut().is_some_and(|run| run.hold_back(effect)) {
            return;
        }
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
        crate::infinitime_services::send_music_event(effect.kind, effect.number(0));
        if let Some(line) = app_phone::line_for_effect(effect.kind, effect.number(0)) {
            // Answering, rejecting or muting the call silences the watch too.
            if effect.kind == EffectKind::CallControl && self.stop_call_ring() {
                vibration::request(Some(Vibration::Stop));
            }
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
            EffectKind::StartHeartRate => heart_rate::start(),
            EffectKind::StopHeartRate => heart_rate::stop(),
            EffectKind::MeasureHeartRate => heart_rate::measure_once(),
            EffectKind::SetHeartRateLed => {
                heart_rate::flash_led(effect.number(0) != 0, effect.number(1), effect.number(2))
            }
            EffectKind::SetBluetooth => settings::set_by_name(Setting::Bluetooth.name(), effect.number(0)),
            EffectKind::SetClock => {
                clock::set_by_hand(effect.number(0), effect.number(1), effect.number(2));
            }
            EffectKind::ChooseWatchFace if self.installed.face.is_some() => {
                self.request = Request::Open(Showing::FaceChooser)
            }
            EffectKind::OpenFolder => self.folder_wanted = effect.text(0).map(alloc::string::String::from),
            EffectKind::ConfirmFirmware => {
                self.files.confirm_firmware_now();
            }
            EffectKind::RollBackFirmware => self.request = Request::RollBackFirmware,
            EffectKind::PrefetchFile => {
                self.prefetch_file(effect.text(0).unwrap_or(""), effect.number(1), effect.number(2))
            }
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
#[inline(never)]
fn state_path(id: &str) -> heapless::String<64> {
    let mut path = heapless::String::new();
    let _ = write!(path, "{}/{}/state", battletime_packages::APPS_DIRECTORY, id);
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
    drawing_screen: DrawingScreen,
    lit: bool,
    /// While the help screen shows, when its time changes next, so it is painted once a minute.
    help_drawn_at: Option<Value>,
    /// Whether the screen went off by dimming, for an always-on screen, rather than by sleeping.
    dimmed: bool,
    /// Whether the light is lowered for the last moments before the screen timeout.
    light_lowered: bool,
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
    /// Whether the open app was reached round the quick-app ring, so its swipes step on round it and the way back is the face.
    in_quick_ring: bool,
    /// The screens moved on from, so going back retraces them in reverse; empty on the face.
    way_back: WayBack<Visit>,
    /// Until when the phone's find ring plays, unless the wearer touches the watch first.
    finding_until: Option<Instant>,
    /// A touch taken from the queue while merging drags, handled next.
    waiting_touch: Option<Touch>,
    /// Set when a new screen opens, so the rest of a touch begun on the old one goes nowhere until the finger lifts.
    finger_left_on_old_screen: bool,
    waking_touch: WakingTouch,
    /// The crash or not-enough-memory page, shown in place of an app.
    firmware_page: Option<ShownPage>,
    /// The id of the temporary app the phone ran, until it closes it or disconnects.
    temporary_id: Option<AppId>,
    /// The face's last tap, which a second close by makes a double tap that puts the watch to sleep.
    face_double_tap: DoubleTapFinder,
    /// How opening the temporary app went: the heap it needs, or why it would not open.
    temporary_outcome: Option<Result<u32, PackageError>>,
    /// Set while a closed app that just vibrated is opened, so opening does not stop the vibration.
    keeps_vibration_on_open: bool,
}

impl Shell {
    /// Closes what is open and opens another, coming in by `transition`; the face is the fallback when an image will not open.
    fn open(&mut self, showing: Showing, transition: Transition) {
        self.battery_screen_returns_to = None;
        self.in_quick_ring = false;
        // The face is home, so there is nothing to go back to from it.
        if showing == Showing::Face {
            self.way_back.clear();
        }
        let now = clock::utc_now();
        self.finger_left_on_old_screen = true;
        self.close_open_app(now);
        self.save_settings();
        if !self.keeps_vibration_on_open {
            vibration::request(Some(Vibration::Stop));
        }
        heart_rate::stop();
        self.finding_until = None;
        self.system.call_ringing_until = None;
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
        let installed = &self.system.installed;
        let (id, built_in) = match showing {
            Showing::Face | Showing::FaceChooser => (installed.face.clone(), None),
            Showing::Launcher => (installed.has_launcher.then(|| apps::LAUNCHER_ID.into()), None),
            Showing::App(place) => (installed.listed_id(place).map(alloc::string::String::from), None),
            Showing::Notifications => (Some("notifications".into()), Some(NOTIFICATIONS_IMAGE)),
            Showing::QuickSettings => (Some("quick-settings".into()), Some(QUICK_SETTINGS_IMAGE)),
            Showing::Temporary => (self.temporary_id.as_deref().map(Into::into), None),
        };
        restart::set_open_app(id.as_deref().unwrap_or(""));
        let directory = match showing {
            Showing::Temporary => battletime_packages::TEMPORARY_DIRECTORY,
            _ => battletime_packages::APPS_DIRECTORY,
        };
        // Where the image lies on the flash, from which a paged app's code is read as it runs.
        let image_path = match (&id, built_in) {
            (Some(id), None) => Some(battletime_packages::file_in(
                directory,
                id,
                battletime_packages::IMAGE_FILE,
            )),
            _ => None,
        };
        let image = match built_in {
            Some(_) => None,
            None => match id.and_then(|id| apps::read_image(self.files, directory, &id)) {
                Some(image) => Some(image),
                None => {
                    self.open_failed(showing);
                    return;
                }
            },
        };
        let bytes = match (&image, built_in) {
            (Some(image), _) => image.get(),
            (None, Some(bytes)) => bytes,
            (None, None) => return,
        };
        apps::keep_list_for(&mut self.system.installed, self.files, bytes);
        if showing == Showing::Launcher {
            self.launcher_icons = Some(LauncherIcons::read(self.files, &self.system.installed));
        }
        if let Some(page) = crate::crash_screen::refusal_for(bytes, showing == Showing::Face) {
            if let Some(image) = image {
                image.release();
            }
            restart::set_open_app("");
            // The phone hears why a temporary app did not fit, and the wearer goes back to what it would have covered.
            match (showing, page) {
                (Showing::Temporary, FirmwarePage::NotEnoughMemory { needed_bytes, .. }) => {
                    let refusal = Refusal::TooLarge {
                        need_bytes: needed_bytes,
                    };
                    self.temporary_outcome = Some(Err(PackageError::Refused(refusal)));
                }
                (_, page) => self.firmware_page = Some(ShownPage::new(page)),
            }
            return;
        }
        self.system.free_storage_kilobytes.set(None);
        self.system.free_storage_wanted.set(false);
        // A temporary app starts afresh each time, from the data files beside it.
        let saved = AppImage::parse(bytes)
            .ok()
            .filter(|_| showing != Showing::Temporary)
            .and_then(|parsed| parsed.string(parsed.id_string))
            .and_then(|id| self.files.read_now(&state_path(id)))
            .unwrap_or_default();
        let paged_path =
            image_path.filter(|_| AppImage::parse(bytes).is_ok_and(|parsed| pages_code(&parsed)));
        let Some(resident) = self.start_host(bytes, paged_path.as_deref(), &saved, now) else {
            if let Some(image) = image {
                image.release();
            }
            self.open_failed(showing);
            return;
        };
        if showing == Showing::Temporary {
            let need = AppImage::parse(bytes).map_or(0, |parsed| estimate_memory_need(&parsed).total_bytes);
            self.temporary_outcome = Some(Ok(need));
        }
        match resident {
            // A paged app keeps only its resident image, so the whole one goes before its screen is built.
            Some(resident) => {
                if let Some(image) = image {
                    image.release();
                }
                self.image = Some(resident);
                if let Some(host) = &mut self.host {
                    host.bring_to_front(now, &mut self.system);
                }
            }
            None => self.image = image,
        }
        if let Some(host) = &mut self.host {
            host.set_next_transition(transition);
        }
    }

    /// Closes the open app and saves its state.
    // Out of line, since taking the host moves it onto the stack, and `open` keeps its frame while the next app's handlers run.
    #[inline(never)]
    fn close_open_app(&mut self, now: Value) {
        if let Some(mut host) = self.host.take() {
            host.close(now, &mut self.system);
            self.save_state_of(&mut host);
            self.note_closed_app(&host, now);
        }
        *self.system.prefetch.get_mut() = None;
        *self.system.open_file.get_mut() = None;
        *self.system.icons_file.get_mut() = None;
        *self.system.image_file.get_mut() = None;
        self.system.folder.clear();
        self.system.folder_wanted = None;
    }

    /// Opens a host on the image into `self.host`, giving none when it would not open.
    ///
    /// With `paged_path`, the app's code stays in its image file there, and this gives back the
    /// resident image the host keeps; the caller then frees the whole image and brings the host to the front.
    fn start_host(
        &mut self,
        bytes: &'static [u8],
        paged_path: Option<&str>,
        saved: &[u8],
        now: Value,
    ) -> Option<Option<Lent<[u8]>>> {
        self.system.open_app_name = app_name_of(bytes);
        self.system.open_app_id = AppImage::parse(bytes)
            .ok()
            .and_then(|image| image.string(image.id_string))
            .and_then(|id| heapless::String::try_from(id).ok())
            .unwrap_or_default();
        let resident = match paged_path {
            Some(path) => Some(self.start_paged_host(bytes, path, saved, now)?),
            None => {
                let host =
                    AppHost::open_into(&mut self.host, bytes, self.theme, saved, now, &mut self.system)
                        .ok()?;
                if let Some(library) = self.library {
                    // An app built for a newer library would fail wherever it calls into it, so it is not opened.
                    if let Err(OpenError::LibraryTooOld { .. }) =
                        host.set_library(library, now, &mut self.system)
                    {
                        self.host = None;
                        return None;
                    }
                }
                None
            }
        };
        if let (Some(host), Some(icons)) = (&mut self.host, &self.launcher_icons) {
            host.set_installed_app_icons(icons.list(), &self.system);
        }
        Some(resident)
    }

    /// Opens a paged host on the image's resident part, giving that part, which the shell then keeps.
    #[inline(never)]
    fn start_paged_host(
        &mut self,
        bytes: &'static [u8],
        path: &str,
        saved: &[u8],
        now: Value,
    ) -> Option<Lent<[u8]>> {
        let resident = Lent::new(resident_image(bytes).ok()?.into_boxed_slice());
        // Opened here, so its look-up in the store never runs below the interpreter, whose code reads then need only a DMA read.
        if self.system.read_through_open_file(path, 0, &mut [0; 4]).is_none() {
            resident.release();
            return None;
        }
        let opened = AppHost::open_paged_into(
            &mut self.host,
            bytes,
            resident.get(),
            path,
            self.library,
            self.theme,
            saved,
            now,
            &mut self.system,
        );
        if opened.is_err() {
            self.host = None;
            resident.release();
            return None;
        }
        Some(resident)
    }

    /// Falls back to the face when anything else will not open, or to the help screen without one.
    fn open_failed(&mut self, showing: Showing) {
        if showing == Showing::Temporary {
            self.temporary_outcome = Some(Err(PackageError::WouldNotOpen));
        } else if showing == Showing::Face {
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
        self.show_first_screen();
    }

    /// Shows the crash page while the previous run's crash waits for the wearer, or else the face; nothing is open before.
    fn show_first_screen(&mut self) {
        self.find_closed_apps();
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
    #[inline(never)]
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
        // Places in the launcher's list move when an app is turned on or off, so remembered ones could open the wrong app.
        self.way_back.clear();
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
    #[inline(never)]
    fn save_state_of(&self, host: &mut AppHost<'static>) {
        // A temporary app keeps no state, and has no folder in `/apps` to keep it in.
        if self.showing == Showing::Temporary {
            return;
        }
        let Some(id) = host.image().string(host.image().id_string) else {
            return;
        };
        // Only as much as this app's persistent state can take, so closing a large app never runs out of heap.
        let size = battletime_runtime::memory_need::saved_state_bytes(host.image());
        if size == 0 {
            return;
        }
        let mut buffer = vec![0; size];
        if let Ok(Some(length)) = host.save_changed_state(&mut buffer) {
            self.write_state(id, &buffer[..length]);
        }
    }

    /// Applies a change from the companion app, then tells it the settings as they now are.
    #[inline(never)]
    fn change_settings(&mut self, change: &SettingsChange) {
        self.set_theme(change.applied_to(self.theme));
        settings::set_values(&change.values);
        let report = battletime_companion::settings_report(&self.theme, &settings::current());
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
        // The face is picked first, so the open app redraws with the new face's name.
        if changed & Setting::WatchFace.bit() != 0 {
            self.face_choice_changed(Transition::None);
        }
        if let Some(host) = &mut self.host {
            app_settings::tell_app_about_changes(host, changed, clock::utc_now(), &mut self.system);
        }
        if changed & Setting::HeartRateInterval.bit() != 0 {
            heart_rate::interval_changed();
        }
        if !self.lit {
            if changed & (Setting::WakeOnTouch.bit() | Setting::SleepMode.bit()) != 0 {
                touch::listen_for(listening_while_dark());
            }
            self.save_settings();
        }
    }

    /// Picks the face again, and shows the new one at once, coming in by `transition`, when the face or the chooser is on the screen.
    #[inline(never)]
    fn face_choice_changed(&mut self, transition: Transition) {
        let face_before = self.system.installed.face.clone();
        self.system.installed.choose_face_again(self.files);
        let face_showing =
            matches!(self.showing, Showing::Face | Showing::FaceChooser) && self.firmware_page.is_none();
        if face_showing && self.system.installed.face != face_before {
            self.open(self.showing, transition);
        }
    }

    /// Chooses the face `steps` places on among the faces turned on and slides it in by `transition`; with one face nothing moves.
    fn step_face(&mut self, steps: i32, transition: Transition) {
        if let Some(number) = self.system.installed.stepped_face_number(steps) {
            settings::set_by_name(Setting::WatchFace.name(), number as Value);
            self.face_choice_changed(transition);
        }
    }

    /// Buzzes when an hour or half hour, as the chimes setting asks, has passed since the last check.
    #[inline(never)]
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

    /// Moves on to `showing`, coming in by `transition`, and records the screen left on the way back.
    fn go_forward(&mut self, showing: Showing, transition: Transition, in_quick_ring: bool) {
        let from = Visit {
            showing: self.showing,
            in_quick_ring: self.in_quick_ring,
        };
        self.way_back.forward(
            from,
            Visit {
                showing,
                in_quick_ring,
            },
            transition,
        );
        self.open(showing, transition);
        self.in_quick_ring = in_quick_ring && self.showing == showing;
    }

    /// Goes back to the screen left last, the way it went, or to the face when nothing was left; the battery screen goes back to what it covered, by `transition`.
    fn go_back(&mut self, transition: Transition) {
        if let Some(showing) = self.battery_screen_returns_to {
            self.open(showing, transition);
            return;
        }
        if self.showing == Showing::Face {
            return;
        }
        match self.way_back.back() {
            Some(step) => {
                self.open(step.screen.showing, step.way_back());
                self.in_quick_ring = step.screen.in_quick_ring && self.showing == step.screen.showing;
            }
            None => self.open(Showing::Face, transition),
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
        // The chooser's face never hears a touch, so picking it cannot change its options.
        if self.showing == Showing::FaceChooser {
            if let Touch::Swipe(direction) = touch {
                self.swipe(direction);
            }
            return;
        }
        let Some(host) = &mut self.host else {
            return;
        };
        let now = clock::utc_now();
        let result = host.touch(touch, now, &mut self.system);
        if self.showing == Showing::Face {
            let taken_by_face = result != EventResult::NotHandled;
            if self.face_double_tap.touch(&touch, taken_by_face, now) {
                self.system.screen_request = Some(false);
                return;
            }
        }
        if let (EventResult::NotHandled, Touch::Swipe(direction)) = (result, touch) {
            self.swipe(direction);
        }
    }

    /// Acts on a swipe the open screen did not take, as `swipes` says.
    fn swipe(&mut self, direction: SwipeDirection) {
        let screen = match self.showing {
            Showing::Face => SwipeScreen::Face,
            Showing::Launcher => SwipeScreen::Launcher,
            Showing::Notifications => SwipeScreen::Notifications,
            Showing::QuickSettings => SwipeScreen::QuickSettings,
            Showing::App(_) if self.in_quick_ring => SwipeScreen::QuickApp,
            Showing::App(_) | Showing::Temporary => SwipeScreen::Other,
            Showing::FaceChooser => SwipeScreen::FaceChooser,
        };
        match swipes::action_for(direction, screen) {
            SwipeAction::Nothing => {}
            SwipeAction::OpenLauncher => self.go_forward(Showing::Launcher, Transition::Up, false),
            SwipeAction::OpenNotifications => {
                self.go_forward(Showing::Notifications, Transition::Down, false)
            }
            SwipeAction::OpenQuickSettings => {
                self.go_forward(Showing::QuickSettings, Transition::Down, false)
            }
            SwipeAction::GoBack => self.go_back(Transition::Down),
            SwipeAction::NextQuickApp => self.step_round_quick_apps(Step::Next, Transition::Left),
            SwipeAction::PreviousQuickApp => self.step_round_quick_apps(Step::Previous, Transition::Right),
            SwipeAction::NextFace => self.step_face(1, Transition::Left),
            SwipeAction::PreviousFace => self.step_face(-1, Transition::Right),
        }
    }

    /// Opens the next or previous app in the quick-app ring, or the face once past either end.
    fn step_round_quick_apps(&mut self, step: Step, transition: Transition) {
        let saved_ring = self.files.read_now(quick_apps::QUICK_APPS_PATH);
        let ring = saved_ring
            .as_deref()
            .and_then(|bytes| core::str::from_utf8(bytes).ok())
            .unwrap_or(quick_apps::DEFAULT_RING);
        let installed = &self.system.installed;
        let current = match self.showing {
            Showing::App(place) if self.in_quick_ring => installed.listed_id(place),
            _ => None,
        };
        let place = match quick_apps::step(ring, current, step, |id| installed.place_of(id).is_some()) {
            Destination::Face => None,
            Destination::App(id) => installed.place_of(id),
        };
        match place {
            Some(place) => self.go_forward(Showing::App(place), transition, true),
            None => self.go_forward(Showing::Face, transition, false),
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
        if let (true, Some(host)) = (
            side_button::goes_back_in_the_app_first(gesture, &situation),
            &mut self.host,
        ) {
            if host.go_back_a_screen(clock::utc_now(), &mut self.system) != EventResult::NotHandled {
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
                self.go_forward(Showing::Notifications, Transition::Down, false)
            }
        }
    }

    fn button_situation(&self) -> ButtonSituation {
        let open_screen = match self.showing {
            Showing::Face => OpenScreen::Face,
            Showing::Launcher => OpenScreen::Launcher,
            Showing::Notifications => OpenScreen::Notifications,
            Showing::App(_) | Showing::QuickSettings | Showing::Temporary => OpenScreen::App,
            Showing::FaceChooser => OpenScreen::FaceChooser,
        };
        ButtonSituation {
            screen_is_lit: self.lit,
            open_screen,
        }
    }

    #[inline(never)]
    fn system_values_changed(&mut self, changed: u32) {
        self.settings_changed();
        if changed & SystemValue::IsPhoneConnected.bit() != 0 && !phone::is_connected() {
            self.stop_temporary();
            app_events::with_state_hold(StateHold::release_all);
        }
        if changed & SystemValue::HeartRate.bit() != 0 {
            let (beats_per_minute, confidence) = heart_rate::latest_reading();
            self.deliver(PushedEvent::HeartRate {
                beats_per_minute,
                confidence,
            });
        }
        if let Some(host) = &mut self.host {
            host.report_system_value_changes(changed, clock::utc_now(), &mut self.system);
        }
        self.take_battery_events();
    }

    /// Runs or stops a temporary app, as the phone asked on the packages characteristic.
    fn temporary_changed(&mut self, change: Change) {
        match change {
            Change::RunTemporary(id) => self.run_temporary(id),
            Change::StopTemporary => self.stop_temporary(),
            Change::None | Change::Apps | Change::RunModule(_) => {}
        }
    }

    /// Opens the temporary app over whatever shows, turns the screen on and tells the phone; running it again while it shows starts it afresh.
    ///
    /// An app that will not open goes back to what it would have covered, and the phone hears why.
    fn run_temporary(&mut self, id: AppId) {
        let was_showing = self.showing == Showing::Temporary && self.firmware_page.is_none();
        self.temporary_id = Some(id);
        self.temporary_outcome = None;
        if was_showing {
            self.open(Showing::Temporary, Transition::None);
        } else {
            self.go_forward(Showing::Temporary, Transition::Up, false);
        }
        let outcome = self
            .temporary_outcome
            .take()
            .unwrap_or(Err(PackageError::WouldNotOpen));
        let opened = outcome.is_ok();
        let _ = packages::REPLIES.try_send(run_reply(outcome));
        if opened {
            self.system.screen_request = Some(true);
        } else {
            self.stop_temporary();
        }
    }

    /// Closes the temporary app, going back to what it covered.
    fn stop_temporary(&mut self) {
        if self.temporary_id.is_none() {
            return;
        }
        self.way_back.forget(|visit| visit.showing == Showing::Temporary);
        if self.battery_screen_returns_to == Some(Showing::Temporary) {
            self.battery_screen_returns_to = Some(Showing::Face);
        }
        if self.showing == Showing::Temporary {
            self.go_back(Transition::Down);
        }
        self.temporary_id = None;
    }

    /// Keeps and buzzes for a phone notification, takes the phone's other updates, and passes readings and notifications to the open app.
    #[inline(never)]
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
                // A ringing find or call goes on, so the notification neither buzzes over it nor opens an app.
                if !self.is_finding() && !self.system.is_ringing_for_call() {
                    self.show_notification_preview();
                    vibration::request(haptics::notification_vibration(is_do_not_disturb()));
                }
            }
            AppEvent::NotificationDismissed(id) => {
                self.system.notifications_changed |= self.system.notifications.dismiss(id);
            }
            // A hold let go of without a write leaves only a kept save to write, which the loop does next.
            AppEvent::StateWritten(app, written) => {
                if written {
                    self.state_written(&app);
                }
            }
            AppEvent::Acceleration(reading) => self.deliver(PushedEvent::Acceleration {
                x: reading.x_in_milli_g as Value,
                y: reading.y_in_milli_g as Value,
                z: reading.z_in_milli_g as Value,
            }),
            AppEvent::Shake => self.deliver(PushedEvent::Motion(MotionKind::Shake)),
            AppEvent::WakeGesture(gesture) => self.take_wake_gesture(gesture),
        }
    }

    /// Wakes a dark screen for a raised or shaken wrist, and puts a lit one to sleep for a lowered arm.
    fn take_wake_gesture(&mut self, gesture: WakeGesture) {
        let screen_request = match gesture {
            WakeGesture::Raise | WakeGesture::Shake => !self.lit,
            WakeGesture::Lower => self.lit && !self.screen_is_held_on(),
        };
        if screen_request {
            self.system.screen_request = Some(!self.lit);
        }
    }

    /// Whether the open app or a status page keeps the screen on.
    fn screen_is_held_on(&self) -> bool {
        self.system.keep_screen_on || status_holds_screen()
    }

    fn deliver(&mut self, event: PushedEvent) {
        if let Some(host) = &mut self.host {
            host.deliver(event, clock::utc_now(), &mut self.system);
        }
    }

    /// Opens the notifications on their preview of the newest one, outside do not disturb, and turns a dark screen on.
    ///
    /// The preview goes back by itself after a few seconds, so it opens over what is open and goes back there.
    /// The screen turns on through `follow_requests`, as for the battery screen, so it wakes once and counts as input.
    fn show_notification_preview(&mut self) {
        // The crash and memory pages stay up until the wearer answers them.
        if is_do_not_disturb() || self.firmware_page.is_some() {
            return;
        }
        // The notifications already show the new one, so they are left as they are.
        if self.showing != Showing::Notifications {
            let transition = if self.lit {
                Transition::Down
            } else {
                Transition::None
            };
            self.go_forward(Showing::Notifications, transition, false);
            if let (Showing::Notifications, Some(host)) = (self.showing, &mut self.host) {
                host.start_on_screen(NOTIFICATION_PREVIEW_SCREEN, clock::utc_now(), &mut self.system);
            }
        }
        if !self.lit {
            self.system.screen_request = Some(true);
        }
    }

    /// Buzzes and rings as the phone asks, keeps its music and weather, and tells the open app.
    fn phone_updated(&mut self, mut updates: PhoneUpdates) {
        let now = clock::utc_now();
        let is_do_not_disturb = is_do_not_disturb();
        if let Some(count) = updates.buzzes {
            vibration::request(haptics::phone_buzzes_vibration(count, is_do_not_disturb));
        }
        if let Some(call) = &updates.call {
            self.take_call(call.kind, is_do_not_disturb);
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

    /// Reads the accelerometer only as often as the open app needs while the screen is on, and for the wake gestures watched for.
    #[inline(never)]
    fn match_accelerometer_to_app(&self) {
        let period = match &self.host {
            Some(host) if self.lit => app_motion::reading_period_in_milliseconds(host),
            _ => None,
        };
        motion::read_acceleration_every(period);
        let screen = ScreenState {
            is_lit: self.lit,
            is_held_on: self.screen_is_held_on(),
        };
        motion::watch_for(wake_gestures::gestures_to_watch(&settings::current(), screen));
        let shows_shake_speed = self.lit && self.host.as_ref().is_some_and(app_motion::shows_shake_speed);
        motion::report_shake_speed(shows_shake_speed);
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
        self.light_lowered = false;
        // A finger still down as the panel stops reporting would otherwise never be seen to lift.
        foreground::set_finger_down(false);
        touch::listen_for(listening_while_dark());
        self.dimmed = app_settings::screen_dims_instead_of_sleeping(&settings::current());
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
        self.files.let_flash_sleep(true);
    }

    fn turn_screen_on(&mut self) {
        self.lit = true;
        self.files.let_flash_sleep(false);
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

    /// Lowers the light shortly before the screen timeout, brings it back after input, and turns the screen off at the timeout.
    fn follow_screen_timeout(&mut self, last_input: Instant) {
        let held_lit = !self.lit || self.system.keep_screen_on || status_holds_screen();
        let since_input = Instant::now().saturating_duration_since(last_input).as_millis();
        let stage = if held_lit {
            ScreenTimeoutStage::Lit
        } else {
            screen_timeout_stage(since_input, screen_timeout().as_millis())
        };
        match stage {
            ScreenTimeoutStage::TimedOut => self.turn_screen_off(),
            ScreenTimeoutStage::LightLowered if !self.light_lowered => {
                self.backlight.lower();
                self.light_lowered = true;
            }
            ScreenTimeoutStage::Lit if self.light_lowered => {
                self.backlight.turn_on();
                self.light_lowered = false;
            }
            _ => {}
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
            Request::Open(showing) => self.go_forward(showing, Transition::Up, false),
            Request::Back => self.go_back(Transition::Down),
            Request::RollBackFirmware => {
                self.save_settings();
                self.files.roll_back_firmware_now();
            }
        }
        screen_request == Some(true)
    }

    /// Whether the open app is part way through a transition, which the shell finishes before anything else.
    /// Whether a transition or a redraw is part way through, with more slices to paint before the next input.
    fn is_drawing_under_way(&self) -> bool {
        self.lit && !status_is_shown() && self.host.as_ref().is_some_and(|host| host.needs_drawing())
    }

    /// Acts on one touch: wakes the screen, hides the status page or goes to the open app or the shell.
    fn take_touch(&mut self, touch: Touch, last_input: &mut Instant) {
        let touch = self.latest_of(touch);
        *last_input = Instant::now();
        if self.stop_finding() {
            return;
        }
        if !self.lit {
            if app_settings::touch_wakes_screen(&settings::current(), &touch) {
                self.turn_screen_on();
                self.waking_touch.woke_screen();
            }
            return;
        }
        if !self.waking_touch.lets_through(&touch) {
            return;
        }
        if status_is_shown() {
            if !status_holds_screen() && matches!(touch, Touch::Release { .. }) {
                hide_status();
            }
            return;
        }
        if self.finger_left_on_old_screen {
            match touch {
                Touch::Press { .. } => self.finger_left_on_old_screen = false,
                Touch::Release { .. } => {
                    self.finger_left_on_old_screen = false;
                    return;
                }
                _ => return,
            }
        }
        self.touch(touch);
    }

    /// Acts on one side button gesture, unless it hides the status page or stops the phone finder.
    fn take_button_gesture(&mut self, gesture: Gesture, last_input: &mut Instant) {
        *last_input = Instant::now();
        if self.stop_finding() {
            return;
        }
        if self.lit && status_is_shown() {
            if !status_holds_screen() {
                hide_status();
            }
            return;
        }
        self.button_gesture(gesture);
    }

    /// Takes every touch and button gesture waiting, between two steps of a redraw.
    ///
    /// Nothing is taken while new packages wait to be loaded, since the open app may still read the old ones.
    fn take_queued_input(&mut self, last_input: &mut Instant) {
        while !packages::CHANGED.signaled() {
            if let Some(touch) = self.waiting_touch.take().or_else(|| TOUCHES.try_receive().ok()) {
                self.take_touch(touch, last_input);
            } else if let Ok(gesture) = BUTTON_GESTURES.try_receive() {
                self.take_button_gesture(gesture, last_input);
            } else {
                return;
            }
        }
    }

    async fn draw(&mut self) {
        // The app host renders first and awaits the bus before each strip it sends, so only it skips the wait.
        let only_the_host_draws = !REDRAW_EVERYTHING.load(Ordering::Relaxed)
            && self.firmware_page.is_none()
            && self.host.as_ref().is_some_and(|host| host.needs_drawing());
        // The shell's other drawing blocks on the bus, so a strip going out is awaited here instead.
        if !only_the_host_draws {
            crate::display_dma::bus_is_free().await;
        }
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
            Some(host) if host.needs_drawing() => draw_host(&mut self.drawing_screen, host).await,
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

/// The app whose battery page shows when the charger is plugged in or out, or the charge goes low.
const BATTERY_APP_ID: &str = "about";

/// The battery app's screen that opens for a battery event, in place of its first.
const BATTERY_SCREEN: &str = "battery";

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
                if let Some(host) = &mut self.host {
                    host.start_on_screen(BATTERY_SCREEN, clock::utc_now(), &mut self.system);
                }
            }
            if self.showing == battery_app && returns_to != battery_app {
                self.battery_screen_returns_to = Some(returns_to);
            }
        }
        self.system.screen_request = Some(true);
    }
}

// ============================================================================
// The call screen
// ============================================================================

/// The app that opens for an incoming call, offering to answer, reject or mute it.
const CALL_APP_ID: &str = "call";

impl Shell {
    /// Opens the call app and rings for an incoming call, or stops the ring once the call is answered or ended.
    ///
    /// Without the call app an incoming call only buzzes once and lights the screen, since nothing on it could stop a ring.
    fn take_call(&mut self, kind: battletime_companion::CallKind, is_do_not_disturb: bool) {
        let is_ringing = self.system.stop_call_ring();
        let Some(vibration) = haptics::call_vibration(kind, is_do_not_disturb, is_ringing) else {
            return;
        };
        if vibration == Vibration::CallRing {
            self.system.screen_request = Some(true);
            if !self.show_call_screen() {
                vibration::request(haptics::notification_vibration(is_do_not_disturb));
                return;
            }
            self.system.call_ringing_until = Some(Instant::now() + vibration::CALL_RING_DURATION);
        }
        vibration::request(Some(vibration));
    }

    /// Opens the call app over what is open, so going back returns there; false when it is not installed or a firmware page is up.
    fn show_call_screen(&mut self) -> bool {
        // The crash and memory pages stay up until the wearer answers them.
        if self.firmware_page.is_some() {
            return false;
        }
        let Some(place) = self.system.installed.place_of(CALL_APP_ID) else {
            return false;
        };
        let call_app = Showing::App(place);
        if self.showing != call_app {
            self.go_forward(call_app, Transition::Up, false);
        }
        self.showing == call_app && self.host.is_some()
    }
}

/// Waits out what is left of the panel's gap between going to sleep and waking, which is rarely any time at all.
fn wait_until_panel_can_switch(switched_at: Instant) {
    let can_switch_at = switched_at + Duration::from_millis(SLEEP_SWITCH_GAP_IN_MILLISECONDS);
    embassy_time::block_for(can_switch_at.saturating_duration_since(Instant::now()));
}

/// Polls a future in a stack frame of its own, so its locals never add to the frame of the task awaiting it.
pub(crate) struct OutOfLine<Inner>(pub(crate) Inner);

impl<Inner: core::future::Future> core::future::Future for OutOfLine<Inner> {
    type Output = Inner::Output;

    #[inline(never)]
    fn poll(
        self: core::pin::Pin<&mut Self>,
        context: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Inner::Output> {
        // SAFETY: the inner future is never moved out of the pinned wrapper.
        unsafe { self.map_unchecked_mut(|wrapper| &mut wrapper.0) }.poll(context)
    }
}

/// Draws one step of what changed into the screen's strip buffers, awaiting each strip before the next starts.
///
/// The last strip is left to go out by itself; the time counted includes the waits for the bus.
async fn draw_host(screen: &mut DrawingScreen, host: &mut AppHost<'static>) {
    let started = Instant::now();
    // The screen lends its own buffers, so no strip is needed here.
    let _ = host
        .draw_step_at_async(screen, &mut [], started.as_millis())
        .await;
    diagnostics::add(Counter::DrawMicroseconds, started.elapsed().as_micros() as u32);
}

/// The name an app image gives itself, cut to fit a notification's source.
// Kept out of the shell task, so the image's parse never adds to the task's stack frame.
#[inline(never)]
fn app_name_of(bytes: &[u8]) -> heapless::String<NOTIFICATION_SOURCE_BYTES> {
    AppImage::parse(bytes)
        .ok()
        .and_then(|parsed| parsed.string(parsed.name_string))
        .map(battletime_notifications::cut_to_fit)
        .unwrap_or_default()
}

#[embassy_executor::task]
pub async fn run_shell(
    display: &'static SharedDisplay,
    backlight: Backlight,
    files: &'static storage::Files,
) {
    crate::tasks::name_this_task("shell");
    Timer::after(START_PAGE_TIME).await;
    // An update that began during the start-up page keeps it.
    if !status_holds_screen() {
        hide_status();
    }
    files.wait_until_free().await;
    settings::load(files);
    // The heart rate task started before the saved interval was read.
    heart_rate::interval_changed();
    clock::load_time_zone(files);
    files.wait_until_free().await;

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
            prefetch: RefCell::new(None),
            open_file: RefCell::new(None),
            icons_file: RefCell::new(None),
            image_file: RefCell::new(None),
            folder: FolderListing::new(),
            folder_wanted: None,
            call_ringing_until: None,
            closed_apps: ClosedApps::NONE,
            closed_run: None,
        },
        theme,
        app_set_brightness: false,
        files,
        backlight,
        display,
        drawing_screen: {
            // Zeroed in the static itself, so the 3,840 B never pass through the stack.
            static STRIP_BUFFERS: static_cell::ConstStaticCell<StripBuffers> =
                static_cell::ConstStaticCell::new([[0; STRIP_BUFFER_PIXELS]; 2]);
            DrawingScreen::new(display, STRIP_BUFFERS.take())
        },
        lit: true,
        help_drawn_at: None,
        dimmed: false,
        light_lowered: false,
        panel_switched_at: Instant::from_ticks(0),
        settings_unsaved: false,
        chimes_checked_at: clock::now(),
        image: None,
        launcher_icons: None,
        library: apps::shared_library(),
        battery_screen_returns_to: None,
        in_quick_ring: false,
        way_back: WayBack::default(),
        finding_until: None,
        waiting_touch: None,
        finger_left_on_old_screen: false,
        waking_touch: WakingTouch::default(),
        firmware_page: None,
        temporary_id: None,
        temporary_outcome: None,
        keeps_vibration_on_open: false,
        face_double_tap: DoubleTapFinder::default(),
    };
    shell.show_first_screen();
    let mut last_input = Instant::now();

    loop {
        diagnostics::add_one(Counter::ShellLoops);
        diagnostics::set(Counter::ScreenLit, shell.lit as u32);
        diagnostics::note_flash_cache();
        shell.leave_crash_page_once_dealt_with();
        OutOfLine(shell.draw()).await;
        foreground::set_drawing(shell.is_drawing_under_way());
        while shell.is_drawing_under_way() {
            yield_now().await;
            shell.files.wait_until_free().await;
            // Input that came during the last step is taken now, so it never waits for the whole redraw and the queue never fills.
            shell.take_queued_input(&mut last_input);
            OutOfLine(shell.draw()).await;
        }
        foreground::set_drawing(false);
        shell.system.count_free_storage_if_wanted();
        shell.system.list_folder_if_wanted();
        shell.system.find_bootloader_record_if_wanted();
        shell.match_accelerometer_to_app();

        // A closed app must load its whole saved state, so a store write under way, such as the phone's, ends first.
        shell.files.wait_until_free().await;
        shell.write_released_saves();
        // A closed app that ran may have opened itself or changed the screen, so the loop draws again first.
        if shell.run_due_closed_apps() {
            if shell.follow_requests() {
                last_input = Instant::now();
            }
            continue;
        }
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
        wake_at = wake_at.min(shell.next_closed_app_due());
        if shell.lit && !shell.system.keep_screen_on && !status_holds_screen() {
            wake_at = wake_at.min(next_screen_timeout_step(last_input));
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
                        select3(
                            app_events::next(),
                            STATUS_CHANGED.wait(),
                            packages::TEMPORARY.wait(),
                        ),
                    ),
                )
                .await
            }
        };
        // Another task may have taken the store while the shell waited; the shell's calls below need it free.
        files.wait_until_free().await;
        // A new library may already lie where the open app's library was, so nothing reads it before the reload.
        if !matches!(woke, Either4::Fourth(Either4::Third(_))) && packages::CHANGED.try_take().is_some() {
            shell.reload_apps();
            continue;
        }
        match woke {
            Either4::First(touch) => {
                diagnostics::set(Counter::ShellLastWake, 1);
                shell.take_touch(touch, &mut last_input);
            }
            Either4::Second(gesture) => {
                diagnostics::set(Counter::ShellLastWake, 2);
                shell.take_button_gesture(gesture, &mut last_input);
            }
            Either4::Third(()) => {
                diagnostics::set(Counter::ShellLastWake, 3);
                shell.follow_screen_timeout(last_input);
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
            Either4::Fourth(Either4::Fourth(Either3::First(event))) => {
                diagnostics::set(Counter::ShellLastWake, 7);
                shell.app_event(event);
            }
            Either4::Fourth(Either4::Fourth(Either3::Third(change))) => {
                diagnostics::set(Counter::ShellLastWake, 9);
                shell.temporary_changed(change);
            }
            Either4::Fourth(Either4::Fourth(Either3::Second(()))) => {
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
        shell.follow_screen_timeout(last_input);
    }
}
