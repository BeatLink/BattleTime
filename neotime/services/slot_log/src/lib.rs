//! Day files of six-minute slots in the flash store: the step, heart rate and battery logs.
//!
//! Each log keeps a folder, such as `/logs/heart`, with a file for each day,
//! `<folder>/<year>/<month>-<day>`, holding one value for each six-minute slot from local midnight.
//! A slot is written only when it ends, so a file stops after the day's last written slot, and the
//! slots before it that were never written hold the log's empty value. Days older than
//! `DAYS_KEPT` are deleted when a new day starts.
//!
//! The heart rate log keeps one byte a slot, the average beats per minute measured in it, or 0.
//! The battery log keeps one byte a slot, the charge in percent at the slot's end with the top bit
//! set while charging, or `0xff` for a slot the watch was off.

#![no_std]

use core::fmt::Write;

use neotime_current_time::{date_from_days_since_1970, days_since_1970, Date};
use neotime_flash_store::{FileAccess, FileError};

#[cfg(test)]
mod tests;

// ============================================================================
// Slots and days
// ============================================================================

pub const MILLISECONDS_PER_SLOT: i64 = 6 * 60 * 1000;
pub const MILLISECONDS_PER_DAY: i64 = 86_400_000;
pub const SLOTS_PER_DAY: usize = 240;
/// How many days each log keeps.
pub const DAYS_KEPT: i64 = 90;

pub const HEART_RATE_DIRECTORY: &str = "/logs/heart";
pub const BATTERY_DIRECTORY: &str = "/logs/battery";
/// A heart rate slot with no reading in it.
pub const NO_HEART_RATE: u8 = 0;
/// A battery slot the watch was not running for.
pub const NO_BATTERY_READING: u8 = 0xff;
/// Set in a battery slot while the battery charges.
pub const CHARGING_BIT: u8 = 0x80;

/// One six-minute slot: the day, counted from 1970-01-01 in local time, and the slot from midnight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotTime {
    pub day: i64,
    pub slot: usize,
}

impl SlotTime {
    /// The slot holding a local time in milliseconds since 1970.
    pub fn at(local_time: i64) -> SlotTime {
        SlotTime {
            day: local_time.div_euclid(MILLISECONDS_PER_DAY),
            slot: (local_time.rem_euclid(MILLISECONDS_PER_DAY) / MILLISECONDS_PER_SLOT) as usize,
        }
    }
}

/// A slot that has just ended, and the day of the slot after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndedSlot {
    pub time: SlotTime,
    pub today: i64,
}

/// Follows the clock and says when the slot being recorded ends.
#[derive(Clone, Copy, Debug, Default)]
pub struct SlotClock {
    recording: Option<SlotTime>,
}

impl SlotClock {
    pub const fn new() -> SlotClock {
        SlotClock { recording: None }
    }

    /// The slot that ended since the last look, if the local time has moved into another one.
    pub fn look(&mut self, local_time: i64) -> Option<EndedSlot> {
        let now = SlotTime::at(local_time);
        let ended = self
            .recording
            .replace(now)
            .filter(|recording| *recording != now)?;
        Some(EndedSlot {
            time: ended,
            today: now.day,
        })
    }
}

/// The average of the values measured in a slot, kept as a sum and a count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Average {
    sum: u32,
    count: u32,
}

impl Average {
    pub const fn new() -> Average {
        Average { sum: 0, count: 0 }
    }

    pub fn add(&mut self, value: u32) {
        self.sum = self.sum.saturating_add(value);
        self.count += 1;
    }

    /// The rounded average, at most 255, or 0 when nothing was added; the average starts again empty.
    pub fn take(&mut self) -> u8 {
        let Average { sum, count } = core::mem::take(self);
        if count == 0 {
            return 0;
        }
        ((sum + count / 2) / count).min(u8::MAX as u32) as u8
    }
}

/// A battery slot's byte: the charge in percent, with the top bit set while charging.
pub fn battery_byte(percent: u8, is_charging: bool) -> u8 {
    percent.min(100) | if is_charging { CHARGING_BIT } else { 0 }
}

// ============================================================================
// Paths
// ============================================================================

/// Long enough for `/logs/battery/` and a five-digit year, with the month and day.
pub type DayPath = heapless::String<32>;

/// The file holding a day of the log in `directory`, such as `/logs/heart/2026/01-05`.
pub fn day_path(directory: &str, day: i64) -> DayPath {
    let date = date_from_days_since_1970(day);
    let mut path = DayPath::new();
    let _ = write!(
        path,
        "{directory}/{}/{:02}-{:02}",
        date.year, date.month, date.day
    );
    path
}

/// The day a path in `directory` holds, or nothing for any other path.
pub fn day_of_path(directory: &str, path: &str) -> Option<i64> {
    let rest = path.strip_prefix(directory)?.strip_prefix('/')?;
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
// Writing
// ============================================================================

/// Puts one byte into its slot of its day's file, filling any slots before it with `empty`.
///
/// A day is at most 240 bytes, so the file is read, changed and written whole, and the store keeps it as a record.
pub async fn write_byte_slot<Files: FileAccess>(
    files: &mut Files,
    directory: &str,
    time: SlotTime,
    value: u8,
    empty: u8,
) -> Result<(), FileError> {
    let path = day_path(directory, time.day);
    let mut bytes = [empty; SLOTS_PER_DAY];
    let length = files.read_file(&path, 0, &mut bytes).await?.unwrap_or(0);
    let slot = time.slot.min(SLOTS_PER_DAY - 1);
    bytes[length.min(SLOTS_PER_DAY)..].fill(empty);
    bytes[slot] = value;
    files.write_file(&path, &bytes[..length.max(slot + 1)]).await
}

/// Deletes the days in `directory` older than the log keeps, counting back from `today`.
pub async fn forget_old_days<Files: FileAccess>(
    files: &mut Files,
    directory: &str,
    today: i64,
) -> Result<(), FileError> {
    let mut old_paths: heapless::Vec<DayPath, 8> = heapless::Vec::new();
    loop {
        old_paths.clear();
        files
            .for_each_file(directory, &mut |path| {
                if day_of_path(directory, path).is_some_and(|day| day <= today - DAYS_KEPT) {
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

/// Writes an ended slot's heart rate, if it had one, and its battery byte, and forgets old days when a new day began.
pub async fn store_ended_slot<Files: FileAccess>(
    files: &mut Files,
    ended: EndedSlot,
    beats_per_minute: u8,
    battery: u8,
) -> Result<(), FileError> {
    let mut written = Ok(());
    if beats_per_minute != NO_HEART_RATE {
        written = write_byte_slot(
            files,
            HEART_RATE_DIRECTORY,
            ended.time,
            beats_per_minute,
            NO_HEART_RATE,
        )
        .await;
    }
    let battery_written =
        write_byte_slot(files, BATTERY_DIRECTORY, ended.time, battery, NO_BATTERY_READING).await;
    if ended.today != ended.time.day {
        let _ = forget_old_days(files, HEART_RATE_DIRECTORY, ended.today).await;
        let _ = forget_old_days(files, BATTERY_DIRECTORY, ended.today).await;
    }
    written.and(battery_written)
}
