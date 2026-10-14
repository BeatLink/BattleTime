extern crate std;

use std::vec::Vec;

use neotime_runtime::{EffectKind, SystemValue, NONE};

use crate::*;

fn field_of(message: Message<'_>) -> PhoneField {
    field(Some(message))
}

fn field(message: Option<Message<'_>>) -> PhoneField {
    match message {
        Some(Message::PhoneField(field)) => field,
        other => panic!("{other:?}"),
    }
}

/// Puts each message through the updates into the kept data, as the shell does.
fn kept(messages: Vec<Message<'_>>) -> (PhoneData, PhoneChanges) {
    let mut updates = PhoneUpdates::new();
    for message in messages {
        assert!(updates.record(message));
    }
    let mut data = PhoneData::default();
    let changes = data.apply(&mut updates, 0);
    assert!(updates.is_empty());
    (data, changes)
}

// ============================================================================
// Music
// ============================================================================

#[test]
fn the_artist_and_track_are_read_as_text_up_to_a_zero_byte() {
    assert_eq!(
        field(music_write(0x03, b"Artist\0junk")),
        PhoneField::MusicArtist("Artist".try_into().unwrap())
    );
    assert_eq!(
        field(music_write(0x04, b"Track")),
        PhoneField::MusicTrack("Track".try_into().unwrap())
    );
}

#[test]
fn a_long_or_broken_artist_is_cut_at_a_whole_character() {
    let long = "é".repeat(40);
    let PhoneField::MusicArtist(artist) = field(music_write(0x03, long.as_bytes())) else {
        panic!();
    };
    assert_eq!(artist.len(), MUSIC_ARTIST_BYTES);
    assert_eq!(
        field(music_write(0x03, &[b'A', 0xff, b'B'])),
        PhoneField::MusicArtist("A".try_into().unwrap())
    );
}

#[test]
fn the_status_byte_says_whether_it_plays() {
    assert_eq!(field(music_write(0x02, &[1])), PhoneField::MusicIsPlaying(true));
    assert_eq!(field(music_write(0x02, &[0])), PhoneField::MusicIsPlaying(false));
    assert_eq!(music_write(0x02, &[]), None);
}

#[test]
fn position_and_length_are_big_endian_seconds() {
    assert_eq!(
        field(music_write(0x06, &[0, 0, 1, 2])),
        PhoneField::MusicPositionInSeconds(258)
    );
    assert_eq!(
        field(music_write(0x07, &[0xff, 0xff, 0xff, 0xff])),
        PhoneField::MusicLengthInSeconds(u32::MAX.into())
    );
    assert_eq!(music_write(0x06, &[0, 1, 2]), None);
}

#[test]
fn values_the_watch_has_no_place_for_are_passed_over() {
    for characteristic in [0x01, 0x05, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0xff] {
        assert_eq!(
            music_write(characteristic, &[0, 0, 0, 1]),
            None,
            "{characteristic:#x}"
        );
    }
}

#[test]
fn single_music_values_change_only_their_own_part() {
    let (data, changes) = kept(Vec::from([
        music_write(0x03, b"Artist").unwrap(),
        music_write(0x04, b"Track").unwrap(),
        music_write(0x02, &[1]).unwrap(),
        music_write(0x06, &[0, 0, 0, 30]).unwrap(),
        music_write(0x07, &[0, 0, 0, 200]).unwrap(),
        music_write(0x03, b"Other").unwrap(),
    ]));

    assert!(changes.music && !changes.navigation && !changes.weather);
    assert_eq!(data.read_text(SystemValue::MusicArtist), Some("Other"));
    assert_eq!(data.read_text(SystemValue::MusicTrack), Some("Track"));
    assert_eq!(data.read(SystemValue::MusicIsPlaying), Some(1));
    assert_eq!(data.read(SystemValue::MusicPosition), Some(30_000));
    assert_eq!(data.read(SystemValue::MusicLength), Some(200_000));
}

