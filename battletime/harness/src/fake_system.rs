//! A fake firmware for the app host: a clock the test moves, system values the test sets, and a
//! record of every effect an app asks for and every failure of its code.

use std::cell::Cell;

use battletime_app_host::{folder::FolderListing, AppEffect, AppFailure, System};
use battletime_notifications::NotificationList;
use battletime_packages::faces::face_number;
use battletime_runtime::library_icons::LIBRARY_ICONS_PATH;
use battletime_runtime::resources::ImageReference;
use battletime_runtime::{EffectKind, SystemValue, SystemValues, Value, NONE};

// ============================================================================
// Starting values
// ============================================================================

/// Monday 5 January 2026, 09:00:00 UTC, so every test starts at the same moment.
pub const START_TIME_IN_MILLISECONDS: Value = 1_767_603_600_000;

const MINUTE_IN_MILLISECONDS: Value = 60_000;
const HOUR_IN_MILLISECONDS: Value = 60 * MINUTE_IN_MILLISECONDS;

/// The note the phone sent, which the notes app shows, at the path the companion app writes it to.
pub const STARTING_NOTE_PATH: &str = "/notes/note.txt";
/// Its lines are short enough not to wrap, so the browser preview, which cannot wrap as the watch does, shows the same lines.
pub const STARTING_NOTE: &str =
    "Packing list\n\n- Charger\n- Passport\n- Rain jacket\n- Two books\n\nCall the hotel\nbefore Friday.\n";

/// The question the phone ran the prompt app with, at the path the companion app writes it to; short enough not to wrap.
pub const STARTING_QUESTION_PATH: &str = "/temp/prompt/text";
pub const STARTING_QUESTION: &str = "Leave now?";
/// A QR code the phone sent, in the first of the QR code app's slots: `https://example.com/battletime`, 29 modules square.
pub const STARTING_QR_CODE_PATH: &str = "/qr/0";
pub const STARTING_QR_CODE: [u8; 118] = [
    0x1d, 0x1d, 0xfe, 0x07, 0x23, 0xf8, 0x82, 0xa1, 0x4a, 0x08, 0xba, 0xca, 0x8a, 0xe8, 0xba, 0xe3, 0x4a,
    0xe8, 0xba, 0x57, 0x1a, 0xe8, 0x82, 0x16, 0x82, 0x08, 0xfe, 0xaa, 0xab, 0xf8, 0x00, 0x94, 0x38, 0x00,
    0x82, 0xab, 0x56, 0x70, 0x8d, 0x62, 0x75, 0xb0, 0x62, 0x88, 0x2d, 0x80, 0x09, 0xeb, 0x99, 0x40, 0x7a,
    0x66, 0x0b, 0x08, 0x21, 0x86, 0x73, 0x98, 0xc6, 0x6f, 0x8b, 0xe0, 0x0d, 0x96, 0x82, 0xa8, 0x2a, 0x1b,
    0x50, 0x60, 0x85, 0xa8, 0xbb, 0xb8, 0xc2, 0x9e, 0xd3, 0xc8, 0x90, 0xfb, 0x9c, 0x00, 0xb3, 0x88, 0xbf,
    0xb8, 0x00, 0xbc, 0xe8, 0xc0, 0xfe, 0x37, 0xda, 0xe0, 0x82, 0x5c, 0x78, 0x98, 0xba, 0x35, 0xdf, 0xd0,
    0xba, 0x40, 0xb9, 0x70, 0xba, 0x30, 0x27, 0xf0, 0x82, 0x45, 0x05, 0xe8, 0xfe, 0xcc, 0xb7, 0xa0,
];
/// The QR code app's list: two codes, the one above captioned `example.com` and a second in slot 3 with no file.
pub const STARTING_QR_LIST_PATH: &str = "/qr/list";
pub const STARTING_QR_LIST: [u8; 2 + 2 * 26] = qr_list([(0, "example.com"), (3, "Guest Wi-Fi")]);

/// The survival app's list: CPR in slot 0 and severe bleeding in slot 1 under first aid (topic 0), and being lost
/// in slot 9 under wilderness survival (topic 1).
pub const STARTING_SURVIVAL_LIST_PATH: &str = "/survival/list";
pub const STARTING_SURVIVAL_LIST: [u8; 2 + 3 * 26] =
    survival_list([(0, 0, "CPR"), (1, 0, "Severe bleeding"), (9, 1, "Lost in the wild")]);
