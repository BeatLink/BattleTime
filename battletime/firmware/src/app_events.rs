//! Sensor readings and phone messages for the open app, passed from the tasks that notice them to the shell.
//!
//! Each kind keeps only its latest, so a slow shell never falls behind: the newest acceleration
//! reading, whether a shake happened, the newest wake gesture, and the newest notification from the phone, which waits on
//! the heap rather than in static RAM. The phone's calls, music, weather, find and vibrate
//! messages wait together in one `PhoneUpdates`, again only the latest of each. Dismissals from
//! the phone queue up a few deep instead, so two close together both arrive, as do the apps whose
//! saved state the phone rewrote. The app whose state the phone is rewriting is held here at once, as
//! its message arrives, so the shell never saves it between the phone's read and write.

extern crate alloc;

use alloc::boxed::Box;
use core::cell::RefCell;

use battletime_accelerometer::{wake_gestures::WakeGesture, Acceleration};
use battletime_app_host::closed_apps::StateHold;
use battletime_companion::{Message, PhoneNotification, PhoneUpdates, StateStep};
use embassy_futures::select::{select, select3, select4, Either, Either3, Either4};
use embassy_sync::{
    blocking_mutex::{raw::CriticalSectionRawMutex, Mutex},
    channel::Channel,
    signal::Signal,
};
use embassy_time::{Duration, Instant};

static ACCELERATION: Signal<CriticalSectionRawMutex, Acceleration> = Signal::new();
static SHAKES: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static WAKE_GESTURES: Signal<CriticalSectionRawMutex, WakeGesture> = Signal::new();
static NOTIFICATIONS: Signal<CriticalSectionRawMutex, Box<PhoneNotification>> = Signal::new();
static PHONE_UPDATES: Mutex<CriticalSectionRawMutex, RefCell<PhoneUpdates>> =
    Mutex::new(RefCell::new(PhoneUpdates::new()));
static PHONE_UPDATED: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static DISMISSALS: Channel<CriticalSectionRawMutex, i64, 4> = Channel::new();
static STATES_WRITTEN: Channel<CriticalSectionRawMutex, (StateWritten, bool), 2> = Channel::new();
/// How long a hold lasts when the phone never lets go.
const HOLD_LASTS: Duration = Duration::from_secs(30);
static STATE_HOLD: Mutex<CriticalSectionRawMutex, RefCell<StateHold>> =
    Mutex::new(RefCell::new(StateHold::NONE));

/// The id of an app whose saved state the phone rewrote.
pub type StateWritten = heapless::String<{ battletime_companion::MOST_APP_ID_BYTES }>;

pub enum AppEvent {
    Acceleration(Acceleration),
    Shake,
    /// A raised or shaken wrist, or a lowered arm, which wakes the screen or puts it to sleep.
    WakeGesture(WakeGesture),
    Notification(Box<PhoneNotification>),
    Phone(PhoneUpdates),
    /// The phone dismissed the notification with this id.
    NotificationDismissed(i64),
    /// The phone let go of this app's saved state, true when it rewrote it.
    StateWritten(StateWritten, bool),
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

/// Holds or lets go of an app's state as the phone asked, and passes on the end of a hold; one that finds the queue full is dropped.
pub fn app_state_changed(app: &str, step: StateStep) {
    let (true, Ok(id)) = (cfg!(with_shell), StateWritten::try_from(app)) else {
        return;
    };
    let ends_at = (Instant::now() + HOLD_LASTS).as_ticks();
    with_state_hold(|hold| match step {
        StateStep::Hold => hold.hold(app, ends_at),
        StateStep::Written => hold.release(app, true),
        StateStep::Released => hold.release(app, false),
    });
    if step != StateStep::Hold {
        let _ = STATES_WRITTEN.try_send((id, step == StateStep::Written));
    }
}

/// The app whose state the phone holds, kept by the Bluetooth task and read by the shell.
pub fn with_state_hold<Answer>(use_hold: impl FnOnce(&mut StateHold) -> Answer) -> Answer {
    STATE_HOLD.lock(|hold| use_hold(&mut hold.borrow_mut()))
}

/// Waits for the next event: a notification, then the phone's updates, a dismissal, a shake, a wake gesture and a reading.
pub async fn next() -> AppEvent {
    let phone_updated = async {
        PHONE_UPDATED.wait().await;
        PHONE_UPDATES.lock(|updates| core::mem::take(&mut *updates.borrow_mut()))
    };
    match select4(
        NOTIFICATIONS.wait(),
        select3(phone_updated, DISMISSALS.receive(), STATES_WRITTEN.receive()),
        select(SHAKES.wait(), WAKE_GESTURES.wait()),
        ACCELERATION.wait(),
    )
    .await
    {
        Either4::First(notification) => AppEvent::Notification(notification),
        Either4::Second(Either3::First(updates)) => AppEvent::Phone(updates),
        Either4::Second(Either3::Second(id)) => AppEvent::NotificationDismissed(id),
        Either4::Second(Either3::Third((app, written))) => AppEvent::StateWritten(app, written),
        Either4::Third(Either::First(())) => AppEvent::Shake,
        Either4::Third(Either::Second(gesture)) => AppEvent::WakeGesture(gesture),
        Either4::Fourth(reading) => AppEvent::Acceleration(reading),
    }
}
