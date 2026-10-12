//! The wall clock: the time a phone last set, counted on by the chip's timer.
//!
//! Until a phone sets it, it starts from when the firmware was built. Times are milliseconds since
//! 1970 with no time zone; the phone writes local time, so the offset from UTC is always 0 for now.
//! The time zone a phone writes is kept only to serve back to it.

use core::cell::Cell;

use embassy_sync::blocking_mutex::{raw::CriticalSectionRawMutex, Mutex};
use embassy_time::Instant;
use neotime_current_time::{
    decode_current_time, encode_current_time, LocalTimeInformation, CURRENT_TIME_BYTES,
    LOCAL_TIME_INFORMATION_BYTES,
};
use neotime_runtime::Value;

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
// The Bluetooth Current Time service
// ============================================================================

/// The time zone a phone last wrote, served back on reads; the clock already holds local time, so it never moves it.
static TIME_ZONE: Mutex<CriticalSectionRawMutex, Cell<LocalTimeInformation>> =
    Mutex::new(Cell::new(LocalTimeInformation::UTC));

/// The Current Time (0x2A2B) value for the clock's time now.
pub fn current_time() -> [u8; CURRENT_TIME_BYTES] {
    encode_current_time(now())
}

/// Sets the clock from a written Current Time value, unless it is not a real time.
pub fn set_from_current_time(bytes: &[u8]) -> bool {
    let Some(time) = decode_current_time(bytes) else {
        return false;
    };
    set(time);
    true
}

/// The Local Time Information (0x2A0F) value: the zone a phone last wrote, or UTC.
pub fn local_time_information() -> [u8; LOCAL_TIME_INFORMATION_BYTES] {
    TIME_ZONE.lock(Cell::get).encode()
}

/// Keeps a written Local Time Information value, unless it is outside what the field allows.
pub fn set_local_time_information(bytes: &[u8]) -> bool {
    let Some(time_zone) = LocalTimeInformation::decode(bytes) else {
        return false;
    };
    TIME_ZONE.lock(|stored| stored.set(time_zone));
    true
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