/// The start of the companion app's CPR guide, in the survival app's first slot.
/// Its lines are broken where the watch wraps them, so the browser preview, which cannot wrap as the watch does, shows the same lines.
pub const STARTING_SURVIVAL_GUIDE_PATH: &str = "/survival/0";
pub const STARTING_SURVIVAL_GUIDE: &str = "Use when someone\ndoes not respond\nand is not\nbreathing\nnormally.\n\n\
1. Check it is\nsafe to go near.\n2. Shout and tap\ntheir shoulders.\nNo answer?\n3. Call the\nemergency number\n\
(112, 911, 999) on\nspeaker.\n";

/// The start of the bleeding and lost guides, in slots 1 and 9, with lines as short.
pub const STARTING_BLEEDING_GUIDE_PATH: &str = "/survival/1";
pub const STARTING_BLEEDING_GUIDE: &str = "1. Call the\nemergency\nnumber.\n2. Press hard\non the wound.\n";
pub const STARTING_LOST_GUIDE_PATH: &str = "/survival/9";
pub const STARTING_LOST_GUIDE: &str = "STOP:\nStop.\nThink.\nObserve.\nPlan.\n";

/// A list file holding these slots and captions, in the format `apps/qr-code` reads.
const fn qr_list<const COUNT: usize, const LENGTH: usize>(entries: [(u8, &str); COUNT]) -> [u8; LENGTH] {
    let mut file = [0u8; LENGTH];
    file[0] = 1;
    file[1] = COUNT as u8;
    let mut place = 0;
    while place < COUNT {
        let (slot, caption) = entries[place];
        let start = 2 + place * 26;
        file[start] = slot;
        file[start + 1] = caption.len() as u8;
        let mut index = 0;
        while index < caption.len() {
            file[start + 2 + index] = caption.as_bytes()[index];
            index += 1;
        }
        place += 1;
    }
    file
}

/// A survival list file holding these slots, topics and titles, in the format `apps/survival` reads.
const fn survival_list<const COUNT: usize, const LENGTH: usize>(entries: [(u8, u8, &str); COUNT]) -> [u8; LENGTH] {
    let mut file = [0u8; LENGTH];
    file[0] = 2;
    file[1] = COUNT as u8;
    let mut place = 0;
    while place < COUNT {
        let (slot, topic, title) = entries[place];
        let start = 2 + place * 26;
        file[start] = slot;
        file[start + 1] = topic;
        file[start + 2] = title.len() as u8;
        let mut index = 0;
        while index < title.len() {
            file[start + 3 + index] = title.as_bytes()[index];
            index += 1;
        }
        place += 1;
    }
    file
}

/// The random generator's first state; any number but zero works.
const RANDOM_SEED: u32 = 0x2545_f491;

/// The phone sends at most this many days of forecast.
const MOST_FORECAST_DAYS: usize = 5;

/// Steps on each day of the history, today first; today's match the starting `steps_today`.
pub const STARTING_DAY_STEPS: [Value; STEP_HISTORY_DAYS] = [4321, 8765, 10_234, 6543, 12_001, 7890, 3456];
pub const STARTING_TRIP_STEPS: Value = 23_456;
pub const STEP_HISTORY_DAYS: usize = 7;
pub const HOURS_PER_DAY: usize = 24;

/// Today's steps by 09:00, split between the two hours before it.
const STEPS_TODAY_BEFORE_EIGHT: Value = 1500;

/// The steps in each hour of the history, at `days_back * 24 + hour`: today's by 09:00, and each earlier
/// day's spread over 08:00 to 17:00 with the rest in the last of those hours.
pub fn starting_step_hours() -> Vec<Value> {
    let mut hours = vec![0; STEP_HISTORY_DAYS * HOURS_PER_DAY];
    hours[7] = STEPS_TODAY_BEFORE_EIGHT;
    hours[8] = STARTING_DAY_STEPS[0] - STEPS_TODAY_BEFORE_EIGHT;
    for (days_back, total) in STARTING_DAY_STEPS.iter().enumerate().skip(1) {
        let day = &mut hours[days_back * HOURS_PER_DAY..(days_back + 1) * HOURS_PER_DAY];
        day[8..18].fill(total / 10);
        day[17] += total % 10;
    }
    hours
}

