//! The battery: its voltage on P0.31, and whether power is present and charging on P0.19 and P0.12.
//!
//! `neotime_battery::Battery` turns each reading into the steady charge the watch shows; this task
//! takes the readings, keeps what apps read, and passes the events on to the shell.

use core::sync::atomic::{AtomicBool, AtomicU16, AtomicU8, Ordering};

use embassy_nrf::{
    gpio::Input,
    saadc::{ChannelConfig, Config, Saadc},
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal};
use embassy_time::{Duration, Timer};
use neotime_battery::{millivolts_from_reading, Battery, BatteryEvent, Measurement};
use neotime_runtime::SystemValue;

use crate::system_values;

static PERCENT: AtomicU8 = AtomicU8::new(0);
static IS_CHARGING: AtomicBool = AtomicBool::new(false);
static MILLIVOLTS: AtomicU16 = AtomicU16::new(0);

/// Raised when the charge changes, so the Battery service can notify a phone.
pub static PERCENT_CHANGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Events for the shell, such as the charger being plugged in; it takes them when the battery values change.
pub static EVENTS: Channel<CriticalSectionRawMutex, BatteryEvent, 4> = Channel::new();

const READ_INTERVAL: Duration = Duration::from_secs(60);

/// How long the charger's pins are left to settle after one of them changes, before they are read.
const SETTLE_TIME: Duration = Duration::from_millis(500);

pub fn percent() -> u8 {
    PERCENT.load(Ordering::Relaxed)
}

pub fn is_charging() -> bool {
    IS_CHARGING.load(Ordering::Relaxed)
}

pub fn millivolts() -> u16 {
    MILLIVOLTS.load(Ordering::Relaxed)
}

/// Reads the battery at start and then once a minute, or soon after power is plugged in or out.
#[embassy_executor::task]
pub async fn watch_battery(
    mut adc: Saadc<'static, 1>,
    mut charging_pin: Input<'static>,
    mut power_present_pin: Input<'static>,
) {
    adc.calibrate().await;
    let mut battery = Battery::new();
    loop {
        let mut reading = [0; 1];
        adc.sample(&mut reading).await;
        let changes = battery.measure(Measurement {
            millivolts: millivolts_from_reading(reading[0]),
            is_power_present: power_present_pin.is_low(),
            is_charge_pin_active: charging_pin.is_low(),
        });

        let percent_changed = PERCENT.swap(battery.percent(), Ordering::Relaxed) != battery.percent();
        IS_CHARGING.store(battery.is_charging(), Ordering::Relaxed);
        MILLIVOLTS.store(
            battery.millivolts().clamp(0, u16::MAX as i32) as u16,
            Ordering::Relaxed,
        );
        for event in changes.events.iter() {
            let _ = EVENTS.try_send(event);
        }
        if changes.charge_changed || changes.millivolts_changed || !changes.events.is_empty() {
            system_values::changed(&[SystemValue::BatteryPercent, SystemValue::IsCharging]);
        }
        if percent_changed {
            PERCENT_CHANGED.signal(());
        }

        let woke = embassy_futures::select::select3(
            Timer::after(READ_INTERVAL),
            power_present_pin.wait_for_any_edge(),
            charging_pin.wait_for_any_edge(),
        )
        .await;
        if !matches!(woke, embassy_futures::select::Either3::First(())) {
            Timer::after(SETTLE_TIME).await;
        }
    }
}

/// Builds the converter for the battery pin.
pub fn battery_adc(
    saadc: embassy_nrf::Peri<'static, embassy_nrf::peripherals::SAADC>,
    battery_pin: embassy_nrf::Peri<'static, embassy_nrf::peripherals::P0_31>,
) -> Saadc<'static, 1> {
    Saadc::new(
        saadc,
        crate::Interrupts,
        Config::default(),
        [ChannelConfig::single_ended(battery_pin)],
    )
}
