//! The watch-wide settings: what each one is, its limits and default, and the bytes they are saved as.
//!
//! The firmware keeps one `Settings` for the whole watch. Apps change it with `set_setting`, the
//! companion app through the firmware, and both go through `Settings::set`, so a value outside its
//! limits never gets in. They are saved in `/settings/system` as records: one byte naming the
//! setting, then its value as a little-endian `i32`. A setting this firmware does not know is
//! skipped, so a file from older or newer firmware still reads. Bluetooth is saved too but never read
//! back, so a restart always turns the radio on again. Updates on until the next restart read back as
//! off. Sleep mode is a third state of do not disturb: turning it on turns do not disturb on, and
//! turning do not disturb off ends it.

#![no_std]

// ============================================================================
// The settings
// ============================================================================

/// One setting; its number is fixed forever, because it is what the file and Bluetooth carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    Uses24HourClock = 0,
    UsesMetricUnits = 1,
    StepGoal = 2,
    Brightness = 3,
    ScreenTimeoutSeconds = 4,
    AlwaysOnScreen = 5,
    Chimes = 6,
    DoNotDisturb = 7,
    Bluetooth = 8,
    AllowsUpdates = 9,
    WakeOnTouch = 10,
    AlarmsInDoNotDisturb = 11,
    HeartRateInterval = 12,
    RaiseToWake = 13,
    LowerToSleep = 14,
    ShakeToWake = 15,
    ShakeThreshold = 16,
    SleepMode = 17,
    WatchFace = 18,
    /// Number 19 is kept for a setting another change adds.
    GadgetbridgeName = 20,
}

/// The lowest and highest value a setting takes, both included, and the value it starts at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub lowest: i32,
    pub highest: i32,
    pub default: i32,
}

/// What `Setting::Chimes` holds.
pub mod chimes {
    pub const OFF: i32 = 0;
    pub const EVERY_HOUR: i32 = 1;
    pub const EVERY_HALF_HOUR: i32 = 2;
}

/// What `Setting::HeartRateInterval` holds: InfiniTime's choices for measuring with no app asking.
pub mod heart_rate_interval {
    pub const OFF: i32 = 0;
    pub const CONTINUOUS: i32 = 1;
    pub const EVERY_30_SECONDS: i32 = 2;
    pub const EVERY_MINUTE: i32 = 3;
    pub const EVERY_5_MINUTES: i32 = 4;
    pub const EVERY_10_MINUTES: i32 = 5;
    pub const EVERY_30_MINUTES: i32 = 6;

    /// How long between measurements for a choice that measures now and then, or `None` for off and continuous.
    pub fn seconds_between(choice: i32) -> Option<u32> {
        Some(match choice {
            EVERY_30_SECONDS => 30,
            EVERY_MINUTE => 60,
            EVERY_5_MINUTES => 5 * 60,
            EVERY_10_MINUTES => 10 * 60,
            EVERY_30_MINUTES => 30 * 60,
            _ => return None,
        })
    }
}

/// What `Setting::AllowsUpdates` holds, as InfiniTime's firmware update screen offers it.
pub mod allows_updates {
    pub const OFF: i32 = 0;
    pub const ON: i32 = 1;
    pub const UNTIL_RESTART: i32 = 2;
}

/// What `Setting::WakeOnTouch` holds.
pub mod wake_on_touch {
    pub const OFF: i32 = 0;
    pub const TAP: i32 = 1;
    pub const DOUBLE_TAP: i32 = 2;
}

/// What `Setting::WatchFace` holds: 0 before a face is chosen, else the face's number that `neotime_packages::faces::face_number` makes from its id.
pub mod watch_face {
    pub const NONE_CHOSEN: i32 = 0;
    pub const HIGHEST: i32 = i32::MAX;
}

/// What `Setting::ShakeThreshold` holds: InfiniTime's shake speed a wrist must pass to wake the screen.
pub mod shake_threshold {
    pub const LOWEST: i32 = 0;
    pub const HIGHEST: i32 = 4095;
    pub const DEFAULT: i32 = 150;
}

impl Setting {
    /// How many settings there are; numbers may skip one, so a setting's place in `ALL` is its `index`.
    pub const COUNT: usize = 20;

