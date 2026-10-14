extern crate std;

use std::{format, string::String, vec::Vec};

use neotime_runtime::{theme::rgb565, ThemeColor, DEFAULT_THEME};
use neotime_settings::{Setting, Settings};

use super::*;

fn lines_from(writes: &[&[u8]]) -> Vec<String> {
    let mut assembler = LineAssembler::new();
    let mut lines = Vec::new();
    for write in writes {
        assembler.push(write, |line| {
            lines.push(String::from_utf8(line.to_vec()).unwrap())
        });
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

#[test]
fn a_finished_line_gives_its_memory_back() {
    let mut assembler = LineAssembler::new();
    assembler.push(b"{\"t\":\"sett", |_| {});
    assert!(assembler.buffer.capacity() > 0);
    assembler.push(b"ings\"}\n", |_| {});
    assert_eq!(assembler.buffer.capacity(), 0);
}

// ============================================================================
// Messages
// ============================================================================

#[test]
fn message_kind_reads_only_the_kind() {
    assert_eq!(message_kind(b"GB({\"t\":\"status\"})"), Ok("status"));
    assert_eq!(
        message_kind(br#"{"t":"slots","recovery":[413696,397312],"theme":"ocean"}"#),
        Ok("slots")
    );
    assert_eq!(message_kind(b"not json"), Err(MessageError::NotJson));
    assert_eq!(message_kind(&[0xff]), Err(MessageError::NotText));
}

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
    let line =
        br##"GB({"t":"settings","theme":"light","theme_colors":{"accent":"#ff0000"},"brightness":3})"##;
    let Ok(Message::Settings(change)) = parse_line(line) else {
        panic!();
    };
    let theme = change.applied_to(DEFAULT_THEME);

    assert_eq!(theme.color(ThemeColor::Background), rgb565(255, 255, 255));
    assert_eq!(theme.color(ThemeColor::Accent), rgb565(255, 0, 0));
    assert_eq!(change.value(Setting::Brightness), Some(3));
}

#[test]
fn unknown_keys_and_messages_are_passed_over() {
    let settings = parse_line(br#"{"t":"settings","launcher":"grid","theme":"ocean"}"#).unwrap();
    assert!(matches!(settings, Message::Settings(change) if change.preset.is_some()));
    assert_eq!(
        parse_line(br#"GB({"t":"alarm","d":[]})"#),
        Ok(Message::Unknown { kind: "alarm" })
    );
}

#[test]
fn a_state_message_names_the_app_whose_state_the_phone_rewrote() {
    assert_eq!(
        parse_line(br#"GB({"app":"alarm","t":"state"})"#),
        Ok(Message::StateWritten { app: "alarm" })
    );
    assert_eq!(parse_line(br#"{"t":"state"}"#), Err(MessageError::NotJson));
    assert_eq!(
        parse_line(br#"{"t":"state","app":""}"#),
        Err(MessageError::NotJson)
    );
    let long = format!(r#"{{"t":"state","app":"{}"}}"#, "a".repeat(MOST_APP_ID_BYTES + 1));
    assert_eq!(parse_line(long.as_bytes()), Err(MessageError::NotJson));
}

// ============================================================================
// Notifications
// ============================================================================

#[test]
fn a_notification_carries_its_id_app_title_and_body() {
    let line =
        br#"GB({"t":"notify","id":1234,"src":"Messages","title":"Sam","body":"On my way","sender":"Sam"})"#;

    let Ok(Message::Notification(notification)) = parse_line(line) else {
        panic!("{:?}", parse_line(line));
    };

    assert_eq!(notification.id, 1234);
    assert_eq!(notification.source, "Messages");
    assert_eq!(notification.title, "Sam");
    assert_eq!(notification.body, "On my way");
}

#[test]
fn a_dismissal_names_the_notification_by_its_id() {
    let line = br#"GB({"t":"notify-","id":1234})"#;

    assert_eq!(parse_line(line), Ok(Message::NotificationDismissed { id: 1234 }));
}

#[test]
fn a_notification_with_fields_left_out_reads_as_empty() {
    let Ok(Message::Notification(notification)) = parse_line(br#"{"t":"notify"}"#) else {
        panic!();
    };
    assert_eq!(notification, PhoneNotification::default());
}

#[test]
fn notification_text_has_its_escapes_undone() {
    let line = br#"{"t":"notify","id":1,"title":"\"Quoted\"","body":"Line one\nCaf\u00e9"}"#;

    let Ok(Message::Notification(notification)) = parse_line(line) else {
        panic!();
    };

    assert_eq!(notification.title, "\"Quoted\"");
    assert_eq!(notification.body, "Line one\nCafé");
}

#[test]
fn long_notification_text_is_cut_at_a_whole_character() {
    let body = "é".repeat(NOTIFICATION_BODY_BYTES);
    let line = format!(r#"{{"t":"notify","id":2,"body":"{body}"}}"#);

    let Ok(Message::Notification(notification)) = parse_line(line.as_bytes()) else {
        panic!();
    };

    assert_eq!(notification.body.chars().count(), NOTIFICATION_BODY_BYTES / 2);
}

#[test]
fn a_bad_escape_ends_the_text_there() {
    let line = br#"{"t":"notify","id":3,"title":"Hi \ud83d there"}"#;

    let Ok(Message::Notification(notification)) = parse_line(line) else {
        panic!();
    };

    assert_eq!(notification.title, "Hi ");
}

#[test]
fn bad_values_are_refused() {
    assert_eq!(
        parse_line(br#"{"t":"settings","theme":"neon"}"#),
        Err(MessageError::UnknownTheme)
    );
    assert_eq!(
        parse_line(br#"{"t":"settings","theme_colors":{"text":"red"}}"#),
        Err(MessageError::BadColor)
    );
    assert_eq!(
        parse_line(br#"{"t":"settings","brightness":9}"#),
        Err(MessageError::BadSetting)
    );
    assert_eq!(parse_line(b"GB(oops)"), Err(MessageError::NotJson));
}

// ============================================================================
// Replies
// ============================================================================

#[test]
fn the_report_names_a_preset_and_gives_every_colour() {
    let report = settings_report(&DEFAULT_THEME, &Settings::default());
    assert!(
        report.starts_with("{\"t\":\"settings\",\"theme\":\"dark\","),
        "{report}"
    );
    assert!(report.contains("\"accent\":\"#00aaa5\""), "{report}");
    assert!(report.contains("\"themes\":[\"dark\",\"light\",\"ocean\",\"forest\",\"sunset\",\"contrast\"]"));
    assert!(report.contains("\"brightness\":2,"), "{report}");
}

#[test]
fn a_changed_theme_is_reported_as_custom_and_reads_back() {
    let line = br##"{"t":"settings","theme_colors":{"accent":"#123456"}}"##;
    let Ok(Message::Settings(change)) = parse_line(line) else {
        panic!();
    };
    let theme = change.applied_to(DEFAULT_THEME);
    let report = settings_report(&theme, &Settings::default());
    assert!(report.contains("\"theme\":\"custom\""));

    let Ok(Message::Settings(reread)) = parse_line(report.trim_end().as_bytes()) else {
        panic!("{report}");
    };
    assert_eq!(reread.applied_to(DEFAULT_THEME), theme);
}

// ============================================================================
// Watch-wide settings
// ============================================================================

#[test]
fn every_setting_has_a_key_the_wasp_os_companion_already_knows_where_it_can() {
    let line = br#"{"t":"settings","blank_after":30,"clock_24h":false,"units":"Imperial","step_goal":8000,"always_on":true,"chimes":2,"do_not_disturb":true,"bluetooth":false,"updates":false,"wake_on_touch":2,"alarms_in_dnd":false}"#;
    let Ok(Message::Settings(change)) = parse_line(line) else {
        panic!();
    };

    assert_eq!(change.value(Setting::ScreenTimeoutSeconds), Some(30));
    assert_eq!(change.value(Setting::Uses24HourClock), Some(0));
    assert_eq!(change.value(Setting::UsesMetricUnits), Some(0));
    assert_eq!(change.value(Setting::StepGoal), Some(8000));
    assert_eq!(change.value(Setting::AlwaysOnScreen), Some(1));
    assert_eq!(change.value(Setting::Chimes), Some(2));
    assert_eq!(change.value(Setting::DoNotDisturb), Some(1));
    assert_eq!(change.value(Setting::Bluetooth), Some(0));
    assert_eq!(change.value(Setting::AllowsUpdates), Some(0));
    assert_eq!(change.value(Setting::WakeOnTouch), Some(2));
    assert_eq!(change.value(Setting::AlarmsInDoNotDisturb), Some(0));
    assert_eq!(change.value(Setting::Brightness), None);
}

#[test]
fn updates_turn_on_until_the_next_restart_with_a_key_of_their_own() {
    let until_restart = br#"{"t":"settings","updates":false,"updates_until_restart":true}"#;
    let for_good = br#"{"t":"settings","updates":true,"updates_until_restart":false}"#;
    let neither = br#"{"t":"settings","updates_until_restart":false}"#;

    let value_of = |line: &[u8]| match parse_line(line) {
        Ok(Message::Settings(change)) => change.value(Setting::AllowsUpdates),
        _ => panic!(),
    };

    assert_eq!(value_of(until_restart), Some(allows_updates::UNTIL_RESTART));
    assert_eq!(value_of(for_good), Some(allows_updates::ON));
    assert_eq!(value_of(neither), None);
}

#[test]
fn the_reply_says_whether_updates_are_on_and_whether_only_until_the_next_restart() {
    let mut settings = Settings::default();
    let report = |settings: &Settings| settings_report(&DEFAULT_THEME, settings);

    assert!(report(&settings).contains("\"updates\":true,\"updates_until_restart\":false"));
    settings
        .set(Setting::AllowsUpdates, allows_updates::UNTIL_RESTART)
        .unwrap();
    assert!(report(&settings).contains("\"updates\":true,\"updates_until_restart\":true"));
    settings.set(Setting::AllowsUpdates, allows_updates::OFF).unwrap();
    assert!(report(&settings).contains("\"updates\":false,\"updates_until_restart\":false"));
}

#[test]
fn the_longest_settings_reply_is_whole() {
    let mut settings = Settings::default();
    for setting in Setting::ALL {
        let limits = setting.limits();
        let longest = if limits.highest > 1 {
            limits.highest
        } else {
            limits.lowest
        };
        settings.set(setting, longest).unwrap();
    }
    settings.set(Setting::AllowsUpdates, allows_updates::OFF).unwrap();
    let contrast = neotime_runtime::preset_named("contrast").unwrap().theme;

    let report = settings_report(&contrast, &settings);

    assert!(report.ends_with("}\n"), "{report}");
}

#[test]
fn the_wake_gestures_and_sleep_mode_have_keys_of_their_own() {
    let line = br#"{"t":"settings","raise_to_wake":true,"lower_to_sleep":true,"shake_to_wake":false,"shake_threshold":900,"sleep_mode":true}"#;
    let Ok(Message::Settings(change)) = parse_line(line) else {
        panic!();
    };

    assert_eq!(change.value(Setting::RaiseToWake), Some(1));
    assert_eq!(change.value(Setting::LowerToSleep), Some(1));
    assert_eq!(change.value(Setting::ShakeToWake), Some(0));
    assert_eq!(change.value(Setting::ShakeThreshold), Some(900));
    assert_eq!(change.value(Setting::SleepMode), Some(1));
    assert_eq!(
        parse_line(br#"{"t":"settings","shake_threshold":5000}"#),
        Err(MessageError::BadSetting)
    );
}

#[test]
fn the_watch_face_is_chosen_by_its_number() {
    let line = br#"{"t":"settings","watch_face":823453713}"#;
    let Ok(Message::Settings(change)) = parse_line(line) else {
        panic!();
    };

    assert_eq!(change.value(Setting::WatchFace), Some(0x3114_E811));
    assert_eq!(
        parse_line(br#"{"t":"settings","watch_face":-5}"#),
        Err(MessageError::BadSetting)
    );
}

#[test]
fn the_gadgetbridge_name_is_turned_on_and_off() {
    let Ok(Message::Settings(change)) = parse_line(br#"{"t":"settings","gadgetbridge_name":true}"#) else {
        panic!();
    };

    assert_eq!(change.value(Setting::GadgetbridgeName), Some(1));
    assert!(parse_line(br#"{"t":"settings","gadgetbridge_name":1}"#).is_err());
}

#[test]
fn one_bad_setting_refuses_the_whole_message() {
    let too_short = br#"{"t":"settings","brightness":3,"blank_after":1}"#;
    let unknown_units = br#"{"t":"settings","units":"Furlongs"}"#;
    let huge = br#"{"t":"settings","step_goal":99999999999}"#;
    let unknown_wake = br#"{"t":"settings","wake_on_touch":3}"#;

    for line in [&too_short[..], &unknown_units[..], &huge[..], &unknown_wake[..]] {
        assert_eq!(parse_line(line), Err(MessageError::BadSetting));
    }
}

#[test]
fn the_report_gives_every_setting_and_reads_back_the_same() {
    let mut settings = Settings::default();
    settings.set(Setting::UsesMetricUnits, 0).unwrap();
    settings.set(Setting::Chimes, 1).unwrap();
    settings.set(Setting::ScreenTimeoutSeconds, 60).unwrap();
    settings.set(Setting::WatchFace, i32::MAX).unwrap();
    settings.set(Setting::GadgetbridgeName, 1).unwrap();

    let report = settings_report(&DEFAULT_THEME, &settings);

    assert!(report.len() < MOST_REPLY_BYTES);
    assert!(report.contains("\"units\":\"Imperial\""), "{report}");
    assert!(report.contains("\"clock_24h\":true"), "{report}");
    assert!(report.contains("\"do_not_disturb\":false"), "{report}");
    assert!(report.contains("\"alarms_in_dnd\":true"), "{report}");
    assert!(report.contains("\"heart_rate_interval\":0"), "{report}");
    assert!(report.contains("\"raise_to_wake\":false"), "{report}");
    assert!(report.contains("\"shake_threshold\":150"), "{report}");
    assert!(report.contains("\"sleep_mode\":false"), "{report}");
    assert!(report.contains("\"watch_face\":2147483647"), "{report}");
    assert!(report.ends_with("\"gadgetbridge_name\":true}\n"), "{report}");
    let Ok(Message::Settings(reread)) = parse_line(report.trim_end().as_bytes()) else {
        panic!("{report}");
    };
    for setting in Setting::ALL {
        assert_eq!(reread.value(setting), Some(settings.get(setting)), "{setting:?}");
    }
}
