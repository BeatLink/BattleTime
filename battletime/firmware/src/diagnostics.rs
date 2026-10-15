//! Counters a computer can read over Bluetooth, to see what a part of the firmware is doing on the watch.
//!
//! After the counters come the words of `battletime_crash_record::last_restart_words`: why the watch
//! last restarted, and the panic or fault that ended the run before, set once at start, and then
//! the counters added since, so the words before them keep their places. The companion app writes
//! `DEBUG_REPORT_SAVED` once it has saved that crash in a debug report.

use core::sync::atomic::{AtomicU32, Ordering};

use battletime_crash_record::LAST_RESTART_WORDS;

#[derive(Clone, Copy)]
pub enum Counter {
    /// Times round the shell's loop.
    ShellLoops,
    /// Which input woke the shell last: 1 touch, 2 button, 3 timer, 4 system values, 5 settings, 6 apps changed, 7 app event, 8 status page, 9 temporary app.
    ShellLastWake,
    /// Whether the screen is lit: 1 lit, 0 off.
    ScreenLit,
    /// The touch panel: 1 started, 2 failed to start.
    TouchState,
    /// Reports read from the touch panel.
    TouchReports,
    /// Touches sent to the shell.
    TouchesSent,
    /// Button gestures sent to the shell.
    ButtonPresses,
    /// The accelerometer: 1 started, 2 failed to start, 3 failed to read, 4 waiting for a shared library with its configuration.
    AccelerometerState,
    /// Microseconds the shell has spent drawing apps, counting round past the largest `u32`.
    DrawMicroseconds = FIRST_LATER_WORD as isize,
    /// Microseconds spent sending to the display, the status page and help screen included, counting round the same way.
    DisplaySendMicroseconds,
    /// Sectors the flash store erased.
    StoreSectorsErased,
    /// Store calls the shell made while another task held the store, which then found nothing.
    StoreCallsFoundBusy,
    /// The heart rate sensor: 1 started, 2 failed to start, 3 failed to read.
    HeartRateState,
    /// The heart rate sensor's latest reading, in beats per minute.
    HeartRateBeatsPerMinute,
    /// Bytes the external flash's DMA reads have moved, counting round past the largest `u32`.
    FlashReadBytes,
    /// Times a flash read stopped after a piece to give the bus to the display.
    FlashReadPauses,
    /// App file reads served from a part read ahead with `prefetch_file`, without reading the flash.
    PrefetchedReads,
    /// Instruction fetches the flash cache answered since start, which stops at the largest `u32`.
    FlashCacheHits,
    /// Instruction fetches the flash cache had to read from flash, stopping the same way.
    FlashCacheMisses,
}

const COUNTER_COUNT: usize = 8;

/// The byte the companion app writes to the diagnostics characteristic once it saved the crash.
pub const DEBUG_REPORT_SAVED: u8 = 1;
const FIRST_LATER_WORD: usize = COUNTER_COUNT + LAST_RESTART_WORDS;
const LATER_COUNTER_COUNT: usize = 11;
const WORD_COUNT: usize = FIRST_LATER_WORD + LATER_COUNTER_COUNT;
pub const SNAPSHOT_BYTES: usize = WORD_COUNT * 4;

static COUNTERS: [AtomicU32; WORD_COUNT] = [const { AtomicU32::new(0) }; WORD_COUNT];

pub fn add_one(counter: Counter) {
    COUNTERS[counter as usize].fetch_add(1, Ordering::Relaxed);
}

#[cfg_attr(not(with_shell), allow(dead_code))]
pub fn get(counter: Counter) -> u32 {
    COUNTERS[counter as usize].load(Ordering::Relaxed)
}

pub fn add(counter: Counter, amount: u32) {
    COUNTERS[counter as usize].fetch_add(amount, Ordering::Relaxed);
}

pub fn set(counter: Counter, value: u32) {
    COUNTERS[counter as usize].store(value, Ordering::Relaxed);
}

pub fn set_last_restart(words: [u32; LAST_RESTART_WORDS]) {
    for (counter, word) in COUNTERS[COUNTER_COUNT..].iter().zip(words) {
        counter.store(word, Ordering::Relaxed);
    }
}

/// Every counter, little-endian: the first eight of `Counter`, the last restart's words, then the counters added since.
pub fn snapshot() -> [u8; SNAPSHOT_BYTES] {
    let mut bytes = [0; SNAPSHOT_BYTES];
    for (counter, chunk) in COUNTERS.iter().zip(bytes.as_chunks_mut::<4>().0) {
        *chunk = counter.load(Ordering::Relaxed).to_le_bytes();
    }
    bytes
}

/// Copies the flash cache's hit and miss counts into the counters, so a computer can compare them with the emulator's.
pub fn note_flash_cache() {
    set(Counter::FlashCacheHits, nrf_pac::NVMC.ihit().read());
    set(Counter::FlashCacheMisses, nrf_pac::NVMC.imiss().read());
}
