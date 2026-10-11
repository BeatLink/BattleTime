extern crate std;

use std::{string::String, vec::Vec};

use neotime_runtime::{theme::rgb565, ThemeColor, DEFAULT_THEME};

use super::*;

fn lines_from(writes: &[&[u8]]) -> Vec<String> {
    let mut assembler = LineAssembler::new();
    let mut lines = Vec::new();
    for write in writes {
        assembler.push(write, |line| lines.push(String::from_utf8(line.to_vec()).unwrap()));
    }
    lines
}

// ============================================================================
// Lines
// ============================================================================

#[test]
fn a_line_split_across_writes_comes_out_whole() {
    let lines = lines_from(&[b"GB({\"t\":\"sett", b"ings\"})\nGB({\"t\":", b"\"find\"})\n"]);
    assert_eq!(lines, ["GB({\"t\":\"settings\"})", "GB({\"t\":\"find\"})"]);
}

#[test]
fn a_line_longer_than_the_buffer_is_dropped_and_the_next_is_read() {
    let long = [b'x'; MOST_LINE_BYTES + 10];
    let lines = lines_from(&[&long, b"\n{\"t\":\"settings\"}\n"]);
    assert_eq!(lines, ["{\"t\":\"settings\"}"]);
}

// ============================================================================
// Messages
// ============================================================================

#[test]
fn settings_with_no_keys_asks_for_a_report() {
    let message = parse_line(b"GB({\"t\":\"settings\"})").unwrap();
    let Message::Settings(change) = message else {
        panic!("{message:?}");
    };
    assert!(!change.changes_anything());
}

#[test]
fn a_preset_and_single_colours_change_the_theme_in_that_order() {
    let line = br##"GB({"t":"settings","theme":"light","theme_colors":{"accent":"#ff0000"},"brightness":3})"##;
    let Ok(Message::Settings(change)) = parse_line(line) else {
        panic!();
    };
    let theme = change.applied_to(DEFAULT_THEME);

    assert_eq!(theme.color(ThemeColor::Background), rgb565(255, 255, 255));
    assert_eq!(theme.color(ThemeColor::Accent), rgb565(255, 0, 0));
    assert_eq!(change.brightness, Some(3));
}

#[test]
fn unknown_keys_and_messages_are_passed_over() {
    let settings = parse_line(br#"{"t":"settings","launcher":"grid","theme":"ocean"}"#).unwrap();
    assert!(matches!(settings, Message::Settings(change) if change.preset.is_some()));
    assert_eq!(
        parse_line(br#"GB({"t":"notify","id":3,"title":"Hi"})"#),
        Ok(Message::Unknown { kind: "notify" })
    );
}

#[test]
fn bad_values_are_refused() {
    assert_eq!(parse_line(br#"{"t":"settings","theme":"neon"}"#), Err(MessageError::UnknownTheme));
    assert_eq!(
        parse_line(br#"{"t":"settings","theme_colors":{"text":"red"}}"#),
        Err(MessageError::BadColor)
    );
    assert_eq!(parse_line(br#"{"t":"settings","brightness":9}"#), Err(MessageError::BadBrightness));
    assert_eq!(parse_line(b"GB(oops)"), Err(MessageError::NotJson));
}

// ============================================================================
// Replies
// ============================================================================

#[test]
fn the_report_names_a_preset_and_gives_every_colour() {
    let report = settings_report(&DEFAULT_THEME, 2);
    assert!(report.starts_with("{\"t\":\"settings\",\"theme\":\"dark\","), "{report}");
    assert!(report.contains("\"accent\":\"#00aaa5\""), "{report}");
    assert!(report.contains("\"themes\":[\"dark\",\"light\",\"ocean\",\"forest\",\"sunset\",\"contrast\"]"));
    assert!(report.ends_with("\"brightness\":2}\n"));
}

#[test]
fn a_changed_theme_is_reported_as_custom_and_reads_back() {
    let line = br##"{"t":"settings","theme_colors":{"accent":"#123456"}}"##;
    let Ok(Message::Settings(change)) = parse_line(line) else {
        panic!();
    };
    let theme = change.applied_to(DEFAULT_THEME);
    let report = settings_report(&theme, 1);
    assert!(report.contains("\"theme\":\"custom\""));

    let Ok(Message::Settings(reread)) = parse_line(report.trim_end().as_bytes()) else {
        panic!("{report}");
    };
    assert_eq!(reread.applied_to(DEFAULT_THEME), theme);
}
