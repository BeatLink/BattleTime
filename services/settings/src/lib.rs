//! The watch-wide settings: what each one is, its limits and default, and the bytes they are saved as.
//!
//! The firmware keeps one `Settings` for the whole watch. Apps change it with `set_setting`, the
//! companion app through the firmware, and both go through `Settings::set`, so a value outside its
//! limits never gets in. They are saved in `/settings/system` as records: one byte naming the
//! setting, then its value as a little-endian `i32`. A setting this firmware does not know is
//! skipped, so a file from older or newer firmware still reads.

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

impl Setting {
    pub const COUNT: usize = 8;

    pub const ALL: [Setting; Setting::COUNT] = [
        Setting::Uses24HourClock,
        Setting::UsesMetricUnits,
        Setting::StepGoal,
        Setting::Brightness,
        Setting::ScreenTimeoutSeconds,
        Setting::AlwaysOnScreen,
        Setting::Chimes,
        Setting::DoNotDisturb,
    ];

    pub fn from_number(number: u8) -> Option<Setting> {
        Setting::ALL.get(number as usize).copied()
    }

    pub fn number(self) -> u8 {
        self as u8
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
        }
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
        self.values[setting as usize]
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
        self.values[setting as usize] = value;
        Ok(changed)
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
        let mut changed = 0;
        for record in bytes.as_chunks::<RECORD_BYTES>().0 {
            let Some(setting) = Setting::from_number(record[0]) else {
                continue;
            };
            let value = i32::from_le_bytes([record[1], record[2], record[3], record[4]]);
            if let Ok(true) = self.set(setting, value) {
                changed |= setting.bit();
            }
        }
        changed
    }

    pub fn write_file(&self, buffer: &mut [u8; FILE_BYTES]) -> usize {
        buffer[0] = FILE_FORMAT;
        let mut records = [0; ALL_RECORDS_BYTES];
        self.write_records(&mut records);
        buffer[1..].copy_from_slice(&records);
        FILE_BYTES
    }

    /// Reads a saved file; anything missing, unknown or out of limits keeps its default.
    pub fn from_file(bytes: &[u8]) -> Settings {
        let mut settings = Settings::default();
        if let [FILE_FORMAT, records @ ..] = bytes {
            settings.apply_records(records);
        }
        settings
    }
}

#[cfg(test)]
mod tests;
