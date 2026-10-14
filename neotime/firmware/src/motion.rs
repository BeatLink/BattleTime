//! The accelerometer: the step history from the chip's own step counter, and acceleration readings
//! with the shakes found in them, taken only while the open app wants them or the shell watches for
//! a wake gesture: a raised or shaken wrist while the screen is dark, a lowered arm while it is lit.
//! Each reading also updates the wrist's shake speed, which apps see only while a screen shows it.
//!
//! `neotime-steps` keeps the history: today and the six days before by hour, and the trip count.
//! Each six-minute slot with steps is added to that day's file in `/steps` as it ends, and the
//! history is read back from there at start. A new day starts at local midnight by itself.

use core::{
    cell::RefCell,
    sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering},
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
use neotime_accelerometer::{
    shake::ShakeDetector,
    wake_gestures::{WakeGestureDetector, Watching},
    Accelerometer, Configurations, Error, Settings,
};
use neotime_app_host::wake::WakeGestures;
use neotime_runtime::{AppImage, LibraryData, SystemValue, Value};
use neotime_steps::{DayChange, StepHistory, HOURS_PER_DAY};

use crate::{
    app_events, apps, clock,
    diagnostics::{self, Counter},
    foreground,
    storage_requests::{self, StorageRequest},
    system_values, touch, I2cDeviceOnBus,
};

/// The step history, shared by this task and the shell, which both run in thread mode.
static HISTORY: Mutex<ThreadModeRawMutex, RefCell<StepHistory>> =
    Mutex::new(RefCell::new(StepHistory::new()));

static TRIP_RESET: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// How often acceleration is read, in milliseconds, or 0 while no app wants readings.
static READING_PERIOD_IN_MILLISECONDS: AtomicU32 = AtomicU32::new(0);
static READING_PERIOD_CHANGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

const STEP_READ_INTERVAL: Duration = Duration::from_secs(30);

/// The wake gestures the shell watches for, packed by `pack_watching`.
static WATCHING: AtomicU32 = AtomicU32::new(0);

/// Whether a screen shows the shake speed, so each new speed is passed on.
static SHAKE_SPEED_WANTED: AtomicBool = AtomicBool::new(false);
static SHAKE_SPEED: AtomicI32 = AtomicI32::new(0);

/// Wake gestures are looked for in readings taken ten times a second, as InfiniTime takes them.
const WAKE_GESTURE_PERIOD_IN_MILLISECONDS: u32 = 100;

const RAISE_BIT: u32 = 1 << 0;
const LOWER_BIT: u32 = 1 << 1;
const SHAKE_BIT: u32 = 1 << 2;
const THRESHOLD_SHIFT: u32 = 8;

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
/// The chips' configuration files from the shared library inside the firmware, or none in a build without one.
fn configurations() -> Configurations<'static> {
    let Some(library) = apps::shared_library().and_then(|bytes| AppImage::parse(bytes).ok()) else {
        return Configurations::default();
    };
    Configurations {
        bma421: library.data(LibraryData::Bma421Configuration as u16),
        bma425: library.data(LibraryData::Bma425Configuration as u16),
    }
}

pub fn read_acceleration_every(period_in_milliseconds: Option<Value>) {
    let period = period_in_milliseconds.map_or(0, |period| period.clamp(1, u32::MAX as Value) as u32);
    if READING_PERIOD_IN_MILLISECONDS.swap(period, Ordering::Relaxed) != period {
        READING_PERIOD_CHANGED.signal(());
    }
}

/// Sets the wake gestures to watch for; the accelerometer is read while any is watched.
pub fn watch_for(gestures: WakeGestures) {
    let packed = pack_watching(gestures);
    if WATCHING.swap(packed, Ordering::Relaxed) != packed {
        READING_PERIOD_CHANGED.signal(());
    }
}

/// Whether a screen shows the shake speed; while one does, each new speed tells the shell.
pub fn report_shake_speed(wanted: bool) {
    SHAKE_SPEED_WANTED.store(wanted, Ordering::Relaxed);
}

/// The wrist's shake speed at the latest reading, as InfiniTime counts it.
pub fn shake_speed() -> i32 {
    SHAKE_SPEED.load(Ordering::Relaxed)
}

fn pack_watching(gestures: WakeGestures) -> u32 {
    let threshold = gestures.shake_threshold.clamp(0, u16::MAX as i32) as u32;
    let mut packed = threshold << THRESHOLD_SHIFT;
    for (is_watched, bit) in [
        (gestures.raise, RAISE_BIT),
        (gestures.lower, LOWER_BIT),
        (gestures.shake, SHAKE_BIT),
    ] {
        if is_watched {
            packed |= bit;
        }
    }
    packed
}

fn unpack_watching(packed: u32) -> Watching {
    Watching {
        raise: packed & RAISE_BIT != 0,
        lower: packed & LOWER_BIT != 0,
        shake: packed & SHAKE_BIT != 0,
        shake_threshold: (packed >> THRESHOLD_SHIFT) as i32,
    }
}

