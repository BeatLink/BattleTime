//! Sensor readings and phone messages for the open app, passed from the tasks that notice them to the shell.
//!
//! Each kind keeps only its latest, so a slow shell never falls behind: the newest acceleration
//! reading, whether a shake happened, and the newest notification from the phone, which waits on
//! the heap rather than in static RAM. The phone's calls, music, weather, find and vibrate
//! messages wait together in one `PhoneUpdates`, again only the latest of each.

extern crate alloc;

use alloc::boxed::Box;
use core::cell::RefCell;

use embassy_futures::select::{select4, Either4};
use embassy_sync::{
    blocking_mutex::{raw::CriticalSectionRawMutex, Mutex},
    signal::Signal,
};
use neotime_accelerometer::Acceleration;
use neotime_companion::{Message, PhoneNotification, PhoneUpdates};

static ACCELERATION: Signal<CriticalSectionRawMutex, Acceleration> = Signal::new();
static SHAKES: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static NOTIFICATIONS: Signal<CriticalSectionRawMutex, Box<PhoneNotification>> = Signal::new();
static PHONE_UPDATES: Mutex<CriticalSectionRawMutex, RefCell<PhoneUpdates>> =
    Mutex::new(RefCell::new(PhoneUpdates::new()));
static PHONE_UPDATED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

pub enum AppEvent {
    Acceleration(Acceleration),
    Shake,
    Notification(Box<PhoneNotification>),
    Phone(PhoneUpdates),
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

/// Keeps a call, music, weather, find or vibrate message for the shell; builds without the shell drop it.
pub fn phone_message_arrived(message: Message<'_>) {
    if cfg!(with_shell) && PHONE_UPDATES.lock(|updates| updates.borrow_mut().record(message)) {
        PHONE_UPDATED.signal(());
    }
}

/// Waits for the next event, a notification before the phone's updates before a shake before a reading.
pub async fn next() -> AppEvent {
    let phone_updated = async {
        PHONE_UPDATED.wait().await;
        PHONE_UPDATES.lock(|updates| core::mem::take(&mut *updates.borrow_mut()))
    };
    match select4(
        NOTIFICATIONS.wait(),
        phone_updated,
        SHAKES.wait(),
        ACCELERATION.wait(),
    )
    .await
    {
        Either4::First(notification) => AppEvent::Notification(notification),
        Either4::Second(updates) => AppEvent::Phone(updates),
        Either4::Third(()) => AppEvent::Shake,
        Either4::Fourth(reading) => AppEvent::Acceleration(reading),
    }
}
