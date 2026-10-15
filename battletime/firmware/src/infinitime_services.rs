//! InfiniTime's own GATT services, so Gadgetbridge's InfiniTime support and other InfiniTime phone
//! apps work with BattleTime: Simple Weather, Music, Navigation, Immediate Alert and Motion.
//!
//! Writes are read by `battletime_companion` into the same phone messages the companion's lines give,
//! so the shell keeps them and the music, navigation and weather apps show them. The watch notifies
//! the music app's controls, the step count, and raw acceleration while a phone subscribes to it.
//! Each service comes with a cargo feature, since flash is short; the recovery firmware serves none
//! of them, and a build without a service has versions of these functions that do nothing.

#[cfg(motion_service)]
use battletime_accelerometer::Acceleration;
use battletime_runtime::{EffectKind, Value};
#[cfg(all(motion_service, not(music_service)))]
use embassy_futures::select::{select, Either};
#[cfg(all(music_service, motion_service))]
use embassy_futures::select::{select3, Either3};
#[cfg(music_service)]
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use trouble_host::prelude::*;

use crate::Server;

// ============================================================================
// Notifications
// ============================================================================

/// Music events waiting for the Bluetooth task, so a few quick presses all reach the phone.
#[cfg(music_service)]
static MUSIC_EVENTS: Channel<CriticalSectionRawMutex, u8, 4> = Channel::new();

/// Something to notify a subscribed phone of.
pub enum Notification {
    #[cfg(music_service)]
    MusicEvent(u8),
    #[cfg(motion_service)]
    Steps,
    #[cfg(motion_service)]
    Motion(Acceleration),
}

/// Queues the music event for an app's `music_control` or `request_music_state`, while a phone is connected.
#[cfg_attr(not(music_service), allow(unused_variables))]
pub fn send_music_event(kind: EffectKind, command: Value) {
    #[cfg(music_service)]
    if let (true, Some(event)) = (
        crate::phone::is_connected(),
        battletime_companion::music_event(kind, command),
    ) {
        let _ = MUSIC_EVENTS.try_send(event);
    }
}

/// Waits for the next music event, step count or acceleration reading to notify.
pub async fn next_notification() -> Notification {
    #[cfg(all(music_service, motion_service))]
    return match select3(
        MUSIC_EVENTS.receive(),
        crate::motion::STEPS_CHANGED_FOR_PHONE.wait(),
        crate::motion::READING_FOR_PHONE.wait(),
    )
    .await
    {
        Either3::First(event) => Notification::MusicEvent(event),
        Either3::Second(()) => Notification::Steps,
        Either3::Third(reading) => Notification::Motion(reading),
    };
    #[cfg(all(music_service, not(motion_service)))]
    return Notification::MusicEvent(MUSIC_EVENTS.receive().await);
    #[cfg(all(motion_service, not(music_service)))]
    return match select(
        crate::motion::STEPS_CHANGED_FOR_PHONE.wait(),
        crate::motion::READING_FOR_PHONE.wait(),
    )
    .await
    {
        Either::First(()) => Notification::Steps,
        Either::Second(reading) => Notification::Motion(reading),
    };
    #[cfg(not(any(music_service, motion_service)))]
    core::future::pending().await
}

// ============================================================================
// The services
// ============================================================================

/// InfiniTime's Simple Weather service: one characteristic taking the current weather or the forecast.
#[cfg(weather_service)]
#[gatt_service(uuid = "00050000-78fc-48fe-8e23-433b3a1942d0")]
pub struct SimpleWeatherService {
    /// Writes are read as they arrive and never stored, so one byte of storage takes any length.
    #[characteristic(uuid = "00050001-78fc-48fe-8e23-433b3a1942d0", write)]
    pub data: u8,
}