/// Passes a reading to the open app and the detectors, and on to the shell whatever they find.
fn take_reading(
    reading: neotime_accelerometer::Acceleration,
    time_in_milliseconds: u64,
    app_wants_readings: bool,
    shakes: &mut ShakeDetector,
    wake_gestures: &mut WakeGestureDetector,
) {
    if app_wants_readings {
        app_events::acceleration_read(reading);
        if shakes.feed(reading, time_in_milliseconds) {
            app_events::shake_found();
        }
    }
    let watching = unpack_watching(WATCHING.load(Ordering::Relaxed));
    if let Some(gesture) = wake_gestures.feed(reading, time_in_milliseconds, watching) {
        app_events::wake_gesture_found(gesture);
    }
    let speed = wake_gestures.shake_speed();
    let speed_changed = SHAKE_SPEED.swap(speed, Ordering::Relaxed) != speed;
    if speed_changed && SHAKE_SPEED_WANTED.load(Ordering::Relaxed) {
        system_values::changed(&[SystemValue::ShakeSpeed]);
    }
}

// ============================================================================
// The step history and its log
// ============================================================================

/// The clock keeps local time, which is what the step history counts days in.
fn local_time_now() -> Value {
    clock::now()
}

/// Starts the history for a counter that starts from 0, and waits while the storage task reads back what the log
/// holds, even if the chip never starts.
#[inline(never)]
async fn start_history() {
    HISTORY.lock(|history| history.borrow_mut().start(local_time_now()));
    storage_requests::REQUESTS.send(StorageRequest::LoadSteps).await;
    storage_requests::STEPS_LOADED.wait().await;
    system_values::changed(&[SystemValue::StepsToday]);
}

/// Adds a reading of the counter, and asks the storage task to write the slot it ended to the log.
#[inline(never)]
async fn record_steps(count: u32) {
    let recorded = HISTORY.lock(|history| history.borrow_mut().record(local_time_now(), count));
    let worth_storing = recorded.finished.is_some() || recorded.day_change == DayChange::Jump;
    if worth_storing {
        storage_requests::REQUESTS
            .send(StorageRequest::StoreSteps(recorded))
            .await;
    }
    if recorded.changed {
        system_values::changed(&[SystemValue::StepsToday]);
    }
}

#[inline(never)]
async fn reset_trip_now() {
    HISTORY.lock(|history| history.borrow_mut().set_trip_steps(0));
    storage_requests::REQUESTS.send(StorageRequest::SaveTrip(0)).await;
    system_values::changed(&[SystemValue::StepTrip]);
}

// ============================================================================
// What the storage task reads and writes
// ============================================================================

/// The day being recorded, counted from 1970-01-01 in local time.
pub fn recording_day() -> i64 {
    HISTORY.lock(|history| history.borrow().recording().day)
}

pub fn trip_steps() -> u32 {
    HISTORY.lock(|history| history.borrow().trip_steps())
}

pub fn set_trip_steps(steps: u32) {
    HISTORY.lock(|history| history.borrow_mut().set_trip_steps(steps));
}

/// Sets one remembered day's hours, read back from the log.
pub fn set_day(days_back: usize, hours: [u16; HOURS_PER_DAY]) {
    HISTORY.lock(|history| history.borrow_mut().set_day(days_back, hours));
}

// ============================================================================
// The task
// ============================================================================

#[embassy_executor::task]
pub async fn run_accelerometer(i2c_device: I2cDeviceOnBus) {
    crate::tasks::name_this_task("motion");
    start_history().await;
    touch::STARTED.wait().await;
    let mut accelerometer = Accelerometer::new(i2c_device);
    match accelerometer
        .start(Settings::DEFAULT, &mut Delay, configurations())
        .await
    {
        Ok(chip_model) => crate::about::note_accelerometer_chip(chip_model.chip_id()),
        // The library comes only with a firmware update, so a build without one never counts steps.
        Err(Error::NoConfiguration { chip_model }) => {
            crate::about::note_accelerometer_chip(chip_model.chip_id());
            diagnostics::set(Counter::AccelerometerState, 4);
            return;
        }
        Err(_) => {
            diagnostics::set(Counter::AccelerometerState, 2);
            return;
        }
    }
    diagnostics::set(Counter::AccelerometerState, 1);
    let mut shakes = ShakeDetector::new();
    let mut wake_gestures = WakeGestureDetector::new();
    let mut next_step_read = Instant::now();
    let mut next_acceleration_read = Instant::now();
    loop {
        let now = Instant::now();
        if now >= next_step_read {
            match accelerometer.read_step_count().await {
                Ok(count) => {
                    foreground::wait_for_a_pause().await;
                    record_steps(count).await
                }
                Err(_) => diagnostics::set(Counter::AccelerometerState, 3),
            }
            next_step_read = now + STEP_READ_INTERVAL;
        }

        let app_period = READING_PERIOD_IN_MILLISECONDS.load(Ordering::Relaxed);
        let gesture_period = match unpack_watching(WATCHING.load(Ordering::Relaxed)).anything() {
            true => WAKE_GESTURE_PERIOD_IN_MILLISECONDS,
            false => 0,
        };
        let period = match (app_period, gesture_period) {
            (0, other) | (other, 0) => other,
            (app, gesture) => app.min(gesture),
        };
        if app_period == 0 {
            shakes.reset();
        }
        if period == 0 {
            wake_gestures.reset();
        } else if now >= next_acceleration_read {
            if let Ok(reading) = accelerometer.read_acceleration().await {
                let app_wants_readings = app_period != 0;
                take_reading(
                    reading,
                    now.as_millis(),
                    app_wants_readings,
                    &mut shakes,
                    &mut wake_gestures,
                );
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
            Either3::Third(()) => reset_trip_now().await,
        }
    }
}
