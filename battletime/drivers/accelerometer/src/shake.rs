//! Finds shakes in acceleration readings taken about ten times a second.
//!
//! A jolt is a reading that differs from the one before by more than a threshold, summed over the
//! three axes. Three jolts within a second make a shake, and the next second is ignored, so one
//! shake of the wrist counts once.

use crate::Acceleration;

/// How far one reading must differ from the last, summed over the axes, to count as a jolt.
pub const JOLT_THRESHOLD_IN_MILLI_G: i32 = 1200;

pub const JOLTS_PER_SHAKE: u8 = 3;

/// The jolts of one shake all fall within this time.
pub const SHAKE_WINDOW_IN_MILLISECONDS: u64 = 1000;

/// After a shake, readings are ignored for this long.
pub const QUIET_AFTER_SHAKE_IN_MILLISECONDS: u64 = 1000;

/// Readings further apart than this are not compared, such as when reading starts again.
pub const LONGEST_READING_GAP_IN_MILLISECONDS: u64 = 500;

#[derive(Clone, Copy)]
struct TimedReading {
    reading: Acceleration,
    time_in_milliseconds: u64,
}

#[derive(Default)]
pub struct ShakeDetector {
    previous: Option<TimedReading>,
    first_jolt_at: u64,
    jolt_count: u8,
    quiet_until: u64,
}

impl ShakeDetector {
    pub fn new() -> ShakeDetector {
        ShakeDetector::default()
    }

    /// Takes the next reading and its time, and says whether it completes a shake.
    pub fn feed(&mut self, reading: Acceleration, time_in_milliseconds: u64) -> bool {
        let previous = self.previous.replace(TimedReading {
            reading,
            time_in_milliseconds,
        });
        let Some(previous) = previous else {
            return false;
        };
        let gap = time_in_milliseconds.saturating_sub(previous.time_in_milliseconds);
        if gap > LONGEST_READING_GAP_IN_MILLISECONDS || time_in_milliseconds < self.quiet_until {
            return false;
        }
        if change_in_milli_g(previous.reading, reading) <= JOLT_THRESHOLD_IN_MILLI_G {
            return false;
        }

        let window_has_passed = time_in_milliseconds - self.first_jolt_at > SHAKE_WINDOW_IN_MILLISECONDS;
        if self.jolt_count == 0 || window_has_passed {
            self.first_jolt_at = time_in_milliseconds;
            self.jolt_count = 0;
        }
        self.jolt_count += 1;
        if self.jolt_count < JOLTS_PER_SHAKE {
            return false;
        }
        self.jolt_count = 0;
        self.quiet_until = time_in_milliseconds + QUIET_AFTER_SHAKE_IN_MILLISECONDS;
        true
    }

    /// Forgets the readings so far, such as when reading stops.
    pub fn reset(&mut self) {
        *self = ShakeDetector::default();
    }
}

fn change_in_milli_g(before: Acceleration, after: Acceleration) -> i32 {
    let axis_change = |before: i16, after: i16| (after as i32 - before as i32).abs();
    axis_change(before.x_in_milli_g, after.x_in_milli_g)
        + axis_change(before.y_in_milli_g, after.y_in_milli_g)
        + axis_change(before.z_in_milli_g, after.z_in_milli_g)
}
