//! The wall clock: UTC counted on by the chip's timer, with the zone it is shown in.
//!
//! Twice a second the clock is kept in RAM a reset leaves as it was, so after a soft or watchdog
//! reset it counts on from where it was; after a power cut, or the first start of a new build, it
//! starts from when the firmware was built, in UTC, until a phone or the wearer sets it. The time zone and
//! daylight saving offset are also saved to `/settings/time_zone`, so local time survives a restart
//! once the clock is set again. `neotime-current-time` holds the logic; docs/companion-protocol.md
//! says how each way of setting the clock treats UTC and the zone.

use core::{
    cell::Cell,
    mem::MaybeUninit,
    ptr::{addr_of, addr_of_mut},
    sync::atomic::{AtomicBool, Ordering},
};

use embassy_sync::blocking_mutex::{raw::CriticalSectionRawMutex, Mutex};
use embassy_time::Instant;
use neotime_current_time::{
    parse_time_line, LocalTimeInformation, WallClock, CURRENT_TIME_BYTES, KEPT_CLOCK_WORDS,
    LOCAL_TIME_INFORMATION_BYTES,
};
use neotime_runtime::{SystemValue, Value};

use crate::{
    storage,
    storage_requests::{self, StorageRequest},
    system_values,
};

/// When the firmware was built, in UTC: where the clock starts with nothing kept, and the earliest it takes.
const BUILD_TIME: Value = parse_or_zero(env!("NEOTIME_BUILD_TIME_MS"));

static CLOCK: Mutex<CriticalSectionRawMutex, Cell<WallClock>> =
    Mutex::new(Cell::new(WallClock::starting_at(BUILD_TIME)));

/// Whether the zone changed since it was last saved or loaded.
static ZONE_UNSAVED: AtomicBool = AtomicBool::new(false);

const TIME_ZONE_PATH: &str = "/settings/time_zone";

fn uptime() -> Value {
    Instant::now().as_millis() as Value
}

/// Runs a change on the clock, then tells the shell the time moved and notes a changed zone for saving.
fn change(apply: impl FnOnce(&mut WallClock) -> bool) -> bool {
    let (before, after, applied) = CLOCK.lock(|cell| {
        let before = cell.get();
        let mut after = before;
        let applied = apply(&mut after);
        cell.set(after);
        (before, after, applied)
    });
    note_change(before.zone() != after.zone(), applied);
    applied
}

/// Kept apart from `change`, so each kind of change does not carry its own copy.
#[inline(never)]
fn note_change(zone_changed: bool, applied: bool) {
    if zone_changed {
        ZONE_UNSAVED.store(true, Ordering::Relaxed);
    }
    if applied {
        system_values::changed(&[SystemValue::Now, SystemValue::UtcOffset, SystemValue::StartedAt]);
    }
}

// ============================================================================
// Reading the clock
// ============================================================================

/// Local time: milliseconds since 1970 as the wearer's clock reads, for midnight, chimes and drawing.
pub fn now() -> Value {
    CLOCK.lock(Cell::get).local(uptime())
}

/// Milliseconds since 1970 in UTC, the instant apps see as `now`.
pub fn utc_now() -> Value {
    CLOCK.lock(Cell::get).utc(uptime())
}

/// When the watch last started, in UTC, which moves with the clock when a phone sets it.
pub fn started_at() -> Value {
    CLOCK.lock(Cell::get).utc(0)
}

/// How far local time is ahead of UTC, the value apps see as `utc_offset`.
pub fn utc_offset() -> Value {
    CLOCK.lock(Cell::get).utc_offset_in_milliseconds()
}

/// The chip's timer reading at a UTC time, for sleeping until then.
pub fn instant_at(utc: Value) -> Instant {
    Instant::from_millis(CLOCK.lock(Cell::get).uptime_at(utc) as u64)
}

// ============================================================================
// Keeping the clock across a reset
// ============================================================================