    pub const ALL: [Setting; Setting::COUNT] = [
        Setting::Uses24HourClock,
        Setting::UsesMetricUnits,
        Setting::StepGoal,
        Setting::Brightness,
        Setting::ScreenTimeoutSeconds,
        Setting::AlwaysOnScreen,
        Setting::Chimes,
        Setting::DoNotDisturb,
        Setting::Bluetooth,
        Setting::AllowsUpdates,
        Setting::WakeOnTouch,
        Setting::AlarmsInDoNotDisturb,
        Setting::HeartRateInterval,
        Setting::RaiseToWake,
        Setting::LowerToSleep,
        Setting::ShakeToWake,
        Setting::ShakeThreshold,
        Setting::SleepMode,
        Setting::WatchFace,
        Setting::GadgetbridgeName,
    ];

    pub fn from_number(number: u8) -> Option<Setting> {
        Setting::ALL
            .into_iter()
            .find(|setting| setting.number() == number)
    }

    pub const fn number(self) -> u8 {
        self as u8
    }

    /// The setting's place in `ALL`, and in every array kept in that order.
    pub const fn index(self) -> usize {
        match self {
            Setting::GadgetbridgeName => 19,
            other => other as usize,
        }
    }

    /// The name apps pass to `set_setting` and read under `settings`.
    pub fn name(self) -> &'static str {
        match self {
            Setting::Uses24HourClock => "uses_24_hour_clock",
            Setting::UsesMetricUnits => "uses_metric_units",
            Setting::StepGoal => "step_goal",
            Setting::Brightness => "brightness",
            Setting::ScreenTimeoutSeconds => "screen_timeout_seconds",
            Setting::AlwaysOnScreen => "always_on_screen",
            Setting::Chimes => "chimes",
            Setting::DoNotDisturb => "do_not_disturb",
            Setting::Bluetooth => "bluetooth",
            Setting::AllowsUpdates => "allows_updates",
            Setting::WakeOnTouch => "wake_on_touch",
            Setting::AlarmsInDoNotDisturb => "alarms_in_do_not_disturb",
            Setting::HeartRateInterval => "heart_rate_interval",
            Setting::RaiseToWake => "raise_to_wake",
            Setting::LowerToSleep => "lower_to_sleep",
            Setting::ShakeToWake => "shake_to_wake",
            Setting::ShakeThreshold => "shake_threshold",
            Setting::SleepMode => "sleep_mode",
            Setting::WatchFace => "watch_face",
            Setting::GadgetbridgeName => "gadgetbridge_name",
        }
    }

    pub fn from_name(name: &str) -> Option<Setting> {
        Setting::ALL.into_iter().find(|setting| setting.name() == name)
    }

    pub const fn limits(self) -> Limits {
        const fn limits(lowest: i32, highest: i32, default: i32) -> Limits {
            Limits {
                lowest,
                highest,
                default,
            }
        }
        match self {
            Setting::Uses24HourClock => limits(0, 1, 1),
            Setting::UsesMetricUnits => limits(0, 1, 1),
            Setting::StepGoal => limits(1_000, 50_000, 10_000),
            Setting::Brightness => limits(1, 3, 2),
            Setting::ScreenTimeoutSeconds => limits(5, 300, 15),
            Setting::AlwaysOnScreen => limits(0, 1, 0),
            Setting::Chimes => limits(chimes::OFF, chimes::EVERY_HALF_HOUR, chimes::OFF),
            Setting::DoNotDisturb => limits(0, 1, 0),
            Setting::Bluetooth => limits(0, 1, 1),
            Setting::AllowsUpdates => limits(
                allows_updates::OFF,
                allows_updates::UNTIL_RESTART,
                allows_updates::ON,
            ),
            Setting::WakeOnTouch => limits(wake_on_touch::OFF, wake_on_touch::DOUBLE_TAP, wake_on_touch::TAP),
            Setting::AlarmsInDoNotDisturb => limits(0, 1, 1),
            Setting::HeartRateInterval => limits(
                heart_rate_interval::OFF,
                heart_rate_interval::EVERY_30_MINUTES,
                heart_rate_interval::OFF,
            ),
            Setting::RaiseToWake => limits(0, 1, 0),
            Setting::LowerToSleep => limits(0, 1, 0),
            Setting::ShakeToWake => limits(0, 1, 0),
            Setting::ShakeThreshold => limits(
                shake_threshold::LOWEST,
                shake_threshold::HIGHEST,
                shake_threshold::DEFAULT,
            ),
            Setting::SleepMode => limits(0, 1, 0),
            Setting::WatchFace => limits(
                watch_face::NONE_CHOSEN,
                watch_face::HIGHEST,
                watch_face::NONE_CHOSEN,
            ),
            Setting::GadgetbridgeName => limits(0, 1, 0),
        }
    }

    /// Whether a saved value is read back at start; Bluetooth is not, so a watch with no way to turn it on again can restart instead.
    pub fn survives_restart(self) -> bool {
        self != Setting::Bluetooth
    }

    /// The setting's bit in a mask of settings, such as the ones that changed.
    pub fn bit(self) -> u32 {
        1 << self.number()
    }
}