/// One coming day of the forecast.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForecastDay {
    pub condition: Value,
    pub low: Value,
    pub high: Value,
}

/// An app the launcher lists, by the name it shows and the id `open_app` takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledApp {
    pub name: String,
    pub id: String,
}

/// A launcher group: its name, its icon as an image reference, and the run of `installed_apps` it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeLauncherGroup {
    pub name: String,
    pub icon: Value,
    pub first: Value,
    pub count: Value,
}

/// An installed app or face as the Software app lists it, with the numbers the package manager gives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakePackage {
    pub id: String,
    pub name: String,
    pub is_face: bool,
    pub enabled: bool,
    pub memory: Value,
    pub refusal: Value,
}

/// The heap budget an open app has, as the watch gives it.
pub const STARTING_APP_MEMORY_BUDGET: Value = battletime_runtime::memory_need::APP_HEAP_BUDGET_BYTES as Value;

/// The only face among the starting packages, which the watch shows, so `settings.watch_face` starts at its number.
pub const STARTING_FACE_ID: &str = "digital";
pub const STARTING_FACE_NAME: &str = "Digital";

/// A watch with an app too large to turn on, a face, the launcher, the Software app and two apps.
pub fn starting_packages() -> Vec<FakePackage> {
    let package = |id: &str, name: &str, is_face, enabled, memory, refusal| FakePackage {
        id: id.to_string(),
        name: name.to_string(),
        is_face,
        enabled,
        memory,
        refusal,
    };
    vec![
        package("atlas", "Atlas", false, false, 41_000, 1),
        package("dice", "Dice", false, false, 11_700, 0),
        package(STARTING_FACE_ID, STARTING_FACE_NAME, true, true, 12_700, 4),
        package("launcher", "Apps", false, true, 9_100, 3),
        package("software", "Software", false, true, 12_100, 5),
        package("timer", "Timer", false, true, 13_600, 0),
    ]
}

// ============================================================================
// Recorded effects
// ============================================================================

/// One effect an app asked for, with its arguments as numbers and, where they are text, as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedEffect {
    pub kind: EffectKind,
    pub numbers: Vec<Value>,
    pub texts: Vec<Option<String>>,
}

impl RecordedEffect {
    pub fn number(&self, position: usize) -> Value {
        self.numbers.get(position).copied().unwrap_or(0)
    }

    /// A text argument, such as a toast's message; None for an argument that is a number.
    pub fn text(&self, position: usize) -> Option<&str> {
        self.texts.get(position).and_then(Option::as_deref)
    }
}

/// Which of an effect's arguments are text, so they are recorded as text too.
fn text_arguments(kind: EffectKind) -> &'static [usize] {
    use EffectKind::*;
    match kind {
        Go | OpenApp | ShowToast | Log | SetSetting | SetAppEnabled | PrefetchFile | OpenFolder => &[0],
        Notify | SendToPhone => &[0, 1],
        _ => &[],
    }
}

// ============================================================================
// The fake system
// ============================================================================

/// One of the firmware's tasks, with how often it ran and for how long.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeTask {
    pub name: String,
    pub runs: Value,
    pub busy_milliseconds: Value,
}

/// Three tasks, as sdk/battletime/tests/parity.tsx starts them too.
pub fn starting_tasks() -> Vec<FakeTask> {
    [
        ("main", 1, 412),
        ("shell", 5_210, 61_250),
        ("bluetooth", 830, 2_470),
    ]
    .into_iter()
    .map(|(name, runs, busy_milliseconds)| FakeTask {
        name: name.to_string(),
        runs,
        busy_milliseconds,
    })
    .collect()
}