/// The clock as last kept, in `.uninit` beside the crash record, which start-up leaves as it was.
#[link_section = ".uninit.KEPT_CLOCK"]
static mut KEPT_CLOCK: MaybeUninit<[u32; KEPT_CLOCK_WORDS]> = MaybeUninit::uninit();

/// Writes the clock into RAM a reset keeps; the watchdog task calls it twice a second.
pub fn keep() {
    let words = CLOCK.lock(Cell::get).kept_words(uptime());
    unsafe {
        addr_of_mut!(KEPT_CLOCK)
            .cast::<[u32; KEPT_CLOCK_WORDS]>()
            .write_volatile(words)
    };
}

/// Takes the clock the previous run kept, if it left a whole one, and keeps it again at once.
pub fn restore_kept() {
    let words = unsafe {
        addr_of!(KEPT_CLOCK)
            .cast::<[u32; KEPT_CLOCK_WORDS]>()
            .read_volatile()
    };
    if let Some(kept) = WallClock::from_kept_words(words, uptime(), BUILD_TIME) {
        change(|clock| {
            *clock = kept;
            true
        });
        // The saved zone file still loads over it, so a start never writes the zone back unasked.
        ZONE_UNSAVED.store(false, Ordering::Relaxed);
    }
    keep();
}

// ============================================================================
// Saving the zone
// ============================================================================

/// Reads the saved zone at start-up, unless a phone already wrote one; a missing or bad file leaves UTC.
pub fn load_time_zone(files: &storage::Files) {
    let Some(zone) = files
        .read_now(TIME_ZONE_PATH)
        .and_then(|bytes| LocalTimeInformation::decode(&bytes))
    else {
        return;
    };
    if !ZONE_UNSAVED.load(Ordering::Relaxed) {
        change(|clock| {
            clock.set_zone_keeping_utc(zone);
            true
        });
        ZONE_UNSAVED.store(false, Ordering::Relaxed);
    }
}

/// Saves the zone if it changed since it was last saved.
pub async fn save_time_zone_if_changed() {
    if ZONE_UNSAVED.swap(false, Ordering::Relaxed) {
        let mut bytes = heapless::Vec::new();
        let _ = bytes.extend_from_slice(&CLOCK.lock(Cell::get).zone().encode());
        let request = StorageRequest::WriteSmallFile {
            path: TIME_ZONE_PATH,
            bytes,
        };
        storage_requests::REQUESTS.send(request).await;
    }
}

// ============================================================================
// Setting the clock
// ============================================================================

/// The Current Time (0x2A2B) value: the local time now.
pub fn current_time() -> [u8; CURRENT_TIME_BYTES] {
    CLOCK.lock(Cell::get).current_time(uptime())
}

/// Sets the local time from a written Current Time value, unless it is not a real time.
pub fn set_from_current_time(bytes: &[u8]) -> bool {
    change(|clock| clock.write_current_time(bytes, uptime()))
}

/// The Local Time Information (0x2A0F) value: the zone the clock is shown in.
pub fn local_time_information() -> [u8; LOCAL_TIME_INFORMATION_BYTES] {
    CLOCK.lock(Cell::get).zone().encode()
}

/// Takes the zone from a written Local Time Information value, keeping the local time.
pub fn set_local_time_information(bytes: &[u8]) -> bool {
    change(|clock| clock.write_local_time_information(bytes))
}

/// Sets the local time the wearer chose in the Settings app, as a day of a year and a minute of it, keeping the zone.
pub fn set_by_hand(year: Value, day_of_year: Value, minute_of_day: Value) -> bool {
    change(|clock| clock.set_by_hand(year, day_of_year, minute_of_day, uptime()))
}

/// Applies a Gadgetbridge `setTime(...)` / `E.setTimeZone(...)` line; false for any other line.
pub fn read_time_line(line: &[u8]) -> bool {
    let Some(time_line) = parse_time_line(line) else {
        return false;
    };
    change(|clock| {
        clock.apply_time_line(time_line, uptime());
        true
    });
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