// ============================================================================
// Holding the values
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingError {
    UnknownSetting,
    OutOfLimits { lowest: i32, highest: i32 },
}

/// Every setting's current value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    values: [i32; Setting::COUNT],
}

impl Default for Settings {
    fn default() -> Settings {
        Settings::DEFAULT
    }
}

impl Settings {
    /// Every setting at its default, usable where a constant is needed, such as a `static`.
    pub const DEFAULT: Settings = {
        let mut values = [0; Setting::COUNT];
        let mut place = 0;
        while place < Setting::COUNT {
            values[place] = Setting::ALL[place].limits().default;
            place += 1;
        }
        Settings { values }
    };

    pub fn get(&self, setting: Setting) -> i32 {
        self.values[setting.index()]
    }

    pub fn is_on(&self, setting: Setting) -> bool {
        self.get(setting) != 0
    }

    /// Sets a value inside its limits; returns whether it changed.
    pub fn set(&mut self, setting: Setting, value: i32) -> Result<bool, SettingError> {
        let limits = setting.limits();
        if !(limits.lowest..=limits.highest).contains(&value) {
            return Err(SettingError::OutOfLimits {
                lowest: limits.lowest,
                highest: limits.highest,
            });
        }
        let changed = self.get(setting) != value;
        self.values[setting.index()] = value;
        self.keep_sleep_mode_inside_do_not_disturb(setting);
        Ok(changed)
    }

    /// Sleep mode is the third state of do not disturb, so turning it on turns do not disturb on, and turning do not disturb off ends it.
    fn keep_sleep_mode_inside_do_not_disturb(&mut self, changed: Setting) {
        match changed {
            Setting::SleepMode if self.is_on(Setting::SleepMode) => {
                self.values[Setting::DoNotDisturb.index()] = 1;
            }
            Setting::DoNotDisturb if !self.is_on(Setting::DoNotDisturb) => {
                self.values[Setting::SleepMode.index()] = 0;
            }
            _ => {}
        }
    }

    /// The bits of every setting whose value differs from `before`.
    pub fn differences_from(&self, before: &Settings) -> u32 {
        Setting::ALL
            .into_iter()
            .filter(|&setting| self.get(setting) != before.get(setting))
            .fold(0, |bits, setting| bits | setting.bit())
    }

    /// Whether sleep mode is on, which also turns off every wake gesture and the always-on screen.
    pub fn is_sleep_mode(&self) -> bool {
        self.is_on(Setting::SleepMode)
    }

    /// Sets a setting by the name apps use, taking any number an app can hold.
    pub fn set_by_name(&mut self, name: &str, value: i64) -> Result<Setting, SettingError> {
        let setting = Setting::from_name(name).ok_or(SettingError::UnknownSetting)?;
        let value = i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX });
        self.set(setting, value)?;
        Ok(setting)
    }
}

// ============================================================================
// Records: the bytes on flash and over Bluetooth
// ============================================================================

/// One record: the setting's number, then its value as a little-endian `i32`.
pub const RECORD_BYTES: usize = 5;

/// Every setting as records.
pub const ALL_RECORDS_BYTES: usize = Setting::COUNT * RECORD_BYTES;

