//! Simulated pins, timers and the heart rate sensor, standing in for the parts of the board the drivers need.

use std::{
    convert::Infallible,
    f32::consts::PI,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use battletime_heart_rate::ppg::SAMPLES_PER_SECOND;
use embedded_hal::{
    digital::{ErrorType, OutputPin},
    i2c::Operation,
};

// ============================================================================
// Delay
// ============================================================================

/// Waits by sleeping the thread, which is fine because each simulated task has its own thread.
pub struct ThreadDelay;

impl embedded_hal::delay::DelayNs for ThreadDelay {
    fn delay_ns(&mut self, duration_in_nanoseconds: u32) {
        thread::sleep(Duration::from_nanos(duration_in_nanoseconds as u64));
    }
}

impl embedded_hal_async::delay::DelayNs for ThreadDelay {
    async fn delay_ns(&mut self, duration_in_nanoseconds: u32) {
        thread::sleep(Duration::from_nanos(duration_in_nanoseconds as u64));
    }
}

// ============================================================================
// Pins
// ============================================================================

/// A pin that goes nowhere, such as the panel's reset line.
pub struct UnconnectedPin;

impl ErrorType for UnconnectedPin {
    type Error = Infallible;
}

impl OutputPin for UnconnectedPin {
    fn set_low(&mut self) -> Result<(), Infallible> {
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Infallible> {
        Ok(())
    }
}

/// The motor pin, which runs the motor while low, as on the PineTime.
pub struct MotorPin {
    pub motor_running: Arc<AtomicBool>,
}

impl ErrorType for MotorPin {
    type Error = Infallible;
}

impl OutputPin for MotorPin {
    fn set_low(&mut self) -> Result<(), Infallible> {
        self.motor_running.store(true, Ordering::Relaxed);
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Infallible> {
        self.motor_running.store(false, Ordering::Relaxed);
        Ok(())
    }
}

// ============================================================================
// Heart rate sensor
// ============================================================================

const HEART_RATE_ID_REGISTER: usize = 0x00;
const HEART_RATE_ID: u8 = 0x21;
const HEART_RATE_ENABLE_REGISTER: usize = 0x01;
const HEART_RATE_MEASURING: u8 = 0x80;
const HEART_RATE_DATA_REGISTER: usize = 0x08;
const PULSE_LEVEL: f32 = 9000.0;
const PULSE_HEIGHT: f32 = 10.0;
const AMBIENT_LIGHT: u32 = 40;

/// The HRS3300 on a wrist with a steady pulse; each read of its data gives the next conversion, 24 a second.
pub struct SimulatedHeartRateChip {
    pub beats_per_minute: f32,
    registers: [u8; 32],
    conversions: u32,
}

impl SimulatedHeartRateChip {
    pub fn new(beats_per_minute: f32) -> SimulatedHeartRateChip {
        let mut registers = [0; 32];
        registers[HEART_RATE_ID_REGISTER] = HEART_RATE_ID;
        SimulatedHeartRateChip {
            beats_per_minute,
            registers,
            conversions: 0,
        }
    }

    pub fn is_measuring(&self) -> bool {
        self.registers[HEART_RATE_ENABLE_REGISTER] & HEART_RATE_MEASURING != 0
    }

    /// Lays the next conversion out over the data registers as the chip does, or zeros while it is off.
    fn convert(&mut self) {
        let (pulse, ambient_light) = if self.is_measuring() {
            let seconds = self.conversions as f32 / SAMPLES_PER_SECOND as f32;
            let phase = 2.0 * PI * seconds * self.beats_per_minute / 60.0;
            let shape = phase.sin() + 0.1 * (2.0 * phase + PI / 4.0).sin();
            self.conversions += 1;
            ((PULSE_LEVEL + PULSE_HEIGHT * shape).round() as u32, AMBIENT_LIGHT)
        } else {
            (0, 0)
        };
        let data = &mut self.registers[HEART_RATE_DATA_REGISTER..HEART_RATE_DATA_REGISTER + 8];
        data[0] = (ambient_light >> 3) as u8;
        data[1] = (pulse >> 8) as u8;
        data[2] = (pulse >> 4 & 0x0f) as u8;
        data[5] = (ambient_light >> 11 & 0x3f) as u8;
        data[6] = (ambient_light & 0x07) as u8;
        data[7] = (pulse & 0x0f) as u8;
    }
}

impl embedded_hal::i2c::ErrorType for SimulatedHeartRateChip {
    type Error = Infallible;
}

impl embedded_hal_async::i2c::I2c for SimulatedHeartRateChip {
    async fn transaction(
        &mut self,
        _address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Infallible> {
        let mut register = 0;
        for operation in operations {
            match operation {
                Operation::Write([first, values @ ..]) => {
                    register = *first as usize;
                    for (offset, value) in values.iter().enumerate() {
                        self.registers[register + offset] = *value;
                    }
                }
                Operation::Write([]) => {}
                Operation::Read(buffer) => {
                    if register == HEART_RATE_DATA_REGISTER {
                        self.convert();
                    }
                    buffer.copy_from_slice(&self.registers[register..register + buffer.len()]);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "hardware_tests.rs"]
mod hardware_tests;
