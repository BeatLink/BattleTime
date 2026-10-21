//! Measures what reading the external flash costs beside drawing, for docs/external-flash-architecture.md.
//!
//! Built only with the `bench` feature, it runs in place of the shell: it owns the display, times
//! flash reads with the bus idle, times full-screen redraws with and without reads made while a
//! strip is drawn, and times one loop run from internal flash, from RAM and from RAM's code alias.
//! Each result is in CPU cycles, 64 to a microsecond; it never writes the flash.
//!
//! `tools/bench_external_flash.py` reads `RESULTS` from the emulator; on the watch the diagnostics
//! characteristic carries the same words after its counters.

use alloc::boxed::Box;
use core::sync::atomic::{AtomicU32, Ordering};

use embassy_time::Timer;
use embedded_graphics::{prelude::*, primitives::Rectangle};

use crate::{display_dma, storage, SharedDisplay};

// ============================================================================
// Results
// ============================================================================

/// Each measurement, in the order `RESULTS` holds them; the runner names them the same way.
#[derive(Clone, Copy)]
pub enum Measure {
    /// Set to `FINISHED` once every measurement is in.
    State,
    /// Raw DMA reads with the bus idle, mean then most cycles: 32 B, 256 B, 1 KB, 4 KB.
    RawRead32Mean,
    RawRead32Most,
    RawRead256Mean,
    RawRead256Most,
    RawRead1kMean,
    RawRead1kMost,
    RawRead4kMean,
    RawRead4kMost,
    /// The same reads through the store, as `read_part_now` does them for the app host.
    StoreRead32Mean,
    StoreRead32Most,
    StoreRead256Mean,
    StoreRead256Most,
    StoreRead1kMean,
    StoreRead1kMost,
    StoreRead4kMean,
    StoreRead4kMost,
    /// A whole file kept as a record (under 1 KB), read with `read_now`.
    RecordReadMean,
    RecordReadMost,
    /// A full-screen redraw of 60 strips that cost nothing to draw: alone, then with a read made while drawing.
    FrameEmpty,
    FrameEmptyRead64EveryStrip,
    FrameEmptyRead256EveryStrip,
    FrameEmptyRead1920EveryStrip,
    /// The same with each strip costing half a millisecond to draw, as a busy screen does.
    FrameBusy,
    FrameBusyRead64EveryStrip,
    FrameBusyRead256EightStrips,
    FrameBusyRead256EveryStrip,
    FrameBusyRead1920EveryStrip,
    /// A busy redraw with a 4 KB read started in the background as it begins, as `prefetch_file` does.
    FrameBusyBackground4k,
    /// Cycles for the small loop (one cache line) and the large one (3 KB of code), run from each place.
    SmallLoopFlash,
    SmallLoopRam,
    SmallLoopCodeAlias,
    LargeLoopFlash,
    LargeLoopRam,
    LargeLoopCodeAlias,
    /// The flash cache's hits and misses over the large loop from internal flash.
    LargeLoopCacheHits,
    LargeLoopCacheMisses,
    /// Bytes the last 4 KB store read found, 0 when `/bench/data` is missing, so its store timings are not real.
    StoreReadBytesFound,
    /// The same reads through an `OpenFile`, as the shell's `read_file` makes them.
    OpenRead32Mean,
    OpenRead32Most,
    OpenRead256Mean,
    OpenRead256Most,
    OpenRead1kMean,
    OpenRead1kMost,
    OpenRead4kMean,
    OpenRead4kMost,
    /// 1 when every open-file read gave the same bytes as the store read, 0 when one differed.
    OpenReadsMatch,
    /// The longest a strip waited for the bus in a busy redraw with a 4 KB read in the background, which streams
    /// to its end, since an async wait for the bus does not ask it to pause.
    LongestStripWaitBackground4k,
    /// A sector erase left alone, on a sector of the free room between the slots.
    EraseAlone,
    /// From asking the chip to pause an erase until it read idle; the whole rest of the erase if it never paused.
    EraseSuspendTime,
    /// The second status register once paused, whose top bit is the chip's suspended bit.
    EraseSuspendStatus2,
    /// 1 when a read made while the erase was paused gave the right bytes.
    EraseSuspendReadMatches,
    /// From resuming until the erase ended.
    EraseAfterResume,
    /// 1 when the sector read back erased at the end.
    EraseSuspendErased,
}

