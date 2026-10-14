//! The standard Alert Notification service's New Alert, written to the watch or notified by a phone.
//!
//! Any Bluetooth tool, such as BlueZ's `bluetoothctl` or ITD, can write one to show a notification.
//! A write is read the way InfiniTime reads it: a category byte, two more header bytes, then the
//! text. A phone's own service notifies the standard's layout instead: a category byte, a count
//! byte, then the text. Either text is a title, a zero byte and the body, or only a body when there
//! is no zero byte. An incoming call opens the call screen instead.

use crate::{CallKind, Message, PhoneCall, PhoneNotification};

/// The category, the count of new alerts, and the byte InfiniTime's writers leave before the text.
pub const NEW_ALERT_HEADER_BYTES: usize = 3;

/// The category and the count of new alerts, as the standard lays a phone's New Alert out.
pub const PHONE_NEW_ALERT_HEADER_BYTES: usize = 2;

/// The longest New Alert the watch takes: the most an ATT write holds at its 128-byte MTU.
pub const MOST_NEW_ALERT_BYTES: usize = 125;

/// The standard's category id for an incoming call.
const INCOMING_CALL_CATEGORY: u8 = 3;

/// The standard alert category ids, each named as the notification's app.
const CATEGORY_NAMES: [&str; 10] = [
    "Alert",
    "Email",
    "News",
    "Call",
    "Missed call",
    "SMS",
    "Voicemail",
    "Schedule",
    "Alert",
    "Messages",
];

/// The call or notification a New Alert write carries, a notification taking this id; none when it holds no text.
pub fn parse_new_alert(write: &[u8], id: i64) -> Option<Message<'static>> {
    parse_alert(write, NEW_ALERT_HEADER_BYTES, id)
}

/// The call or notification a phone's New Alert notification carries, read as the standard lays it out.
pub fn parse_phone_new_alert(notification: &[u8], id: i64) -> Option<Message<'static>> {
    parse_alert(notification, PHONE_NEW_ALERT_HEADER_BYTES, id)
}

/// Reads the category byte, skips the rest of the header and splits the text into a title and a body.
fn parse_alert(alert: &[u8], header_bytes: usize, id: i64) -> Option<Message<'static>> {
    let (&category, _) = alert.split_first()?;
    let text = whole_characters(alert.get(header_bytes..)?).trim_end_matches('\0');
    if text.is_empty() {
        return None;
    }
    let (title, body) = text.split_once('\0').unwrap_or(("", text));
    if category == INCOMING_CALL_CATEGORY {
        let caller = if title.is_empty() { body } else { title };
        return Some(Message::Call(PhoneCall {
            kind: CallKind::Incoming,
            caller: cut_to_fit(caller),
        }));
    }
    let source = CATEGORY_NAMES
        .get(usize::from(category))
        .copied()
        .unwrap_or("Alert");
    Some(Message::Notification(PhoneNotification {
        id,
        source: cut_to_fit(source),
        title: cut_to_fit(title),
        body: cut_to_fit(body),
    }))
}

/// The text up to its first byte that is not part of a whole UTF-8 character.
fn whole_characters(bytes: &[u8]) -> &str {
    match core::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => core::str::from_utf8(&bytes[..error.valid_up_to()]).unwrap_or(""),
    }
}

/// Text cut at the last whole character that fits.
fn cut_to_fit<const BYTES: usize>(text: &str) -> heapless::String<BYTES> {
    let mut end = text.len().min(BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut cut = heapless::String::new();
    let _ = cut.push_str(&text[..end]);
    cut
}
