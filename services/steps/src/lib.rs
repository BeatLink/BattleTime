//! The step history: steps in each hour of today and the six days before, a trip count, and a
//! log of every day in six-minute slots on the file system.
//!
//! The firmware reads the accelerometer's step counter every 30 seconds and hands each reading to
//! `StepHistory::record` with the local time. The counter only counts up, except when the chip
//! starts again from 0, so each reading adds what the counter gained, or all of it after a restart.
//! The steps go to the six-minute slot in progress, its hour and the trip count. When a slot ends,
//! `log::store` adds it to that day's file, so the history outlives a restart of the watch. Local
//! midnight starts a new day by itself; nothing resets the chip and no app needs to be open.

#![no_std]

extern crate alloc;

pub mod log;

#[cfg(test)]
mod tests;

// ============================================================================
// Slots and days
// ============================================================================

pub const MILLISECONDS_PER_SLOT: i64 = 6 * 60 * 1000;
pub const MILLISECONDS_PER_DAY: i64 = 86_400_000;
pub const SLOTS_PER_DAY: usize = 240;
pub const HOURS_PER_DAY: usize = 24;
pub const SLOTS_PER_HOUR: usize = SLOTS_PER_DAY / HOURS_PER_DAY;
/// Today and the six days before it, which apps can read.
pub const DAYS_REMEMBERED: usize = 7;

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

    pub fn hour(self) -> usize {
        self.slot / SLOTS_PER_HOUR
    }
}

/// A slot that has ended with steps in it, for the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FinishedSlot {
    pub time: SlotTime,
    pub steps: u32,
}

/// How the day moved since the last reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayChange {
    SameDay,
    /// Midnight passed, so each remembered day moved one back.
    NextDay,
    /// The clock was set to another day, so the remembered days must be read again from the log.
    Jump,
}

/// What one reading did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Recorded {
    /// The slot that ended, when it had steps.
    pub finished: Option<FinishedSlot>,
    pub day_change: DayChange,
    /// Whether anything an app reads changed.
    pub changed: bool,
}

// ============================================================================
// The history
// ============================================================================

#[derive(Clone)]
pub struct StepHistory {
    recording: SlotTime,
    steps_in_slot: u32,
    last_count: u32,
    trip_steps: u32,
    /// Steps in each hour, today first, then each day before it.
    hours: [[u16; HOURS_PER_DAY]; DAYS_REMEMBERED],
}

impl Default for StepHistory {
    fn default() -> Self {
        StepHistory::new()
    }
}

impl StepHistory {
    pub const fn new() -> StepHistory {
        StepHistory {
            recording: SlotTime { day: 0, slot: 0 },
            steps_in_slot: 0,
            last_count: 0,
            trip_steps: 0,
            hours: [[0; HOURS_PER_DAY]; DAYS_REMEMBERED],
        }
    }

    /// Starts recording at a local time, with every day empty, for a counter that starts from 0.
    pub fn start(&mut self, local_time: i64) {
        *self = StepHistory {
            recording: SlotTime::at(local_time),
            trip_steps: self.trip_steps,
            ..StepHistory::new()
        };
    }

    /// Adds what the counter gained since the last reading, and ends the slot when its time is over.
    pub fn record(&mut self, local_time: i64, count: u32) -> Recorded {
        let gained = count.checked_sub(self.last_count).unwrap_or(count);
        self.last_count = count;
        if gained > 0 {
            self.steps_in_slot = self.steps_in_slot.saturating_add(gained);
            self.trip_steps = self.trip_steps.saturating_add(gained);
            let hour = &mut self.hours[0][self.recording.hour()];
            *hour = hour.saturating_add(gained.min(u16::MAX as u32) as u16);
        }

        let now = SlotTime::at(local_time);
        let mut recorded = Recorded {
            finished: None,
            day_change: DayChange::SameDay,
            changed: gained > 0,
        };
        if now == self.recording {
            return recorded;
        }
        let steps = core::mem::take(&mut self.steps_in_slot);
        recorded.finished = (steps > 0).then_some(FinishedSlot {
            time: self.recording,
            steps,
        });
        if now.day != self.recording.day {
            recorded.day_change = self.move_to_day(now.day);
            recorded.changed = true;
        }
        self.recording = now;
        recorded
    }

    fn move_to_day(&mut self, day: i64) -> DayChange {
        if day == self.recording.day + 1 {
            self.hours.copy_within(0..DAYS_REMEMBERED - 1, 1);
            self.hours[0] = [0; HOURS_PER_DAY];
            DayChange::NextDay
        } else {
            self.hours = [[0; HOURS_PER_DAY]; DAYS_REMEMBERED];
            DayChange::Jump
        }
    }

    /// The slot steps are being counted in now.
    pub fn recording(&self) -> SlotTime {
        self.recording
    }

    /// Steps in one hour of a remembered day, counting days back from today; 0 outside them.
    pub fn steps_in_hour(&self, days_back: usize, hour: usize) -> u32 {
        self.hours
            .get(days_back)
            .and_then(|hours| hours.get(hour))
            .map_or(0, |&steps| steps as u32)
    }

    pub fn steps_on_day(&self, days_back: usize) -> u32 {
        self.hours
            .get(days_back)
            .map_or(0, |hours| hours.iter().map(|&steps| steps as u32).sum())
    }

    pub fn steps_today(&self) -> u32 {
        self.steps_on_day(0)
    }

    /// Steps since the trip count was last reset, whatever the day.
    pub fn trip_steps(&self) -> u32 {
        self.trip_steps
    }

    pub fn set_trip_steps(&mut self, steps: u32) {
        self.trip_steps = steps;
    }

    /// Fills a remembered day with the hourly steps the log holds for it.
    pub fn set_day(&mut self, days_back: usize, hours: [u16; HOURS_PER_DAY]) {
        if let Some(day) = self.hours.get_mut(days_back) {
            *day = hours;
        }
    }
}