pub const RESULT_COUNT: usize = Measure::EraseSuspendErased as usize + 1;
const FINISHED: u32 = 0xBE7C_0001;

pub static RESULTS: [AtomicU32; RESULT_COUNT] = [const { AtomicU32::new(0) }; RESULT_COUNT];

fn record(measure: Measure, value: u32) {
    RESULTS[measure as usize].store(value, Ordering::Relaxed);
}

/// The results, little-endian, for the diagnostics characteristic.
pub fn result_words() -> [u32; RESULT_COUNT] {
    core::array::from_fn(|index| RESULTS[index].load(Ordering::Relaxed))
}

// ============================================================================
// Clocks
// ============================================================================

// The DWT counts only cycles the CPU runs, so it times the loops; TIMER2, which the firmware never
// uses, keeps counting while the CPU sleeps in WFE waiting for the bus, so it times reads and redraws.
const DEMCR: *mut u32 = 0xE000_EDFC as *mut u32;
const DWT_CTRL: *mut u32 = 0xE000_1000 as *mut u32;
const DWT_CYCCNT: *mut u32 = 0xE000_1004 as *mut u32;
const TRACE_ENABLE: u32 = 1 << 24;
const CYCLE_COUNTER_ENABLE: u32 = 1;
/// TIMER2 counts at 16 MHz, a quarter of the CPU's clock.
const CYCLES_PER_TIMER_TICK: u32 = 4;

fn start_clocks() {
    unsafe {
        DEMCR.write_volatile(DEMCR.read_volatile() | TRACE_ENABLE);
        DWT_CYCCNT.write_volatile(0);
        DWT_CTRL.write_volatile(DWT_CTRL.read_volatile() | CYCLE_COUNTER_ENABLE);
    }
    let timer = nrf_pac::TIMER2;
    timer.tasks_stop().write_value(1);
    timer
        .mode()
        .write(|w| w.set_mode(nrf_pac::timer::vals::Mode::Timer));
    timer
        .bitmode()
        .write(|w| w.set_bitmode(nrf_pac::timer::vals::Bitmode::_32bit));
    timer.prescaler().write(|w| w.set_prescaler(0));
    timer.tasks_clear().write_value(1);
    timer.tasks_start().write_value(1);
}

/// CPU cycles run, which stops while the CPU sleeps.
fn cycles_run() -> u32 {
    unsafe { DWT_CYCCNT.read_volatile() }
}

/// Time passed, in CPU cycles, counting while the CPU sleeps too.
fn cycles() -> u32 {
    let timer = nrf_pac::TIMER2;
    timer.tasks_capture(0).write_value(1);
    timer.cc(0).read().wrapping_mul(CYCLES_PER_TIMER_TICK)
}

/// The time one call of `work` takes, in CPU cycles.
fn time(work: impl FnOnce()) -> u32 {
    let start = cycles();
    work();
    cycles().wrapping_sub(start)
}

/// The cycles the CPU runs for one call of `work`, which never sleeps.
fn time_run(work: impl FnOnce()) -> u32 {
    let start = cycles_run();
    work();
    cycles_run().wrapping_sub(start)
}

// ============================================================================
// The run
// ============================================================================