/// Everything the app host asks of the firmware, kept in memory for a test to set and read.
pub struct FakeSystem {
    pub now_in_milliseconds: Value,
    values: [Value; SystemValue::COUNT],
    texts: [String; SystemValue::COUNT],
    pub forecast: Vec<ForecastDay>,
    pub installed_apps: Vec<InstalledApp>,
    /// The launcher groups holding at least one of `installed_apps`, in the launcher's order.
    pub launcher_groups: Vec<FakeLauncherGroup>,
    /// Every installed app and face, on or off, as the Software app lists them.
    pub packages: Vec<FakePackage>,
    /// Set when an app turned a package on or off, so the watch tells the app once the call returns.
    pub packages_changed: bool,
    /// Set when an app set the clock, so the watch tells the app once the call returns.
    pub clock_changed: bool,
    /// Set when an app confirmed the firmware on trial, so the watch tells the app once the call returns.
    pub firmware_state_changed: bool,
    /// The notifications the watch keeps, which `notify` and the phone add to.
    pub notifications: NotificationList,
    /// The open app's name, which a notification it makes shows as its source.
    pub app_name: String,
    /// Set when an effect changed the notifications, so the watch tells the app once the call returns.
    pub notifications_changed: bool,
    /// The firmware's tasks, as the About app's task page lists them.
    pub tasks: Vec<FakeTask>,
    /// The steps in each hour of the step history, at `days_back * 24 + hour`.
    pub step_hours: Vec<Value>,
    /// While a closed app runs, what it asked of the screen.
    pub closed_run: Option<battletime_app_host::closed_apps::ClosedRun>,
    /// The watch's files by path, such as the note the phone sent.
    pub files: Vec<(String, Vec<u8>)>,
    /// Every line the app's effects sent the phone, newline included, as the firmware sends them.
    pub phone_lines: Vec<String>,
    /// The image files of apps opened paged, by path, which their code is read from as it runs.
    pub image_files: Vec<(String, &'static [u8])>,
    /// The folder the app last opened with `open_folder`, listed from `files`.
    pub folder: FolderListing,
    /// Set when the app opened a folder, so the watch tells the app once the call returns.
    pub folder_changed: bool,
    /// Set when an app asked for the face chooser, which the test watch opens once the handler returns.
    pub face_chooser_wanted: bool,
    random_state: Cell<u32>,
    effects: Vec<RecordedEffect>,
    failures: Vec<AppFailure>,
}

impl FakeSystem {
    /// A watch at `START_TIME_IN_MILLISECONDS` with plausible readings, like the simulator's.
    pub fn new() -> Self {
        let now = START_TIME_IN_MILLISECONDS;
        let mut system = FakeSystem {
            now_in_milliseconds: now,
            values: [0; SystemValue::COUNT],
            texts: std::array::from_fn(|_| String::new()),
            forecast: vec![
                ForecastDay {
                    condition: 1,
                    low: 9,
                    high: 17,
                },
                ForecastDay {
                    condition: 3,
                    low: 7,
                    high: 14,
                },
                ForecastDay {
                    condition: 0,
                    low: 10,
                    high: 20,
                },
            ],
            installed_apps: Vec::new(),
            launcher_groups: Vec::new(),
            packages: starting_packages(),
            packages_changed: false,
            clock_changed: false,
            firmware_state_changed: false,
            notifications: NotificationList::new(),
            app_name: String::new(),
            notifications_changed: false,
            step_hours: starting_step_hours(),
            closed_run: None,
            tasks: starting_tasks(),
            files: vec![
                (STARTING_NOTE_PATH.to_string(), STARTING_NOTE.as_bytes().to_vec()),
                (
                    STARTING_QUESTION_PATH.to_string(),
                    STARTING_QUESTION.as_bytes().to_vec(),
                ),
                (STARTING_QR_CODE_PATH.to_string(), STARTING_QR_CODE.to_vec()),
                (STARTING_QR_LIST_PATH.to_string(), STARTING_QR_LIST.to_vec()),
                (
                    STARTING_SURVIVAL_LIST_PATH.to_string(),
                    STARTING_SURVIVAL_LIST.to_vec(),
                ),
                (
                    STARTING_SURVIVAL_GUIDE_PATH.to_string(),
                    STARTING_SURVIVAL_GUIDE.as_bytes().to_vec(),
                ),
                (
                    STARTING_BLEEDING_GUIDE_PATH.to_string(),
                    STARTING_BLEEDING_GUIDE.as_bytes().to_vec(),
                ),
                (
                    STARTING_LOST_GUIDE_PATH.to_string(),
                    STARTING_LOST_GUIDE.as_bytes().to_vec(),
                ),
            ],
            phone_lines: Vec::new(),
            image_files: Vec::new(),
            folder: FolderListing::new(),
            folder_changed: false,
            face_chooser_wanted: false,
            random_state: Cell::new(RANDOM_SEED),
            effects: Vec::new(),
            failures: Vec::new(),
        };
        use SystemValue::*;
        let starting_values = [
            (BatteryPercent, 87),
            (BatteryMillivolts, 4056),
            (StepsToday, 4321),
            (StepGoal, 10_000),
            (Brightness, 2),
            (HeartRate, 64),
            (HeartRateMeasuredAt, now - 5 * MINUTE_IN_MILLISECONDS),
            (Uses24HourClock, 1),
            (UsesMetricUnits, 1),
            (IsBluetoothEnabled, 1),
            (IsPhoneConnected, 1),
            (MusicIsPlaying, 1),
            (NextAlarm, NONE),
            (WeatherTemperature, 18),
            (WeatherLow, 11),
            (WeatherHigh, 21),
            (WeatherReceivedAt, now - 20 * MINUTE_IN_MILLISECONDS),
            (WeatherSunrise, now - 2 * HOUR_IN_MILLISECONDS),
            (WeatherSunset, now + 8 * HOUR_IN_MILLISECONDS),
            (MusicPosition, 72_000),
            (MusicLength, 215_000),
            (NavigationDistance, 350),
            (NavigationProgressPercent, 40),
            (StepTrip, STARTING_TRIP_STEPS),
            (
                StartedAt,
                now - 3 * HOUR_IN_MILLISECONDS - 25 * MINUTE_IN_MILLISECONDS,
            ),
            (FreeStorageKilobytes, 2840),
            (AllowsUpdates, 1),
            (WakeOnTouch, 1),
            (AlarmsInDoNotDisturb, 1),
            (WatchFace, face_number(STARTING_FACE_ID) as Value),
            (HeartRateInterval, 0),
            (ShakeThreshold, 150),
            (CallState, 2),
            (AppMemoryBudget, STARTING_APP_MEMORY_BUDGET),
            (BootloaderCrc, 0x0C4E_2D17),
            (BluetoothAddress, 0xD12A_5C7E_903B),
            (FlashId, 0x0B_4016),
            (TouchChipId, 0xB4),
            (TouchFirmwareVersion, 2),
            (AccelerometerChipId, 0x13),
            (FreeHeapBytes, 18_432),
            (LeastFreeHeapBytes, 12_288),
            (AllocationFailures, 1),
            (StackOverflows, 0),
        ];
        for (value, number) in starting_values {
            system.set(value, number);
        }
        system.set_text(WeatherLocation, "Test City");
        system.set_text(MusicArtist, "The Testers");
        system.set_text(MusicTrack, "Test Pattern");
        system.set_text(NavigationInstruction, "Turn left onto Main Street");
        system.set_text(FirmwareVersion, "0.1.0+1b19aed");
        system.set_text(ResetReason, "watchdog");
        system.set_text(FirmwareBuildDate, "2026-01-04");
        system.set_text(BootloaderVersion, "0.1.0+0c4e2d1");
        system.set_text(BootloaderBuildDate, "2025-12-20");
        system.set_text(WatchFaceName, STARTING_FACE_NAME);
        system
    }

