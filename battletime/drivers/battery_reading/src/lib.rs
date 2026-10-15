//! One blocking reading of the PineTime's battery voltage, for the programs that run before the firmware.
//!
//! The bootloader, its updater and the reloader have no executor and no interrupts, so this drives
//! the SAADC's registers itself. It reads the battery's pin P0.31 (AIN7) set up as the firmware's
//! reading is: gain 1/6, the internal 0.6 V reference and 12 bits. Every wait gives up after a
//! bounded number of polls, so a converter that never answers cannot stop the program.

#![no_std]

use core::sync::atomic::{compiler_fence, Ordering};

use battletime_battery::millivolts_from_reading;

// ============================================================================
// The converter's registers
// ============================================================================

/// The SAADC's registers, so tests can stand in for the chip.
pub trait Registers {
    fn read(&self, offset: u32) -> u32;
    fn write(&mut self, offset: u32, value: u32);
    /// Points the converter's results at a word of RAM.
    fn point_result_at(&mut self, result: *mut i16);
}

/// The nRF52832's own SAADC.
pub struct Saadc;

const SAADC_BASE: u32 = 0x4000_7000;

impl Registers for Saadc {
    fn read(&self, offset: u32) -> u32 {
        unsafe { ((SAADC_BASE + offset) as *const u32).read_volatile() }
    }

    fn write(&mut self, offset: u32, value: u32) {
        unsafe { ((SAADC_BASE + offset) as *mut u32).write_volatile(value) }
    }

    fn point_result_at(&mut self, result: *mut i16) {
        self.write(RESULT_POINTER, result as u32);
    }
}

pub const TASKS_START: u32 = 0x000;
pub const TASKS_SAMPLE: u32 = 0x004;
pub const TASKS_STOP: u32 = 0x008;
pub const TASKS_CALIBRATE_OFFSET: u32 = 0x00C;
pub const EVENTS_STARTED: u32 = 0x100;
pub const EVENTS_END: u32 = 0x104;
pub const EVENTS_CALIBRATE_DONE: u32 = 0x110;
pub const EVENTS_STOPPED: u32 = 0x114;
pub const ENABLE: u32 = 0x500;
pub const CHANNEL_0_POSITIVE_INPUT: u32 = 0x510;
pub const CHANNEL_0_CONFIG: u32 = 0x518;
pub const RESOLUTION: u32 = 0x5F0;
pub const RESULT_POINTER: u32 = 0x62C;
pub const RESULT_COUNT: u32 = 0x630;

/// AIN7, the battery's pin P0.31.
pub const BATTERY_INPUT: u32 = 8;
/// Gain 1/6, the internal reference and a 10 microsecond acquisition time.
pub const BATTERY_CHANNEL_CONFIG: u32 = 0x0002_0000;
pub const TWELVE_BITS: u32 = 2;

// ============================================================================
// Reading
// ============================================================================

/// Readings averaged into one, to calm the noise.
const SAMPLES: i32 = 4;
/// Polls of an event before giving up; far longer than any step takes.
const POLLS_BEFORE_GIVING_UP: u32 = 200_000;

/// The battery's voltage in millivolts, or none when the converter never answered.
pub fn read_millivolts(registers: &mut impl Registers) -> Option<i32> {
    // Lives until the converter is stopped, since the converter writes into it.
    let mut result: i16 = 0;
    let result_pointer: *mut i16 = &mut result;
    registers.write(ENABLE, 1);
    registers.write(CHANNEL_0_POSITIVE_INPUT, BATTERY_INPUT);
    registers.write(CHANNEL_0_CONFIG, BATTERY_CHANNEL_CONFIG);
    registers.write(RESOLUTION, TWELVE_BITS);
    registers.point_result_at(result_pointer);
    registers.write(RESULT_COUNT, 1);
    let total = calibrate_and_sample(registers, result_pointer);
    let _ = run_task(registers, TASKS_STOP, EVENTS_STOPPED);
    registers.write(ENABLE, 0);
    Some(millivolts_from_reading((total? / SAMPLES) as i16))
}

fn calibrate_and_sample(registers: &mut impl Registers, result: *const i16) -> Option<i32> {
    run_task(registers, TASKS_CALIBRATE_OFFSET, EVENTS_CALIBRATE_DONE)?;
    // The nRF52832 never ends a sample started straight after a calibration (anomaly 86), so stop first.
    run_task(registers, TASKS_STOP, EVENTS_STOPPED)?;
    let mut total = 0;
    for _ in 0..SAMPLES {
        run_task(registers, TASKS_START, EVENTS_STARTED)?;
        run_task(registers, TASKS_SAMPLE, EVENTS_END)?;
        // The converter wrote the result by DMA, behind the compiler's back.
        compiler_fence(Ordering::SeqCst);
        total += unsafe { result.read_volatile() } as i32;
    }
    Some(total)
}

/// Clears the event, starts the task and waits for the event.
fn run_task(registers: &mut impl Registers, task: u32, event: u32) -> Option<()> {
    registers.write(event, 0);
    registers.write(task, 1);
    (0..POLLS_BEFORE_GIVING_UP)
        .any(|_| registers.read(event) != 0)
        .then_some(())
}

#[cfg(test)]
mod tests;
