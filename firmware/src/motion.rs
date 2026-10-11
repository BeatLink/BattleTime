//! The accelerometer: today's steps, from the chip's own step counter, reset at midnight.

use core::sync::atomic::{AtomicU32, Ordering};

use embassy_time::{Delay, Duration, Timer};
use neotime_accelerometer::{Accelerometer, Settings};
use neotime_runtime::SystemValue;

use crate::{clock, system_values, I2cDeviceOnBus};

static STEPS_TODAY: AtomicU32 = AtomicU32::new(0);

const READ_INTERVAL: Duration = Duration::from_secs(30);
const MILLISECONDS_PER_DAY: i64 = 86_400_000;

pub fn steps_today() -> u32 {
    STEPS_TODAY.load(Ordering::Relaxed)
}

#[embassy_executor::task]
pub async fn count_steps(i2c_device: I2cDeviceOnBus) {
    let mut accelerometer = Accelerometer::new(i2c_device);
    if accelerometer.start(Settings::DEFAULT, &mut Delay).await.is_err() {
        return;
    }
    let mut day = clock::now().div_euclid(MILLISECONDS_PER_DAY);
    loop {
        let today = clock::now().div_euclid(MILLISECONDS_PER_DAY);
        if today != day {
            day = today;
            let _ = accelerometer.reset_step_count(&mut Delay).await;
        }
        if let Ok(steps) = accelerometer.read_step_count().await {
            if STEPS_TODAY.swap(steps, Ordering::Relaxed) != steps {
                system_values::changed(&[SystemValue::StepsToday]);
            }
        }
        Timer::after(READ_INTERVAL).await;
    }
}
