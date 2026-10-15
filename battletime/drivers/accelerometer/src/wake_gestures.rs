//! Raise to wake, lower to sleep and shake to wake, found in readings taken five or ten times a second.
//!
//! Ported from InfiniTime's `MotionController`: `ShouldRaiseWake`, `ShouldLowerSleep` and
//! `CurrentShakeSpeed`. Each reading is first turned into InfiniTime's units and axes, 1024 to a g
//! with the y axis reversed, so its thresholds and its shake speeds carry over unchanged. The
//! detector keeps the last eight readings, and judges the wrist's roll from the mean of the newest
//! two against the mean of the oldest two.

use crate::Acceleration;

// ============================================================================
// What is watched for
// ============================================================================

/// The gestures the detector reports, and how fast a shake must be to count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Watching {
    pub raise: bool,
    pub lower: bool,
    pub shake: bool,
    pub shake_threshold: i32,
}

impl Watching {
    pub const NOTHING: Watching = Watching {
        raise: false,
        lower: false,
        shake: false,
        shake_threshold: 0,
    };

    pub fn anything(&self) -> bool {
        self.raise || self.lower || self.shake
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeGesture {
    Raise,
    Shake,
    Lower,
}

// ============================================================================
// Limits, from InfiniTime
// ============================================================================

/// Readings kept, the newest first.
pub const HISTORY_LENGTH: usize = 8;

/// Readings in each of the two means the roll is judged between.
const AVERAGED_READINGS: usize = 2;

/// Readings further apart than this start the history afresh, such as when reading starts again.
pub const LONGEST_READING_GAP_IN_MILLISECONDS: u64 = 500;

/// After a gesture is reported, none is reported for this long, so one movement counts once.
pub const QUIET_AFTER_GESTURE_IN_MILLISECONDS: u64 = 1000;

/// The shake speed is counted per tenth of a second, the time between InfiniTime's readings.
const SPEED_PERIOD_IN_MILLISECONDS: i32 = 100;

/// One g in InfiniTime's units.
const ONE_G: i32 = 1024;

const STILL_VARIANCE: i32 = 56 * 56;
const RAISE_MOST_SIDEWAYS: i32 = 384;
const RAISE_HIGHEST_Y: i32 = -64;
const RAISE_LOWEST_Y_BEFORE_CHECKING_Z: i32 = -724;
const RAISE_ROLL_IN_DEGREES: i32 = -45;
const LOWER_SIDEWAYS_X: i32 = 887;
const LOWER_SIDEWAYS_ROLL_IN_DEGREES: i32 = 30;
const LOWER_LOWEST_Y: i32 = 724;
const LOWER_ROLL_IN_DEGREES: i32 = 30;
const LOWER_HELD_Y: i32 = 265;

// ============================================================================
// The detector
// ============================================================================

/// One reading in InfiniTime's units and axes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Sample {
    x: i32,
    y: i32,
    z: i32,
}

impl Sample {
    fn from_reading(reading: Acceleration) -> Sample {
        let to_infinitime_units = |milli_g: i16| milli_g as i32 * ONE_G / 1000;
        Sample {
            x: to_infinitime_units(reading.x_in_milli_g),
            y: -to_infinitime_units(reading.y_in_milli_g),
            z: to_infinitime_units(reading.z_in_milli_g),
        }
    }
}

/// The means of the newest and oldest readings, and how much the newest vary.
struct Statistics {
    mean: Sample,
    previous_mean: Sample,
    y_variance: i32,
    z_variance: i32,
}

#[derive(Default)]
pub struct WakeGestureDetector {
    history: [Sample; HISTORY_LENGTH],
    newest: usize,
    readings_kept: usize,
    last_reading_at: Option<u64>,
    accumulated_speed: i32,
    quiet_until: u64,
}

impl WakeGestureDetector {
    pub fn new() -> WakeGestureDetector {
        WakeGestureDetector::default()
    }