#[test]
fn values_wait_in_order_and_the_last_of_each_wins() {
    let mut updates = PhoneUpdates::new();
    for name in [&b"One"[..], b"Two", b"Three"] {
        updates.record(music_write(0x03, name).unwrap());
    }
    for _ in 0..20 {
        updates.record(music_write(0x04, b"Track").unwrap());
    }
    assert_eq!(updates.fields.len(), 16);

    let mut data = PhoneData::default();
    data.apply(&mut updates, 0);
    assert_eq!(data.read_text(SystemValue::MusicArtist), Some("Three"));
    assert_eq!(data.read_text(SystemValue::MusicTrack), Some("Track"));
}

#[test]
fn each_music_command_has_infinitimes_event() {
    let events: Vec<_> = (0..6)
        .map(|command| music_event(EffectKind::MusicControl, command))
        .collect();
    assert_eq!(
        events,
        [
            Some(0x00),
            Some(0x01),
            Some(0x03),
            Some(0x04),
            Some(0x05),
            Some(0x06)
        ]
    );
    assert_eq!(music_event(EffectKind::MusicControl, 6), None);
    assert_eq!(music_event(EffectKind::MusicControl, -1), None);
    assert_eq!(
        music_event(EffectKind::RequestMusicState, 0),
        Some(MUSIC_OPEN_EVENT)
    );
    assert_eq!(music_event(EffectKind::FindPhone, 1), None);
}

// ============================================================================
// Navigation
// ============================================================================

#[test]
fn each_kind_of_icon_name_has_its_turn() {
    let turns = [
        ("turn-left", 1),
        ("turn-right", 2),
        ("turn-slight-left", 3),
        ("continue-slight-right", 4),
        ("turn-sharp-left", 5),
        ("on-ramp-sharp-right", 6),
        ("uturn", 7),
        ("continue-uturn", 7),
        ("roundabout-left", 8),
        ("rotary", 8),
        ("arrive-left", 9),
        ("flag", 9),
        ("continue", 0),
        ("turn-straight", 0),
        ("invalid", 0),
        ("", 0),
    ];
    let by_number = [
        NavigationTurn::Straight,
        NavigationTurn::Left,
        NavigationTurn::Right,
        NavigationTurn::SlightLeft,
        NavigationTurn::SlightRight,
        NavigationTurn::SharpLeft,
        NavigationTurn::SharpRight,
        NavigationTurn::UTurn,
        NavigationTurn::Roundabout,
        NavigationTurn::Arrive,
    ];
    for (flag, turn) in turns {
        assert_eq!(
            field(navigation_write(0x01, flag.as_bytes())),
            PhoneField::NavigationTurn(by_number[turn]),
            "{flag}"
        );
    }
}

#[test]
fn distances_are_read_as_the_bangle_js_line_reads_them() {
    for (text, metres) in [
        ("350m", Some(350)),
        ("1.2km", Some(1200)),
        ("0,5 mi", Some(805)),
        ("far", None),
    ] {
        assert_eq!(
            navigation_write(0x03, text.as_bytes()).map(field_of),
            metres.map(PhoneField::NavigationDistanceInMetres),
            "{text}"
        );
    }
}

#[test]
fn the_narrative_and_progress_are_kept() {
    let (data, changes) = kept(Vec::from([
        navigation_write(0x01, b"turn-left").unwrap(),
        navigation_write(0x02, b"Turn left onto Main Street").unwrap(),
        navigation_write(0x03, b"250m").unwrap(),
        navigation_write(0x04, &[140]).unwrap(),
    ]));

    assert!(changes.navigation && !changes.music);
    assert_eq!(data.read(SystemValue::NavigationTurn), Some(1));
    assert_eq!(
        data.read_text(SystemValue::NavigationInstruction),
        Some("Turn left onto Main Street")
    );
    assert_eq!(data.read(SystemValue::NavigationDistance), Some(250));
    assert_eq!(data.read(SystemValue::NavigationProgressPercent), Some(100));
    assert_eq!(navigation_write(0x04, &[]), None);
    assert_eq!(navigation_write(0x05, b"x"), None);
}

