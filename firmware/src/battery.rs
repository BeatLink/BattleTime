//! The battery: its charge from the voltage on P0.31, and whether it is charging from P0.12 and P0.19.

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use embassy_nrf::{
    gpio::Input,
    saadc::{ChannelConfig, Config, Saadc},
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};

/// Signalled when the charge or the charging state changes, so the shell redraws what shows them.
pub static CHANGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

static PERCENT: AtomicU8 = AtomicU8::new(0);
static IS_CHARGING: AtomicBool = AtomicBool::new(false);

const READ_INTERVAL: Duration = Duration::from_secs(60);

/// The SAADC's full scale with gain 1/6 and the 0.6 V reference, in millivolts.
const FULL_SCALE_IN_MILLIVOLTS: i32 = 3600;
const FULL_SCALE_READING: i32 = 4095;
/// The board halves the battery's voltage before the pin.
const VOLTAGE_DIVIDER: i32 = 2;

/// A lithium cell's charge at each voltage under a light load, in millivolts and percent.
const DISCHARGE_CURVE: [(i32, i32); 10] = [
    (3500, 0),
    (3600, 10),
    (3700, 30),
    (3750, 45),
    (3800, 55),
    (3850, 65),
    (3900, 75),
    (4000, 88),
    (4100, 96),
    (4180, 100),
];

pub fn percent() -> u8 {
    PERCENT.load(Ordering::Relaxed)
}

pub fn is_charging() -> bool {
    IS_CHARGING.load(Ordering::Relaxed)
}

/// The charge at a voltage, read along the discharge curve.
fn percent_at(millivolts: i32) -> u8 {
    let (first_millivolts, _) = DISCHARGE_CURVE[0];
    if millivolts <= first_millivolts {
        return 0;
    }
    for pair in DISCHARGE_CURVE.windows(2) {
        let [(low_millivolts, low_percent), (high_millivolts, high_percent)] = [pair[0], pair[1]];
        if millivolts <= high_millivolts {
            let percent = low_percent
                + (high_percent - low_percent) * (millivolts - low_millivolts) / (high_millivolts - low_millivolts);
            return percent as u8;
        }
    }
    100
}

/// Reads the battery at start and then once a minute, or at once when power is plugged in or out.
#[embassy_executor::task]
pub async fn watch_battery(
    mut adc: Saadc<'static, 1>,
    mut charging_pin: Input<'static>,
    mut power_present_pin: Input<'static>,
) {
    adc.calibrate().await;
    loop {
        let mut reading = [0; 1];
        adc.sample(&mut reading).await;
        let millivolts = reading[0].max(0) as i32 * FULL_SCALE_IN_MILLIVOLTS * VOLTAGE_DIVIDER / FULL_SCALE_READING;
        let percent = percent_at(millivolts);
        let is_charging = power_present_pin.is_low() && charging_pin.is_low();

        let percent_changed = PERCENT.swap(percent, Ordering::Relaxed) != percent;
        let charging_changed = IS_CHARGING.swap(is_charging, Ordering::Relaxed) != is_charging;
        if percent_changed || charging_changed {
            CHANGED.signal(());
        }

        embassy_futures::select::select3(
            Timer::after(READ_INTERVAL),
            power_present_pin.wait_for_any_edge(),
            charging_pin.wait_for_any_edge(),
        )
        .await;
    }
}

/// Builds the converter for the battery pin.
pub fn battery_adc(
    saadc: embassy_nrf::Peri<'static, embassy_nrf::peripherals::SAADC>,
    battery_pin: embassy_nrf::Peri<'static, embassy_nrf::peripherals::P0_31>,
) -> Saadc<'static, 1> {
    Saadc::new(saadc, crate::Interrupts, Config::default(), [ChannelConfig::single_ended(battery_pin)])
}
