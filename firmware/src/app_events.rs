//! Sensor readings and phone messages for the open app, passed from the tasks that notice them to the shell.
//!
//! Each kind keeps only its latest, so a slow shell never falls behind: the newest acceleration
//! reading, whether a shake happened, and the newest notification from the phone, which waits on
//! the heap rather than in static RAM.

extern crate alloc;

use alloc::boxed::Box;

use embassy_futures::select::{select3, Either3};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use neotime_accelerometer::Acceleration;
use neotime_companion::PhoneNotification;

static ACCELERATION: Signal<CriticalSectionRawMutex, Acceleration> = Signal::new();
static SHAKES: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static NOTIFICATIONS: Signal<CriticalSectionRawMutex, Box<PhoneNotification>> = Signal::new();

pub enum AppEvent {
    Acceleration(Acceleration),
    Shake,
    Notification(Box<PhoneNotification>),
}

pub fn acceleration_read(reading: Acceleration) {
    ACCELERATION.signal(reading);
}

pub fn shake_found() {
    SHAKES.signal(());
}

/// Passes a notification on to the shell; builds without the shell drop it, since nothing would take it.
pub fn notification_arrived(notification: PhoneNotification) {
    if cfg!(with_shell) {
        NOTIFICATIONS.signal(Box::new(notification));
    }
}

/// Waits for the next event, a notification before a shake before a reading when several wait.
pub async fn next() -> AppEvent {
    match select3(NOTIFICATIONS.wait(), SHAKES.wait(), ACCELERATION.wait()).await {
        Either3::First(notification) => AppEvent::Notification(notification),
        Either3::Second(()) => AppEvent::Shake,
        Either3::Third(reading) => AppEvent::Acceleration(reading),
    }
}
