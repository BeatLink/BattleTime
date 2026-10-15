//! Days of six-minute slots in the flash store: the step, heart rate and battery logs.
//!
//! Each log keeps its days in blocks, the store's blank files of one sector, `/day_blocks/<log>/<block>`,
//! block `n` holding the days from `n` times the days a block holds, counted from 1970-01-01 in
//! local time: 8 days of steps, or 16 of heart rate or battery. A day is one value for each
//! six-minute slot from local midnight. A slot is programmed into its place when it ends, without an
//! erase, so a block costs one erase in all. Each byte is stored flipped by the log's mask, so a slot
//! never written reads as the log's empty value, and a slot keeps the first value written to it. A
//! block is deleted once all its days are older than `DAYS_KEPT`.
//!
//! Older firmware kept each day as a file of its own, `<folder>/<year>/<month>-<day>`, such as
//! `/logs/heart/2026/01-05`, in the same form unflipped and stopping after its last written slot.
//! Those files stay, and are read beside the blocks, until they are older than `DAYS_KEPT`.
//!
//! The heart rate log keeps one byte a slot, the average beats per minute measured in it, or 0.
//! The battery log keeps one byte a slot, the charge in percent at the slot's end with the top bit
//! set while charging, or `0xff` for a slot the watch was off.

#![no_std]

use core::fmt::Write;

use neotime_current_time::{date_from_days_since_1970, days_since_1970, Date};
use neotime_flash_store::{FileAccess, FileError, BLANK_FILE_BYTES};

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

pub const STEPS_DIRECTORY: &str = "/steps";
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
// Logs and their blocks
// ============================================================================

/// One log: the folder of the day files older firmware kept, the folder of its blocks, and how it keeps a slot.
#[derive(Clone, Copy, Debug)]
pub struct SlotLog {
    pub directory: &'static str,
    pub block_directory: &'static str,
    pub bytes_per_slot: usize,
    /// Each byte is stored flipped by this mask, so an erased slot reads as the log's empty value.
    pub flip: u8,
}

pub const STEPS: SlotLog = SlotLog {
    directory: STEPS_DIRECTORY,
    block_directory: "/day_blocks/steps",
    bytes_per_slot: 2,
    flip: 0xff,
};
pub const HEART_RATE: SlotLog = SlotLog {
    directory: HEART_RATE_DIRECTORY,
    block_directory: "/day_blocks/heart",
    bytes_per_slot: 1,
    flip: !NO_HEART_RATE,
};
pub const BATTERY: SlotLog = SlotLog {
    directory: BATTERY_DIRECTORY,
    block_directory: "/day_blocks/battery",
    bytes_per_slot: 1,
    flip: !NO_BATTERY_READING,
};

impl SlotLog {
    pub const fn day_bytes(&self) -> usize {
        SLOTS_PER_DAY * self.bytes_per_slot
    }

    pub const fn days_per_block(&self) -> i64 {
        (BLANK_FILE_BYTES / self.day_bytes()) as i64
    }

    /// The block holding a day, such as `/day_blocks/heart/1273`, and where the day starts in it.
    pub fn place_of_day(&self, day: i64) -> (DayPath, usize) {
        let mut path = DayPath::new();
        let _ = write!(
            path,
            "{}/{}",
            self.block_directory,
            day.div_euclid(self.days_per_block())
        );
        let start = day.rem_euclid(self.days_per_block()) as usize * self.day_bytes();
        (path, start)
    }

    /// The block a path names, or nothing for any other path.
    pub fn block_of_path(&self, path: &str) -> Option<i64> {
        path.strip_prefix(self.block_directory)?
            .strip_prefix('/')?
            .parse()
            .ok()
    }

    /// The newest day a day file or block of this log holds, or nothing for any other path.
    pub fn newest_day_of_path(&self, path: &str) -> Option<i64> {
        match self.block_of_path(path) {
            Some(block) => Some((block + 1) * self.days_per_block() - 1),
            None => day_of_path(self.directory, path),
        }
    }
}

// ============================================================================
// Paths
// ============================================================================

/// Long enough for `/logs/battery/` and a five-digit year, with the month and day, or a block's path.
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
// Writing and reading
// ============================================================================

/// Programs a slot's value into its day's block, which the store makes when it is missing; a slot already written
/// keeps its value.
pub async fn write_slot<Files: FileAccess>(
    files: &mut Files,
    log: &SlotLog,
    time: SlotTime,
    value: u16,
) -> Result<(), FileError> {
    let (block, day_start) = log.place_of_day(time.day);
    let at = day_start + time.slot.min(SLOTS_PER_DAY - 1) * log.bytes_per_slot;
    let mut stored = value.to_le_bytes();
    let stored = &mut stored[..log.bytes_per_slot];
    flip(stored, log.flip);
    files.program_file(&block, at, stored).await
}

/// Reads a day from its block into the start of `buffer`, every slot of it, unwritten ones as the empty value;
/// gives nothing when the day's block is missing.
pub async fn read_block_day<Files: FileAccess>(
    files: &mut Files,
    log: &SlotLog,
    day: i64,
    buffer: &mut [u8],
) -> Result<Option<usize>, FileError> {
    let (block, day_start) = log.place_of_day(day);
    let length = log.day_bytes().min(buffer.len());
    let read = files.read_file(&block, day_start, &mut buffer[..length]).await?;
    flip(&mut buffer[..length], log.flip);
    Ok(read)
}

fn flip(bytes: &mut [u8], mask: u8) {
    for byte in bytes {
        *byte ^= mask;
    }
}

// ============================================================================
// Forgetting
// ============================================================================

/// Deletes a log's days older than it keeps, counting back from `today`: day files older firmware kept, and blocks
/// all of whose days are that old.
pub async fn forget_old_days<Files: FileAccess>(
    files: &mut Files,
    log: &SlotLog,
    today: i64,
) -> Result<(), FileError> {
    let mut old_paths: heapless::Vec<DayPath, 8> = heapless::Vec::new();
    for directory in [log.directory, log.block_directory] {
        loop {
            old_paths.clear();
            files
                .for_each_file(directory, &mut |path| {
                    if log
                        .newest_day_of_path(path)
                        .is_some_and(|day| day <= today - DAYS_KEPT)
                    {
                        let mut kept = DayPath::new();
                        if kept.push_str(path).is_ok() {
                            let _ = old_paths.push(kept);
                        }
                    }
                })
                .await?;
            if old_paths.is_empty() {
                break;
            }
            for path in &old_paths {
                files.delete_file(path).await?;
            }
        }
    }
    Ok(())
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
        written = write_slot(files, &HEART_RATE, ended.time, beats_per_minute as u16).await;
    }
    let battery_written = write_slot(files, &BATTERY, ended.time, battery as u16).await;
    if ended.today != ended.time.day {
        let _ = forget_old_days(files, &HEART_RATE, ended.today).await;
        let _ = forget_old_days(files, &BATTERY, ended.today).await;
    }
    written.and(battery_written)
}
