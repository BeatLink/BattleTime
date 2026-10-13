//! The simulated heart rate sensor: the real driver and monitor on their own thread, over the simulated chip.
//!
//! Apps start and stop it with the same effects as on the watch, and it sends the sensor's status
//! and each reading back to the app's loop.

use std::{
    sync::{
        atomic::{AtomicU8, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

use embassy_futures::block_on;
use neotime_heart_rate::{
    monitor::{Monitor, Reading, Status},
    ppg::SAMPLES_PER_SECOND,
    HeartRateSensor,
};

use crate::hardware::SimulatedHeartRateChip;

// ============================================================================
// Requests and updates
// ============================================================================

/// The pulse on the simulated wrist.
pub const SIMULATED_BEATS_PER_MINUTE: f32 = 72.0;

const OFF: u8 = 0;
const UNTIL_STOPPED: u8 = 1;
const UNTIL_A_READING: u8 = 2;

const CHIP_NEVER_FAILS: &str = "the simulated chip never fails";
const SAMPLE_PERIOD: Duration = Duration::from_micros(1_000_000 / SAMPLES_PER_SECOND as u64);
const IDLE_PAUSE: Duration = Duration::from_millis(50);

/// A change from the sensor; a reading also means the status is now measuring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Update {
    Status(Status),
    Reading(Reading),
}

/// How an app reaches the sensor's thread: what it asks for, and what comes back.
pub struct HeartRateLink {
    request: Arc<AtomicU8>,
    pub updates: mpsc::Receiver<Update>,
}

impl HeartRateLink {
    pub fn start(&self) {
        self.request.store(UNTIL_STOPPED, Ordering::Relaxed);
    }

    pub fn stop(&self) {
        self.request.store(OFF, Ordering::Relaxed);
    }

    /// Measures until the next reading, unless the sensor already runs until stopped.
    pub fn measure_once(&self) {
        let _ = self
            .request
            .compare_exchange(OFF, UNTIL_A_READING, Ordering::Relaxed, Ordering::Relaxed);
    }
}

// ============================================================================
// The sensor
// ============================================================================

/// The driver and monitor, stepped once a sample; the thread steps it in time, tests as fast as they like.
pub struct SimulatedSensor {
    sensor: HeartRateSensor<SimulatedHeartRateChip>,
    monitor: Option<Monitor>,
    request: Arc<AtomicU8>,
}

impl SimulatedSensor {
    pub fn new(chip: SimulatedHeartRateChip) -> (SimulatedSensor, HeartRateLink, mpsc::Sender<Update>) {
        let request = Arc::new(AtomicU8::new(OFF));
        let (sender, updates) = mpsc::channel();
        let sensor = SimulatedSensor {
            sensor: HeartRateSensor::new(chip),
            monitor: None,
            request: request.clone(),
        };
        (sensor, HeartRateLink { request, updates }, sender)
    }

    pub fn is_measuring(&self) -> bool {
        self.monitor.is_some()
    }

    /// Starts, stops or samples the sensor as the request says, and gives what changed.
    pub fn step(&mut self) -> Option<Update> {
        let is_wanted = self.request.load(Ordering::Relaxed) != OFF;
        match (is_wanted, &mut self.monitor) {
            (false, None) => None,
            (true, None) => {
                block_on(self.sensor.start()).expect(CHIP_NEVER_FAILS);
                let monitor = Monitor::new();
                let status = monitor.status();
                self.monitor = Some(monitor);
                Some(Update::Status(status))
            }
            (false, Some(_)) => {
                block_on(self.sensor.stop()).expect(CHIP_NEVER_FAILS);
                self.monitor = None;
                Some(Update::Status(Status::Stopped))
            }
            (true, Some(monitor)) => {
                let sample = block_on(self.sensor.read()).expect(CHIP_NEVER_FAILS);
                let status_before = monitor.status();
                if let Some(reading) = monitor.feed(sample) {
                    let _ = self.request.compare_exchange(
                        UNTIL_A_READING,
                        OFF,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    );
                    return Some(Update::Reading(reading));
                }
                (monitor.status() != status_before).then_some(Update::Status(monitor.status()))
            }
        }
    }
}

/// Runs the sensor on its own thread, 24 samples a second while measuring, until the app's loop goes away.
pub fn start_heart_rate_sensor() -> HeartRateLink {
    let (mut sensor, link, updates) =
        SimulatedSensor::new(SimulatedHeartRateChip::new(SIMULATED_BEATS_PER_MINUTE));
    thread::spawn(move || loop {
        thread::sleep(if sensor.is_measuring() {
            SAMPLE_PERIOD
        } else {
            IDLE_PAUSE
        });
        let Some(update) = sensor.step() else {
            continue;
        };
        if updates.send(update).is_err() {
            return;
        }
    });
    link
}

#[cfg(test)]
#[path = "heart_rate_tests.rs"]
mod heart_rate_tests;