    /// Takes the next reading and its time, and gives a gesture it completes among those watched for.
    pub fn feed(
        &mut self,
        reading: Acceleration,
        time_in_milliseconds: u64,
        watching: Watching,
    ) -> Option<WakeGesture> {
        self.record(Sample::from_reading(reading), time_in_milliseconds);
        if time_in_milliseconds < self.quiet_until {
            return None;
        }
        let gesture = self.gesture_for(watching)?;
        self.quiet_until = time_in_milliseconds + QUIET_AFTER_GESTURE_IN_MILLISECONDS;
        Some(gesture)
    }

    /// How fast the wrist is shaking, as InfiniTime counts it for its shake threshold.
    pub fn shake_speed(&self) -> i32 {
        self.accumulated_speed
    }

    /// Forgets the readings so far, such as when reading stops.
    pub fn reset(&mut self) {
        *self = WakeGestureDetector::default();
    }

    fn record(&mut self, sample: Sample, time_in_milliseconds: u64) {
        let gap = self
            .last_reading_at
            .map(|last| time_in_milliseconds.saturating_sub(last));
        if gap.is_none_or(|gap| gap > LONGEST_READING_GAP_IN_MILLISECONDS) {
            self.readings_kept = 0;
            self.accumulated_speed = 0;
        }
        self.last_reading_at = Some(time_in_milliseconds);
        self.newest = (self.newest + 1) % HISTORY_LENGTH;
        self.history[self.newest] = sample;
        self.readings_kept = (self.readings_kept + 1).min(HISTORY_LENGTH);

        if let (Some(gap), true) = (gap, self.readings_kept >= 2) {
            let previous = self.back(1);
            let change = (sample.z - previous.z) + (sample.y - previous.y) / 2 + (sample.x - previous.x) / 4;
            let gap = (gap as i32).max(1);
            let speed = change.abs() * SPEED_PERIOD_IN_MILLISECONDS / gap;
            self.accumulated_speed = speed / 5 + self.accumulated_speed * 4 / 5;
        }
    }

    /// The reading this many before the newest.
    fn back(&self, readings_back: usize) -> Sample {
        self.history[(self.newest + HISTORY_LENGTH - readings_back) % HISTORY_LENGTH]
    }

    fn gesture_for(&self, watching: Watching) -> Option<WakeGesture> {
        if watching.shake && self.readings_kept >= 2 && self.accumulated_speed > watching.shake_threshold {
            return Some(WakeGesture::Shake);
        }
        if self.readings_kept < HISTORY_LENGTH {
            return None;
        }
        let statistics = self.statistics();
        if watching.raise && should_raise_wake(&statistics) {
            return Some(WakeGesture::Raise);
        }
        if watching.lower && self.should_lower_sleep(&statistics) {
            return Some(WakeGesture::Lower);
        }
        None
    }

    fn statistics(&self) -> Statistics {
        let mean_of = |readings_back: [usize; AVERAGED_READINGS]| {
            let mut sum = Sample::default();
            for back in readings_back {
                let sample = self.back(back);
                sum.x += sample.x;
                sum.y += sample.y;
                sum.z += sample.z;
            }
            let count = AVERAGED_READINGS as i32;
            Sample {
                x: sum.x / count,
                y: sum.y / count,
                z: sum.z / count,
            }
        };
        let mean = mean_of([0, 1]);
        let variance_of = |along: fn(Sample) -> i32| {
            let average = along(mean);
            [0, 1]
                .into_iter()
                .map(|back| (along(self.back(back)) - average).pow(2))
                .sum::<i32>()
                / AVERAGED_READINGS as i32
        };
        Statistics {
            mean,
            previous_mean: mean_of([HISTORY_LENGTH - 1, HISTORY_LENGTH - 2]),
            y_variance: variance_of(|sample| sample.y),
            z_variance: variance_of(|sample| sample.z),
        }
    }