/// The file the store reads come from: 16 KB the runner puts on the flash, so it spans sectors.
const DATA_PATH: &str = "/bench/data";
/// A file under 1 KB, which the store keeps as a record.
const RECORD_PATH: &str = "/bench/record";
/// Raw reads come from the start of the file system region, whatever it holds.
const RAW_ADDRESS: u32 = battletime_boot::FILE_SYSTEM.start;
const READ_SIZES: [usize; 4] = [32, 256, 1024, 4096];
const REPEATS: u32 = 16;
/// Between measurements, so the watchdog task, Bluetooth and the flash's own work get their turn.
const PAUSE: embassy_time::Duration = embassy_time::Duration::from_millis(20);

#[embassy_executor::task]
pub async fn run_bench(display: &'static SharedDisplay, files: &'static storage::Files) {
    crate::tasks::name_this_task("bench");
    start_clocks();
    // Lets start-up finish and Bluetooth start advertising first, as a shell would.
    Timer::after_secs(3).await;
    let mut buffer = alloc::vec![0u8; 4096].into_boxed_slice();

    measure_raw_reads(&mut buffer, files).await;
    measure_store_reads(&mut buffer, files).await;
    measure_open_reads(&mut buffer, files).await;
    measure_frames(display, files).await;
    measure_loops().await;
    measure_erase_suspend(files).await;

    record(Measure::State, FINISHED);
    loop {
        Timer::after_secs(60).await;
    }
}

/// A sector of the free room between the update and recovery slots, which nothing keeps anything in.
const SCRATCH_SECTOR: u32 = 0x000A_0000;

/// Whether the chip pauses an erase for a read and carries it on afterwards.
async fn measure_erase_suspend(files: &storage::Files) {
    Timer::after(PAUSE).await;
    files.wait_until_free().await;
    let Some(times) = files.measure_erase_suspend(SCRATCH_SECTOR, RAW_ADDRESS, cycles) else {
        return;
    };
    record(Measure::EraseAlone, times.erase_alone);
    record(Measure::EraseSuspendTime, times.suspend);
    record(Measure::EraseSuspendStatus2, times.status_2_while_paused as u32);
    record(
        Measure::EraseSuspendReadMatches,
        times.read_while_paused_matches as u32,
    );
    record(Measure::EraseAfterResume, times.after_resume);
    record(Measure::EraseSuspendErased, times.erased as u32);
}

/// The mean and the most of `REPEATS` timings.
async fn repeat(mut work: impl FnMut() -> u32) -> (u32, u32) {
    let mut total = 0u64;
    let mut most = 0;
    for _ in 0..REPEATS {
        let taken = work();
        total += taken as u64;
        most = most.max(taken);
        Timer::after(PAUSE).await;
    }
    ((total / REPEATS as u64) as u32, most)
}

async fn measure_raw_reads(buffer: &mut [u8], files: &storage::Files) {
    const MEASURES: [(Measure, Measure); 4] = [
        (Measure::RawRead32Mean, Measure::RawRead32Most),
        (Measure::RawRead256Mean, Measure::RawRead256Most),
        (Measure::RawRead1kMean, Measure::RawRead1kMost),
        (Measure::RawRead4kMean, Measure::RawRead4kMost),
    ];
    for (size, (mean, most)) in READ_SIZES.into_iter().zip(MEASURES) {
        let (taken_mean, taken_most) =
            repeat(|| time(|| files.read_flash_now(RAW_ADDRESS, &mut buffer[..size]))).await;
        record(mean, taken_mean);
        record(most, taken_most);
    }
}

