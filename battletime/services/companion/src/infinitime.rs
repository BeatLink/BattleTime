//! InfiniTime's own GATT services, read and written in its byte formats, so Gadgetbridge's InfiniTime
//! support and other InfiniTime phone apps work with BattleTime.
//!
//! The phone writes music, navigation and weather one characteristic at a time; each write is read
//! here into the same messages the companion's lines give, so the shell keeps them in `PhoneData`.
//! The music events and motion values the watch notifies are built here too. Every reader takes
//! any bytes and gives `None` for a write it cannot use, so a bad write never panics.

use battletime_runtime::{EffectKind, Value};

use crate::alert::{cut_to_fit, whole_characters};
#[cfg(feature = "single-values")]
use crate::phone::metres_from_text;
use crate::{Forecast, ForecastDay, Message, Weather, WeatherCondition, MOST_FORECAST_DAYS};
#[cfg(feature = "single-values")]
use crate::{NavigationTurn, PhoneField};

// ============================================================================
// Music
// ============================================================================

#[cfg(feature = "single-values")]
/// The music characteristics the watch reads, by the last byte of their UUID's first part.
const MUSIC_STATUS: u8 = 0x02;
#[cfg(feature = "single-values")]
const MUSIC_ARTIST: u8 = 0x03;
#[cfg(feature = "single-values")]
const MUSIC_TRACK: u8 = 0x04;
#[cfg(feature = "single-values")]
const MUSIC_POSITION: u8 = 0x06;
#[cfg(feature = "single-values")]
const MUSIC_TOTAL_LENGTH: u8 = 0x07;

/// The event InfiniTime notifies when its music app opens, which asks the phone for the track.
pub const MUSIC_OPEN_EVENT: u8 = 0xe0;

/// The event for each `music_control` command, in the order the compiler numbers them.
const MUSIC_COMMAND_EVENTS: [u8; 6] = [0x00, 0x01, 0x03, 0x04, 0x05, 0x06];

#[cfg(feature = "single-values")]
/// The value a write to a music characteristic carries, or `None` for one the watch keeps no value for.
pub fn music_write(characteristic: u8, write: &[u8]) -> Option<Message<'static>> {
    let text = text_of(write);
    let field = match characteristic {
        MUSIC_STATUS => PhoneField::MusicIsPlaying(*write.first()? != 0),
        MUSIC_ARTIST => PhoneField::MusicArtist(cut_to_fit(text)),
        MUSIC_TRACK => PhoneField::MusicTrack(cut_to_fit(text)),
        MUSIC_POSITION => PhoneField::MusicPositionInSeconds(big_endian_seconds(write)?),
        MUSIC_TOTAL_LENGTH => PhoneField::MusicLengthInSeconds(big_endian_seconds(write)?),
        _ => return None,
    };
    Some(Message::PhoneField(field))
}

/// The event to notify for an app's `music_control` or `request_music_state`, or `None` for any other effect.
pub fn music_event(kind: EffectKind, command: Value) -> Option<u8> {
    match kind {
        EffectKind::MusicControl => usize::try_from(command)
            .ok()
            .and_then(|place| MUSIC_COMMAND_EVENTS.get(place))
            .copied(),
        EffectKind::RequestMusicState => Some(MUSIC_OPEN_EVENT),
        _ => None,
    }
}

#[cfg(feature = "single-values")]
/// A count of seconds written as four big-endian bytes.
fn big_endian_seconds(write: &[u8]) -> Option<Value> {
    let bytes: [u8; 4] = write.get(..4)?.try_into().ok()?;
    Some(Value::from(u32::from_be_bytes(bytes)))
}

/// The text before the first zero byte, up to its last whole character.
fn text_of(write: &[u8]) -> &str {
    let end = write.iter().position(|&byte| byte == 0).unwrap_or(write.len());
    whole_characters(&write[..end])
}

// ============================================================================
// Navigation
// ============================================================================

#[cfg(feature = "single-values")]
const NAVIGATION_FLAG: u8 = 0x01;
#[cfg(feature = "single-values")]
const NAVIGATION_NARRATIVE: u8 = 0x02;
#[cfg(feature = "single-values")]
const NAVIGATION_DISTANCE: u8 = 0x03;
#[cfg(feature = "single-values")]
const NAVIGATION_PROGRESS: u8 = 0x04;

