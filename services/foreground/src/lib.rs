//! Whether the wearer is watching the screen move, so background work that blocks every task waits for a pause.
//!
//! Every task shares one executor, and littlefs and the flash chips block it while they work: a
//! sector erase holds every task for tens of milliseconds. Work the wearer is not waiting on, such
//! as the step log, package commands and files from the phone, asks `may_start` before it begins,
//! and holds back while the screen is drawing or a finger is down, but never for longer than
//! `LONGEST_HOLD_IN_MILLISECONDS`, so a screen kept busy cannot starve it.

#![no_std]

#[cfg(test)]
mod tests;

// ============================================================================
// Limits
// ============================================================================

/// How long background work holds back for a busy screen before it starts anyway.
pub const LONGEST_HOLD_IN_MILLISECONDS: u64 = 1_000;

/// How often held-back work looks again for a pause.
pub const CHECK_INTERVAL_IN_MILLISECONDS: u64 = 20;

// ============================================================================
// The foreground
// ============================================================================

/// What the wearer may be watching right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Foreground {
    /// A transition or a redraw is part way through.
    pub is_drawing: bool,
    /// A finger is on the screen, perhaps dragging a list.
    pub is_finger_down: bool,
}

impl Foreground {
    pub fn is_busy(self) -> bool {
        self.is_drawing || self.is_finger_down
    }

    /// Whether background work that has held back for `held_in_milliseconds` may start now.
    pub fn may_start(self, held_in_milliseconds: u64) -> bool {
        !self.is_busy() || held_in_milliseconds >= LONGEST_HOLD_IN_MILLISECONDS
    }
}