/// InfiniTime's music service: the phone writes the player's state, and the watch notifies its controls.
///
/// The album, track number and total, playback speed, repeat and shuffle are left out, since the
/// watch keeps none of them; Gadgetbridge skips a characteristic the watch does not serve.
#[cfg(music_service)]
#[gatt_service(uuid = "00000000-78fc-48fe-8e23-433b3a1942d0")]
pub struct MusicService {
    #[characteristic(uuid = "00000001-78fc-48fe-8e23-433b3a1942d0", notify)]
    pub event: u8,
    #[characteristic(uuid = "00000002-78fc-48fe-8e23-433b3a1942d0", write)]
    pub status: u8,
    #[characteristic(uuid = "00000003-78fc-48fe-8e23-433b3a1942d0", write)]
    pub artist: u8,
    #[characteristic(uuid = "00000004-78fc-48fe-8e23-433b3a1942d0", write)]
    pub track: u8,
    #[characteristic(uuid = "00000006-78fc-48fe-8e23-433b3a1942d0", write)]
    pub position: u8,
    #[characteristic(uuid = "00000007-78fc-48fe-8e23-433b3a1942d0", write)]
    pub total_length: u8,
}

/// InfiniTime's navigation service: the next turn's icon name, instruction, distance and the route's progress.
#[cfg(navigation_service)]
#[gatt_service(uuid = "00010000-78fc-48fe-8e23-433b3a1942d0")]
pub struct NavigationService {
    #[characteristic(uuid = "00010001-78fc-48fe-8e23-433b3a1942d0", write)]
    pub flag: u8,
    #[characteristic(uuid = "00010002-78fc-48fe-8e23-433b3a1942d0", write)]
    pub narrative: u8,
    #[characteristic(uuid = "00010003-78fc-48fe-8e23-433b3a1942d0", write)]
    pub manoeuvre_distance: u8,
    #[characteristic(uuid = "00010004-78fc-48fe-8e23-433b3a1942d0", write)]
    pub progress: u8,
}

/// The standard Immediate Alert service, which phones use to find the watch.
#[cfg(alert_service)]
#[gatt_service(uuid = "1802")]
pub struct ImmediateAlertService {
    #[characteristic(uuid = "2a06", write, write_without_response)]
    pub alert_level: u8,
}

/// InfiniTime's motion service: today's steps, and the latest acceleration while a phone subscribes to it.
#[cfg(motion_service)]
#[gatt_service(uuid = "00030000-78fc-48fe-8e23-433b3a1942d0")]
pub struct MotionService {
    #[characteristic(uuid = "00030001-78fc-48fe-8e23-433b3a1942d0", read, notify)]
    pub step_count: [u8; 4],
    #[characteristic(uuid = "00030002-78fc-48fe-8e23-433b3a1942d0", read, notify)]
    pub values: [u8; 6],
}

// ============================================================================
// Writes
// ============================================================================

/// A write-only characteristic takes two handles, its declaration and its value, so a run of them steps by two.
#[cfg(any(music_service, navigation_service))]
const HANDLES_PER_CHARACTERISTIC: u16 = 2;

/// The UUID numbers of the music characteristics the phone writes, in the order the table holds them.
#[cfg(music_service)]
const MUSIC_NUMBERS: [u8; 5] = [0x02, 0x03, 0x04, 0x06, 0x07];

/// The UUID numbers of the navigation characteristics, in the order the table holds them.
#[cfg(navigation_service)]
const NAVIGATION_NUMBERS: [u8; 4] = [0x01, 0x02, 0x03, 0x04];

#[cfg(weather_service)]
const MILLISECONDS_PER_DAY: Value = 24 * 60 * 60 * 1000;

/// The UUID number of the characteristic whose value has this handle, in a run of them starting at `first`.
#[cfg(any(music_service, navigation_service))]
fn number_in_run(first: u16, numbers: &[u8], handle: u16) -> Option<u8> {
    let offset = handle.checked_sub(first)?;
    if offset % HANDLES_PER_CHARACTERISTIC != 0 {
        return None;
    }
    numbers
        .get(usize::from(offset / HANDLES_PER_CHARACTERISTIC))
        .copied()
}

