//! Whether the firmware's parts are still making progress, so the watchdog is fed only while they are.
//!
//! The watchdog task checks once a second. Each part shows progress through a counter that moves:
//! the shell counts its loops, and the radio counts its interrupts. A part whose counter stands
//! still for too many checks has stopped, and the watchdog is left to reset the watch. Checks are
//! counted, not time, so a long blocking job that also stops the watchdog task never counts against
//! a part.

#![no_std]

#[cfg(test)]
mod tests;

// ============================================================================
// Limits
// ============================================================================

/// Checks without a shell loop before the shell is asked to loop once.
pub const SHELL_PING_AFTER: u32 = 1;
/// Checks without a shell loop, pings included, before the shell counts as stopped.
pub const SHELL_STOPPED_AFTER: u32 = 3;
/// Checks without a radio interrupt, while the radio has work, before it counts as stopped. Longer
/// than the longest connection interval, four seconds, at one check a second.
pub const RADIO_STOPPED_AFTER: u32 = 6;

// ============================================================================
// One part
// ============================================================================

/// How long one counter has stood still.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Counter {
    last: u32,
    still_checks: u32,
}

impl Counter {
    /// Takes the counter's value at one check and gives how many checks it has now stood still.
    fn check(&mut self, value: u32) -> u32 {
        if value == self.last {
            self.still_checks = self.still_checks.saturating_add(1);
        } else {
            self.last = value;
            self.still_checks = 0;
        }
        self.still_checks
    }

    fn reset(&mut self) {
        self.still_checks = 0;
    }
}

// ============================================================================
// The parts together
// ============================================================================

/// What the watchdog task read at one check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reading {
    /// The shell's loop count, or None while there is no shell to wait for: before its first loop,
    /// and in a build without one.
    pub shell_loops: Option<u32>,
    /// The radio interrupts so far.
    pub radio_events: u32,
    /// True while the radio has nothing to do: Bluetooth off, or the controller not started.
    pub radio_idle: bool,
}

/// What the watchdog task should do after a check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verdict {
    /// Every part is making progress, so the watchdog may be fed.
    pub alive: bool,
    /// The shell has been quiet a while, so wake it once to prove it still runs.
    pub ping_shell: bool,
}

#[derive(Default)]
pub struct Liveness {
    shell: Counter,
    radio: Counter,
}

impl Liveness {
    pub const fn new() -> Liveness {
        Liveness {
            shell: Counter {
                last: 0,
                still_checks: 0,
            },
            radio: Counter {
                last: 0,
                still_checks: 0,
            },
        }
    }

    pub fn check(&mut self, reading: Reading) -> Verdict {
        let shell_still = match reading.shell_loops {
            Some(loops) => self.shell.check(loops),
            None => {
                self.shell.reset();
                0
            }
        };
        let radio_still = if reading.radio_idle {
            self.radio.last = reading.radio_events;
            self.radio.reset();
            0
        } else {
            self.radio.check(reading.radio_events)
        };
        Verdict {
            alive: shell_still < SHELL_STOPPED_AFTER && radio_still < RADIO_STOPPED_AFTER,
            ping_shell: shell_still >= SHELL_PING_AFTER,
        }
    }
}
