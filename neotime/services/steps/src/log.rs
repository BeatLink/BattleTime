//! The step log in the flash store, in wasp-os's step logger format.
//!
//! Each day has a file, `/steps/<year>/<month>-<day>`, holding the steps of each six-minute slot
//! from midnight as a little-endian `u16`. A slot is added when it ends with steps in it, so a file
//! stops after the day's last slot with steps, and a slot past its end had none. Nothing is ever
//! deleted. The trip count is kept apart, in `/steps/trip`, as a little-endian `u32`.

use core::fmt::Write;

use neotime_current_time::{date_from_days_since_1970, days_since_1970, Date};
use neotime_flash_store::{FileAccess, FileError};

use crate::{
    DayChange, FinishedSlot, Recorded, StepHistory, DAYS_REMEMBERED, HOURS_PER_DAY, SLOTS_PER_DAY,
    SLOTS_PER_HOUR,
};

pub const STEPS_DIRECTORY: &str = "/steps";
/// How many days the log keeps; each is a record of up to 480 bytes, so 90 take at most about 47 KB of the
/// store's records.
pub const DAYS_KEPT: i64 = 90;
pub const TRIP_PATH: &str = "/steps/trip";
pub const BYTES_PER_SLOT: usize = 2;
pub const LARGEST_DAY_FILE_BYTES: usize = SLOTS_PER_DAY * BYTES_PER_SLOT;
const TRIP_FILE_BYTES: usize = 4;

/// Long enough for `/steps/` and a five-digit year, with the month and day.
pub type DayPath = heapless::String<24>;

// ============================================================================
// Paths
// ============================================================================

/// The file holding a day, counted from 1970-01-01, such as `/steps/2026/01-05`.
pub fn day_path(day: i64) -> DayPath {
    let date = date_from_days_since_1970(day);
    let mut path = DayPath::new();
    let _ = write!(
        path,
        "{STEPS_DIRECTORY}/{}/{:02}-{:02}",
        date.year, date.month, date.day
    );
    path
}

/// The day a path such as `/steps/2026/01-05` holds, or nothing for any other path.
pub fn day_of_path(path: &str) -> Option<i64> {
    let rest = path.strip_prefix(STEPS_DIRECTORY)?.strip_prefix('/')?;
    let (year, month_day) = rest.split_once('/')?;
    let (month, day) = month_day.split_once('-')?;
    let date = Date {
        year: year.parse().ok()?,
        month: month.parse().ok()?,
        day: day.parse().ok()?,
    };
    Some(days_since_1970(date))
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

/// A day's file read into RAM, as long as it can grow.
type DayBytes = heapless::Vec<u8, LARGEST_DAY_FILE_BYTES>;

async fn read_day<Files: FileAccess>(files: &mut Files, day: i64) -> Result<DayBytes, FileError> {
    let mut bytes = DayBytes::new();
    let _ = bytes.resize(LARGEST_DAY_FILE_BYTES, 0);
    let length = files.read_file(&day_path(day), 0, &mut bytes).await?.unwrap_or(0);
    bytes.truncate(length);
    Ok(bytes)
}

/// The steps in each hour of a day; a day with no file had none.
pub async fn read_hours<Files: FileAccess>(files: &mut Files, day: i64) -> [u16; HOURS_PER_DAY] {
    read_day(files, day)
        .await
        .map_or([0; HOURS_PER_DAY], |bytes| hours_from_file(&bytes))
}

// ============================================================================
// Writing a slot
// ============================================================================

/// Adds a finished slot's steps to its day's file, padding any slots before it with zeros.
///
/// A day's file is at most 480 bytes, so it is read, changed and written whole, and the store keeps it as a record.
pub async fn write_slot<Files: FileAccess>(
    files: &mut Files,
    finished: FinishedSlot,
) -> Result<(), FileError> {
    let mut bytes = read_day(files, finished.time.day).await?;
    let offset = finished.time.slot * BYTES_PER_SLOT;
    if bytes.len() < offset + BYTES_PER_SLOT {
        let _ = bytes.resize(offset + BYTES_PER_SLOT, 0);
    }
    let added = finished.steps.min(u16::MAX as u32) as u16;
    let steps = slot_steps(&bytes, finished.time.slot).saturating_add(added);
    bytes[offset..offset + BYTES_PER_SLOT].copy_from_slice(&steps.to_le_bytes());
    files.write_file(&day_path(finished.time.day), &bytes).await
}

/// Deletes the days older than the log keeps, counting back from `today`.
pub async fn forget_old_days<Files: FileAccess>(files: &mut Files, today: i64) -> Result<(), FileError> {
    let mut old_paths: heapless::Vec<DayPath, 8> = heapless::Vec::new();
    loop {
        old_paths.clear();
        files
            .for_each_file(STEPS_DIRECTORY, &mut |path| {
                if day_of_path(path).is_some_and(|day| day <= today - DAYS_KEPT) {
                    let mut kept = DayPath::new();
                    if kept.push_str(path).is_ok() {
                        let _ = old_paths.push(kept);
                    }
                }
            })
            .await?;
        if old_paths.is_empty() {
            return Ok(());
        }
        for path in &old_paths {
            files.delete_file(path).await?;
        }
    }
}

// ============================================================================
// The trip count
// ============================================================================

pub async fn save_trip<Files: FileAccess>(files: &mut Files, steps: u32) -> Result<(), FileError> {
    files.write_file(TRIP_PATH, &steps.to_le_bytes()).await
}

/// The saved trip count, or `None` when there is none or it is not four bytes.
pub async fn load_trip<Files: FileAccess>(files: &mut Files) -> Option<u32> {
    let mut bytes = [0u8; TRIP_FILE_BYTES + 1];
    let length = files.read_file(TRIP_PATH, 0, &mut bytes).await.ok()??;
    let trip: [u8; TRIP_FILE_BYTES] = bytes[..length].try_into().ok()?;
    Some(u32::from_le_bytes(trip))
}

// ============================================================================
// Keeping the history and the log together
// ============================================================================

/// Fills the remembered days from the log, counting back from the day being recorded.
pub async fn load_days<Files: FileAccess>(files: &mut Files, history: &mut StepHistory) {
    let today = history.recording().day;
    for days_back in 0..DAYS_REMEMBERED {
        history.set_day(days_back, read_hours(files, today - days_back as i64).await);
    }
}

/// Fills the history from the log at start-up: the remembered days and the trip count.
pub async fn load_history<Files: FileAccess>(files: &mut Files, history: &mut StepHistory) {
    load_days(files, history).await;
    if let Some(steps) = load_trip(files).await {
        history.set_trip_steps(steps);
    }
}

/// Writes the slot a reading ended, with the trip count, and reads the days again after the clock jumped.
pub async fn store<Files: FileAccess>(
    files: &mut Files,
    history: &mut StepHistory,
    recorded: Recorded,
) -> Result<(), FileError> {
    let written = match recorded.finished {
        Some(finished) => match write_slot(files, finished).await {
            Ok(()) => save_trip(files, history.trip_steps()).await,
            Err(error) => Err(error),
        },
        None => Ok(()),
    };
    if recorded.day_change != DayChange::SameDay {
        let _ = forget_old_days(files, history.recording().day).await;
    }
    if recorded.day_change == DayChange::Jump {
        load_days(files, history).await;
    }
    written
}
