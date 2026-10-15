//! Writes to InfiniTime's music, navigation and Simple Weather services, read as the watch reads them
//! and shown by the same apps the companion's lines fill.

use battletime_app_testing::{EffectKind, SystemValue, TestWatch};
use battletime_companion::{music_event, music_write, navigation_write, simple_weather_write, Message};
use battletime_runtime::SystemValues;

fn receive(watch: &mut TestWatch, message: Option<Message<'_>>) {
    watch
        .receive_phone_message(message.expect("a write the watch reads"))
        .unwrap();
}

// ============================================================================
// Music
// ============================================================================

#[test]
fn the_music_app_shows_what_the_music_service_was_written() {
    let mut watch = TestWatch::open_app("music").unwrap();

    receive(&mut watch, music_write(0x03, b"The Artist"));
    receive(&mut watch, music_write(0x04, b"A Song"));
    receive(&mut watch, music_write(0x07, &200u32.to_be_bytes()));
    receive(&mut watch, music_write(0x06, &65u32.to_be_bytes()));
    receive(&mut watch, music_write(0x02, &[1]));

    assert!(watch.shows_text("The Artist"), "{:?}", watch.visible_texts());
    assert!(watch.shows_text("A Song"));
    assert!(watch.shows_text("01:05/03:20"), "{:?}", watch.visible_texts());
    assert!(watch.failures().is_empty(), "{:?}", watch.failures());
}

#[test]
fn the_music_apps_controls_have_infinitimes_events() {
    let mut watch = TestWatch::open_app("music").unwrap();
    let opened = watch.effects_of(EffectKind::RequestMusicState);
    assert_eq!(opened.len(), 1);
    assert_eq!(music_event(EffectKind::RequestMusicState, 0), Some(0xe0));

    // The play button, in the middle of the bottom row, plays while nothing plays.
    watch.tap(120, 205);
    let play = watch.effects_of(EffectKind::MusicControl);
    assert_eq!(play.len(), 1);
    assert_eq!(
        music_event(EffectKind::MusicControl, play[0].number(0)),
        Some(0x00)
    );
}

// ============================================================================
// Navigation
// ============================================================================

#[test]
fn the_navigation_app_follows_the_navigation_service() {
    let mut watch = TestWatch::open_app("navigation").unwrap();

    receive(&mut watch, navigation_write(0x01, b"turn-slight-left"));
    receive(&mut watch, navigation_write(0x02, b"Bear left onto High Street"));
    receive(&mut watch, navigation_write(0x03, b"1.2km"));
    receive(&mut watch, navigation_write(0x04, &[40]));

    let system = watch.system();
    assert_eq!(system.read(SystemValue::NavigationTurn), 3);
    assert_eq!(system.read(SystemValue::NavigationProgressPercent), 40);
    assert!(
        watch.shows_text("Bear left onto High Street"),
        "{:?}",
        watch.visible_texts()
    );
    assert!(watch.shows_text("1.2 km"), "{:?}", watch.visible_texts());
    assert!(watch.failures().is_empty(), "{:?}", watch.failures());
}

// ============================================================================
// Simple Weather
// ============================================================================

/// A version 1 current weather write: 12.4 °C, low 8, high 15, rain, in Bergen, sun 06:30 to 20:00.
fn current_weather_write() -> Vec<u8> {
    let mut write = vec![0, 1];
    write.extend_from_slice(&0u64.to_le_bytes());
    for hundredths in [1240i16, 800, 1500] {
        write.extend_from_slice(&hundredths.to_le_bytes());
    }
    let mut location = [0u8; 32];
    location[..6].copy_from_slice(b"Bergen");
    write.extend_from_slice(&location);
    write.push(5);
    write.extend_from_slice(&390i16.to_le_bytes());
    write.extend_from_slice(&1200i16.to_le_bytes());
    write
}

/// A forecast of two days: 3 to 9 °C with snow, then 10 to 21 °C and clear.
fn forecast_write() -> Vec<u8> {
    let mut write = vec![1, 0];
    write.extend_from_slice(&0u64.to_le_bytes());
    write.push(2);
    for (low, high, icon) in [(300i16, 900i16, 7u8), (1000, 2100, 0)] {
        write.extend_from_slice(&low.to_le_bytes());
        write.extend_from_slice(&high.to_le_bytes());
        write.push(icon);
    }
    write
}

#[test]
fn the_weather_app_shows_the_simple_weather_services_weather_and_forecast() {
    let mut watch = TestWatch::open_app("weather").unwrap();
    let midnight = 1_767_571_200_000;

    receive(
        &mut watch,
        simple_weather_write(&current_weather_write(), midnight),
    );
    receive(&mut watch, simple_weather_write(&forecast_write(), midnight));

    let system = watch.system();
    assert_eq!(system.read(SystemValue::WeatherTemperature), 12);
    assert_eq!(system.read(SystemValue::WeatherSunrise), midnight + 390 * 60_000);
    assert_eq!(system.read(SystemValue::WeatherForecastLength), 2);
    assert_eq!(system.read_element(SystemValue::WeatherForecastHigh, 1), 21);
    assert!(watch.shows_text("Bergen"), "{:?}", watch.visible_texts());
    assert!(watch.shows_text("Rain"));
    assert!(watch.shows_text("21"), "{:?}", watch.visible_texts());
    assert!(watch.failures().is_empty(), "{:?}", watch.failures());
}