#[cfg(feature = "single-values")]
const MOST_PROGRESS_PERCENT: u8 = 100;

#[cfg(feature = "single-values")]
/// The value a write to a navigation characteristic carries, or `None` for a distance it cannot read.
pub fn navigation_write(characteristic: u8, write: &[u8]) -> Option<Message<'static>> {
    let text = text_of(write);
    let field = match characteristic {
        NAVIGATION_FLAG => PhoneField::NavigationTurn(turn_of_flag(text)),
        NAVIGATION_NARRATIVE => PhoneField::NavigationInstruction(cut_to_fit(text)),
        NAVIGATION_DISTANCE => PhoneField::NavigationDistanceInMetres(metres_from_text(text)?),
        NAVIGATION_PROGRESS => {
            PhoneField::NavigationProgressPercent(Value::from((*write.first()?).min(MOST_PROGRESS_PERCENT)))
        }
        _ => return None,
    };
    Some(Message::PhoneField(field))
}

#[cfg(feature = "single-values")]
/// The words of the phone's icon names that pick a turn, each standing for one bit in `turn_of_flag`.
const TURN_WORDS: [&str; 9] = [
    "arrive",
    "flag",
    "roundabout",
    "rotary",
    "uturn",
    "left",
    "right",
    "slight",
    "sharp",
];
#[cfg(feature = "single-values")]
const ARRIVE_BITS: u16 = 0b11;
#[cfg(feature = "single-values")]
const ROUNDABOUT_BITS: u16 = 0b1100;
#[cfg(feature = "single-values")]
const U_TURN_BIT: u16 = 1 << 4;
#[cfg(feature = "single-values")]
const LEFT_BIT: u16 = 1 << 5;
#[cfg(feature = "single-values")]
const RIGHT_BIT: u16 = 1 << 6;
#[cfg(feature = "single-values")]
const SLIGHT_BIT: u16 = 1 << 7;
#[cfg(feature = "single-values")]
const SHARP_BIT: u16 = 1 << 8;

#[cfg(feature = "single-values")]
/// The turn for an InfiniTime icon name, such as `turn-slight-left` or `roundabout-right`, read by its words.
fn turn_of_flag(flag: &str) -> NavigationTurn {
    let mut found = 0;
    for part in flag.split('-') {
        if let Some(place) = TURN_WORDS.iter().position(|word| *word == part) {
            found |= 1 << place;
        }
    }
    let (straight, slight, sharp) = match found {
        _ if found & ARRIVE_BITS != 0 => return NavigationTurn::Arrive,
        _ if found & ROUNDABOUT_BITS != 0 => return NavigationTurn::Roundabout,
        _ if found & U_TURN_BIT != 0 => return NavigationTurn::UTurn,
        _ if found & LEFT_BIT != 0 => (
            NavigationTurn::Left,
            NavigationTurn::SlightLeft,
            NavigationTurn::SharpLeft,
        ),
        _ if found & RIGHT_BIT != 0 => (
            NavigationTurn::Right,
            NavigationTurn::SlightRight,
            NavigationTurn::SharpRight,
        ),
        _ => return NavigationTurn::Straight,
    };
    match found {
        _ if found & SLIGHT_BIT != 0 => slight,
        _ if found & SHARP_BIT != 0 => sharp,
        _ => straight,
    }
}

// ============================================================================
// Simple Weather
// ============================================================================

const CURRENT_WEATHER: u8 = 0;
const FORECAST: u8 = 1;

/// Where each part of a current weather write starts.
const TEMPERATURE_AT: usize = 10;
const LOW_AT: usize = 12;
const HIGH_AT: usize = 14;
const LOCATION_AT: usize = 16;
const LOCATION_BYTES: usize = 32;
const ICON_AT: usize = LOCATION_AT + LOCATION_BYTES;
const SUNRISE_AT: usize = ICON_AT + 1;
const SUNSET_AT: usize = SUNRISE_AT + 2;

/// Where a forecast's day count and its days start, and the bytes each day takes.
const DAY_COUNT_AT: usize = 10;
const DAYS_AT: usize = 11;
const DAY_BYTES: usize = 5;

