//! Turns the pulse channel into beats per minute, with wasp-os's filter chain.
//!
//! Each sample, taken 24 times a second, has its starting level taken off and passes a high-pass
//! filter, a gain control that follows the peaks and drops spikes, and a low-pass filter, leaving a
//! small signed number. Once a window of them is kept, the rate is the shift at which the window
//! best matches itself, found once for one beat and checked again two, three and four beats on.

// ============================================================================
// Constants
// ============================================================================

pub const SAMPLES_PER_SECOND: u32 = 24;

/// About eight seconds of samples, the least the rate is worked out from.
pub const WINDOW_SAMPLES: usize = 200;

/// The newest samples kept after each window, so the next rate comes in about four seconds.
const SAMPLES_KEPT: usize = 104;

/// One beat lasts 7 to 48 samples, which is 205 down to 30 beats a minute.
const SHORTEST_BEAT_SAMPLES: usize = 7;
const LONGEST_BEAT_SAMPLES: usize = 48;

/// A shift is compared over at least this many samples.
const LEAST_OVERLAP_SAMPLES: usize = 24;

const SAMPLES_PER_MINUTE: usize = 60 * SAMPLES_PER_SECOND as usize;

// ============================================================================
// Filters
// ============================================================================

/// A second-order filter in direct form II.
struct Biquad {
    feed_forward: [f32; 3],
    feedback: [f32; 2],
    delays: [f32; 2],
}

impl Biquad {
    const fn new(feed_forward: [f32; 3], feedback: [f32; 2]) -> Biquad {
        Biquad {
            feed_forward,
            feedback,
            delays: [0.0; 2],
        }
    }

    /// Kept out of line, since both filters run it.
    #[inline(never)]
    fn step(&mut self, input: f32) -> f32 {
        let [first_delay, second_delay] = self.delays;

        let middle = input - self.feedback[0] * first_delay - self.feedback[1] * second_delay;
        let output = self.feed_forward[0] * middle
            + self.feed_forward[1] * first_delay
            + self.feed_forward[2] * second_delay;
        self.delays = [middle, first_delay];
        output
    }
}

/// Follows the size of the peaks, drops samples far above them, and scales the rest to about ±50.
struct PeakTrackingGain {
    peak: f32,
}

impl PeakTrackingGain {
    const STARTING_PEAK: f32 = 20.0;
    const DECAY: f32 = 0.971;
    const SPIKE_THRESHOLD: f32 = 2.0;
    const SCALED_PEAK: f32 = 50.0;

    fn step(&mut self, sample: f32) -> f32 {
        let size = if sample < 0.0 { -sample } else { sample };
        if size > self.peak {
            self.peak /= Self::DECAY;
        } else {
            self.peak *= Self::DECAY;
        }

        if size > self.peak * Self::SPIKE_THRESHOLD {
            return 0.0;
        }
        Self::SCALED_PEAK * sample / self.peak
    }
}

// ============================================================================
// The processor
// ============================================================================

/// What one sample brought.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowResult {
    /// The window is still filling.
    Filling,
    /// A full window had no steady beat in it.
    NoRate,
    Rate(Rate),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rate {
    pub beats_per_minute: u32,
    /// How many beats on the match was confirmed: 3, or 4 for the more precise rate.
    pub beats_matched: u32,
}

pub struct Ppg {
    starting_level: Option<f32>,
    high_pass: Biquad,
    gain: PeakTrackingGain,
    low_pass: Biquad,
    window: [i8; WINDOW_SAMPLES],
    window_length: usize,
}

impl Default for Ppg {
    fn default() -> Ppg {
        Ppg::new()
    }
}

impl Ppg {
    pub const fn new() -> Ppg {
        Ppg {
            starting_level: None,
            high_pass: Biquad::new(
                [0.870_330_8, -1.740_661_6, 0.870_330_8],
                [-1.723_776_2, 0.757_546_9],
            ),
            gain: PeakTrackingGain {
                peak: PeakTrackingGain::STARTING_PEAK,
            },
            low_pass: Biquad::new(
                [0.115_952_49, 0.231_904_98, 0.115_952_49],
                [-0.721_681_4, 0.185_491_38],
            ),
            window: [0; WINDOW_SAMPLES],
            window_length: 0,
        }
    }

    /// Takes one pulse sample, and works out the rate whenever the window is full.
    pub fn feed(&mut self, pulse: u32) -> WindowResult {
        let level = pulse as f32;
        let starting_level = *self.starting_level.get_or_insert(level);
        let high_passed = self.high_pass.step(level - starting_level);
        let filtered = self.low_pass.step(self.gain.step(high_passed));
        // The cast saturates, so a sample past the range of a byte keeps its sign and goes no further.
        self.window[self.window_length] = filtered as i8;
        self.window_length += 1;
        if self.window_length < WINDOW_SAMPLES {
            return WindowResult::Filling;
        }

        let result = rate_in(&self.window).map_or(WindowResult::NoRate, WindowResult::Rate);
        self.window.copy_within(WINDOW_SAMPLES - SAMPLES_KEPT.., 0);
        self.window_length = SAMPLES_KEPT;
        result
    }
}

// ============================================================================
// Finding the rate
// ============================================================================

/// How unlike the window is to itself moved on by `shift` samples: the sum of squared differences.
fn dissimilarity(window: &[i8], shift: usize) -> i32 {
    window[shift..]
        .iter()
        .zip(window)
        .map(|(later, earlier)| {
            let difference = i32::from(*later) - i32::from(*earlier);
            difference * difference
        })
        .sum()
}

/// The first shift from `lowest`, at least 2, to `highest` where the dissimilarity bottoms out.
fn trough(window: &[i8], lowest: usize, highest: usize) -> Option<usize> {
    let highest = highest.min(window.len() - LEAST_OVERLAP_SAMPLES);
    if lowest > highest {
        return None;
    }

    let mut before_previous = dissimilarity(window, lowest - 2);
    let mut previous = dissimilarity(window, lowest - 1);
    for shift in lowest..=highest {
        let current = dissimilarity(window, shift);
        if before_previous > previous && previous < current {
            return Some(shift - 1);
        }
        before_previous = previous;
        previous = current;
    }
    None
}

/// The rate of the beat that repeats through the window, if one repeats at least three times.
fn rate_in(window: &[i8]) -> Option<Rate> {
    let one_beat = trough(window, SHORTEST_BEAT_SAMPLES, LONGEST_BEAT_SAMPLES)?;
    let two_beats = trough(window, one_beat * 2 - 5, one_beat * 2 + 5)?;
    let three_beats = trough(window, two_beats * 3 / 2 - 5, two_beats * 3 / 2 + 4)?;

    let rate = match trough(window, three_beats * 4 / 3 - 4, three_beats * 4 / 3 + 4) {
        Some(four_beats) => Rate {
            beats_per_minute: (SAMPLES_PER_MINUTE * 4 / four_beats) as u32,
            beats_matched: 4,
        },
        None => Rate {
            beats_per_minute: (SAMPLES_PER_MINUTE * 3 / three_beats) as u32,
            beats_matched: 3,
        },
    };
    Some(rate)
}
