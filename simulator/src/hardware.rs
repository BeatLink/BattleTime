//! Simulated pins and timers, standing in for the parts of the board the drivers need.

use std::{
    convert::Infallible,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use embedded_hal::digital::{ErrorType, OutputPin};

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