// ============================================================================
// Simple Weather
// ============================================================================

const MIDNIGHT: i64 = 1_767_571_200_000;

/// A current weather write as Gadgetbridge builds it, with sunrise and sunset when given.
fn current_weather(temperature: i16, low: i16, high: i16, icon: u8, sun: Option<(i16, i16)>) -> Vec<u8> {
    let mut write = Vec::from([0, sun.is_some() as u8]);
    write.extend_from_slice(&1_767_600_000u64.to_le_bytes());
    for hundredths in [temperature, low, high] {
        write.extend_from_slice(&hundredths.to_le_bytes());
    }
    let mut location = [0u8; 32];
    location[..6].copy_from_slice(b"Bergen");
    write.extend_from_slice(&location);
    write.push(icon);
    if let Some((sunrise, sunset)) = sun {
        write.extend_from_slice(&sunrise.to_le_bytes());
        write.extend_from_slice(&sunset.to_le_bytes());
    }
    write
}

fn weather(write: &[u8]) -> Weather {
    match simple_weather_write(write, MIDNIGHT) {
        Some(Message::Weather(weather)) => weather,
        other => panic!("{other:?}"),
    }
}

#[test]
fn current_weather_is_read_in_whole_degrees() {
    let weather = weather(&current_weather(1249, -351, 2050, 2, None));
    assert_eq!(weather.temperature_in_celsius, Some(12));
    assert_eq!(weather.low_in_celsius, Some(-4));
    assert_eq!(weather.high_in_celsius, Some(21));
    assert_eq!(weather.condition, Some(WeatherCondition::Clouds));
    assert_eq!(weather.location, "Bergen");
    assert_eq!((weather.sunrise, weather.sunset), (None, None));
}

#[test]
fn each_icon_has_its_sky() {
    let skies = [
        WeatherCondition::Clear,
        WeatherCondition::FewClouds,
        WeatherCondition::Clouds,
        WeatherCondition::HeavyClouds,
        WeatherCondition::Showers,
        WeatherCondition::Rain,
        WeatherCondition::Thunderstorm,
        WeatherCondition::Snow,
        WeatherCondition::Mist,
    ];
    for (icon, sky) in skies.into_iter().enumerate() {
        assert_eq!(
            weather(&current_weather(0, 0, 0, icon as u8, None)).condition,
            Some(sky)
        );
    }
    assert_eq!(weather(&current_weather(0, 0, 0, 255, None)).condition, None);
}

#[test]
fn version_one_adds_sunrise_and_sunset_on_the_local_day() {
    let weather = weather(&current_weather(0, 0, 0, 0, Some((390, 1200))));
    assert_eq!(weather.sunrise, Some(MIDNIGHT + 390 * 60_000));
    assert_eq!(weather.sunset, Some(MIDNIGHT + 1200 * 60_000));
}

#[test]
fn sun_times_that_are_unknown_or_out_of_order_are_left_out() {
    for sun in [
        (-1, 1200),
        (390, -1),
        (-2, -2),
        (390, -2),
        (1200, 390),
        (0, 1440),
        (500, 500),
    ] {
        let weather = weather(&current_weather(0, 0, 0, 0, Some(sun)));
        assert_eq!((weather.sunrise, weather.sunset), (None, None), "{sun:?}");
    }
}

#[test]
fn short_or_unknown_weather_writes_are_passed_over() {
    let full = current_weather(0, 0, 0, 0, Some((390, 1200)));
    for length in 0..full.len() {
        assert_eq!(
            simple_weather_write(&full[..length], MIDNIGHT),
            None,
            "{length} bytes"
        );
    }
    let mut later_version = full.clone();
    later_version[1] = 2;
    assert_eq!(simple_weather_write(&later_version, MIDNIGHT), None);
    let mut other_kind = full;
    other_kind[0] = 2;
    assert_eq!(simple_weather_write(&other_kind, MIDNIGHT), None);
}