/// Reads a write to one of these services and passes what it carries to the shell; false for any other handle.
#[cfg_attr(
    not(any(weather_service, music_service, navigation_service, alert_service)),
    allow(unused_variables, unreachable_code, unused_labels)
)]
pub fn take_write(server: &Server<'_>, handle: u16, write: &[u8]) -> bool {
    let taken = 'message: {
        #[cfg(weather_service)]
        if handle == server.simple_weather.data.handle {
            break 'message Some(battletime_companion::simple_weather_write(
                write,
                local_midnight(),
            ));
        }
        #[cfg(music_service)]
        if let Some(number) = number_in_run(server.music.status.handle, &MUSIC_NUMBERS, handle) {
            break 'message Some(battletime_companion::music_write(number, write));
        }
        #[cfg(navigation_service)]
        if let Some(number) = number_in_run(server.navigation.flag.handle, &NAVIGATION_NUMBERS, handle) {
            break 'message Some(battletime_companion::navigation_write(number, write));
        }
        #[cfg(alert_service)]
        if handle == server.immediate_alert.alert_level.handle {
            break 'message Some(
                write
                    .first()
                    .map(|&level| battletime_companion::Message::Find { is_on: level != 0 }),
            );
        }
        None
    };
    let Some(message) = taken else {
        return false;
    };
    if let Some(message) = message {
        crate::app_events::phone_message_arrived(message);
    }
    true
}

/// Today's local midnight, in milliseconds since 1970, which the weather's sunrise and sunset count from.
#[cfg(weather_service)]
fn local_midnight() -> Value {
    let local_now = crate::clock::now();
    local_now - local_now.rem_euclid(MILLISECONDS_PER_DAY) - crate::clock::utc_offset()
}

// ============================================================================
// Reads and notifications
// ============================================================================

/// Fills in the step count before a phone reads it.
#[cfg_attr(not(motion_service), allow(unused_variables))]
pub fn fill_read(server: &Server<'_>, handle: u16) {
    #[cfg(motion_service)]
    if handle == server.motion.step_count.handle {
        let steps = crate::motion::steps_today().to_le_bytes();
        let _ = server.set(&server.motion.step_count, &steps);
    }
}

/// Notifies a subscribed phone, keeping the value for later reads.
#[cfg_attr(not(any(music_service, motion_service)), allow(unused_variables))]
pub async fn notify(
    connection: &GattConnection<'_, '_, DefaultPacketPool>,
    server: &Server<'_>,
    notification: Notification,
) {
    #[cfg(any(music_service, motion_service))]
    {
        let mut value = [0; 6];
        let (characteristic, length) = match notification {
            #[cfg(music_service)]
            Notification::MusicEvent(event) => {
                value[0] = event;
                (server.music.event.to_raw(), 1)
            }
            #[cfg(motion_service)]
            Notification::Steps => {
                value[..4].copy_from_slice(&crate::motion::steps_today().to_le_bytes());
                (server.motion.step_count.to_raw(), 4)
            }
            #[cfg(motion_service)]
            Notification::Motion(reading) => {
                value = battletime_companion::motion_values(
                    reading.x_in_milli_g.into(),
                    reading.y_in_milli_g.into(),
                    reading.z_in_milli_g.into(),
                );
                (server.motion.values.to_raw(), 6)
            }
        };
        let _ = crate::notify_bytes(connection, &characteristic, &value[..length], true).await;
    }
    #[cfg(not(any(music_service, motion_service)))]
    match notification {}
}

/// Reads acceleration for the phone while it subscribes to the motion values, and stops when it goes.
#[cfg_attr(not(motion_service), allow(unused_variables))]
pub fn note_subscriptions(
    server: &Server<'_>,
    connection: Option<&GattConnection<'_, '_, DefaultPacketPool>>,
) {
    #[cfg(motion_service)]
    crate::motion::send_readings_to_phone(
        connection.is_some_and(|connection| server.motion.values.should_notify(connection)),
    );
}
