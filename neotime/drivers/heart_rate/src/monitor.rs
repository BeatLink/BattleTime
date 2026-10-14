//! Turns samples into readings and the sensor's status, as apps see them.
//!
//! The status starts as settling and becomes measuring with the first rate. As InfiniTime does, the
//! ambient light at each good reading sets a limit at twice that level, and light above it means
//! the watch has left the wrist, so the window starts again and the status says no skin.

use crate::{
    ppg::{Ppg, WindowResult},
    Sample,
};

/// The sensor's state, numbered as apps read it in `heart_rate_status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Stopped = 0,
    NoSkin = 1,
    Settling = 2,
    Measuring = 3,
}

/// One measurement, with how sure it is, in percent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reading {
    pub beats_per_minute: u32,
    pub confidence_percent: u32,
}

const AMBIENT_LIGHT_FACTOR: u32 = 2;

/// A rate matched over four beats is surer than one matched over three.
const CONFIDENCE_PER_BEAT_MATCHED_PERCENT: u32 = 25;

pub struct Monitor {
    ppg: Ppg,
    status: Status,
    ambient_light_limit: u32,
}

impl Default for Monitor {
    fn default() -> Monitor {
        Monitor::new()
    }
}

impl Monitor {
    /// Built out of line, so a caller that starts and restarts the monitor holds one copy of its setup.
    #[inline(never)]
    pub const fn new() -> Monitor {
        Monitor {
            ppg: Ppg::new(),
            status: Status::Settling,
            ambient_light_limit: u32::MAX,
        }
    }

    pub fn status(&self) -> Status {
        self.status
    }

    /// Takes one sample, and gives a reading when a window yields a rate.
    pub fn feed(&mut self, sample: Sample) -> Option<Reading> {
        if sample.ambient_light > self.ambient_light_limit {
            *self = Monitor::new();
            self.status = Status::NoSkin;
            return None;
        }

        match self.ppg.feed(sample.pulse) {
            WindowResult::Filling => None,
            WindowResult::NoRate => {
                self.status = Status::Settling;
                None
            }
            WindowResult::Rate(rate) => {
                self.status = Status::Measuring;
                self.ambient_light_limit = sample.ambient_light.max(1).saturating_mul(AMBIENT_LIGHT_FACTOR);
                Some(Reading {
                    beats_per_minute: rate.beats_per_minute,
                    confidence_percent: rate.beats_matched * CONFIDENCE_PER_BEAT_MATCHED_PERCENT,
                })
            }
        }
    }
}