/// A forecast write of these days, each its low, high and icon, claiming `count` days.
fn forecast_write(count: u8, days: &[(i16, i16, u8)]) -> Vec<u8> {
    let mut write = Vec::from([1, 0]);
    write.extend_from_slice(&1_767_600_000u64.to_le_bytes());
    write.push(count);
    for &(low, high, icon) in days {
        write.extend_from_slice(&low.to_le_bytes());
        write.extend_from_slice(&high.to_le_bytes());
        write.push(icon);
    }
    write
}

#[test]
fn a_forecast_fills_the_coming_days() {
    let write = forecast_write(3, &[(800, 1500, 0), (-250, 449, 7), (1000, 2000, 4)]);
    let (data, changes) = kept(Vec::from([simple_weather_write(&write, MIDNIGHT).unwrap()]));

    assert!(changes.weather);
    assert_eq!(data.read(SystemValue::WeatherForecastLength), Some(3));
    assert_eq!(data.read_element(SystemValue::WeatherForecastLow, 1), Some(-3));
    assert_eq!(data.read_element(SystemValue::WeatherForecastHigh, 1), Some(4));
    assert_eq!(
        data.read_element(SystemValue::WeatherForecastCondition, 2),
        Some(WeatherCondition::Showers as i64)
    );
    assert_eq!(data.read_element(SystemValue::WeatherForecastLow, 3), Some(NONE));
    assert_eq!(data.read_element(SystemValue::WeatherForecastLow, -1), Some(NONE));
    assert_eq!(data.read_element(SystemValue::StepDayTotal, 0), None);
}

#[test]
fn a_forecast_keeps_only_the_days_it_holds_and_at_most_five() {
    let Some(Message::Forecast(days)) =
        simple_weather_write(&forecast_write(4, &[(0, 0, 0), (0, 0, 0)]), MIDNIGHT)
    else {
        panic!();
    };
    assert_eq!(days.len(), 2);
    let seven = [(0, 0, 0); 7];
    let Some(Message::Forecast(days)) = simple_weather_write(&forecast_write(7, &seven), MIDNIGHT) else {
        panic!();
    };
    assert_eq!(days.len(), MOST_FORECAST_DAYS);
    assert_eq!(simple_weather_write(&[1, 0, 0, 0], MIDNIGHT), None);
}

#[test]
fn the_forecast_reads_empty_before_the_phone_sends_one() {
    let data = PhoneData::default();
    assert_eq!(data.read(SystemValue::WeatherForecastLength), Some(0));
    assert_eq!(data.read_element(SystemValue::WeatherForecastHigh, 0), Some(NONE));
    assert_eq!(data.read(SystemValue::WeatherSunrise), Some(NONE));
}

#[test]
fn the_kept_weather_gives_its_sunrise_and_sunset() {
    let write = current_weather(0, 0, 0, 0, Some((390, 1200)));
    let (data, _) = kept(Vec::from([simple_weather_write(&write, MIDNIGHT).unwrap()]));
    assert_eq!(
        data.read(SystemValue::WeatherSunrise),
        Some(MIDNIGHT + 390 * 60_000)
    );
    assert_eq!(
        data.read(SystemValue::WeatherSunset),
        Some(MIDNIGHT + 1200 * 60_000)
    );
}

// ============================================================================
// Motion
// ============================================================================

#[test]
fn motion_values_are_three_little_endian_numbers_held_to_sixteen_bits() {
    assert_eq!(motion_values(1, -2, 1000), [1, 0, 0xfe, 0xff, 0xe8, 0x03]);
    assert_eq!(
        motion_values(100_000, -100_000, 0),
        [0xff, 0x7f, 0x00, 0x80, 0, 0]
    );
}