    /// Writes a whole file, as the phone does over Bluetooth, replacing any file at that path.
    pub fn write_file(&mut self, path: &str, bytes: &[u8]) {
        self.files.retain(|(name, _)| name != path);
        self.files.push((path.to_string(), bytes.to_vec()));
    }

    /// Sets a number value, such as the battery; `now` is the clock and is set through `now_in_milliseconds`.
    pub fn set(&mut self, value: SystemValue, number: Value) {
        self.values[value as usize] = number;
    }

    /// Sets a text value, such as the playing track.
    pub fn set_text(&mut self, value: SystemValue, text: &str) {
        self.texts[value as usize] = text.to_string();
    }

    /// Moves the clock to a local time chosen in the Settings app, keeping the zone, as the firmware's clock does.
    fn set_clock_by_hand(&mut self, year: Value, day_of_year: Value, minute_of_day: Value) {
        let Some(local) =
            battletime_current_time::local_time_from_day_and_minute(year, day_of_year, minute_of_day)
        else {
            return;
        };
        let utc = local - self.values[SystemValue::UtcOffset as usize];
        let moved_by = utc - self.now_in_milliseconds;
        self.now_in_milliseconds = utc;
        self.values[SystemValue::StartedAt as usize] += moved_by;
        self.clock_changed = true;
    }

