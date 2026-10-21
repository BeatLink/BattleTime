extern crate std;

use std::{format, string::String};

use crate::*;

fn app_message(line: &str) -> AppMessage {
    match parse_line(line.as_bytes()) {
        Ok(Message::App(message)) => *message,
        other => panic!("{line}: {other:?}"),
    }
}

fn refused(line: &str) -> bool {
    parse_line(line.as_bytes()).is_err()
}

// ============================================================================
// Reading
// ============================================================================

#[test]
fn a_message_names_its_app_and_carries_numbers_and_text() {
    let message = app_message(r#"{"t":"app","ch":"sos","d":{"on":0,"level":-12,"label":"Stop now"}}"#);
    assert_eq!(message.topic, "sos");
    assert_eq!(message.number_fields().as_slice(), [("on", 0), ("level", -12)]);
    assert_eq!(message.text_field(), Some(("label", "Stop now")));
}

#[test]
fn gadgetbridge_wrapping_white_space_and_key_order_are_all_read() {
    let message =
        app_message(r#"GB({ "d" : { "on" : true , "off": false } , "t" : "app", "ch" : "remote-2" })"#);
    assert_eq!(message.topic, "remote-2");
    assert_eq!(message.number_fields().as_slice(), [("on", 1), ("off", 0)]);
    assert_eq!(message.text_field(), None);
}

#[test]
fn an_empty_data_object_is_a_message_with_no_fields() {
    let message = app_message(r#"{"t":"app","ch":"buzz","d":{}}"#);
    assert_eq!(message.topic, "buzz");
    assert!(message.numbers.is_empty());
    assert!(message.text.is_none());
}

#[test]
fn text_is_unescaped_and_cut_at_a_whole_character() {
    let message = app_message(r#"{"t":"app","ch":"note","d":{"text":"Line \"one\"\nTwo é"}}"#);
    assert_eq!(message.text_field(), Some(("text", "Line \"one\"\nTwo é")));

    let long: String = "é".repeat(APP_MESSAGE_TEXT_BYTES);
    let message = app_message(&format!(r#"{{"t":"app","ch":"note","d":{{"text":"{long}"}}}}"#));
    let (_, text) = message.text.unwrap();
    assert_eq!(text.len(), APP_MESSAGE_TEXT_BYTES);
    assert!(text.chars().all(|character| character == 'é'));
}

#[test]
fn fractions_are_rounded_before_the_message_is_read() {
    let line = br#"{"t":"app","ch":"level","d":{"tilt":2.5}}"#;
    let rounded = with_fractions_rounded(line).unwrap();
    let Ok(Message::App(message)) = parse_line(&rounded) else {
        panic!();
    };
    assert_eq!(message.number_fields().as_slice(), [("tilt", 3)]);
}

#[test]
fn other_keys_beside_the_topic_and_data_are_passed_over() {
    let message =
        app_message(r#"{"t":"app","id":7,"extra":{"a":[1,"]",{"b":null}]},"ch":"sos","d":{"on":1}}"#);
    assert_eq!(message.topic, "sos");
    assert_eq!(message.number_fields().as_slice(), [("on", 1)]);
}

// ============================================================================
// Refusing
// ============================================================================

#[test]
fn more_than_four_numbers_or_a_second_text_are_refused() {
    assert!(!refused(
        r#"{"t":"app","ch":"a","d":{"a":1,"b":2,"c":3,"d":4,"e":"five"}}"#
    ));
    assert!(refused(
        r#"{"t":"app","ch":"a","d":{"a":1,"b":2,"c":3,"d":4,"e":5}}"#
    ));
    assert!(refused(r#"{"t":"app","ch":"a","d":{"a":"one","b":"two"}}"#));
}

#[test]
fn values_other_than_whole_numbers_booleans_and_text_are_refused() {
    for value in [
        "null",
        "[1]",
        r#"{"x":1}"#,
        "1e3",
        "-",
        "2147483648",
        "-2147483649",
        "yes",
    ] {
        let line = format!(r#"{{"t":"app","ch":"a","d":{{"x":{value}}}}}"#);
        assert!(refused(&line), "{line}");
    }
}

#[test]
fn numbers_fill_32_bits_and_a_key_may_come_only_once() {
    let message = app_message(r#"{"t":"app","ch":"a","d":{"high":2147483647,"low":-2147483648}}"#);
    assert_eq!(
        message.number_fields().as_slice(),
        [("high", 2147483647), ("low", -2147483648)]
    );

    assert!(refused(r#"{"t":"app","ch":"a","d":{"on":1,"on":2}}"#));
    assert!(refused(r#"{"t":"app","ch":"a","d":{"on":1,"on":"x"}}"#));
}

#[test]
fn a_bad_topic_or_key_or_a_missing_part_is_refused() {
    let long = "a".repeat(MOST_APP_MESSAGE_KEY_BYTES + 1);
    for line in [
        String::from(r#"{"t":"app","d":{"on":1}}"#),
        String::from(r#"{"t":"app","ch":"sos"}"#),
        String::from(r#"{"t":"app","ch":"","d":{}}"#),
        String::from(r#"{"t":"app","ch":"a b","d":{}}"#),
        String::from(r#"{"t":"app","ch":"sos","d":{"o n":1}}"#),
        String::from(r#"{"t":"app","ch":"sos","d":{"":1}}"#),
        String::from(r#"{"t":"app","ch":"sos","d":[1]}"#),
        String::from(r#"{"t":"app","ch":"sos","d":{"on":1}"#),
        String::from(r#"{"t":"app","ch":7,"d":{}}"#),
        format!(r#"{{"t":"app","ch":"{long}","d":{{}}}}"#),
        format!(r#"{{"t":"app","ch":"sos","d":{{"{long}":1}}}}"#),
    ] {
        assert!(refused(&line), "{line}");
    }
}

#[test]
fn the_recovery_firmware_reads_only_the_kind() {
    assert_eq!(message_kind(br#"{"t":"app","ch":"sos","d":{"on":0}}"#), Ok("app"));
}
