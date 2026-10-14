//! The heart rate sensor: measured while an app asks or the background interval comes due, 24 times
//! a second, or flashed as a beacon, with its LED off otherwise.
//!
//! An app's `start_heart_rate` keeps it measuring until `stop_heart_rate` or the app closes,
//! `measure_heart_rate` measures until the first reading, and `set_heart_rate_led` flashes it until
//! the app closes. With no app asking, the heart rate interval setting measures always, never or
//! now and then, as `neotime_heart_rate::schedule` decides. Each reading wakes the shell as a system
//! value change, which passes it to the open app, and the latest one, its time and the sensor's
//! status stay for apps and faces to read. The sample window lives on the heap only while
//! measuring, so the sensor costs little RAM the rest of the time.

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
    schedule::{self, AppRequest, Background, Mode, Schedule},
    Command, HeartRateSensor,
};
use neotime_runtime::{SystemValue, Value};
use neotime_settings::{heart_rate_interval, Setting};

use crate::{
    clock,
    diagnostics::{self, Counter},
    settings, system_values, I2cDeviceOnBus,
};

// ============================================================================
// Requests from apps
// ============================================================================

const OFF: u8 = 0;
const UNTIL_STOPPED: u8 = 1;
const UNTIL_A_READING: u8 = 2;
const FLASHING: u8 = 3;

static REQUEST: AtomicU8 = AtomicU8::new(OFF);
static FLASH_DRIVE: AtomicU8 = AtomicU8::new(0);
static FLASH_WAIT: AtomicU8 = AtomicU8::new(0);
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

/// Flashes the LED at this brightness (0 to 3) and period in milliseconds, or stops flashing.
pub fn flash_led(is_on: bool, brightness: Value, period_in_milliseconds: Value) {
    if !is_on {
        let _ = REQUEST.compare_exchange(FLASHING, OFF, Ordering::Relaxed, Ordering::Relaxed);
    } else if let AppRequest::Flash { drive, wait } =
        schedule::flash_request(brightness, period_in_milliseconds)
    {
        FLASH_DRIVE.store(drive, Ordering::Relaxed);
        FLASH_WAIT.store(wait, Ordering::Relaxed);
        REQUEST.store(FLASHING, Ordering::Relaxed);
    }
    REQUEST_CHANGED.signal(());
}

/// Wakes the task to read the heart rate interval setting again.
pub fn interval_changed() {
    REQUEST_CHANGED.signal(());
}

fn app_request() -> AppRequest {
    match REQUEST.load(Ordering::Relaxed) {
        OFF => AppRequest::Nothing,
        FLASHING => AppRequest::Flash {
            drive: FLASH_DRIVE.load(Ordering::Relaxed),
            wait: FLASH_WAIT.load(Ordering::Relaxed),
        },
        _ => AppRequest::Measure,
    }
}

fn background() -> Background {
    let choice = settings::current().get(Setting::HeartRateInterval);
    match heart_rate_interval::seconds_between(choice) {
        Some(seconds) => Background::Every { seconds },
        None if choice == heart_rate_interval::CONTINUOUS => Background::Continuous,
        None => Background::Off,
    }
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

/// What the sensor is doing.
enum Running {
    Off,
    Measuring(Box<Monitor>),
    Flashing(Mode),
}

/// Starts, samples, flashes or stops the sensor as the schedule says.
///
/// The chip is reached from one place only, which keeps this task small in flash.
#[embassy_executor::task]
pub async fn run_heart_rate_sensor(i2c_device: I2cDeviceOnBus) {
    crate::tasks::name_this_task("heart");
    let mut sensor = HeartRateSensor::new(i2c_device);
    let mut schedule = Schedule::new();
    let mut running = Running::Off;
    let mut next_sample = Instant::now();
    loop {
        let background = background();
        let mode = schedule.mode(app_request(), background, Instant::now().as_millis());
        let command = match (mode, &running) {
            (Mode::Measure, Running::Measuring(_)) => {
                if let Either::Second(()) = select(Timer::at(next_sample), REQUEST_CHANGED.wait()).await {
                    continue;
                }
                next_sample += SAMPLE_PERIOD;
                Command::Read
            }
            (Mode::Measure, _) => Command::Start,
            (Mode::Flash { drive, wait }, Running::Flashing(flashing)) if *flashing != mode => {
                Command::Flash { drive, wait }
            }
            (Mode::Flash { drive, wait }, Running::Off | Running::Measuring(_)) => {
                Command::Flash { drive, wait }
            }
            (Mode::Off, Running::Measuring(_) | Running::Flashing(_)) => Command::Stop,
            (Mode::Off | Mode::Flash { .. }, _) => {
                let wake_at = schedule
                    .wake_at_in_milliseconds()
                    .map_or(Instant::MAX, Instant::from_millis);
                select(Timer::at(wake_at), REQUEST_CHANGED.wait()).await;
                continue;
            }
        };

        let result = sensor.perform(command).await;

        let now = Instant::now().as_millis();
        match (command, result) {
            (Command::Start, Ok(_)) => {
                diagnostics::set(Counter::HeartRateState, 1);
                let started = Box::new(Monitor::new());
                set_status(started.status());
                running = Running::Measuring(started);
                next_sample = Instant::now();
            }
            (Command::Read, Ok(sample)) => {
                let Running::Measuring(monitor) = &mut running else {
                    continue;
                };
                let reading = monitor.feed(sample);
                set_status(monitor.status());
                if let Some(reading) = reading {
                    keep_reading(reading);
                    let _ =
                        REQUEST.compare_exchange(UNTIL_A_READING, OFF, Ordering::Relaxed, Ordering::Relaxed);
                    schedule.reading_taken(background, now);
                }
            }
            (Command::Flash { .. }, Ok(_)) => {
                running = Running::Flashing(mode);
                set_status(Status::Stopped);
            }
            (Command::Stop, _) => {
                running = Running::Off;
                set_status(Status::Stopped);
            }
            (Command::Read, Err(_)) => diagnostics::set(Counter::HeartRateState, 3),
            (Command::Start | Command::Flash { .. }, Err(_)) => {
                // A chip that does not answer is left alone until something new is asked of it.
                diagnostics::set(Counter::HeartRateState, 2);
                REQUEST.store(OFF, Ordering::Relaxed);
                schedule.reading_taken(background, now);
                REQUEST_CHANGED.wait().await;
            }
        }
    }
}
