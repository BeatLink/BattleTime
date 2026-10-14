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

// ============================================================================
// Heart rate sensor
// ============================================================================

fn read_registers(chip: &mut SimulatedHeartRateChip, first: u8, buffer: &mut [u8]) {
    embassy_futures::block_on(embedded_hal_async::i2c::I2c::write_read(
        chip,
        0x44,
        &[first],
        buffer,
    ))
    .unwrap();
}

#[test]
fn the_heart_rate_chip_gives_its_id_and_zeros_while_off() {
    let mut chip = SimulatedHeartRateChip::new(72.0);
    let mut id = [0];
    let mut data = [0xff; 8];

    read_registers(&mut chip, 0x00, &mut id);
    read_registers(&mut chip, 0x08, &mut data);

    assert_eq!(id, [0x21]);
    assert_eq!(data, [0; 8]);
}

#[test]
fn the_heart_rate_chip_gives_a_moving_pulse_once_turned_on() {
    let mut chip = SimulatedHeartRateChip::new(72.0);
    embassy_futures::block_on(embedded_hal_async::i2c::I2c::write(
        &mut chip,
        0x44,
        &[0x01, 0xe0],
    ))
    .unwrap();
    let mut first = [0; 8];
    let mut second = [0; 8];

    read_registers(&mut chip, 0x08, &mut first);
    read_registers(&mut chip, 0x08, &mut second);

    assert!(chip.is_measuring());
    assert_eq!(first[1], (9000 >> 8) as u8);
    assert_ne!(first, second);
}
