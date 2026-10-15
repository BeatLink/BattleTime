//! Plans when the radio's 1 MHz timer and the 64 MHz crystal sleep between operations, and when a 32768 Hz counter wakes them.
//!
//! The controller's clock is microseconds since the real-time counter started. While the timer is
//! stopped the clock is read from the counter. A counter tick starts the timer again, so the timer
//! always counts from a moment the clock knows exactly: the microsecond that tick fell on.

use crate::link_layer::RadioOperation;

// ============================================================================
// Timing limits
// ============================================================================

/// Ticks of the 32768 Hz counter allowed for the crystal to start before the timer does, about 1 ms.
pub const CRYSTAL_START_TICKS: u64 = 32;

/// The counter cannot fire a compare set to its current tick or the one after.
pub const MINIMUM_COUNTER_LEAD_TICKS: u64 = 2;

/// Microseconds between the timer starting and the earliest operation it can start.
pub const TIMER_LEAD: u32 = 50;

/// An operation this far ahead or more lets the timer and the crystal sleep until shortly before it.
pub const SLEEP_THRESHOLD: u32 = 2_000;

/// The real-time counter's width; the controller extends it to 64 bits in software.
pub const COUNTER_MASK: u32 = 0x00FF_FFFF;

// ============================================================================
// Counter ticks and microseconds
// ============================================================================

/// The clock's reading at a counter tick: 1 000 000 / 32768 is 15625 / 512 microseconds a tick.
pub fn microseconds_at_tick(tick: u64) -> u32 {
    (tick * 15_625 / 512) as u32
}

/// Whole ticks that fit in `microseconds`.
fn ticks_within(microseconds: u32) -> u64 {
    microseconds as u64 * 512 / 15_625
}

/// Extends a fresh 24-bit counter reading past the last extended tick, which must be under 512 s old.
pub fn extend_tick(last_tick: u64, counter: u32) -> u64 {
    let ticks_since = counter.wrapping_sub(last_tick as u32) & COUNTER_MASK;
    last_tick + ticks_since as u64
}

// ============================================================================
// Planning
// ============================================================================

/// When the counter starts the crystal and then the timer, and how far the operation had to move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wake {
    pub crystal_tick: u64,
    pub timer_tick: u64,
    /// The clock's reading at `timer_tick`, where the timer counts from zero.
    pub timer_base: u32,
    /// Microseconds the operation moves later, when it was asked for sooner than the crystal can start.
    pub delay: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plan {
    /// Keep the timer running and start the operation from it.
    StayAwake,
    /// Stop the timer and the crystal, with nothing to wake for.
    Sleep,
    /// Stop the timer and the crystal, and have the counter start them again in time.
    SleepThenWake(Wake),
}

/// Decides whether to sleep before `operation`, given the clock and the counter now.
pub fn plan(operation: RadioOperation, now: u32, now_tick: u64, timer_is_running: bool) -> Plan {
    let Some(start_at) = start_of(operation) else {
        return Plan::Sleep;
    };
    let lead = start_at.wrapping_sub(now) as i32;
    if timer_is_running && lead < SLEEP_THRESHOLD as i32 {
        return Plan::StayAwake;
    }

    let earliest_timer_tick = now_tick + MINIMUM_COUNTER_LEAD_TICKS + CRYSTAL_START_TICKS;
    let wanted = start_at
        .wrapping_sub(TIMER_LEAD)
        .wrapping_sub(microseconds_at_tick(now_tick)) as i32;
    let wanted_timer_tick = now_tick + ticks_within(wanted.max(0) as u32);
    let timer_tick = wanted_timer_tick.max(earliest_timer_tick);
    let timer_base = microseconds_at_tick(timer_tick);

    let delay = if timer_tick > wanted_timer_tick {
        (timer_base.wrapping_add(TIMER_LEAD).wrapping_sub(start_at) as i32).max(0) as u32
    } else {
        0
    };

    Plan::SleepThenWake(Wake {
        crystal_tick: timer_tick - CRYSTAL_START_TICKS,
        timer_tick,
        timer_base,
        delay,
    })
}

/// When an operation turns the radio on, if it does.
pub fn start_of(operation: RadioOperation) -> Option<u32> {
    match operation {
        RadioOperation::Off => None,
        RadioOperation::Transmit { start_at, .. } | RadioOperation::Receive { start_at, .. } => {
            Some(start_at)
        }
    }
}

/// The same operation `delay` microseconds later, listening for as long as before.
pub fn postpone(operation: RadioOperation, delay: u32) -> RadioOperation {
    match operation {
        RadioOperation::Off => RadioOperation::Off,
        RadioOperation::Transmit { start_at, channel } => RadioOperation::Transmit {
            start_at: start_at.wrapping_add(delay),
            channel,
        },
        RadioOperation::Receive {
            start_at,
            give_up_at,
            channel,
        } => RadioOperation::Receive {
            start_at: start_at.wrapping_add(delay),
            give_up_at: give_up_at.wrapping_add(delay),
            channel,
        },
    }
}

#[cfg(test)]
#[path = "wake_plan_tests.rs"]
mod wake_plan_tests;
