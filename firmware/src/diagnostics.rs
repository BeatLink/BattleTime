//! Counters a computer can read over Bluetooth, to see what a part of the firmware is doing on the watch.
//!
//! After the counters come the words of `neotime_crash_record::last_restart_words`: why the watch
//! last restarted, and the panic or fault that ended the run before, set once at start.

use core::sync::atomic::{AtomicU32, Ordering};

use neotime_crash_record::LAST_RESTART_WORDS;

#[derive(Clone, Copy)]
pub enum Counter {
    /// Times round the shell's loop.
    ShellLoops,
    /// Which input woke the shell last: 1 touch, 2 button, 3 timer, 4 system values, 5 settings, 6 apps changed, 7 app event.
    ShellLastWake,
    /// Whether the screen is lit: 1 lit, 0 off.
    ScreenLit,
    /// The touch panel: 1 started, 2 failed to start.
    TouchState,
    /// Reports read from the touch panel.
    TouchReports,
    /// Touches sent to the shell.
    TouchesSent,
    /// Presses sent to the shell.
    ButtonPresses,
    /// The accelerometer: 1 started, 2 failed to start, 3 failed to read.
    AccelerometerState,
}

const COUNTER_COUNT: usize = 8;
const WORD_COUNT: usize = COUNTER_COUNT + LAST_RESTART_WORDS;
pub const SNAPSHOT_BYTES: usize = WORD_COUNT * 4;

static COUNTERS: [AtomicU32; WORD_COUNT] = [const { AtomicU32::new(0) }; WORD_COUNT];

pub fn add_one(counter: Counter) {
    COUNTERS[counter as usize].fetch_add(1, Ordering::Relaxed);
}

pub fn set(counter: Counter, value: u32) {
    COUNTERS[counter as usize].store(value, Ordering::Relaxed);
}

pub fn set_last_restart(words: [u32; LAST_RESTART_WORDS]) {
    for (counter, word) in COUNTERS[COUNTER_COUNT..].iter().zip(words) {
        counter.store(word, Ordering::Relaxed);
    }
}

/// Every counter, little-endian, in the order of `Counter`, then the last restart's words.
pub fn snapshot() -> [u8; SNAPSHOT_BYTES] {
    let mut bytes = [0; SNAPSHOT_BYTES];
    for (counter, chunk) in COUNTERS.iter().zip(bytes.as_chunks_mut::<4>().0) {
        *chunk = counter.load(Ordering::Relaxed).to_le_bytes();
    }
    bytes
}
