//! The step log in the flash store, in wasp-os's step logger format.
//!
//! Each day holds the steps of each six-minute slot from midnight as a little-endian `u16`, written when
//! the slot ends with steps in it. The days are kept in `battletime_slot_log`'s blocks, which the heart rate
//! and battery logs share; a day older firmware kept as a file of its own, `/steps/<year>/<month>-<day>`,
//! which stops after its last slot with steps, is read beside its block. Old days are deleted when a new
//! day starts. The trip count is kept apart, in `/steps/trip`, as a little-endian `u32`; it lives in
//! RAM and is written when a day ends and, through `save_trip_if_changed`, when the screen goes dark.

use battletime_flash_store::{FileAccess, FileError};

use crate::{
    DayChange, FinishedSlot, Recorded, StepHistory, DAYS_REMEMBERED, HOURS_PER_DAY, SLOTS_PER_DAY,
    SLOTS_PER_HOUR,
};

pub const STEPS_DIRECTORY: &str = battletime_slot_log::STEPS_DIRECTORY;
/// How many days the log keeps at least; a block of 8 days goes once all of them are older.
pub const DAYS_KEPT: i64 = battletime_slot_log::DAYS_KEPT;
pub const TRIP_PATH: &str = "/steps/trip";
pub const BYTES_PER_SLOT: usize = 2;
pub const LARGEST_DAY_FILE_BYTES: usize = SLOTS_PER_DAY * BYTES_PER_SLOT;
const TRIP_FILE_BYTES: usize = 4;

pub use battletime_slot_log::DayPath;
use battletime_slot_log::STEPS;

// ============================================================================
// Paths
// ============================================================================

/// The file holding a day, counted from 1970-01-01, such as `/steps/2026/01-05`.
pub fn day_path(day: i64) -> DayPath {
    battletime_slot_log::day_path(STEPS_DIRECTORY, day)
}

/// The day a path such as `/steps/2026/01-05` holds, or nothing for any other path.
pub fn day_of_path(path: &str) -> Option<i64> {
    battletime_slot_log::day_of_path(STEPS_DIRECTORY, path)
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

/// The steps in each hour of a day, from its block and from the file older firmware kept for it; a day with
/// neither had none.
pub async fn read_hours<Files: FileAccess>(files: &mut Files, day: i64) -> [u16; HOURS_PER_DAY] {
    let mut hours = [0u16; HOURS_PER_DAY];
    for in_own_file in [false, true] {
        let mut bytes = [0u8; LARGEST_DAY_FILE_BYTES];
        let read = match in_own_file {
            false => battletime_slot_log::read_block_day(files, &STEPS, day, &mut bytes).await,
            true => files.read_file(&day_path(day), 0, &mut bytes).await,
        };
        let length = read.ok().flatten().unwrap_or(0);
        for (hour, steps) in hours.iter_mut().zip(hours_from_file(&bytes[..length])) {
            *hour = hour.saturating_add(steps);
        }
    }
    hours
}

// ============================================================================
// Writing a slot
// ============================================================================

/// Writes a finished slot's steps into its day's block; a slot already written keeps its count.
pub async fn write_slot<Files: FileAccess>(
    files: &mut Files,
    finished: FinishedSlot,
) -> Result<(), FileError> {
    let steps = finished.steps.min(u16::MAX as u32) as u16;
    let time = battletime_slot_log::SlotTime {
        day: finished.time.day,
        slot: finished.time.slot,
    };
    battletime_slot_log::write_slot(files, &STEPS, time, steps).await
}

/// Deletes the days older than the log keeps, counting back from `today`.
pub async fn forget_old_days<Files: FileAccess>(files: &mut Files, today: i64) -> Result<(), FileError> {
    battletime_slot_log::forget_old_days(files, &STEPS, today).await
}

// ============================================================================
// The trip count
// ============================================================================

pub async fn save_trip<Files: FileAccess>(files: &mut Files, steps: u32) -> Result<(), FileError> {
    files.write_file(TRIP_PATH, &steps.to_le_bytes()).await
}

/// Writes the trip count only when it differs from the saved one, so an idle moment costs a read and no write.
pub async fn save_trip_if_changed<Files: FileAccess>(files: &mut Files, steps: u32) -> Result<(), FileError> {
    if load_trip(files).await == Some(steps) {
        return Ok(());
    }
    save_trip(files, steps).await
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

/// Writes the slot a reading ended, the trip count when a day ended, and reads the days again after the clock jumped.
pub async fn store<Files: FileAccess>(
    files: &mut Files,
    history: &mut StepHistory,
    recorded: Recorded,
) -> Result<(), FileError> {
    let written = match recorded.finished {
        Some(finished) => write_slot(files, finished).await,
        None => Ok(()),
    };
    if recorded.day_change != DayChange::SameDay {
        let _ = save_trip_if_changed(files, history.trip_steps()).await;
        let _ = forget_old_days(files, history.recording().day).await;
    }
    if recorded.day_change == DayChange::Jump {
        load_days(files, history).await;
    }
    written
}
