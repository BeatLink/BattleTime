extern crate std;

use std::{f32::consts::PI, vec::Vec};

use embassy_futures::block_on;
use embedded_hal::i2c::{self, ErrorKind, Operation};

use super::*;
use crate::{
    monitor::{Monitor, Reading, Status},
    ppg::{Ppg, WindowResult, SAMPLES_PER_SECOND, WINDOW_SAMPLES},
};

// ============================================================================
// Fake hardware
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
struct FakeBusError;

impl i2c::Error for FakeBusError {
    fn kind(&self) -> ErrorKind {
        ErrorKind::Other
    }
}

/// A chip that keeps its registers, reads on from the register it was given, and records every write.
struct FakeChip {
    registers: [u8; 32],
    writes: Vec<[u8; 2]>,
    is_broken: bool,
}

impl FakeChip {
    fn new() -> FakeChip {
        let mut registers = [0; 32];
        registers[ID_REGISTER as usize] = HRS3300_ID;
        FakeChip {
            registers,
            writes: Vec::new(),
            is_broken: false,
        }
    }
}

impl I2cErrorType for FakeChip {
    type Error = FakeBusError;
}

impl I2c for FakeChip {
    async fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), FakeBusError> {
        assert_eq!(address, I2C_ADDRESS);
        if self.is_broken {
            return Err(FakeBusError);
        }

        let mut register = 0;
        for operation in operations {
            match operation {
                Operation::Write(bytes) => {
                    register = bytes[0] as usize;
                    if let [_, value] = bytes {
                        self.writes.push([bytes[0], *value]);
                        self.registers[register] = *value;
                    }
                }
                Operation::Read(buffer) => {
                    buffer.copy_from_slice(&self.registers[register..register + buffer.len()]);
                }
            }
        }
        Ok(())
    }
}

/// A pulse shaped like the emulator's: a rise and fall with a small second hump, on a steady level.
fn pulse_at(sample_index: usize, beats_per_minute: f32) -> u32 {
    let seconds = sample_index as f32 / SAMPLES_PER_SECOND as f32;
    let phase = 2.0 * PI * seconds * beats_per_minute / 60.0;
    let shape = phase.sin() + 0.1 * (2.0 * phase + PI / 4.0).sin();
    (9000.0 + 10.0 * shape).round() as u32
}

/// Feeds samples of a steady pulse and gives the rate of every window.
fn rates_for_pulse(beats_per_minute: f32, sample_count: usize) -> Vec<WindowResult> {
    let mut ppg = Ppg::new();
    (0..sample_count)
        .map(|index| ppg.feed(pulse_at(index, beats_per_minute)))
        .filter(|result| *result != WindowResult::Filling)
        .collect()
}

// ============================================================================
// The driver
// ============================================================================

#[test]
fn starting_sets_the_led_resolution_and_gain_then_turns_measuring_on() {
    let mut sensor = HeartRateSensor::new(FakeChip::new());

    block_on(sensor.start()).unwrap();

    assert_eq!(
        sensor.bus.writes,
        [
            [0x01, 0x60],
            [0x0c, 0x6e],
            [0x16, 0x88],
            [0x17, 0x00],
            [0x01, 0xe0]
        ]
    );
}

#[test]
fn starting_refuses_another_chip() {
    let mut chip = FakeChip::new();
    chip.registers[ID_REGISTER as usize] = 0x13;
    let mut sensor = HeartRateSensor::new(chip);

    assert_eq!(block_on(sensor.start()), Err(Error::WrongChip(0x13)));
    assert!(sensor.bus.writes.is_empty());
}

#[test]
fn stopping_turns_measuring_and_the_led_off() {
    let mut sensor = HeartRateSensor::new(FakeChip::new());

    block_on(sensor.stop()).unwrap();

    assert_eq!(sensor.bus.writes, [[0x01, 0x60], [0x0c, 0x00]]);
}

#[test]
fn reading_unpacks_both_channels_from_one_read() {
    let mut chip = FakeChip::new();
    // The pulse is spread over 0x09, 0x0a and 0x0f, and the ambient light over 0x08, 0x0d and 0x0e.
    chip.registers[0x08] = 0xfb;
    chip.registers[0x09] = 0x5a;
    chip.registers[0x0a] = 0xfb;
    chip.registers[0x0d] = 0xea;
    chip.registers[0x0e] = 0xfd;
    chip.registers[0x0f] = 0x2c;
    let mut sensor = HeartRateSensor::new(chip);

    let sample = block_on(sensor.read()).unwrap();

    assert_eq!(sample.pulse, 0x2_5abc);
    assert_eq!(sample.ambient_light, (0x2a << 11) | (0xfb << 3) | 0x05);
}

