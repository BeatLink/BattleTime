extern crate std;

use std::vec::Vec;

use super::alert::*;
use crate::{CallKind, Message, PhoneCall, PhoneNotification};

fn new_alert(category: u8, text: &[u8]) -> Vec<u8> {
    let mut write = std::vec![category, 1, 0];
    write.extend_from_slice(text);
    write
}

/// The notification a write shows, failing the test when it shows a call or nothing.
fn notification_of(write: &[u8], id: i64) -> PhoneNotification {
    match parse_new_alert(write, id) {
        Some(Message::Notification(notification)) => notification,
        other => panic!("not a notification: {other:?}"),
    }
}

#[test]
fn a_title_and_body_split_at_the_zero_byte() {
    let notification = notification_of(&new_alert(5, b"Sam\0On my way"), 7);
    assert_eq!(notification.id, 7);
    assert_eq!(notification.source, "SMS");
    assert_eq!(notification.title, "Sam");
    assert_eq!(notification.body, "On my way");
}

#[test]
fn text_without_a_zero_byte_is_all_body() {
    let notification = notification_of(&new_alert(0, b"Hello"), 1);
    assert_eq!(notification.source, "Alert");
    assert_eq!(notification.title, "");
    assert_eq!(notification.body, "Hello");
}

#[test]
fn trailing_zero_bytes_are_dropped() {
    let notification = notification_of(&new_alert(9, b"Hi\0\0\0"), 1);
    assert_eq!(notification.source, "Messages");
    assert_eq!(notification.title, "");
    assert_eq!(notification.body, "Hi");
}

#[test]
fn a_write_without_text_shows_nothing() {
    assert_eq!(parse_new_alert(&[], 1), None);
    assert_eq!(parse_new_alert(&[3, 1], 1), None);
    assert_eq!(parse_new_alert(&[3, 1, 0], 1), None);
    assert_eq!(parse_new_alert(&new_alert(3, b"\0\0"), 1), None);
}

#[test]
fn an_unknown_category_is_a_plain_alert() {
    let notification = notification_of(&new_alert(200, b"x"), 1);
    assert_eq!(notification.source, "Alert");
}

#[test]
fn every_category_but_a_call_has_a_name() {
    let names: Vec<_> = (0..10)
        .filter(|&category| category != 3)
        .map(|category| notification_of(&new_alert(category, b"x"), 1).source)
        .collect();
    assert_eq!(
        names,
        [
            "Alert",
            "Email",
            "News",
            "Missed call",
            "SMS",
            "Voicemail",
            "Schedule",
            "Alert",
            "Messages"
        ]
    );
}

#[test]
fn a_broken_character_ends_the_text() {
    let notification = notification_of(&new_alert(0, b"Caf\xc3\xa9 \xc3"), 1);
    assert_eq!(notification.body, "Caf\u{e9} ");
}

#[test]
fn long_text_is_cut_at_a_whole_character() {
    let mut text = std::vec![b'T'; 31];
    text.extend_from_slice("\u{e9}".as_bytes());
    text.push(0);
    text.extend_from_slice(&[b'b'; MOST_NEW_ALERT_BYTES]);
    let notification = notification_of(&new_alert(0, &text), 1);
    assert_eq!(notification.title.len(), 31);
    assert_eq!(notification.body.len(), crate::NOTIFICATION_BODY_BYTES);
}

#[test]
fn an_incoming_call_opens_the_call_screen_with_the_title_as_caller() {
    assert_eq!(
        parse_new_alert(&new_alert(3, b"Sam\0+15550100"), 1),
        Some(Message::Call(PhoneCall {
            kind: CallKind::Incoming,
            caller: "Sam".try_into().unwrap(),
        }))
    );
}

#[test]
fn an_incoming_call_without_a_title_names_the_caller_from_the_body() {
    let Some(Message::Call(call)) = parse_new_alert(&new_alert(3, b"+15550100"), 1) else {
        panic!("not a call");
    };
    assert_eq!(call.caller, "+15550100");
}