    pub fn effects(&self) -> &[RecordedEffect] {
        &self.effects
    }

    pub fn take_effects(&mut self) -> Vec<RecordedEffect> {
        std::mem::take(&mut self.effects)
    }

    pub fn failures(&self) -> &[AppFailure] {
        &self.failures
    }
}

impl Default for FakeSystem {
    fn default() -> Self {
        FakeSystem::new()
    }
}

// ============================================================================
// What the app host reads
// ============================================================================

impl SystemValues for FakeSystem {
    fn read(&self, value: SystemValue) -> Value {
        if let Some(number) = self.notifications.read(value).or_else(|| self.folder.read(value)) {
            return number;
        }
        match value {
            SystemValue::Now => self.now_in_milliseconds,
            SystemValue::WeatherForecastLength => self.forecast.len().min(MOST_FORECAST_DAYS) as Value,
            SystemValue::InstalledAppCount => self.installed_apps.len() as Value,
            SystemValue::PackageCount => self.packages.len() as Value,
            SystemValue::LauncherGroupCount => self.launcher_groups.len() as Value,
            SystemValue::StepDayCount => (self.step_hours.len() / HOURS_PER_DAY) as Value,
            SystemValue::StepHourCount => self.step_hours.len() as Value,
            SystemValue::TaskCount => self.tasks.len() as Value,
            other => self.values[other as usize],
        }
    }

    fn read_text(&self, value: SystemValue) -> &str {
        if let Some(text) = self.folder.read_text(value) {
            return text;
        }
        &self.texts[value as usize]
    }

    fn read_element(&self, value: SystemValue, index: Value) -> Value {
        if value.bit() == SystemValue::UnreadNotifications.bit() {
            return self.notifications.read_element(value, index);
        }
        if let Some(number) = self.folder.read_element(value, index) {
            return number;
        }
        let Ok(place) = usize::try_from(index) else {
            return NONE;
        };
        if value == SystemValue::InstalledAppIcon {
            return match u16::try_from(place) {
                Ok(place) if (place as usize) < self.installed_apps.len() => {
                    ImageReference::InstalledApp(place).to_value() as Value
                }
                _ => NONE,
            };
        }
        if let Some(group) = self.launcher_groups.get(place) {
            match value {
                SystemValue::LauncherGroupIcon => return group.icon,
                SystemValue::LauncherGroupFirst => return group.first,
                SystemValue::LauncherGroupSize => return group.count,
                _ => {}
            }
        }
        if let Some(task) = self.tasks.get(place) {
            match value {
                SystemValue::TaskRuns => return task.runs,
                SystemValue::TaskBusyMilliseconds => return task.busy_milliseconds,
                _ => {}
            }
        }
        if let Some(package) = self.packages.get(place) {
            match value {
                SystemValue::PackageEnabled => return package.enabled as Value,
                SystemValue::PackageIsFace => return package.is_face as Value,
                SystemValue::PackageMemory => return package.memory,
                SystemValue::PackageRefusal => return package.refusal,
                _ => {}
            }
        }
        if value == SystemValue::StepHourSteps {
            return self.step_hours.get(place).copied().unwrap_or(NONE);
        }
        if value == SystemValue::StepDayTotal {
            return self
                .step_hours
                .chunks(HOURS_PER_DAY)
                .nth(place)
                .map_or(NONE, |day| day.iter().sum());
        }
        let Some(day) = self.forecast.get(place).filter(|_| place < MOST_FORECAST_DAYS) else {
            return NONE;
        };
        match value {
            SystemValue::WeatherForecastCondition => day.condition,
            SystemValue::WeatherForecastLow => day.low,
            SystemValue::WeatherForecastHigh => day.high,
            _ => NONE,
        }
    }

    fn read_element_text(&self, value: SystemValue, index: Value) -> &str {
        if value.bit() == SystemValue::UnreadNotifications.bit() {
            return self.notifications.read_element_text(value, index);
        }
        if let Some(text) = self.folder.read_element_text(value, index) {
            return text;
        }
        let place = usize::try_from(index).unwrap_or(usize::MAX);
        if value == SystemValue::LauncherGroupName {
            return self
                .launcher_groups
                .get(place)
                .map_or("", |group| group.name.as_str());
        }
        if value == SystemValue::TaskName {
            return self.tasks.get(place).map_or("", |task| task.name.as_str());
        }
        if let Some(package) = self.packages.get(place) {
            match value {
                SystemValue::PackageName => return &package.name,
                SystemValue::PackageId => return &package.id,
                _ => {}
            }
        }
        let Some(app) = self.installed_apps.get(place) else {
            return "";
        };
        match value {
            SystemValue::InstalledAppName => &app.name,
            SystemValue::InstalledAppId => &app.id,
            _ => "",
        }
    }

