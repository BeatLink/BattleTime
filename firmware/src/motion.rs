//! The accelerometer: today's steps, from the chip's own step counter, reset at midnight, and
//! acceleration readings with the shakes found in them, taken only while the open app wants them.

use core::sync::atomic::{AtomicU32, Ordering};

use embassy_futures::select::{select, Either};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Delay, Duration, Instant, Timer};
use neotime_accelerometer::{shake::ShakeDetector, Accelerometer, Settings};
use neotime_runtime::{SystemValue, Value};

use crate::{
    app_events, clock,
    diagnostics::{self, Counter},
    system_values, touch, I2cDeviceOnBus,
};

static STEPS_TODAY: AtomicU32 = AtomicU32::new(0);

/// How often acceleration is read, in milliseconds, or 0 while no app wants readings.
static READING_PERIOD_IN_MILLISECONDS: AtomicU32 = AtomicU32::new(0);
static READING_PERIOD_CHANGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

const STEP_READ_INTERVAL: Duration = Duration::from_secs(30);
const MILLISECONDS_PER_DAY: i64 = 86_400_000;

pub fn steps_today() -> u32 {
    STEPS_TODAY.load(Ordering::Relaxed)
}

/// Starts or stops reading acceleration; `None` stops it, so the chip is read only for steps.
pub fn read_acceleration_every(period_in_milliseconds: Option<Value>) {
    let period = period_in_milliseconds.map_or(0, |period| period.clamp(1, u32::MAX as Value) as u32);
    if READING_PERIOD_IN_MILLISECONDS.swap(period, Ordering::Relaxed) != period {
        READING_PERIOD_CHANGED.signal(());
    }
}

#[embassy_executor::task]
pub async fn run_accelerometer(i2c_device: I2cDeviceOnBus) {
    touch::STARTED.wait().await;
    let mut accelerometer = Accelerometer::new(i2c_device);
    if accelerometer.start(Settings::DEFAULT, &mut Delay).await.is_err() {
        diagnostics::set(Counter::AccelerometerState, 2);
        return;
    }
    diagnostics::set(Counter::AccelerometerState, 1);
    let mut day = clock::now().div_euclid(MILLISECONDS_PER_DAY);
    let mut shakes = ShakeDetector::new();
    let mut next_step_read = Instant::now();
    let mut next_acceleration_read = Instant::now();
    loop {
        let now = Instant::now();
        if now >= next_step_read {
            let today = clock::now().div_euclid(MILLISECONDS_PER_DAY);
            if today != day {
                day = today;
                let _ = accelerometer.reset_step_count(&mut Delay).await;
            }
            let reading = accelerometer.read_step_count().await;
            if reading.is_err() {
                diagnostics::set(Counter::AccelerometerState, 3);
            }
            if let Ok(steps) = reading {
                if STEPS_TODAY.swap(steps, Ordering::Relaxed) != steps {
                    system_values::changed(&[SystemValue::StepsToday]);
                }
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
        if let Either::Second(()) = select(Timer::at(wake_at), READING_PERIOD_CHANGED.wait()).await {
            next_acceleration_read = Instant::now();
        }
    }
}
