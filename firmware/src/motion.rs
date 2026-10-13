//! The accelerometer: the step history from the chip's own step counter, and acceleration readings
//! with the shakes found in them, taken only while the open app wants them.
//!
//! `neotime-steps` keeps the history: today and the six days before by hour, and the trip count.
//! Each six-minute slot with steps is added to that day's file in `/steps` as it ends, and the
//! history is read back from there at start. A new day starts at local midnight by itself.

use core::{
    cell::RefCell,
    sync::atomic::{AtomicU32, Ordering},
};

use embassy_futures::select::{select3, Either3};
use embassy_sync::{
    blocking_mutex::{
        raw::{CriticalSectionRawMutex, ThreadModeRawMutex},
        Mutex,
    },
    signal::Signal,
};
use embassy_time::{Delay, Duration, Instant, Timer};
use neotime_accelerometer::{shake::ShakeDetector, Accelerometer, Settings};
use neotime_runtime::{SystemValue, Value};
use neotime_steps::{log, StepHistory};

use crate::{
    app_events, clock,
    diagnostics::{self, Counter},
    foreground, storage, system_values, touch, I2cDeviceOnBus,
};

/// The step history, shared by this task and the shell, which both run in thread mode.
static HISTORY: Mutex<ThreadModeRawMutex, RefCell<StepHistory>> =
    Mutex::new(RefCell::new(StepHistory::new()));

static TRIP_RESET: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// How often acceleration is read, in milliseconds, or 0 while no app wants readings.
static READING_PERIOD_IN_MILLISECONDS: AtomicU32 = AtomicU32::new(0);
static READING_PERIOD_CHANGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

const STEP_READ_INTERVAL: Duration = Duration::from_secs(30);

/// Reads the step history, as apps see it.
pub fn read_history<Answer>(read: impl FnOnce(&StepHistory) -> Answer) -> Answer {
    HISTORY.lock(|history| read(&history.borrow()))
}

pub fn steps_today() -> u32 {
    read_history(StepHistory::steps_today)
}

/// Asks for the trip count to start again from zero, which this task does and saves.
pub fn reset_trip() {
    TRIP_RESET.signal(());
}

/// Starts or stops reading acceleration; `None` stops it, so the chip is read only for steps.
pub fn read_acceleration_every(period_in_milliseconds: Option<Value>) {
    let period = period_in_milliseconds.map_or(0, |period| period.clamp(1, u32::MAX as Value) as u32);
    if READING_PERIOD_IN_MILLISECONDS.swap(period, Ordering::Relaxed) != period {
        READING_PERIOD_CHANGED.signal(());
    }
}

// ============================================================================
// The step history and its log
// ============================================================================

/// The clock keeps local time, which is what the step history counts days in.
fn local_time_now() -> Value {
    clock::now()
}

/// Starts the history for a counter that starts from 0, and reads back what the log holds, even if the chip never starts.
#[inline(never)]
fn start_history(files: &storage::Files) {
    HISTORY.lock(|history| {
        let mut history = history.borrow_mut();
        history.start(local_time_now());
        if let Some(file_system) = files.file_system() {
            log::load_history(file_system, &mut history);
        }
    });
    system_values::changed(&[SystemValue::StepsToday]);
}

/// Adds a reading of the counter, and writes the slot it ended to the log.
#[inline(never)]
fn record_steps(files: &storage::Files, count: u32) {
    let changed = HISTORY.lock(|history| {
        let mut history = history.borrow_mut();
        let recorded = history.record(local_time_now(), count);
        if let Some(file_system) = files.file_system() {
            let _ = log::store(file_system, &mut history, recorded);
        }
        recorded.changed
    });
    if changed {
        system_values::changed(&[SystemValue::StepsToday]);
    }
}

#[inline(never)]
fn reset_trip_now(files: &storage::Files) {
    HISTORY.lock(|history| history.borrow_mut().set_trip_steps(0));
    if let Some(file_system) = files.file_system() {
        let _ = log::save_trip(file_system, 0);
    }
    system_values::changed(&[SystemValue::StepTrip]);
}

// ============================================================================
// The task
// ============================================================================

#[embassy_executor::task]
pub async fn run_accelerometer(i2c_device: I2cDeviceOnBus, files: &'static storage::Files) {
    start_history(files);
    touch::STARTED.wait().await;
    let mut accelerometer = Accelerometer::new(i2c_device);
    if accelerometer.start(Settings::DEFAULT, &mut Delay).await.is_err() {
        diagnostics::set(Counter::AccelerometerState, 2);
        return;
    }
    diagnostics::set(Counter::AccelerometerState, 1);
    let mut shakes = ShakeDetector::new();
    let mut next_step_read = Instant::now();
    let mut next_acceleration_read = Instant::now();
    loop {
        let now = Instant::now();
        if now >= next_step_read {
            match accelerometer.read_step_count().await {
                Ok(count) => {
                    foreground::wait_for_a_pause().await;
                    record_steps(files, count)
                }
                Err(_) => diagnostics::set(Counter::AccelerometerState, 3),
            }
            next_step_read = now + STEP_READ_INTERVAL;
        }

        let period = READING_PERIOD_IN_MILLISECONDS.load(Ordering::Relaxed);
        if period == 0 {
            shakes.reset();
        } else if now >= next_acceleration_read {
            if let Ok(reading) = accelerometer.read_acceleration().await {
                app_events::acceleration_read(reading);
                if shakes.feed(reading, now.as_millis()) {
                    app_events::shake_found();
                }
            }
            next_acceleration_read = now + Duration::from_millis(period as u64);
        }

        let wake_at = match period {
            0 => next_step_read,
            _ => next_step_read.min(next_acceleration_read),
        };
        match select3(
            Timer::at(wake_at),
            READING_PERIOD_CHANGED.wait(),
            TRIP_RESET.wait(),
        )
        .await
        {
            Either3::First(()) => {}
            Either3::Second(()) => next_acceleration_read = Instant::now(),
            Either3::Third(()) => reset_trip_now(files),
        }
    }
}