    fn should_lower_sleep(&self, statistics: &Statistics) -> bool {
        let Statistics {
            mean, previous_mean, ..
        } = statistics;
        let sideways_roll = degrees_rolled(mean.x, mean.z, previous_mean.x, previous_mean.z);
        if (mean.x > LOWER_SIDEWAYS_X && sideways_roll > LOWER_SIDEWAYS_ROLL_IN_DEGREES)
            || (mean.x < -LOWER_SIDEWAYS_X && sideways_roll < -LOWER_SIDEWAYS_ROLL_IN_DEGREES)
        {
            return true;
        }
        if mean.y < LOWER_LOWEST_Y
            || degrees_rolled(mean.y, mean.z, previous_mean.y, previous_mean.z) < LOWER_ROLL_IN_DEGREES
        {
            return false;
        }
        (1..HISTORY_LENGTH - AVERAGED_READINGS).all(|back| self.back(back).y >= LOWER_HELD_Y)
    }
}

fn should_raise_wake(statistics: &Statistics) -> bool {
    let Statistics {
        mean,
        previous_mean,
        y_variance,
        z_variance,
    } = statistics;
    if mean.x.abs() > RAISE_MOST_SIDEWAYS {
        return false;
    }
    // A wrist held still enough reads only gravity, which the roll is worked out from.
    if *y_variance > STILL_VARIANCE
        || (mean.y < RAISE_LOWEST_Y_BEFORE_CHECKING_Z && *z_variance > STILL_VARIANCE)
        || mean.y > RAISE_HIGHEST_Y
    {
        return false;
    }
    degrees_rolled(mean.y, mean.z, previous_mean.y, previous_mean.z) < RAISE_ROLL_IN_DEGREES
}

// ============================================================================
// Angles
// ============================================================================

/// The sine of each whole degree from 0 to 90, scaled so that 1 is 32767, as LVGL's table that InfiniTime uses.
const SINE_OF_DEGREES: [u16; 91] = [
    0, 572, 1144, 1715, 2286, 2856, 3425, 3993, 4560, 5126, 5690, 6252, 6813, 7371, 7927, 8481, 9032, 9580,
    10126, 10668, 11207, 11743, 12275, 12803, 13328, 13848, 14364, 14876, 15383, 15886, 16383, 16876, 17364,
    17846, 18323, 18794, 19260, 19720, 20173, 20621, 21062, 21497, 21925, 22347, 22762, 23170, 23571, 23964,
    24351, 24730, 25101, 25465, 25821, 26169, 26509, 26841, 27165, 27481, 27788, 28087, 28377, 28659, 28932,
    29196, 29451, 29697, 29934, 30162, 30381, 30591, 30791, 30982, 31163, 31335, 31498, 31650, 31794, 31927,
    32051, 32165, 32269, 32364, 32448, 32523, 32587, 32642, 32687, 32722, 32747, 32762, 32767,
];

/// The angle in whole degrees whose sine is nearest `scaled_sine`, where 32767 is 1.
pub(crate) fn arcsine_in_degrees(scaled_sine: i32) -> i32 {
    let size = scaled_sine.unsigned_abs().min(32767) as u16;
    let above = SINE_OF_DEGREES.partition_point(|&sine| sine < size);
    let nearest = match above {
        0 => 0,
        _ if above == SINE_OF_DEGREES.len() => 90,
        _ if size - SINE_OF_DEGREES[above - 1] < SINE_OF_DEGREES[above] - size => above - 1,
        _ => above,
    } as i32;
    if scaled_sine < 0 {
        -nearest
    } else {
        nearest
    }
}

/// How far the wrist rolled from the earlier pair of readings to the later, in degrees; meaningful only for gravity alone.
pub(crate) fn degrees_rolled(along: i32, across: i32, previous_along: i32, previous_across: i32) -> i32 {
    let angle_of = |value: i32| arcsine_in_degrees((value * 32).clamp(-32767, 32767));
    let previous_angle = angle_of(previous_along);
    let angle = angle_of(along);
    match (across < 0, previous_across < 0) {
        (true, true) => angle - previous_angle,
        (_, true) if along < 0 => -previous_angle - angle - 180,
        (_, true) => -previous_angle - angle + 180,
        (true, false) if along < 0 => previous_angle + angle + 180,
        (true, false) => previous_angle + angle - 180,
        (false, false) => previous_angle - angle,
    }
}
