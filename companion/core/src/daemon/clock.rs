// Notices when this device's clock jumps or its time zone changes, so the watch can be set again.
//
// A clock jumps when it is set by hand, synced from the network, moved to a new zone, or after a sleep.

use std::time::Duration;

// How often the clock is checked.
pub const CLOCK_CHECK_INTERVAL: Duration = Duration::from_secs(60);

// How far the clock may wander from what elapsed before it counts as a jump.
const CLOCK_JUMP_THRESHOLD_IN_MILLISECONDS: i64 = 2000;

// How often the watch is set anyway, so its own drift never builds up.
const CLOCK_RESYNC_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

// One reading of the clocks: the wall clock, a steady clock that never jumps, and the zone's offset.
#[derive(Debug, Clone, Copy)]
pub struct ClockReading {
    pub wall_in_milliseconds: i64,
    pub steady: Duration,
    pub offset_in_seconds: i32,
}

impl ClockReading {
    pub fn now(started: std::time::Instant) -> ClockReading {
        let now = chrono::Local::now();
        ClockReading {
            wall_in_milliseconds: now.timestamp_millis(),
            steady: started.elapsed(),
            offset_in_seconds: now.offset().local_minus_utc(),
        }
    }
}

pub struct ClockWatch {
    last: ClockReading,
    last_sync: Duration,
}

impl ClockWatch {
    pub fn new(first: ClockReading) -> ClockWatch {
        ClockWatch {
            last: first,
            last_sync: first.steady,
        }
    }

    // True after a jump, a zone change, or a long stretch without either.
    pub fn check(&mut self, now: ClockReading) -> bool {
        let elapsed = now.steady.saturating_sub(self.last.steady).as_millis() as i64;
        let expected_wall = self.last.wall_in_milliseconds + elapsed;
        let jumped = (now.wall_in_milliseconds - expected_wall).abs() > CLOCK_JUMP_THRESHOLD_IN_MILLISECONDS;
        let zone_changed = now.offset_in_seconds != self.last.offset_in_seconds;
        let due = now.steady.saturating_sub(self.last_sync) >= CLOCK_RESYNC_INTERVAL;
        self.last = now;
        if jumped || zone_changed || due {
            self.last_sync = now.steady;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(wall_in_seconds: i64, steady_in_seconds: u64, offset_in_seconds: i32) -> ClockReading {
        ClockReading {
            wall_in_milliseconds: wall_in_seconds * 1000,
            steady: Duration::from_secs(steady_in_seconds),
            offset_in_seconds,
        }
    }

    #[test]
    fn a_clock_that_keeps_time_asks_for_nothing() {
        let mut watch = ClockWatch::new(reading(1000, 0, 0));
        assert!(!watch.check(reading(1060, 60, 0)));
        assert!(!watch.check(reading(1121, 121, 0)));
    }

    #[test]
    fn a_jump_or_a_new_zone_asks_for_the_watch_to_be_set() {
        let mut watch = ClockWatch::new(reading(1000, 0, 0));
        assert!(watch.check(reading(5000, 60, 0)));
        assert!(watch.check(reading(5060, 120, 3600)));
        assert!(!watch.check(reading(5120, 180, 3600)));
    }

    #[test]
    fn the_watch_is_set_again_every_six_hours_anyway() {
        let mut watch = ClockWatch::new(reading(0, 0, 0));
        let six_hours = 6 * 60 * 60;
        assert!(!watch.check(reading(six_hours - 60, six_hours as u64 - 60, 0)));
        assert!(watch.check(reading(six_hours, six_hours as u64, 0)));
    }
}
