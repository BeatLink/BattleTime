//! The step log on the file system, in wasp-os's step logger format.
//!
//! Each day has a file, `/steps/<year>/<month>-<day>`, holding the steps of each six-minute slot
//! from midnight as a little-endian `u16`. A slot is added when it ends with steps in it, so a file
//! stops after the day's last slot with steps, and a slot past its end had none. Nothing is ever
//! deleted. The trip count is kept apart, in `/steps/trip`, as a little-endian `u32`.

use core::fmt::Write;

use littlefs_rust::{Error, Filesystem, OpenFlags, SeekFrom, Storage};
use neotime_current_time::date_from_days_since_1970;

use crate::{
    DayChange, FinishedSlot, Recorded, StepHistory, DAYS_REMEMBERED, HOURS_PER_DAY, SLOTS_PER_DAY,
    SLOTS_PER_HOUR,
};

pub const STEPS_DIRECTORY: &str = "/steps";
pub const TRIP_PATH: &str = "/steps/trip";
pub const BYTES_PER_SLOT: usize = 2;
pub const LARGEST_DAY_FILE_BYTES: usize = SLOTS_PER_DAY * BYTES_PER_SLOT;
const TRIP_FILE_BYTES: usize = 4;

/// Long enough for `/steps/` and a five-digit year, with the month and day.
pub type DayPath = heapless::String<24>;

// ============================================================================
// Paths
// ============================================================================

fn year_directory(year: i64) -> DayPath {
    let mut path = DayPath::new();
    let _ = write!(path, "{STEPS_DIRECTORY}/{year}");
    path
}

/// The file holding a day, counted from 1970-01-01, such as `/steps/2026/01-05`.
pub fn day_path(day: i64) -> DayPath {
    let date = date_from_days_since_1970(day);
    let mut path = year_directory(date.year);
    let _ = write!(path, "/{:02}-{:02}", date.month, date.day);
    path
}

// ============================================================================
// Reading a day's file
// ============================================================================

/// The steps of one slot, from the bytes of a day's file.
pub fn slot_steps(bytes: &[u8], slot: usize) -> u16 {
    let start = slot * BYTES_PER_SLOT;
    match bytes.get(start..start + BYTES_PER_SLOT) {
        Some(&[low, high]) => u16::from_le_bytes([low, high]),
        _ => 0,
    }
}

/// The steps in each hour, from the bytes of a day's file.
pub fn hours_from_file(bytes: &[u8]) -> [u16; HOURS_PER_DAY] {
    let mut hours = [0u16; HOURS_PER_DAY];
    for (slot, steps) in (0..SLOTS_PER_DAY).map(|slot| (slot, slot_steps(bytes, slot))) {
        let hour = &mut hours[slot / SLOTS_PER_HOUR];
        *hour = hour.saturating_add(steps);
    }
    hours
}

/// The steps in each hour of a day; a day with no file had none.
pub fn read_hours<S: Storage>(files: &Filesystem<S>, day: i64) -> [u16; HOURS_PER_DAY] {
    files
        .read_to_vec(&day_path(day))
        .map_or([0; HOURS_PER_DAY], |bytes| hours_from_file(&bytes))
}

// ============================================================================
// Writing a slot
// ============================================================================

fn make_directory<S: Storage>(files: &Filesystem<S>, path: &str) -> Result<(), Error> {
    match files.mkdir(path) {
        Err(Error::Exists) => Ok(()),
        result => result,
    }
}

/// Adds a finished slot's steps to its day's file, padding any slots before it with zeros.
pub fn write_slot<S: Storage>(files: &Filesystem<S>, finished: FinishedSlot) -> Result<(), Error> {
    let date = date_from_days_since_1970(finished.time.day);
    make_directory(files, STEPS_DIRECTORY)?;
    make_directory(files, &year_directory(date.year))?;
    let file = files.open(
        &day_path(finished.time.day),
        OpenFlags::READ | OpenFlags::WRITE | OpenFlags::CREATE,
    )?;
    let offset = (finished.time.slot * BYTES_PER_SLOT) as u32;
    let mut slot_bytes = [0u8; BYTES_PER_SLOT];
    if offset < file.size() {
        file.seek(SeekFrom::Start(offset))?;
        file.read(&mut slot_bytes)?;
    } else {
        file.seek(SeekFrom::End(0))?;
        let zeros = [0u8; BYTES_PER_SLOT];
        while file.tell() < offset {
            file.write(&zeros)?;
        }
    }
    let steps = u16::from_le_bytes(slot_bytes).saturating_add(finished.steps.min(u16::MAX as u32) as u16);
    file.seek(SeekFrom::Start(offset))?;
    file.write(&steps.to_le_bytes())?;
    file.close()
}

// ============================================================================
// The trip count
// ============================================================================

pub fn save_trip<S: Storage>(files: &Filesystem<S>, steps: u32) -> Result<(), Error> {
    make_directory(files, STEPS_DIRECTORY)?;
    files.write_file(TRIP_PATH, &steps.to_le_bytes())
}

/// The saved trip count, or `None` when there is none or it is not four bytes.
pub fn load_trip<S: Storage>(files: &Filesystem<S>) -> Option<u32> {
    let bytes: [u8; TRIP_FILE_BYTES] = files.read_to_vec(TRIP_PATH).ok()?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

// ============================================================================
// Keeping the history and the log together
// ============================================================================

/// Fills the remembered days from the log, counting back from the day being recorded.
pub fn load_days<S: Storage>(files: &Filesystem<S>, history: &mut StepHistory) {
    let today = history.recording().day;
    for days_back in 0..DAYS_REMEMBERED {
        history.set_day(days_back, read_hours(files, today - days_back as i64));
    }
}

/// Fills the history from the log at start-up: the remembered days and the trip count.
pub fn load_history<S: Storage>(files: &Filesystem<S>, history: &mut StepHistory) {
    load_days(files, history);
    if let Some(steps) = load_trip(files) {
        history.set_trip_steps(steps);
    }
}

/// Writes the slot a reading ended, with the trip count, and reads the days again after the clock jumped.
pub fn store<S: Storage>(
    files: &Filesystem<S>,
    history: &mut StepHistory,
    recorded: Recorded,
) -> Result<(), Error> {
    let written = match recorded.finished {
        Some(finished) => write_slot(files, finished).and_then(|()| save_trip(files, history.trip_steps())),
        None => Ok(()),
    };
    if recorded.day_change == DayChange::Jump {
        load_days(files, history);
    }
    written
}
