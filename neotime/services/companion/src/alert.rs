//! The standard Alert Notification service's New Alert, read the way InfiniTime reads it.
//!
//! Any Bluetooth tool, such as BlueZ's `bluetoothctl` or ITD, can write one to show a notification.
//! A write is a category byte, two more header bytes, then the text: a title, a zero byte and the
//! body, or only a body when there is no zero byte.

use crate::PhoneNotification;

/// The category, the count of new alerts, and the byte InfiniTime's writers leave before the text.
pub const NEW_ALERT_HEADER_BYTES: usize = 3;

/// The longest New Alert the watch takes: the most an ATT write holds at its 128-byte MTU.
pub const MOST_NEW_ALERT_BYTES: usize = 125;

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

/// The notification a New Alert write carries, with this id; none when it holds no text.
pub fn parse_new_alert(write: &[u8], id: i64) -> Option<PhoneNotification> {
    let (&category, _) = write.split_first()?;
    let text = whole_characters(write.get(NEW_ALERT_HEADER_BYTES..)?).trim_end_matches('\0');
    if text.is_empty() {
        return None;
    }
    let (title, body) = text.split_once('\0').unwrap_or(("", text));
    let source = CATEGORY_NAMES
        .get(usize::from(category))
        .copied()
        .unwrap_or("Alert");
    Some(PhoneNotification {
        id,
        source: cut_to_fit(source),
        title: cut_to_fit(title),
        body: cut_to_fit(body),
    })
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