const MINUTES_PER_DAY: i16 = 1440;
const MILLISECONDS_PER_MINUTE: Value = 60_000;

/// InfiniTime's weather icons, in its numbering, as the skies apps name.
const CONDITION_OF_ICON: [WeatherCondition; 9] = [
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

/// The weather or forecast a Simple Weather write carries, with sunrise and sunset counted from
/// `local_midnight`, today's local midnight in milliseconds since 1970; `None` for a write it cannot read.
pub fn simple_weather_write(write: &[u8], local_midnight: Value) -> Option<Message<'static>> {
    match (*write.first()?, *write.get(1)?) {
        (CURRENT_WEATHER, version @ (0 | 1)) => current_weather(write, version == 1, local_midnight),
        (FORECAST, 0) => forecast(write),
        _ => None,
    }
}

fn current_weather(write: &[u8], has_sun_times: bool, local_midnight: Value) -> Option<Message<'static>> {
    let location = text_of(write.get(LOCATION_AT..ICON_AT)?);
    let condition = condition_of_icon(*write.get(ICON_AT)?);
    let (sunrise, sunset) = match has_sun_times {
        true => sun_times(
            little_endian(write, SUNRISE_AT)?,
            little_endian(write, SUNSET_AT)?,
            local_midnight,
        ),
        false => (None, None),
    };
    Some(Message::Weather(Weather {
        temperature_in_celsius: Some(whole_celsius(little_endian(write, TEMPERATURE_AT)?)),
        low_in_celsius: Some(whole_celsius(little_endian(write, LOW_AT)?)),
        high_in_celsius: Some(whole_celsius(little_endian(write, HIGH_AT)?)),
        condition,
        location: cut_to_fit(location),
        sunrise,
        sunset,
    }))
}

fn forecast(write: &[u8]) -> Option<Message<'static>> {
    let day_count = usize::from(*write.get(DAY_COUNT_AT)?).min(MOST_FORECAST_DAYS);
    let mut days = Forecast::new();
    for day in write
        .get(DAYS_AT..)?
        .as_chunks::<DAY_BYTES>()
        .0
        .iter()
        .take(day_count)
    {
        let _ = days.push(ForecastDay {
            low_in_celsius: whole_celsius(i16::from_le_bytes([day[0], day[1]])),
            high_in_celsius: whole_celsius(i16::from_le_bytes([day[2], day[3]])),
            condition: condition_of_icon(day[4]),
        });
    }
    Some(Message::Forecast(days))
}

fn little_endian(write: &[u8], at: usize) -> Option<i16> {
    let bytes = write.get(at..at + 2)?;
    Some(i16::from_le_bytes([bytes[0], bytes[1]]))
}

/// Hundredths of a degree, rounded to the nearest whole degree.
fn whole_celsius(hundredths: i16) -> Value {
    let hundredths = Value::from(hundredths);
    (hundredths + if hundredths < 0 { -50 } else { 50 }) / 100
}

fn condition_of_icon(icon: u8) -> Option<WeatherCondition> {
    CONDITION_OF_ICON.get(usize::from(icon)).copied()
}

/// Sunrise and sunset from minutes into the local day, or neither when they are unknown, the sun stays up or down, or they are out of order.
fn sun_times(sunrise: i16, sunset: i16, local_midnight: Value) -> (Option<Value>, Option<Value>) {
    let in_day = |minute: i16| (0..MINUTES_PER_DAY).contains(&minute);
    if !(in_day(sunrise) && in_day(sunset) && sunrise < sunset) {
        return (None, None);
    }
    let at = |minute: i16| Some(local_midnight + Value::from(minute) * MILLISECONDS_PER_MINUTE);
    (at(sunrise), at(sunset))
}

// ============================================================================
// Motion
// ============================================================================

/// The x, y and z acceleration in milli-g, as three little-endian 16-bit numbers.
pub fn motion_values(x: i32, y: i32, z: i32) -> [u8; 6] {
    let mut values = [0; 6];
    for (place, axis) in [x, y, z].into_iter().enumerate() {
        let clamped = axis.clamp(i16::MIN.into(), i16::MAX.into()) as i16;
        values[place * 2..place * 2 + 2].copy_from_slice(&clamped.to_le_bytes());
    }
    values
}