async fn measure_store_reads(buffer: &mut [u8], files: &storage::Files) {
    const MEASURES: [(Measure, Measure); 4] = [
        (Measure::StoreRead32Mean, Measure::StoreRead32Most),
        (Measure::StoreRead256Mean, Measure::StoreRead256Most),
        (Measure::StoreRead1kMean, Measure::StoreRead1kMost),
        (Measure::StoreRead4kMean, Measure::StoreRead4kMost),
    ];
    for (size, (mean, most)) in READ_SIZES.into_iter().zip(MEASURES) {
        let mut found = 0;
        let (taken_mean, taken_most) = repeat(|| {
            // An offset past the first sector's start, so 4 KB crosses into a second sector, as most reads would.
            time(|| {
                found = files
                    .read_part_now(DATA_PATH, 3000, &mut buffer[..size])
                    .unwrap_or(0);
            })
        })
        .await;
        record(mean, taken_mean);
        record(most, taken_most);
        record(Measure::StoreReadBytesFound, found as u32);
    }
    let (taken_mean, taken_most) = repeat(|| {
        time(|| {
            let _ = files.read_now(RECORD_PATH);
        })
    })
    .await;
    record(Measure::RecordReadMean, taken_mean);
    record(Measure::RecordReadMost, taken_most);
}

async fn measure_open_reads(buffer: &mut [u8], files: &storage::Files) {
    const MEASURES: [(Measure, Measure); 4] = [
        (Measure::OpenRead32Mean, Measure::OpenRead32Most),
        (Measure::OpenRead256Mean, Measure::OpenRead256Most),
        (Measure::OpenRead1kMean, Measure::OpenRead1kMost),
        (Measure::OpenRead4kMean, Measure::OpenRead4kMost),
    ];
    let Ok(mut file) = files.open_now(DATA_PATH) else {
        return;
    };
    let mut expected = alloc::vec![0u8; 4096].into_boxed_slice();
    let mut all_match = true;
    for (size, (mean, most)) in READ_SIZES.into_iter().zip(MEASURES) {
        let found = files
            .read_part_now(DATA_PATH, 3000, &mut expected[..size])
            .unwrap_or(0);
        let mut matches = true;
        let (taken_mean, taken_most) = repeat(|| {
            let mut read = Ok(0);
            let taken = time(|| read = files.read_open_now(&mut file, 3000, &mut buffer[..size]));
            matches &= read == Ok(found) && buffer[..found] == expected[..found];
            taken
        })
        .await;
        all_match &= matches && found == size;
        record(mean, taken_mean);
        record(most, taken_most);
    }
    record(Measure::OpenReadsMatch, all_match as u32);
}

// ============================================================================
// Redraws
// ============================================================================

const SCREEN_SIDE: u32 = 240;
const STRIP_ROWS: u32 = 4;
const STRIP_COUNT: u32 = SCREEN_SIDE / STRIP_ROWS;
const STRIP_PIXELS: usize = (SCREEN_SIDE * STRIP_ROWS) as usize;
/// Half a millisecond at 64 MHz, about what a strip full of text and icons takes to draw.
const BUSY_DRAW_CYCLES: u32 = 32_000;

/// When a redraw reads the flash while it draws a strip, and how much.
#[derive(Clone, Copy)]
struct Reads {
    bytes: usize,
    /// Reads on strips whose number is a multiple of this; 1 is every strip.
    every: u32,
}