#[test]
fn a_bus_error_is_passed_on() {
    let mut chip = FakeChip::new();
    chip.is_broken = true;
    let mut sensor = HeartRateSensor::new(chip);

    assert_eq!(block_on(sensor.start()), Err(Error::Bus(FakeBusError)));
    assert_eq!(block_on(sensor.read()), Err(Error::Bus(FakeBusError)));
    assert_eq!(block_on(sensor.stop()), Err(Error::Bus(FakeBusError)));
}

// ============================================================================
// Finding the rate
// ============================================================================

#[test]
fn finds_steady_rates_from_45_to_180_within_two_beats() {
    for beats_per_minute in [45, 60, 70, 90, 120, 150, 180] {
        let results = rates_for_pulse(beats_per_minute as f32, WINDOW_SAMPLES * 3);

        for result in &results[1..] {
            let WindowResult::Rate(rate) = result else {
                panic!("no rate at {beats_per_minute}: {results:?}");
            };
            assert!(
                rate.beats_per_minute.abs_diff(beats_per_minute) <= 2,
                "{} for {beats_per_minute}",
                rate.beats_per_minute
            );
        }
    }
}

#[test]
fn a_flat_signal_has_no_rate() {
    let mut ppg = Ppg::new();

    let results: Vec<_> = (0..WINDOW_SAMPLES * 2).map(|_| ppg.feed(9000)).collect();

    assert!(results
        .iter()
        .all(|result| matches!(result, WindowResult::Filling | WindowResult::NoRate)));
    assert!(results.contains(&WindowResult::NoRate));
}

#[test]
fn the_first_rate_comes_after_a_full_window_and_then_every_four_seconds() {
    let mut ppg = Ppg::new();

    let finished: Vec<usize> = (0..WINDOW_SAMPLES * 2)
        .filter(|&index| ppg.feed(pulse_at(index, 70.0)) != WindowResult::Filling)
        .collect();

    assert_eq!(finished, [199, 295, 391]);
}

#[test]
fn a_spike_does_not_spoil_the_rate() {
    let mut ppg = Ppg::new();
    let mut results = Vec::new();

    for index in 0..WINDOW_SAMPLES * 3 {
        let pulse = if index == 250 {
            60_000
        } else {
            pulse_at(index, 70.0)
        };
        results.push(ppg.feed(pulse));
    }

    let last = results
        .iter()
        .rev()
        .find(|result| **result != WindowResult::Filling)
        .unwrap();
    let WindowResult::Rate(rate) = last else {
        panic!("no rate after the spike: {last:?}");
    };
    assert!(rate.beats_per_minute.abs_diff(70) <= 2);
}

// ============================================================================
// The monitor
// ============================================================================

fn feed_pulse(monitor: &mut Monitor, from: usize, count: usize, ambient_light: u32) -> Vec<Reading> {
    (from..from + count)
        .filter_map(|index| {
            monitor.feed(Sample {
                pulse: pulse_at(index, 70.0),
                ambient_light,
            })
        })
        .collect()
}

#[test]
fn the_monitor_settles_then_measures() {
    let mut monitor = Monitor::new();
    assert_eq!(monitor.status(), Status::Settling);

    let readings = feed_pulse(&mut monitor, 0, WINDOW_SAMPLES * 2, 40);

    assert_eq!(monitor.status(), Status::Measuring);
    assert!(readings.len() >= 2);
    assert!(readings
        .iter()
        .all(|reading| reading.beats_per_minute.abs_diff(70) <= 2));
    assert!(readings.iter().all(|reading| reading.confidence_percent == 100));
}

#[test]
fn bright_light_after_a_reading_means_the_watch_left_the_wrist() {
    let mut monitor = Monitor::new();
    feed_pulse(&mut monitor, 0, WINDOW_SAMPLES, 40);
    assert_eq!(monitor.status(), Status::Measuring);

    let readings = feed_pulse(&mut monitor, WINDOW_SAMPLES, 1, 81);

    assert!(readings.is_empty());
    assert_eq!(monitor.status(), Status::NoSkin);
    assert!(feed_pulse(&mut monitor, 0, WINDOW_SAMPLES - 1, 5000).is_empty());
    assert_eq!(feed_pulse(&mut monitor, WINDOW_SAMPLES - 1, 1, 5000).len(), 1);
    assert_eq!(monitor.status(), Status::Measuring);
}

#[test]
fn a_window_without_a_beat_goes_back_to_settling() {
    let mut monitor = Monitor::new();
    feed_pulse(&mut monitor, 0, WINDOW_SAMPLES, 40);

    for _ in 0..WINDOW_SAMPLES {
        monitor.feed(Sample {
            pulse: 9000,
            ambient_light: 40,
        });
    }

    assert_eq!(monitor.status(), Status::Settling);
}