/// The file starts with this byte, so a later layout can be told apart.
pub const FILE_FORMAT: u8 = 1;

pub const FILE_BYTES: usize = 1 + ALL_RECORDS_BYTES;

impl Settings {
    /// Writes every setting as records, in number order; returns the bytes written.
    pub fn write_records(&self, buffer: &mut [u8; ALL_RECORDS_BYTES]) -> usize {
        for (record, setting) in buffer
            .as_chunks_mut::<RECORD_BYTES>()
            .0
            .iter_mut()
            .zip(Setting::ALL)
        {
            record[0] = setting.number();
            record[1..].copy_from_slice(&self.get(setting).to_le_bytes());
        }
        ALL_RECORDS_BYTES
    }

    /// Applies records, skipping unknown settings and values outside their limits.
    ///
    /// Returns the bits of the settings whose value changed. A short record at the end is ignored.
    pub fn apply_records(&mut self, bytes: &[u8]) -> u32 {
        let before = *self;
        for record in bytes.as_chunks::<RECORD_BYTES>().0 {
            let Some(setting) = Setting::from_number(record[0]) else {
                continue;
            };
            let value = i32::from_le_bytes([record[1], record[2], record[3], record[4]]);
            let _ = self.set(setting, value);
        }
        self.differences_from(&before)
    }

    pub fn write_file(&self, buffer: &mut [u8; FILE_BYTES]) -> usize {
        buffer[0] = FILE_FORMAT;
        let mut records = [0; ALL_RECORDS_BYTES];
        self.write_records(&mut records);
        buffer[1..].copy_from_slice(&records);
        FILE_BYTES
    }

    /// Reads a saved file; anything missing, unknown, out of limits or not kept across a restart keeps its default.
    ///
    /// Updates turned on only until the next restart read back as off.
    pub fn from_file(bytes: &[u8]) -> Settings {
        let mut settings = Settings::default();
        if let [FILE_FORMAT, records @ ..] = bytes {
            settings.apply_records(records);
        }
        for setting in Setting::ALL
            .into_iter()
            .filter(|setting| !setting.survives_restart())
        {
            settings.values[setting.index()] = setting.limits().default;
        }
        if settings.get(Setting::AllowsUpdates) == allows_updates::UNTIL_RESTART {
            settings.values[Setting::AllowsUpdates.index()] = allows_updates::OFF;
        }
        settings
    }
}

// ============================================================================
// The screen timeout
// ============================================================================

/// How long before the screen timeout the light drops to its low level, as InfiniTime does.
pub const LOWER_LIGHT_BEFORE_TIMEOUT_IN_MILLISECONDS: u64 = 2_000;

/// Where a lit screen stands in its timeout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenTimeoutStage {
    Lit,
    LightLowered,
    TimedOut,
}

/// How long after the last input the light drops, never more than half the timeout.
fn light_lowers_after_in_milliseconds(timeout_in_milliseconds: u64) -> u64 {
    timeout_in_milliseconds - LOWER_LIGHT_BEFORE_TIMEOUT_IN_MILLISECONDS.min(timeout_in_milliseconds / 2)
}

/// Where a lit screen stands some time after the last input.
pub fn screen_timeout_stage(
    since_input_in_milliseconds: u64,
    timeout_in_milliseconds: u64,
) -> ScreenTimeoutStage {
    if since_input_in_milliseconds >= timeout_in_milliseconds {
        ScreenTimeoutStage::TimedOut
    } else if since_input_in_milliseconds >= light_lowers_after_in_milliseconds(timeout_in_milliseconds) {
        ScreenTimeoutStage::LightLowered
    } else {
        ScreenTimeoutStage::Lit
    }
}

/// How long after the last input the screen next moves on a stage: when its light drops, then when it times out.
pub fn next_screen_timeout_step_in_milliseconds(
    since_input_in_milliseconds: u64,
    timeout_in_milliseconds: u64,
) -> u64 {
    let lowers_after = light_lowers_after_in_milliseconds(timeout_in_milliseconds);
    if since_input_in_milliseconds < lowers_after {
        lowers_after
    } else {
        timeout_in_milliseconds
    }
}

#[cfg(test)]
mod tests;