async fn measure_frames(display: &'static SharedDisplay, files: &storage::Files) {
    let buffers: &'static mut [[u16; STRIP_PIXELS]; 2] = Box::leak(Box::new([[0; STRIP_PIXELS]; 2]));
    let mut read_buffer = alloc::vec![0u8; 1920].into_boxed_slice();
    let every_strip = |bytes| Some(Reads { bytes, every: 1 });
    let frames: [(Measure, u32, Option<Reads>); 9] = [
        (Measure::FrameEmpty, 0, None),
        (Measure::FrameEmptyRead64EveryStrip, 0, every_strip(64)),
        (Measure::FrameEmptyRead256EveryStrip, 0, every_strip(256)),
        (Measure::FrameEmptyRead1920EveryStrip, 0, every_strip(1920)),
        (Measure::FrameBusy, BUSY_DRAW_CYCLES, None),
        (
            Measure::FrameBusyRead64EveryStrip,
            BUSY_DRAW_CYCLES,
            every_strip(64),
        ),
        (
            Measure::FrameBusyRead256EightStrips,
            BUSY_DRAW_CYCLES,
            Some(Reads {
                bytes: 256,
                every: STRIP_COUNT / 8,
            }),
        ),
        (
            Measure::FrameBusyRead256EveryStrip,
            BUSY_DRAW_CYCLES,
            every_strip(256),
        ),
        (
            Measure::FrameBusyRead1920EveryStrip,
            BUSY_DRAW_CYCLES,
            every_strip(1920),
        ),
    ];
    for (measure, draw_cycles, reads) in frames {
        let mut best = u32::MAX;
        for _ in 0..4 {
            let (taken, _) = redraw(
                display,
                buffers,
                files,
                &mut read_buffer,
                draw_cycles,
                reads,
                false,
            )
            .await;
            best = best.min(taken);
            Timer::after(PAUSE).await;
        }
        record(measure, best);
    }
    let (mut best, mut longest_wait) = (u32::MAX, 0);
    for _ in 0..4 {
        let (taken, wait) = redraw(
            display,
            buffers,
            files,
            &mut read_buffer,
            BUSY_DRAW_CYCLES,
            None,
            true,
        )
        .await;
        best = best.min(taken);
        longest_wait = longest_wait.max(wait);
        Timer::after(PAUSE).await;
    }
    record(Measure::FrameBusyBackground4k, best);
    record(Measure::LongestStripWaitBackground4k, longest_wait);
}

/// Sends one full screen the way the painter does: draw a strip, await the bus, send it, and draw the next meanwhile.
///
/// Gives the whole redraw's time and the longest a strip waited for the bus.
async fn redraw(
    display: &'static SharedDisplay,
    buffers: &mut [[u16; STRIP_PIXELS]; 2],
    files: &storage::Files,
    read_buffer: &mut [u8],
    draw_cycles: u32,
    reads: Option<Reads>,
    background_read: bool,
) -> (u32, u32) {
    let start = cycles();
    let mut longest_wait = 0;
    let mut background = match background_read {
        true => files.start_reading_part(DATA_PATH, 0, 4096),
        false => None,
    };
    let mut filling = 0;
    for strip in 0..STRIP_COUNT {
        let colour = (strip as u16).wrapping_mul(0x0841);
        buffers[filling].fill(colour);
        spin_for(draw_cycles);
        if let Some(reads) = reads.filter(|reads| strip % reads.every == 0) {
            files.read_flash_now(RAW_ADDRESS, &mut read_buffer[..reads.bytes]);
        }
        let waiting_since = cycles();
        display_dma::bus_is_free().await;
        longest_wait = longest_wait.max(cycles().wrapping_sub(waiting_since));
        let area = Rectangle::new(
            Point::new(0, (strip * STRIP_ROWS) as i32),
            Size::new(SCREEN_SIDE, STRIP_ROWS),
        );
        let _ = display.lock(|display| display.borrow_mut().send_strip(&area, &buffers[filling]));
        filling = 1 - filling;
    }
    display_dma::bus_is_free().await;
    if let Some(part) = background.as_mut() {
        let _ = part.bytes();
    }
    (cycles().wrapping_sub(start), longest_wait)
}

/// Keeps the CPU busy for about this many cycles, standing in for drawing a strip.
fn spin_for(wanted: u32) {
    let start = cycles_run();
    while cycles_run().wrapping_sub(start) < wanted {
        core::hint::spin_loop();
    }
}

// ============================================================================
// Running code from RAM
// ============================================================================

