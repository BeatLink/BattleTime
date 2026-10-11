//! The wall clock: the time a phone last set, counted on by the chip's timer.
//!
//! Until a phone sets it, it starts from when the firmware was built. Times are milliseconds since
//! 1970 with no time zone; the phone writes local time, so the offset from UTC is always 0 for now.

use core::cell::Cell;

use embassy_sync::blocking_mutex::{raw::CriticalSectionRawMutex, Mutex};
use embassy_time::Instant;
use neotime_runtime::Value;

const MILLISECONDS_PER_SECOND: i64 = 1000;
const SECONDS_PER_DAY: i64 = 86_400;

/// The wall clock time when the chip's timer read 0.
static TIME_AT_START: Mutex<CriticalSectionRawMutex, Cell<Value>> =
    Mutex::new(Cell::new(parse_or_zero(env!("NEOTIME_BUILD_TIME_MS"))));

/// Milliseconds since 1970.
pub fn now() -> Value {
    TIME_AT_START.lock(Cell::get) + Instant::now().as_millis() as Value
}

pub fn set(now_in_milliseconds: Value) {
    TIME_AT_START.lock(|start| start.set(now_in_milliseconds - Instant::now().as_millis() as Value));
}

/// The chip's timer reading at a wall clock time, for sleeping until then.
pub fn instant_at(time: Value) -> Instant {
    let milliseconds_since_start = (time - TIME_AT_START.lock(Cell::get)).max(0);
    Instant::from_millis(milliseconds_since_start as u64)
}

// ============================================================================
// The Bluetooth Current Time characteristic
// ============================================================================

/// Sets the clock from a Current Time (0x2A2B) value: year, month, day, hour, minute, second and more.
pub fn set_from_current_time(bytes: &[u8]) -> bool {
    let [year_low, year_high, month, day, hour, minute, second, ..] = *bytes else {
        return false;
    };
    let year = u16::from_le_bytes([year_low, year_high]) as i64;
    if !(1970..=2200).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return false;
    }
    let days = days_since_1970(year, month as i64, day as i64);
    let seconds = days * SECONDS_PER_DAY + hour as i64 * 3600 + minute as i64 * 60 + second as i64;
    set(seconds * MILLISECONDS_PER_SECOND);
    true
}

/// Days from 1970-01-01 to a date, by Howard Hinnant's `days_from_civil`.
fn days_since_1970(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_from_march = (month + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

// ============================================================================
// Reading the build time
// ============================================================================

/// Reads whole decimal digits at compile time, since `str::parse` cannot run there.
const fn parse_or_zero(text: &str) -> Value {
    let bytes = text.as_bytes();
    let mut value: Value = 0;
    let mut index = 0;
    while index < bytes.len() {
        let digit = bytes[index];
        if !digit.is_ascii_digit() {
            return 0;
        }
        value = value * 10 + (digit - b'0') as Value;
        index += 1;
    }
    value
}
