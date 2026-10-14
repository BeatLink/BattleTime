//! The phone's own services, which the watch uses as a GATT client, as InfiniTime's ServiceDiscovery does.
//!
//! A few seconds after a phone connects, the watch looks once for the phone's Current Time service
//! and sets the clock from it, then for its Alert Notification service and subscribes to New Alert.
//! A phone that serves neither is left alone. `neotime-phone-client` builds the requests and reads
//! the answers; this module carries them over the connection with a time limit on each. The watch
//! keeps reading what the phone sends for the rest of the connection, so a late answer never holds
//! one of the host's packets. docs/companion-protocol.md says what each service carries.

use alloc::boxed::Box;
use core::{
    future::{pending, Future},
    pin::Pin,
    task::{Context, Poll},
};

use embassy_time::{with_timeout, Duration, Timer};
use neotime_phone_client::{self as phone_client, Found, PhoneClient};
use trouble_host::prelude::*;

use crate::clock;

/// How long after connecting the watch waits, so the phone's own discovery of the watch goes first.
const DISCOVERY_DELAY: Duration = Duration::from_secs(3);

/// The most the watch waits for any one answer before it gives up on the phone's services.
const ANSWER_TIME_LIMIT: Duration = Duration::from_secs(5);

/// The heap the client needs free beyond its own size before it starts, so apps keep their room.
const HEAP_KEPT_FREE: usize = 2 * 1024;

/// Room for an answer or a New Alert notification: its opcode and handle, then the alert.
const RECEIVED_BYTES: usize = 3 + neotime_companion::MOST_NEW_ALERT_BYTES;

// ============================================================================
// Starting once per connection
// ============================================================================

/// Uses the phone's services for this connection, and never finishes, so the connection's own loop ends it.
pub async fn use_phone_services(connection: &Connection<'_, DefaultPacketPool>) {
    Timer::after(DISCOVERY_DELAY).await;
    let start_client = || run_client(connection);
    // Kept on the heap only while connected, since a static or the Bluetooth task's state would keep it for good.
    if crate::free_heap_bytes() >= size_of_result(&start_client) + HEAP_KEPT_FREE {
        let mut client: Pin<Box<_>> = Box::pin(start_client());
        core::future::poll_fn(|context| poll_client(client.as_mut(), context)).await;
    }
    pending::<()>().await;
}

/// Polls the client out of line, so its frame is on the stack only while it runs, never beside the connection's.
#[inline(never)]
fn poll_client(client: Pin<&mut impl Future<Output = ()>>, context: &mut Context<'_>) -> Poll<()> {
    client.poll(context)
}

/// The size of what a function returns, found without calling it.
fn size_of_result<Result>(_: &impl Fn() -> Result) -> usize {
    core::mem::size_of::<Result>()
}

/// Finds and uses the services, then reads what the phone sends until it goes away.
async fn run_client(connection: &Connection<'_, DefaultPacketPool>) {
    let mut client = PhoneClient::new();
    let mut received = [0; RECEIVED_BYTES];
    let mut length = 0;
    let mut clock_set = false;
    loop {
        let (found, request) = client.next(&received[..length]);
        match found {
            Some(Found::CurrentTime(current_time)) => clock_set = clock::set_from_current_time(current_time),
            // The zone moves the time a phone just set, so it is taken only with one.
            Some(Found::LocalTimeInformation(zone)) if clock_set => {
                clock::set_local_time_information(zone);
            }
            _ => {}
        }
        let Some(request) = request else {
            break;
        };
        let Some(answer_length) = ask(connection, request, &mut received).await else {
            break;
        };
        length = answer_length;
    }
    if clock_set {
        clock::save_time_zone_if_changed().await;
        clock::SET_FROM_PHONE_SERVICE.signal(());
    }
    while let Some(length) = connection.receive_att_from_server(&mut received).await {
        let new_alert = client.new_alert();
        if let Some(alert) =
            new_alert.and_then(|handle| phone_client::notified_value(&received[..length], handle))
        {
            crate::read_new_alert(alert, neotime_companion::parse_phone_new_alert);
        }
    }
}

/// Sends one request and waits for its answer, passing over anything sent unasked, since nothing is subscribed yet.
async fn ask(
    connection: &Connection<'_, DefaultPacketPool>,
    request: &[u8],
    answer: &mut [u8; RECEIVED_BYTES],
) -> Option<usize> {
    connection.send_att(request).await.ok()?;
    // A late answer would be taken for the next request's, so one that misses the limit ends the asking.
    with_timeout(ANSWER_TIME_LIMIT, async {
        loop {
            let length = connection.receive_att_from_server(answer).await?;
            if !phone_client::is_unasked(&answer[..length]) {
                return Some(length);
            }
        }
    })
    .await
    .ok()
    .flatten()
}
