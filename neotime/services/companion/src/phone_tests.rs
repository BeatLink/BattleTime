extern crate std;

use std::{format, string::String};

use neotime_runtime::{SystemValue, NONE};

use crate::*;

fn parsed(line: &str) -> Message<'_> {
    parse_line(line.as_bytes()).unwrap_or_else(|error| panic!("{line}: {error:?}"))
}

// ============================================================================
// Calls
// ============================================================================

#[test]
fn an_incoming_call_names_its_caller() {
    let message = parsed(r#"GB({"t":"call","cmd":"incoming","name":"Sam","number":"+15550100"})"#);
    assert_eq!(
        message,
        Message::Call(PhoneCall {
            kind: CallKind::Incoming,
            caller: "Sam".try_into().unwrap(),
        })
    );
}

#[test]
fn a_call_without_a_name_shows_the_number() {
    let Message::Call(call) = parsed(r#"{"t":"call","cmd":"start","name":"","number":"+15550100"}"#) else {
        panic!();
    };
    assert_eq!(call.kind, CallKind::Answered);
    assert_eq!(call.caller, "+15550100");
}

#[test]
fn every_gadgetbridge_call_command_has_a_kind() {
    let kinds = [
        ("incoming", CallKind::Incoming),
        ("accept", CallKind::Answered),
        ("outgoing", CallKind::Answered),
        ("start", CallKind::Answered),
        ("reject", CallKind::Ended),
        ("end", CallKind::Ended),
    ];
    for (command, kind) in kinds {
        let line = format!(r#"{{"t":"call","cmd":"{command}"}}"#);
        let Message::Call(call) = parsed(&line) else {
            panic!("{command}");
        };
        assert_eq!(call.kind, kind, "{command}");
    }
}

#[test]
fn a_call_command_the_watch_does_not_know_is_passed_over() {
    assert_eq!(
        parsed(r#"{"t":"call","cmd":"hold"}"#),
        Message::Unknown { kind: "call" }
    );
    assert_eq!(parsed(r#"{"t":"call"}"#), Message::Unknown { kind: "call" });
}

// ============================================================================
// Music
// ============================================================================

#[test]
fn music_info_carries_the_artist_track_and_length() {
    let line = r#"GB({"t":"musicinfo","artist":"Café Band","album":"Live","track":"Opening","dur":215,"c":-1,"n":-1})"#;
    let Message::MusicInfo(info) = parsed(line) else {
        panic!();
    };
    assert_eq!(info.artist, "Café Band");
    assert_eq!(info.track, "Opening");
    assert_eq!(info.length_in_seconds, 215);
}

#[test]
fn a_long_track_title_is_cut_at_a_whole_character() {
    let track = "é".repeat(MUSIC_TRACK_BYTES);
    let line = format!(r#"{{"t":"musicinfo","track":"{track}"}}"#);
    let Message::MusicInfo(info) = parsed(&line) else {
        panic!();
    };
    assert_eq!(info.track.chars().count(), MUSIC_TRACK_BYTES / 2);
    assert_eq!(info.length_in_seconds, 0);
}

#[test]
fn music_state_says_whether_it_plays_and_where() {
    assert_eq!(
        parsed(r#"{"t":"musicstate","state":"play","position":72,"shuffle":1,"repeat":1}"#),
        Message::MusicState(MusicState {
            is_playing: true,
            position_in_seconds: Some(72),
        })
    );
    assert_eq!(
        parsed(r#"{"t":"musicstate","state":"pause"}"#),
        Message::MusicState(MusicState::default())
    );
}

// ============================================================================
// Weather
// ============================================================================

#[test]
fn weather_is_read_in_celsius_with_its_sky_and_place() {
    let line = r#"GB({"t":"weather","temp":291,"hi":294,"lo":284,"hum":60,"rain":10,"uv":2.5,"code":801,"txt":"Few clouds","wind":3.6,"wdir":180,"loc":"Test City"})"#;
    let Message::Weather(weather) = parsed(line) else {
        panic!();
    };
    assert_eq!(weather.temperature_in_celsius, Some(18));
    assert_eq!(weather.high_in_celsius, Some(21));
    assert_eq!(weather.low_in_celsius, Some(11));
    assert_eq!(weather.condition, Some(WeatherCondition::FewClouds));
    assert_eq!(weather.location, "Test City");
}

#[test]
fn weather_left_out_stays_unknown() {
    assert_eq!(parsed(r#"{"t":"weather"}"#), Message::Weather(Weather::default()));
}

#[test]
fn weather_codes_map_to_skies() {
    use WeatherCondition::*;
    let skies = [
        (211, Some(Thunderstorm)),
        (301, Some(Showers)),
        (500, Some(Rain)),
        (511, Some(Snow)),
        (521, Some(Showers)),
        (601, Some(Snow)),
        (741, Some(Mist)),
        (800, Some(Clear)),
        (801, Some(FewClouds)),
        (802, Some(Clouds)),
        (804, Some(HeavyClouds)),
        (900, None),
    ];
    for (code, sky) in skies {
        assert_eq!(WeatherCondition::from_code(code), sky, "{code}");
    }
}

// ============================================================================
// Finding, buzzing and status
// ============================================================================

#[test]
fn find_starts_and_stops_ringing() {
    assert_eq!(
        parsed(r#"GB({"t":"find","n":true})"#),
        Message::Find { is_on: true }
    );
    assert_eq!(
        parsed(r#"{"t":"find","n":false}"#),
        Message::Find { is_on: false }
    );
    assert_eq!(parsed(r#"{"t":"find"}"#), Message::Find { is_on: false });
}

#[test]
fn vibrate_buzzes_one_to_five_times() {
    assert_eq!(parsed(r#"{"t":"vibrate","n":3}"#), Message::Vibrate { times: 3 });
    assert_eq!(parsed(r#"{"t":"vibrate","n":40}"#), Message::Vibrate { times: 5 });
    assert_eq!(parsed(r#"{"t":"vibrate","n":0}"#), Message::Vibrate { times: 1 });
    assert_eq!(parsed(r#"{"t":"vibrate"}"#), Message::Vibrate { times: 1 });
    assert_eq!(parsed(r#"{"t":"vibrate","n":-2}"#), Message::Vibrate { times: 1 });
    assert_eq!(
        parsed(r#"{ "n" : 2 , "t" : "vibrate" }"#),
        Message::Vibrate { times: 2 }
    );
    assert_eq!(parsed(r#"{"n":true,"t":"find"}"#), Message::Find { is_on: true });
}

#[test]
fn a_temperature_with_a_fraction_is_rounded_as_the_line_arrives() {
    let line = br#"GB({"t":"weather","temp":291.6,"hi":299.5,"lo":-0.5,"loc":"Port 3.5","code":800})"#;
    let rounded = with_fractions_rounded(line).unwrap();
    let Ok(Message::Weather(weather)) = parse_line(&rounded) else {
        panic!("{}", String::from_utf8_lossy(&rounded));
    };

    assert_eq!(weather.temperature_in_celsius, Some(19));
    assert_eq!(weather.high_in_celsius, Some(27));
    assert_eq!(weather.low_in_celsius, Some(-274));
    assert_eq!(weather.location, "Port 3.5");
    assert_eq!(weather.condition, Some(WeatherCondition::Clear));
}

#[test]
fn rounding_carries_into_a_new_digit_and_leaves_text_alone() {
    assert_eq!(
        with_fractions_rounded(br#"{"a":9.5,"b":"x\"9.9","c":[0.4,-99.99]}"#).unwrap(),
        br#"{"a":10,"b":"x\"9.9","c":[0,-100]}"#
    );
    assert_eq!(with_fractions_rounded(br#"{"t":"weather","temp":291}"#), None);
}

#[test]
fn a_line_that_is_not_json_keeps_its_fractions() {
    assert_eq!(
        with_fractions_rounded(b"\x10setTime(1767603600);E.setTimeZone(5.5);"),
        None
    );
    assert!(with_fractions_rounded(b"\x10GB({\"t\":\"weather\",\"temp\":2.5})").is_some());
}

#[test]
fn the_line_assembler_rounds_each_json_line() {
    let mut lines = LineAssembler::new();
    let mut seen = std::vec::Vec::new();
    lines.push(b"{\"t\":\"weather\",\"temp\":280.7}\nsetTime(1.5)\n", |line| {
        seen.push(line.to_vec())
    });

    assert_eq!(
        seen,
        [
            b"{\"t\":\"weather\",\"temp\":281}".to_vec(),
            b"setTime(1.5)".to_vec()
        ]
    );
}

#[test]
fn a_temperature_with_an_exponent_is_still_unreadable() {
    let rounded = with_fractions_rounded(br#"{"t":"weather","temp":2.9e2}"#).unwrap();
    assert_eq!(parse_line(&rounded), Err(MessageError::NotJson));
}

// ============================================================================
// Navigation
// ============================================================================

#[test]
fn gadgetbridge_navigation_is_read_with_its_turn_and_distance() {
    let line = r#"GB({"t":"nav","instr":"Turn left onto Main Street","distance":"300 m","action":"left","eta":"12:34"})"#;

    assert_eq!(
        parsed(line),
        Message::Navigation(Navigation {
            turn: Some(NavigationTurn::Left),
            instruction: "Turn left onto Main Street".try_into().unwrap(),
            distance_in_metres: Some(300),
            progress_percent: None,
        })
    );
}

#[test]
fn every_gadgetbridge_action_names_a_turn_but_off_route() {
    use NavigationTurn::*;
    for (action, turn) in [
        ("continue", Some(Straight)),
        ("left", Some(Left)),
        ("left_slight", Some(SlightLeft)),
        ("left_sharp", Some(SharpLeft)),
        ("right", Some(Right)),
        ("right_slight", Some(SlightRight)),
        ("right_sharp", Some(SharpRight)),
        ("keep_left", Some(SlightLeft)),
        ("keep_right", Some(SlightRight)),
        ("uturn_left", Some(UTurn)),
        ("uturn_right", Some(UTurn)),
        ("offroute", None),
        ("roundabout_right", Some(Roundabout)),
        ("roundabout_left", Some(Roundabout)),
        ("roundabout_straight", Some(Roundabout)),
        ("roundabout_uturn", Some(Roundabout)),
        ("finish", Some(Arrive)),
    ] {
        assert_eq!(NavigationTurn::from_action(action), turn, "{action}");
    }
}

#[test]
fn distances_are_read_in_the_units_phones_write() {
    for (text, metres) in [
        ("300 m", Some(300)),
        ("1.2 km", Some(1_200)),
        ("1,5 km", Some(1_500)),
        ("0.5 mi", Some(805)),
        ("800 ft", Some(244)),
        ("100 yd", Some(91)),
        ("42", Some(42)),
        (" 7 M ", Some(7)),
        ("far", None),
        ("3 parsecs", None),
    ] {
        assert_eq!(metres_from_text(text), metres, "{text}");
    }
}

#[test]
fn navigation_is_kept_for_apps_until_an_empty_step_ends_it() {
    let off_route = parsed(r#"{"t":"nav","instr":"Off route","action":"offroute"}"#);
    let Message::Navigation(off_route) = off_route else {
        panic!();
    };
    assert_eq!(off_route.turn, None);

    let mut data = PhoneData::default();

    let mut updates = PhoneUpdates::default();
    assert!(updates.record(parsed(
        r#"{"t":"nav","instr":"Keep right","distance":"1.2 km","action":"keep_right"}"#
    )));
    let changes = data.apply(&mut updates, 1_000);

    assert!(changes.navigation);
    assert!(updates.is_empty());
    assert_eq!(
        data.read(SystemValue::NavigationTurn),
        Some(NavigationTurn::SlightRight as i64)
    );
    assert_eq!(data.read(SystemValue::NavigationDistance), Some(1_200));
    assert_eq!(data.read(SystemValue::NavigationProgressPercent), Some(0));
    assert_eq!(
        data.read_text(SystemValue::NavigationInstruction),
        Some("Keep right")
    );

    updates.record(parsed(r#"{"t":"nav"}"#));
    assert!(data.apply(&mut updates, 2_000).navigation);
    assert_eq!(data.read(SystemValue::NavigationTurn), None);
    assert_eq!(data.read(SystemValue::NavigationDistance), None);
    assert_eq!(data.read_text(SystemValue::NavigationInstruction), None);
}

#[test]
fn a_state_request_names_music_or_navigation() {
    assert_eq!(state_request_line(false), "{\"t\":\"request\",\"n\":\"music\"}\n");
    assert_eq!(state_request_line(true), "{\"t\":\"request\",\"n\":\"nav\"}\n");
}

#[test]
fn a_field_of_the_wrong_type_is_unreadable() {
    for line in [
        r#"{"t":"find","n":"yes"}"#,
        r#"{"t":"vibrate","n":true}"#,
        r#"{"t":"weather","temp":"warm"}"#,
        r#"{"t":"musicinfo","dur":"long"}"#,
        r#"{"t":"musicstate","position":"start"}"#,
        r#"{"t":"call","cmd":1}"#,
        r#"{"t":"nav","distance":300}"#,
    ] {
        assert_eq!(parse_line(line.as_bytes()), Err(MessageError::NotJson), "{line}");
    }
}

#[test]
fn status_asks_for_a_report() {
    assert_eq!(parsed(r#"GB({"t":"status"})"#), Message::StatusRequest);
    let report = status_report(&WatchStatus {
        battery_percent: 87,
        battery_millivolts: 3950,
        steps_today: 4321,
        is_charging: false,
    });
    assert_eq!(
        report,
        "{\"t\":\"status\",\"bat\":87,\"mv\":3950,\"steps\":4321,\"charging\":false}\n"
    );
}

// ============================================================================
// Lines to the phone
// ============================================================================

#[test]
fn commands_to_the_phone_use_gadgetbridge_names() {
    assert_eq!(
        music_control_line(0).unwrap(),
        "{\"t\":\"music\",\"n\":\"play\"}\n"
    );
    assert_eq!(
        music_control_line(4).unwrap(),
        "{\"t\":\"music\",\"n\":\"volumeup\"}\n"
    );
    assert_eq!(music_control_line(6), None);
    assert_eq!(
        call_control_line(0).unwrap(),
        "{\"t\":\"call\",\"n\":\"ACCEPT\"}\n"
    );
    assert_eq!(
        call_control_line(2).unwrap(),
        "{\"t\":\"call\",\"n\":\"IGNORE\"}\n"
    );
    assert_eq!(call_control_line(-1), None);
    assert_eq!(find_phone_line(true), "{\"t\":\"findPhone\",\"n\":true}\n");
    assert_eq!(find_phone_line(false), "{\"t\":\"findPhone\",\"n\":false}\n");
}

// ============================================================================
// Waiting for the shell
// ============================================================================

#[test]
fn updates_keep_only_the_latest_of_each_kind() {
    let mut updates = PhoneUpdates::default();
    assert!(updates.is_empty());

    assert!(updates.record(parsed(r#"{"t":"call","cmd":"incoming","name":"Sam"}"#)));
    assert!(updates.record(parsed(r#"{"t":"call","cmd":"end","name":"Sam"}"#)));
    assert!(updates.record(parsed(r#"{"t":"musicinfo","track":"One"}"#)));
    assert!(updates.record(parsed(r#"{"t":"musicstate","state":"play"}"#)));
    assert!(updates.record(parsed(r#"{"t":"weather","temp":280}"#)));
    assert!(updates.record(parsed(r#"{"t":"find","n":true}"#)));
    assert!(updates.record(parsed(r#"{"t":"vibrate","n":2}"#)));

    assert_eq!(updates.call.as_ref().unwrap().kind, CallKind::Ended);
    assert_eq!(updates.music_info.as_ref().unwrap().track, "One");
    assert!(updates.music_state.unwrap().is_playing);
    assert_eq!(updates.weather.as_ref().unwrap().temperature_in_celsius, Some(7));
    assert_eq!(updates.find, Some(true));
    assert_eq!(updates.buzzes, Some(2));
    assert!(!updates.is_empty());
}

#[test]
fn updates_pass_over_other_messages() {
    let mut updates = PhoneUpdates::default();
    assert!(!updates.record(parsed(r#"{"t":"notify","id":1}"#)));
    assert!(!updates.record(parsed(r#"{"t":"settings"}"#)));
    assert!(!updates.record(Message::StatusRequest));
    assert!(updates.is_empty());
}

// ============================================================================
// What the watch keeps
// ============================================================================

#[test]
fn nothing_received_reads_as_empty() {
    let data = PhoneData::default();
    assert_eq!(data.read(SystemValue::MusicIsPlaying), Some(0));
    assert_eq!(data.read(SystemValue::MusicLength), Some(0));
    assert_eq!(data.read(SystemValue::WeatherTemperature), Some(NONE));
    assert_eq!(data.read(SystemValue::WeatherReceivedAt), Some(NONE));
    assert_eq!(data.read(SystemValue::WeatherForecastLength), Some(0));
    assert_eq!(data.read(SystemValue::BatteryPercent), None);
    assert_eq!(data.read_text(SystemValue::MusicArtist), Some(""));
    assert_eq!(data.read_text(SystemValue::WeatherLocation), Some(""));
    assert_eq!(data.read_text(SystemValue::NavigationInstruction), None);
    assert_eq!(data.read(SystemValue::NavigationTurn), None);
    assert_eq!(data.read_text(SystemValue::FirmwareVersion), None);
}

#[test]
fn the_latest_call_is_kept_for_apps_and_left_for_the_shell() {
    let mut data = PhoneData::default();
    assert_eq!(data.read(SystemValue::CallState), Some(2));
    assert_eq!(data.read_text(SystemValue::CallCaller), Some(""));

    let mut updates = PhoneUpdates::default();
    updates.record(parsed(r#"{"t":"call","cmd":"incoming","name":"Sam"}"#));
    let changes = data.apply(&mut updates, 1_000);
    assert!(changes.call);
    assert!(updates.call.is_some());
    assert_eq!(data.read(SystemValue::CallState), Some(0));
    assert_eq!(data.read_text(SystemValue::CallCaller), Some("Sam"));

    let mut updates = PhoneUpdates::default();
    updates.record(parsed(r#"{"t":"call","cmd":"start","name":"Sam"}"#));
    data.apply(&mut updates, 2_000);
    assert_eq!(data.read(SystemValue::CallState), Some(1));

    let mut updates = PhoneUpdates::default();
    updates.record(parsed(r#"{"t":"call","cmd":"end","name":"Sam"}"#));
    data.apply(&mut updates, 3_000);
    assert_eq!(data.read(SystemValue::CallState), Some(2));
    assert_eq!(data.read_text(SystemValue::CallCaller), Some("Sam"));
}

#[test]
fn music_info_and_state_combine() {
    let mut data = PhoneData::default();
    let mut updates = PhoneUpdates::default();
    updates.record(parsed(
        r#"{"t":"musicinfo","artist":"The Band","track":"Song","dur":215}"#,
    ));
    updates.record(parsed(r#"{"t":"musicstate","state":"play","position":72}"#));

    let changes = data.apply(&mut updates, 1_000);

    assert_eq!(
        changes,
        PhoneChanges {
            music: true,
            weather: false,
            call: false,
            navigation: false,
        }
    );
    assert!(updates.is_empty());
    assert_eq!(data.read(SystemValue::MusicIsPlaying), Some(1));
    assert_eq!(data.read(SystemValue::MusicPosition), Some(72_000));
    assert_eq!(data.read(SystemValue::MusicLength), Some(215_000));
    assert_eq!(data.read_text(SystemValue::MusicArtist), Some("The Band"));
    assert_eq!(data.read_text(SystemValue::MusicTrack), Some("Song"));
}

#[test]
fn a_state_without_a_position_keeps_the_last_one() {
    let mut data = PhoneData::default();
    let mut updates = PhoneUpdates::default();
    updates.record(parsed(r#"{"t":"musicstate","state":"play","position":30}"#));
    data.apply(&mut updates, 0);
    updates.record(parsed(r#"{"t":"musicstate","state":"pause"}"#));

    data.apply(&mut updates, 0);

    assert_eq!(data.read(SystemValue::MusicIsPlaying), Some(0));
    assert_eq!(data.read(SystemValue::MusicPosition), Some(30_000));
}

#[test]
fn weather_is_kept_with_when_it_came() {
    let mut data = PhoneData::default();
    let mut updates = PhoneUpdates::default();
    updates.record(parsed(
        r#"{"t":"weather","temp":291,"lo":284,"code":500,"loc":"Test City"}"#,
    ));

    let changes = data.apply(&mut updates, 5_000);

    assert_eq!(
        changes,
        PhoneChanges {
            music: false,
            weather: true,
            call: false,
            navigation: false,
        }
    );
    assert_eq!(data.read(SystemValue::WeatherTemperature), Some(18));
    assert_eq!(data.read(SystemValue::WeatherLow), Some(11));
    assert_eq!(data.read(SystemValue::WeatherHigh), Some(NONE));
    assert_eq!(
        data.read(SystemValue::WeatherCondition),
        Some(WeatherCondition::Rain as i64)
    );
    assert_eq!(data.read(SystemValue::WeatherReceivedAt), Some(5_000));
    assert_eq!(data.read(SystemValue::WeatherSunrise), Some(NONE));
    assert_eq!(data.read_text(SystemValue::WeatherLocation), Some("Test City"));
}

#[test]
fn updates_with_no_music_or_weather_change_nothing() {
    let mut data = PhoneData::default();
    let mut updates = PhoneUpdates::default();
    updates.record(parsed(r#"{"t":"find","n":true}"#));

    assert_eq!(data.apply(&mut updates, 0), PhoneChanges::default());
    assert_eq!(updates.find, Some(true));
}

#[test]
fn an_app_message_names_its_channel_and_carries_its_data() {
    assert_eq!(
        app_message_line("remote", "{\"button\":2}").as_deref(),
        Some("{\"t\":\"app\",\"ch\":\"remote\",\"d\":{\"button\":2}}\n")
    );
    assert_eq!(
        app_message_line("my_app-2", "{}").as_deref(),
        Some("{\"t\":\"app\",\"ch\":\"my_app-2\",\"d\":{}}\n")
    );
}

#[test]
fn an_app_message_that_could_break_the_line_is_not_sent() {
    let long_data = format!("{{\"a\":\"{}\"}}", "x".repeat(crate::MOST_LINE_BYTES));

    assert_eq!(app_message_line("", "{}"), None);
    assert_eq!(app_message_line("a\"b", "{}"), None);
    assert_eq!(app_message_line(&"c".repeat(33), "{}"), None);
    assert_eq!(app_message_line("remote", "2"), None);
    assert_eq!(app_message_line("remote", "{\"a\":1}\n{"), None);
    assert_eq!(app_message_line("remote", "{\"a\":1"), None);
    assert_eq!(app_message_line("remote", &long_data), None);
}
