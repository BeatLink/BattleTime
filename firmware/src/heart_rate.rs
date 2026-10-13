//! The heart rate sensor: measured only while an app asks, 24 times a second, with its LED off otherwise.
//!
//! An app's `start_heart_rate` keeps it measuring until `stop_heart_rate` or the app closes, and
//! `measure_heart_rate` measures until the first reading. Each reading wakes the shell as a system
//! value change, which passes it to the open app, and the latest one, its time and the sensor's
//! status stay for apps to read. The sample window lives on the heap only while measuring, so the
//! sensor costs little RAM the rest of the time.

extern crate alloc;

use alloc::boxed::Box;
use core::{
    cell::Cell,
    sync::atomic::{AtomicU32, AtomicU8, Ordering},
};

use embassy_futures::select::{select, Either};
use embassy_sync::{
    blocking_mutex::{raw::CriticalSectionRawMutex, Mutex},
    signal::Signal,
};
use embassy_time::{Duration, Instant, Timer};
use neotime_heart_rate::{
    monitor::{Monitor, Reading, Status},
    ppg::SAMPLES_PER_SECOND,
    Command, HeartRateSensor, Sample,
};
use neotime_runtime::{SystemValue, Value};

use crate::{
    clock,
    diagnostics::{self, Counter},
    system_values, I2cDeviceOnBus,
};

// ============================================================================
// Requests from apps
// ============================================================================

const OFF: u8 = 0;
const UNTIL_STOPPED: u8 = 1;
const UNTIL_A_READING: u8 = 2;

static REQUEST: AtomicU8 = AtomicU8::new(OFF);
static REQUEST_CHANGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

fn request(wanted: u8) {
    if REQUEST.swap(wanted, Ordering::Relaxed) != wanted {
        REQUEST_CHANGED.signal(());
    }
}

/// Measures until `stop`.
pub fn start() {
    request(UNTIL_STOPPED);
}

pub fn stop() {
    request(OFF);
}

/// Measures until the next reading, unless the sensor already runs until stopped.
pub fn measure_once() {
    let _ = REQUEST.compare_exchange(OFF, UNTIL_A_READING, Ordering::Relaxed, Ordering::Relaxed);
    REQUEST_CHANGED.signal(());
}

// ============================================================================
// What apps read
// ============================================================================

static BEATS_PER_MINUTE: AtomicU32 = AtomicU32::new(0);
static CONFIDENCE_PERCENT: AtomicU32 = AtomicU32::new(0);
static STATUS: AtomicU8 = AtomicU8::new(Status::Stopped as u8);
static MEASURED_AT: Mutex<CriticalSectionRawMutex, Cell<Value>> =
    Mutex::new(Cell::new(neotime_runtime::NONE));

/// The heart rate values apps read, or `None` for any other value.
pub fn read(value: SystemValue) -> Option<Value> {
    let status = STATUS.load(Ordering::Relaxed);
    Some(match value {
        SystemValue::HeartRate => BEATS_PER_MINUTE.load(Ordering::Relaxed) as Value,
        SystemValue::HeartRateMeasuredAt => MEASURED_AT.lock(Cell::get),
        SystemValue::HeartRateStatus => status as Value,
        SystemValue::IsHeartRateRunning => (status != Status::Stopped as u8) as Value,
        _ => return None,
    })
}

/// The latest reading for the open app: beats per minute and confidence in percent.
pub fn latest_reading() -> (Value, Value) {
    (
        BEATS_PER_MINUTE.load(Ordering::Relaxed) as Value,
        CONFIDENCE_PERCENT.load(Ordering::Relaxed) as Value,
    )
}

/// Kept out of line, since the task sets the status in three places.
#[inline(never)]
fn set_status(status: Status) {
    if STATUS.swap(status as u8, Ordering::Relaxed) != status as u8 {
        system_values::changed(&[SystemValue::HeartRateStatus, SystemValue::IsHeartRateRunning]);
    }
}

/// Keeps the reading, and tells the shell, which passes it to the open app.
fn keep_reading(reading: Reading) {
    diagnostics::set(Counter::HeartRateBeatsPerMinute, reading.beats_per_minute);
    BEATS_PER_MINUTE.store(reading.beats_per_minute, Ordering::Relaxed);
    CONFIDENCE_PERCENT.store(reading.confidence_percent, Ordering::Relaxed);
    MEASURED_AT.lock(|measured_at| measured_at.set(clock::utc_now()));
    system_values::changed(&[SystemValue::HeartRate]);
}

// ============================================================================
// The task
// ============================================================================

const SAMPLE_PERIOD: Duration = Duration::from_micros(1_000_000 / SAMPLES_PER_SECOND as u64);

/// Starts the sensor when an app asks, samples it while measuring, and stops it when no app wants it.
///
/// The chip is reached from one place only, which keeps this task small in flash.
#[embassy_executor::task]
pub async fn run_heart_rate_sensor(i2c_device: I2cDeviceOnBus) {
    let mut sensor = HeartRateSensor::new(i2c_device);
    let mut monitor: Option<Box<Monitor>> = None;
    let mut next_sample = Instant::now();
    loop {
        let is_wanted = REQUEST.load(Ordering::Relaxed) != OFF;
        let command = match (is_wanted, monitor.is_some()) {
            (false, false) => {
                REQUEST_CHANGED.wait().await;
                continue;
            }
            (true, false) => Command::Start,
            (false, true) => Command::Stop,
            (true, true) => {
                if let Either::Second(()) = select(Timer::at(next_sample), REQUEST_CHANGED.wait()).await {
                    continue;
                }
                next_sample += SAMPLE_PERIOD;
                Command::Read
            }
        };

        let result = sensor.perform(command).await;

        match (command, result) {
            (Command::Start, Ok(_)) => {
                diagnostics::set(Counter::HeartRateState, 1);
                let started = Box::new(Monitor::new());
                set_status(started.status());
                monitor = Some(started);
                next_sample = Instant::now();
            }
            (Command::Start, Err(_)) => {
                diagnostics::set(Counter::HeartRateState, 2);
                REQUEST.store(OFF, Ordering::Relaxed);
            }
            (Command::Stop, _) => {
                monitor = None;
                set_status(Status::Stopped);
            }
            (Command::Read, Ok(sample)) => {
                if let Some(monitor) = &mut monitor {
                    take_sample(monitor, sample);
                }
            }
            (Command::Read, Err(_)) => diagnostics::set(Counter::HeartRateState, 3),
        }
    }
}

/// Feeds one sample to the monitor, and keeps and passes on any reading, which ends a `measure_once`.
fn take_sample(monitor: &mut Monitor, sample: Sample) {
    let reading = monitor.feed(sample);
    set_status(monitor.status());
    if let Some(reading) = reading {
        keep_reading(reading);
        let _ = REQUEST.compare_exchange(UNTIL_A_READING, OFF, Ordering::Relaxed, Ordering::Relaxed);
    }
}
