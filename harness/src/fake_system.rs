//! A fake firmware for the app host: a clock the test moves, system values the test sets, and a
//! record of every effect an app asks for and every failure of its code.

use std::cell::Cell;

use neotime_app_host::{AppEffect, AppFailure, System};
use neotime_runtime::resources::ImageReference;
use neotime_runtime::{EffectKind, SystemValue, SystemValues, Value, NONE};

// ============================================================================
// Starting values
// ============================================================================

/// Monday 5 January 2026, 09:00:00 UTC, so every test starts at the same moment.
pub const START_TIME_IN_MILLISECONDS: Value = 1_767_603_600_000;

const MINUTE_IN_MILLISECONDS: Value = 60_000;
const HOUR_IN_MILLISECONDS: Value = 60 * MINUTE_IN_MILLISECONDS;

/// The random generator's first state; any number but zero works.
const RANDOM_SEED: u32 = 0x2545_f491;

/// The phone sends at most this many days of forecast.
const MOST_FORECAST_DAYS: usize = 5;

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
        Go | OpenApp | ShowToast | Log | SendToPhone | SetSetting => &[0],
        Notify => &[0, 1],
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
        ];
        for (value, number) in starting_values {
            system.set(value, number);
        }
        system.set_text(WeatherLocation, "Test City");
        system.set_text(MusicArtist, "The Testers");
        system.set_text(MusicTrack, "Test Pattern");
        system.set_text(NavigationInstruction, "Turn left onto Main Street");
        system
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
        match value {
            SystemValue::Now => self.now_in_milliseconds,
            SystemValue::WeatherForecastLength => self.forecast.len().min(MOST_FORECAST_DAYS) as Value,
            SystemValue::InstalledAppCount => self.installed_apps.len() as Value,
            other => self.values[other as usize],
        }
    }

    fn read_text(&self, value: SystemValue) -> &str {
        &self.texts[value as usize]
    }

    fn read_element(&self, value: SystemValue, index: Value) -> Value {
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
        let Some(app) = usize::try_from(index)
            .ok()
            .and_then(|place| self.installed_apps.get(place))
        else {
            return "";
        };
        match value {
            SystemValue::InstalledAppName => &app.name,
            SystemValue::InstalledAppId => &app.id,
            _ => "",
        }
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
