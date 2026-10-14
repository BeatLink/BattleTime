//! Sensor readings and phone messages for the open app, passed from the tasks that notice them to the shell.
//!
//! Each kind keeps only its latest, so a slow shell never falls behind: the newest acceleration
//! reading, whether a shake happened, the newest wake gesture, and the newest notification from the phone, which waits on
//! the heap rather than in static RAM. The phone's calls, music, weather, find and vibrate
//! messages wait together in one `PhoneUpdates`, again only the latest of each. Dismissals from
//! the phone queue up a few deep instead, so two close together both arrive.

extern crate alloc;

use alloc::boxed::Box;
use core::cell::RefCell;

use embassy_futures::select::{select, select4, Either, Either4};
use embassy_sync::{
    blocking_mutex::{raw::CriticalSectionRawMutex, Mutex},
    channel::Channel,
    signal::Signal,
};
use neotime_accelerometer::{wake_gestures::WakeGesture, Acceleration};
use neotime_companion::{Message, PhoneNotification, PhoneUpdates};

static ACCELERATION: Signal<CriticalSectionRawMutex, Acceleration> = Signal::new();
static SHAKES: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static WAKE_GESTURES: Signal<CriticalSectionRawMutex, WakeGesture> = Signal::new();
static NOTIFICATIONS: Signal<CriticalSectionRawMutex, Box<PhoneNotification>> = Signal::new();
static PHONE_UPDATES: Mutex<CriticalSectionRawMutex, RefCell<PhoneUpdates>> =
    Mutex::new(RefCell::new(PhoneUpdates::new()));
static PHONE_UPDATED: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static DISMISSALS: Channel<CriticalSectionRawMutex, i64, 4> = Channel::new();

pub enum AppEvent {
    Acceleration(Acceleration),
    Shake,
    /// A raised or shaken wrist, or a lowered arm, which wakes the screen or puts it to sleep.
    WakeGesture(WakeGesture),
    Notification(Box<PhoneNotification>),
    Phone(PhoneUpdates),
    /// The phone dismissed the notification with this id.
    NotificationDismissed(i64),
}

pub fn acceleration_read(reading: Acceleration) {
    ACCELERATION.signal(reading);
}

pub fn shake_found() {
    SHAKES.signal(());
}

pub fn wake_gesture_found(gesture: WakeGesture) {
    WAKE_GESTURES.signal(gesture);
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

/// Passes a dismissal on to the shell; one that finds the queue full is dropped.
pub fn notification_dismissed(id: i64) {
    if cfg!(with_shell) {
        let _ = DISMISSALS.try_send(id);
    }
}

/// Waits for the next event: a notification, then the phone's updates, a dismissal, a shake, a wake gesture and a reading.
pub async fn next() -> AppEvent {
    let phone_updated = async {
        PHONE_UPDATED.wait().await;
        PHONE_UPDATES.lock(|updates| core::mem::take(&mut *updates.borrow_mut()))
    };
    match select4(
        NOTIFICATIONS.wait(),
        select(phone_updated, DISMISSALS.receive()),
        select(SHAKES.wait(), WAKE_GESTURES.wait()),
        ACCELERATION.wait(),
    )
    .await
    {
        Either4::First(notification) => AppEvent::Notification(notification),
        Either4::Second(Either::First(updates)) => AppEvent::Phone(updates),
        Either4::Second(Either::Second(id)) => AppEvent::NotificationDismissed(id),
        Either4::Third(Either::First(())) => AppEvent::Shake,
        Either4::Third(Either::Second(gesture)) => AppEvent::WakeGesture(gesture),
        Either4::Fourth(reading) => AppEvent::Acceleration(reading),
    }
}