    fn read_file(&self, path: &str, offset: u32, out: &mut [u8]) -> Option<usize> {
        let image_file = self.image_files.iter().find(|(name, _)| name == path);
        let bytes = match self.files.iter().find(|(name, _)| name == path) {
            Some((_, bytes)) => bytes.as_slice(),
            None if image_file.is_some() => image_file.map_or(&[][..], |(_, bytes)| bytes),
            // The icons the companion puts on every watch, unless a test gives its own.
            None if path == LIBRARY_ICONS_PATH => crate::sources::icons_file(),
            None => return None,
        };
        let rest = bytes.get(offset as usize..).unwrap_or(&[]);
        let length = rest.len().min(out.len());
        out[..length].copy_from_slice(&rest[..length]);
        Some(length)
    }

    /// A xorshift generator, so every run of a test sees the same numbers.
    fn random_number(&self) -> u32 {
        let mut state = self.random_state.get();
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        self.random_state.set(state);
        state
    }
}

// ============================================================================
// What the app host hands back
// ============================================================================

impl battletime_app_host::closed_apps::RunsClosedApps for FakeSystem {
    fn closed_run(&mut self) -> &mut Option<battletime_app_host::closed_apps::ClosedRun> {
        &mut self.closed_run
    }
}

impl System for FakeSystem {
    fn carry_out(&mut self, effect: &AppEffect) {
        if self.closed_run.as_mut().is_some_and(|run| run.hold_back(effect)) {
            return;
        }
        let now = self.now_in_milliseconds;
        if self.notifications.carry_out(effect, &self.app_name, now) {
            self.notifications_changed = true;
        }
        if effect.kind == EffectKind::SetAppEnabled {
            let id = effect.text(0).unwrap_or_default();
            if let Some(package) = self.packages.iter_mut().find(|package| package.id == id) {
                if package.refusal == 0 {
                    package.enabled = effect.number(1) != 0;
                    self.packages_changed = true;
                }
            }
        }
        if effect.kind == EffectKind::OpenFolder {
            self.list_folder(effect.text(0).unwrap_or_default());
        }
        if effect.kind == EffectKind::ChooseWatchFace {
            self.face_chooser_wanted = true;
        }
        if effect.kind == EffectKind::SetClock {
            self.set_clock_by_hand(effect.number(0), effect.number(1), effect.number(2));
        }
        if effect.kind == EffectKind::ConfirmFirmware && self.values[SystemValue::FirmwareState as usize] == 1
        {
            self.values[SystemValue::FirmwareState as usize] = 0;
            self.firmware_state_changed = true;
        }
        if let Some(line) = battletime_app_host::phone::line_for_effect(effect.kind, effect.number(0)) {
            self.phone_lines.push(line.to_string());
        }
        if let Some(line) = battletime_app_host::phone::app_line_for_effect(effect) {
            self.phone_lines.push(line);
        }
        let argument_count = effect.kind.argument_count();
        let text_positions = text_arguments(effect.kind);
        let numbers = (0..argument_count)
            .map(|position| effect.number(position))
            .collect();
        let texts = (0..argument_count)
            .map(|position| match text_positions.contains(&position) {
                true => effect.text(position).map(str::to_string),
                false => None,
            })
            .collect();
        self.effects.push(RecordedEffect {
            kind: effect.kind,
            numbers,
            texts,
        });
    }

    fn app_failed(&mut self, failure: AppFailure) {
        self.failures.push(failure);
    }
}

impl FakeSystem {
    /// Lists a folder of `files` at once, where the watch lists it once the handler returns.
    fn list_folder(&mut self, path: &str) {
        let mut folder = std::mem::take(&mut self.folder);
        if folder.begin(path) {
            for (file_path, bytes) in &self.files {
                folder.add_file(file_path, bytes.len());
            }
        }
        self.folder = folder;
        self.folder_changed = true;
    }
}
