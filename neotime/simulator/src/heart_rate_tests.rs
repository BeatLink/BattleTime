//! Tests for the simulated heart rate sensor, stepped as fast as they like.

use neotime_heart_rate::ppg::WINDOW_SAMPLES;

use super::*;

fn new_sensor() -> (SimulatedSensor, HeartRateLink) {
    let (sensor, link, _updates) =
        SimulatedSensor::new(SimulatedHeartRateChip::new(SIMULATED_BEATS_PER_MINUTE));
    (sensor, link)
}

/// Steps the sensor this many times and gives every update.
fn step(sensor: &mut SimulatedSensor, times: usize) -> Vec<Update> {
    (0..times).filter_map(|_| sensor.step()).collect()
}

#[test]
fn the_sensor_stays_off_until_an_app_asks() {
    let (mut sensor, _link) = new_sensor();

    assert!(step(&mut sensor, 10).is_empty());
    assert!(!sensor.is_measuring());
}

#[test]
fn a_started_sensor_settles_then_reads_the_simulated_pulse() {
    let (mut sensor, link) = new_sensor();
    link.start();

    let updates = step(&mut sensor, WINDOW_SAMPLES * 2);

    assert_eq!(updates[0], Update::Status(Status::Settling));
    let readings: Vec<Reading> = updates
        .iter()
        .filter_map(|update| match update {
            Update::Reading(reading) => Some(*reading),
            Update::Status(_) => None,
        })
        .collect();
    assert!(readings.len() >= 2);
    assert!(readings
        .iter()
        .all(|reading| reading.beats_per_minute.abs_diff(72) <= 2));
    assert!(sensor.is_measuring());
}

#[test]
fn stopping_turns_the_chip_off() {
    let (mut sensor, link) = new_sensor();
    link.start();
    step(&mut sensor, 5);

    link.stop();

    assert_eq!(step(&mut sensor, 5), [Update::Status(Status::Stopped)]);
    assert!(!sensor.is_measuring());
}

#[test]
fn measuring_once_stops_after_the_first_reading() {
    let (mut sensor, link) = new_sensor();
    link.measure_once();

    let updates = step(&mut sensor, WINDOW_SAMPLES * 2);

    let reading_count = updates
        .iter()
        .filter(|update| matches!(update, Update::Reading(_)))
        .count();
    assert_eq!(reading_count, 1);
    assert_eq!(updates.last(), Some(&Update::Status(Status::Stopped)));
}

#[test]
fn measuring_once_leaves_a_sensor_started_until_stopped_running() {
    let (mut sensor, link) = new_sensor();
    link.start();

    link.measure_once();
    step(&mut sensor, WINDOW_SAMPLES * 2);

    assert!(sensor.is_measuring());
}
