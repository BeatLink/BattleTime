//! A fake firmware for the app host: a clock the test moves, system values the test sets, and a
//! record of every effect an app asks for and every failure of its code.

use std::cell::Cell;

use neotime_app_host::{AppEffect, AppFailure, System};
use neotime_notifications::NotificationList;
use neotime_runtime::resources::ImageReference;
use neotime_runtime::{EffectKind, SystemValue, SystemValues, Value, NONE};

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
pub const STARTING_APP_MEMORY_BUDGET: Value = neotime_runtime::memory_need::APP_HEAP_BUDGET_BYTES as Value;

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
        package("digital", "Digital", true, true, 12_700, 4),
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
        Go | OpenApp | ShowToast | Log | SetSetting | SetAppEnabled => &[0],
        Notify | SendToPhone => &[0, 1],
        _ => &[],
    }
}

// ============================================================================
// The fake system
// ============================================================================

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
    /// The notifications the watch keeps, which `notify` and the phone add to.
    pub notifications: NotificationList,
    /// The open app's name, which a notification it makes shows as its source.
    pub app_name: String,
    /// Set when an effect changed the notifications, so the watch tells the app once the call returns.
    pub notifications_changed: bool,
    /// The steps in each hour of the step history, at `days_back * 24 + hour`.
    pub step_hours: Vec<Value>,
    /// The watch's files by path, such as the note the phone sent.
    pub files: Vec<(String, Vec<u8>)>,
    /// Every line the app's effects sent the phone, newline included, as the firmware sends them.
    pub phone_lines: Vec<String>,
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
            notifications: NotificationList::new(),
            app_name: String::new(),
            notifications_changed: false,
            step_hours: starting_step_hours(),
            files: vec![(STARTING_NOTE_PATH.to_string(), STARTING_NOTE.as_bytes().to_vec())],
            phone_lines: Vec::new(),
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
            (AppMemoryBudget, STARTING_APP_MEMORY_BUDGET),
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
        if let Some(number) = self.notifications.read(value) {
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
            other => self.values[other as usize],
        }
    }

    fn read_text(&self, value: SystemValue) -> &str {
        &self.texts[value as usize]
    }

    fn read_element(&self, value: SystemValue, index: Value) -> Value {
        if value.bit() == SystemValue::UnreadNotifications.bit() {
            return self.notifications.read_element(value, index);
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
        let place = usize::try_from(index).unwrap_or(usize::MAX);
        if value == SystemValue::LauncherGroupName {
            return self
                .launcher_groups
                .get(place)
                .map_or("", |group| group.name.as_str());
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
        let (_, bytes) = self.files.iter().find(|(name, _)| name == path)?;
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

impl System for FakeSystem {
    fn carry_out(&mut self, effect: &AppEffect) {
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
        if let Some(line) = neotime_app_host::phone::line_for_effect(effect.kind, effect.number(0)) {
            self.phone_lines.push(line.to_string());
        }
        if let Some(line) = neotime_app_host::phone::app_line_for_effect(effect) {
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
