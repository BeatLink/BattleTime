//! Tests for the simulated pins and delay that stand in for the board.

use std::{
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};

use embedded_hal::digital::OutputPin;

use super::*;

// ============================================================================
// Motor pin
// ============================================================================

#[test]
fn driving_the_motor_pin_low_runs_the_motor() {
    let motor_running = Arc::new(AtomicBool::new(false));
    let mut motor_pin = MotorPin {
        motor_running: motor_running.clone(),
    };

    motor_pin.set_low().unwrap();

    assert!(motor_running.load(Ordering::Relaxed));
}

#[test]
fn driving_the_motor_pin_high_stops_the_motor() {
    let motor_running = Arc::new(AtomicBool::new(true));
    let mut motor_pin = MotorPin {
        motor_running: motor_running.clone(),
    };

    motor_pin.set_high().unwrap();

    assert!(!motor_running.load(Ordering::Relaxed));
}

// ============================================================================
// Unconnected pin
// ============================================================================

#[test]
fn an_unconnected_pin_accepts_either_level() {
    let mut pin = UnconnectedPin;

    assert!(pin.set_low().is_ok());
    assert!(pin.set_high().is_ok());
}

// ============================================================================
// Delay
// ============================================================================

const TWO_MILLISECONDS_IN_NANOSECONDS: u32 = 2_000_000;

#[test]
fn the_blocking_delay_waits_at_least_as_long_as_asked() {
    let started = Instant::now();

    embedded_hal::delay::DelayNs::delay_ns(&mut ThreadDelay, TWO_MILLISECONDS_IN_NANOSECONDS);

    assert!(started.elapsed() >= Duration::from_millis(2));
}

#[test]
fn the_async_delay_waits_at_least_as_long_as_asked() {
    let started = Instant::now();

    embassy_futures::block_on(embedded_hal_async::delay::DelayNs::delay_ns(
        &mut ThreadDelay,
        TWO_MILLISECONDS_IN_NANOSECONDS,
    ));

    assert!(started.elapsed() >= Duration::from_millis(2));
}