// Two loops written in assembly, so their code can be copied anywhere and still run: they use only
// relative branches and touch only the word they are given. The large one is 3 KB, more than the
// flash cache holds, so from flash it misses on every pass.
core::arch::global_asm!(
    ".section .text.bench_loops,\"ax\",%progbits",
    ".balign 4",
    ".global bench_small_loop",
    ".thumb_func",
    "bench_small_loop:",
    "  movs r2, #0",
    "1:",
    "  ldr r3, [r1]",
    "  adds r2, r2, r3",
    "  eor r3, r3, r2, ror #3",
    "  str r3, [r1]",
    "  subs r0, r0, #1",
    "  bne 1b",
    "  mov r0, r2",
    "  bx lr",
    ".global bench_small_loop_end",
    "bench_small_loop_end:",
    ".balign 4",
    ".global bench_large_loop",
    ".thumb_func",
    "bench_large_loop:",
    "  movs r2, #0",
    "2:",
    ".rept 512",
    "  adds r2, r2, r0",
    "  eor r2, r2, r2, ror #5",
    ".endr",
    "  ldr r3, [r1]",
    "  adds r3, r3, r2",
    "  str r3, [r1]",
    "  subs r0, r0, #1",
    "  bne.w 2b",
    "  mov r0, r2",
    "  bx lr",
    ".global bench_large_loop_end",
    "bench_large_loop_end:",
);

type Loop = unsafe extern "C" fn(rounds: u32, word: *mut u32) -> u32;

unsafe extern "C" {
    fn bench_small_loop(rounds: u32, word: *mut u32) -> u32;
    fn bench_large_loop(rounds: u32, word: *mut u32) -> u32;
    static bench_small_loop_end: u8;
    static bench_large_loop_end: u8;
}

const RAM_START: usize = 0x2000_0000;
const CODE_ALIAS_START: usize = 0x0080_0000;
const SMALL_LOOP_ROUNDS: u32 = 200_000;
const LARGE_LOOP_ROUNDS: u32 = 200;

async fn measure_loops() {
    let small: Loop = bench_small_loop;
    let large: Loop = bench_large_loop;
    let small_end = core::ptr::addr_of!(bench_small_loop_end) as usize;
    let large_end = core::ptr::addr_of!(bench_large_loop_end) as usize;
    let mut word = Box::new(1u32);
    let word: *mut u32 = &mut *word;

    for (code, end, rounds, measures) in [
        (
            small,
            small_end,
            SMALL_LOOP_ROUNDS,
            [
                Measure::SmallLoopFlash,
                Measure::SmallLoopRam,
                Measure::SmallLoopCodeAlias,
            ],
        ),
        (
            large,
            large_end,
            LARGE_LOOP_ROUNDS,
            [
                Measure::LargeLoopFlash,
                Measure::LargeLoopRam,
                Measure::LargeLoopCodeAlias,
            ],
        ),
    ] {
        let start = code as usize & !1;
        let length = end - start;
        let mut copy = alloc::vec![0u32; length.div_ceil(4)].into_boxed_slice();
        unsafe {
            core::ptr::copy_nonoverlapping(start as *const u8, copy.as_mut_ptr() as *mut u8, length);
        }
        cortex_m::asm::dsb();
        cortex_m::asm::isb();
        let in_ram = copy.as_ptr() as usize;
        let in_alias = in_ram - RAM_START + CODE_ALIAS_START;
        let run_from = |address: usize| -> u32 {
            let copied: Loop = unsafe { core::mem::transmute::<usize, Loop>(address | 1) };
            time_run(|| unsafe {
                copied(rounds, word);
            })
        };

        let hits_before = nrf_pac::NVMC.ihit().read();
        let misses_before = nrf_pac::NVMC.imiss().read();
        let from_flash = time_run(|| unsafe {
            code(rounds, word);
        });
        if rounds == LARGE_LOOP_ROUNDS {
            record(
                Measure::LargeLoopCacheHits,
                nrf_pac::NVMC.ihit().read().wrapping_sub(hits_before),
            );
            record(
                Measure::LargeLoopCacheMisses,
                nrf_pac::NVMC.imiss().read().wrapping_sub(misses_before),
            );
        }
        record(measures[0], from_flash);
        Timer::after(PAUSE).await;
        record(measures[1], run_from(in_ram));
        Timer::after(PAUSE).await;
        record(measures[2], run_from(in_alias));
        Timer::after(PAUSE).await;
    }
}
