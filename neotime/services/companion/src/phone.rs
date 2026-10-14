//! The phone's calls, music, weather, navigation and other Gadgetbridge messages besides settings and notifications.
//!
//! Each message is read into a small struct whose text is unescaped and cut at a whole character.
//! `PhoneUpdates` keeps the latest of each kind until the shell takes them, and `PhoneData` keeps
//! the music, weather and navigation the apps read as system values. The watch's own lines to the phone, such
//! as the `status` reply and music commands, are written here too.

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
#[cfg(feature = "single-values")]
use alloc::vec::Vec;
use core::fmt::Write;

use neotime_runtime::{SystemValue, Value, NONE};
use serde::Deserialize;
use serde_json_core::str::EscapedStr;

use crate::{unescaped, Message, MessageError};

/// The longest line the watch sends the phone besides the settings report.
pub const SHORT_LINE_BYTES: usize = 96;

/// One short line the watch sends, newline included, small enough for any task's stack.
pub type ShortLine = heapless::String<SHORT_LINE_BYTES>;

// ============================================================================
// Reading calls, music and weather
// ============================================================================

/// The fields of `call`, `musicinfo`, `musicstate`, `weather` and `nav`, read by one parser to keep
/// the firmware small; no two of these messages use a key in different ways.
#[derive(Deserialize)]
struct PhoneFields<'line> {
    #[serde(default, borrow)]
    cmd: Option<&'line str>,
    #[serde(default, borrow)]
    name: Option<EscapedStr<'line>>,
    #[serde(default, borrow)]
    number: Option<EscapedStr<'line>>,
    #[serde(default, borrow)]
    artist: Option<EscapedStr<'line>>,
    #[serde(default, borrow)]
    track: Option<EscapedStr<'line>>,
    #[serde(default)]
    dur: Option<i64>,
    #[serde(default, borrow)]
    state: Option<&'line str>,
    #[serde(default)]
    position: Option<i64>,
    #[serde(default)]
    temp: Option<i64>,
    #[serde(default)]
    lo: Option<i64>,
    #[serde(default)]
    hi: Option<i64>,
    #[serde(default)]
    code: Option<i64>,
    #[serde(default, borrow)]
    loc: Option<EscapedStr<'line>>,
    #[serde(default, borrow)]
    instr: Option<EscapedStr<'line>>,
    #[serde(default, borrow)]
    distance: Option<&'line str>,
    #[serde(default, borrow)]
    action: Option<&'line str>,
}

/// Reads a `call`, `musicinfo`, `musicstate`, `weather` or `nav` message, by its `t`.
#[inline(never)]
pub(crate) fn parse_phone_message<'line>(
    kind: &'line str,
    json: &'line str,
) -> Result<Message<'line>, MessageError> {
    let (fields, _) = serde_json_core::from_str::<PhoneFields>(json).map_err(|_| MessageError::NotJson)?;
    Ok(match kind {
        "call" => call_from(fields),
        "musicinfo" => music_info_from(fields),
        "musicstate" => music_state_from(fields),
        "weather" => Message::Weather(weather_from(fields)),
        "nav" => Message::Navigation(navigation_from(fields)),
        _ => Message::Unknown { kind },
    })
}

// ============================================================================
// Calls
// ============================================================================

/// The longest caller name kept; a longer one is cut short.
pub const CALLER_BYTES: usize = 32;

/// Numbered as apps read `current_call.state`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallKind {
    Incoming,
    Answered,
    Ended,
}

/// A call on the phone: its kind, and the caller's name, or their number when the phone has no name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhoneCall {
    pub kind: CallKind,
    pub caller: heapless::String<CALLER_BYTES>,
}

/// Gadgetbridge's call commands, as the kinds apps see.
fn call_kind_of(command: &str) -> Option<CallKind> {
    match command {
        "incoming" => Some(CallKind::Incoming),
        "accept" | "start" | "outgoing" => Some(CallKind::Answered),
        "reject" | "end" => Some(CallKind::Ended),
        _ => None,
    }
}

fn call_from(fields: PhoneFields<'_>) -> Message<'static> {
    let Some(kind) = fields.cmd.and_then(call_kind_of) else {
        return Message::Unknown { kind: "call" };
    };
    let mut caller = unescaped(fields.name);
    if caller.is_empty() {
        caller = unescaped(fields.number);
    }
    Message::Call(PhoneCall { kind, caller })
}

// ============================================================================
// Music
// ============================================================================

/// The longest artist and track title kept; longer text is cut short.
pub const MUSIC_ARTIST_BYTES: usize = 32;
pub const MUSIC_TRACK_BYTES: usize = 64;

const MILLISECONDS_PER_SECOND: Value = 1000;

/// `musicinfo`: the track the phone's player has.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MusicInfo {
    pub artist: heapless::String<MUSIC_ARTIST_BYTES>,
    pub track: heapless::String<MUSIC_TRACK_BYTES>,
    pub length_in_seconds: Value,
}

/// `musicstate`: whether the player is playing, and how far into the track, when the phone says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MusicState {
    pub is_playing: bool,
    pub position_in_seconds: Option<Value>,
}

fn music_info_from(fields: PhoneFields<'_>) -> Message<'static> {
    Message::MusicInfo(MusicInfo {
        artist: unescaped(fields.artist),
        track: unescaped(fields.track),
        length_in_seconds: fields.dur.unwrap_or(0).max(0),
    })
}

fn music_state_from(fields: PhoneFields<'_>) -> Message<'static> {
    Message::MusicState(MusicState {
        is_playing: fields.state == Some("play"),
        position_in_seconds: fields.position.map(|seconds| seconds.max(0)),
    })
}

// ============================================================================
// Weather
// ============================================================================

/// The longest place name kept; a longer one is cut short.
pub const WEATHER_LOCATION_BYTES: usize = 32;

/// Gadgetbridge sends temperatures in whole kelvin.
const KELVIN_AT_ZERO_CELSIUS: i64 = 273;

/// The skies apps name, numbered in the order the compiler gives `WeatherCondition`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherCondition {
    Clear,
    FewClouds,
    Clouds,
    HeavyClouds,
    Rain,
    Showers,
    Thunderstorm,
    Snow,
    Mist,
}

impl WeatherCondition {
    /// The sky for an OpenWeatherMap condition code, which Gadgetbridge passes on.
    pub fn from_code(code: i64) -> Option<WeatherCondition> {
        use WeatherCondition::*;
        Some(match code {
            200..=299 => Thunderstorm,
            300..=399 | 520..=531 => Showers,
            511 | 600..=699 => Snow,
            500..=599 => Rain,
            700..=799 => Mist,
            800 => Clear,
            801 => FewClouds,
            802 => Clouds,
            803 | 804 => HeavyClouds,
            _ => return None,
        })
    }
}

/// `weather`: the phone's forecast, in whole degrees Celsius.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Weather {
    pub temperature_in_celsius: Option<Value>,
    pub low_in_celsius: Option<Value>,
    pub high_in_celsius: Option<Value>,
    pub condition: Option<WeatherCondition>,
    pub location: heapless::String<WEATHER_LOCATION_BYTES>,
    /// Today's sunrise and sunset in milliseconds since 1970, which only InfiniTime's weather service carries.
    pub sunrise: Option<Value>,
    pub sunset: Option<Value>,
}

/// The most coming days a forecast holds.
pub const MOST_FORECAST_DAYS: usize = 5;

/// One coming day of the forecast, in whole degrees Celsius.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ForecastDay {
    pub condition: Option<WeatherCondition>,
    pub low_in_celsius: Value,
    pub high_in_celsius: Value,
}

/// The coming days, today first.
pub type Forecast = heapless::Vec<ForecastDay, MOST_FORECAST_DAYS>;

fn celsius_from_kelvin(kelvin: Option<i64>) -> Option<Value> {
    kelvin.map(|kelvin| kelvin.saturating_sub(KELVIN_AT_ZERO_CELSIUS))
}

fn weather_from(fields: PhoneFields<'_>) -> Weather {
    Weather {
        temperature_in_celsius: celsius_from_kelvin(fields.temp),
        low_in_celsius: celsius_from_kelvin(fields.lo),
        high_in_celsius: celsius_from_kelvin(fields.hi),
        condition: fields.code.and_then(WeatherCondition::from_code),
        location: unescaped(fields.loc),
        sunrise: None,
        sunset: None,
    }
}

// ============================================================================
// Navigation
// ============================================================================

/// The longest instruction kept; a longer one is cut short.
pub const NAVIGATION_INSTRUCTION_BYTES: usize = 64;

/// The turns apps name, numbered in the order the compiler gives `NavigationTurn`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationTurn {
    Straight,
    Left,
    Right,
    SlightLeft,
    SlightRight,
    SharpLeft,
    SharpRight,
    UTurn,
    Roundabout,
    Arrive,
}

/// Gadgetbridge's Bangle.js `action` names and the turns they show as; `offroute` shows none.
const NAVIGATION_ACTIONS: [(&str, NavigationTurn); 16] = [
    ("continue", NavigationTurn::Straight),
    ("left", NavigationTurn::Left),
    ("right", NavigationTurn::Right),
    ("left_slight", NavigationTurn::SlightLeft),
    ("keep_left", NavigationTurn::SlightLeft),
    ("right_slight", NavigationTurn::SlightRight),
    ("keep_right", NavigationTurn::SlightRight),
    ("left_sharp", NavigationTurn::SharpLeft),
    ("right_sharp", NavigationTurn::SharpRight),
    ("uturn_left", NavigationTurn::UTurn),
    ("uturn_right", NavigationTurn::UTurn),
    ("roundabout_right", NavigationTurn::Roundabout),
    ("roundabout_left", NavigationTurn::Roundabout),
    ("roundabout_straight", NavigationTurn::Roundabout),
    ("roundabout_uturn", NavigationTurn::Roundabout),
    ("finish", NavigationTurn::Arrive),
];

impl NavigationTurn {
    /// The turn for one of the `action` names Gadgetbridge's Bangle.js driver sends.
    pub fn from_action(action: &str) -> Option<NavigationTurn> {
        NAVIGATION_ACTIONS
            .iter()
            .find(|(name, _)| *name == action)
            .map(|&(_, turn)| turn)
    }
}

/// `nav`: the next step of the phone's navigation; one with nothing in it means navigation ended.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Navigation {
    pub turn: Option<NavigationTurn>,
    pub instruction: heapless::String<NAVIGATION_INSTRUCTION_BYTES>,
    pub distance_in_metres: Option<Value>,
    /// How much of the route is done, which only InfiniTime's navigation service carries.
    pub progress_percent: Option<Value>,
}

impl Navigation {
    /// Whether this says navigation ended: no turn, instruction or distance.
    pub fn has_ended(&self) -> bool {
        *self == Navigation::default()
    }
}

fn navigation_from(fields: PhoneFields<'_>) -> Navigation {
    Navigation {
        turn: fields.action.and_then(NavigationTurn::from_action),
        instruction: unescaped(fields.instr),
        distance_in_metres: fields.distance.and_then(metres_from_text),
        progress_percent: None,
    }
}

/// Millimetres in each unit a phone writes a distance in.
const MILLIMETRES_PER_UNIT: [(&str, Value); 5] = [
    ("km", 1_000_000),
    ("mi", 1_609_344),
    ("ft", 305),
    ("yd", 914),
    ("m", 1_000),
];

/// A distance written as the phone shows it, such as `300 m`, `1.2 km` or `0,5 mi`, in whole metres.
///
/// A number with no unit counts as metres; one decimal place is kept, and an unknown unit gives `None`.
pub fn metres_from_text(text: &str) -> Option<Value> {
    let mut tenths_of_units: Value = 0;
    let mut digits = 0;
    let mut fraction_digits = None;
    let mut rest = text.trim().as_bytes();
    while let [byte, after @ ..] = rest {
        match (byte, fraction_digits) {
            (b'0'..=b'9', None) => {
                tenths_of_units = tenths_of_units
                    .checked_mul(10)?
                    .checked_add(Value::from(byte - b'0'))?;
                digits += 1;
            }
            (b'0'..=b'9', Some(0)) => {
                tenths_of_units = tenths_of_units.checked_add(Value::from(byte - b'0'))?;
                fraction_digits = Some(1);
            }
            (b'0'..=b'9', Some(_)) => {}
            (b'.' | b',', None) => {
                tenths_of_units = tenths_of_units.checked_mul(10)?;
                fraction_digits = Some(0);
            }
            _ => break,
        }
        rest = after;
    }
    if fraction_digits.is_none() {
        tenths_of_units = tenths_of_units.checked_mul(10)?;
    }
    let unit = core::str::from_utf8(rest).ok()?.trim();
    let millimetres = match unit {
        "" => 1_000,
        _ => {
            MILLIMETRES_PER_UNIT
                .iter()
                .find(|(name, _)| unit.eq_ignore_ascii_case(name))?
                .1
        }
    };
    (digits > 0).then(|| (tenths_of_units * millimetres + 5_000) / 10_000)
}

// ============================================================================
// Finding the watch and buzzing it
// ============================================================================

/// `vibrate` buzzes at least once and at most this many times.
pub const MOST_PHONE_BUZZES: u8 = 5;

/// The key both messages carry their value in, as it appears in the line.
const N_KEY: &str = "\"n\"";

/// The text of the `n` key's value, in a line `parse_line` has already read as JSON, or `None` without one.
///
/// Read by hand, byte by byte, since a JSON parser for one field cost about 850 B of flash each.
fn value_of_n(json: &str) -> Option<&str> {
    let bytes = json.as_bytes();
    let key_end = bytes
        .windows(N_KEY.len())
        .position(|window| window == N_KEY.as_bytes())?
        + N_KEY.len();
    let is_gap = |byte: &u8| *byte == b' ' || *byte == b':';
    let start = key_end + bytes[key_end..].iter().position(|byte| !is_gap(byte))?;
    let length = bytes[start..]
        .iter()
        .position(|&byte| matches!(byte, b',' | b'}' | b' '))
        .unwrap_or(bytes.len() - start);
    json.get(start..start + length)
}

pub(crate) fn parse_find(json: &str) -> Result<Message<'_>, MessageError> {
    let is_on = match value_of_n(json) {
        None | Some("false") => false,
        Some("true") => true,
        Some(_) => return Err(MessageError::NotJson),
    };
    Ok(Message::Find { is_on })
}

pub(crate) fn parse_vibrate(json: &str) -> Result<Message<'_>, MessageError> {
    let mut times: i64 = 1;
    if let Some(text) = value_of_n(json) {
        let digits = text.strip_prefix('-').unwrap_or(text);
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(MessageError::NotJson);
        }
        times = digits.bytes().fold(0, |count: i64, digit| {
            count.saturating_mul(10).saturating_add(i64::from(digit - b'0'))
        });
        if digits.len() < text.len() {
            times = 0;
        }
    }
    let times = times.clamp(1, MOST_PHONE_BUZZES as i64) as u8;
    Ok(Message::Vibrate { times })
}

// ============================================================================
// Single values from InfiniTime's music and navigation services
// ============================================================================

#[cfg(feature = "single-values")]
/// One music or navigation value, which InfiniTime's services carry each in its own characteristic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhoneField {
    MusicArtist(heapless::String<MUSIC_ARTIST_BYTES>),
    MusicTrack(heapless::String<MUSIC_TRACK_BYTES>),
    MusicIsPlaying(bool),
    MusicPositionInSeconds(Value),
    MusicLengthInSeconds(Value),
    NavigationTurn(NavigationTurn),
    NavigationInstruction(heapless::String<NAVIGATION_INSTRUCTION_BYTES>),
    NavigationDistanceInMetres(Value),
    NavigationProgressPercent(Value),
}

#[cfg(feature = "single-values")]
impl PhoneField {
    fn is_navigation(&self) -> bool {
        matches!(
            self,
            PhoneField::NavigationTurn(_)
                | PhoneField::NavigationInstruction(_)
                | PhoneField::NavigationDistanceInMetres(_)
                | PhoneField::NavigationProgressPercent(_)
        )
    }
}

// ============================================================================
// Waiting for the shell
// ============================================================================

#[cfg(feature = "single-values")]
/// The most single music and navigation values that wait for the shell; more are dropped.
const MOST_WAITING_FIELDS: usize = 16;

/// The phone's messages the shell has not yet taken, only the latest of each kind.
///
/// The larger ones wait on the heap, so an empty set costs a few words.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PhoneUpdates {
    pub call: Option<Box<PhoneCall>>,
    pub music_info: Option<Box<MusicInfo>>,
    pub music_state: Option<MusicState>,
    pub weather: Option<Box<Weather>>,
    pub navigation: Option<Box<Navigation>>,
    pub forecast: Option<Box<Forecast>>,
    #[cfg(feature = "single-values")]
    /// Single music and navigation values in the order they came, the shell taking them all at once.
    pub fields: Vec<PhoneField>,
    /// Whether to start or stop ringing the watch so the wearer can find it.
    pub find: Option<bool>,
    pub buzzes: Option<u8>,
}

impl PhoneUpdates {
    /// No updates, for a static.
    pub const fn new() -> PhoneUpdates {
        PhoneUpdates {
            call: None,
            music_info: None,
            music_state: None,
            weather: None,
            navigation: None,
            forecast: None,
            #[cfg(feature = "single-values")]
            fields: Vec::new(),
            find: None,
            buzzes: None,
        }
    }

    /// Keeps a call, music, weather, navigation, find or vibrate message, replacing one of its kind; false for any other.
    pub fn record(&mut self, message: Message<'_>) -> bool {
        match message {
            Message::Call(call) => self.call = Some(Box::new(call)),
            Message::MusicInfo(info) => self.music_info = Some(Box::new(info)),
            Message::MusicState(state) => self.music_state = Some(state),
            Message::Weather(weather) => self.weather = Some(Box::new(weather)),
            Message::Navigation(navigation) => self.navigation = Some(Box::new(navigation)),
            Message::Forecast(forecast) => self.forecast = Some(Box::new(forecast)),
            #[cfg(feature = "single-values")]
            Message::PhoneField(field) if self.fields.len() < MOST_WAITING_FIELDS => self.fields.push(field),
            #[cfg(feature = "single-values")]
            Message::PhoneField(_) => {}
            Message::Find { is_on } => self.find = Some(is_on),
            Message::Vibrate { times } => self.buzzes = Some(times),
            _ => return false,
        }
        true
    }

    pub fn is_empty(&self) -> bool {
        *self == PhoneUpdates::new()
    }
}

// ============================================================================
// What the watch keeps
// ============================================================================

/// Which of the kept values an update changed, so the open app hears of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PhoneChanges {
    pub music: bool,
    pub weather: bool,
    pub call: bool,
    pub navigation: bool,
}

#[derive(Debug, Default)]
struct Music {
    info: MusicInfo,
    is_playing: bool,
    position_in_seconds: Value,
}

/// The latest music, weather and navigation the phone sent, kept on the heap once the phone sends any.
#[derive(Debug, Default)]
pub struct PhoneData {
    /// The latest call, kept after it ends so its caller can still be shown.
    call: Option<Box<PhoneCall>>,
    music: Option<Box<Music>>,
    weather: Option<Box<Weather>>,
    weather_received_at: Value,
    /// The step navigation is at, or none once it ends.
    navigation: Option<Box<Navigation>>,
    forecast: Option<Box<Forecast>>,
}

/// The system values `PhoneData` gives, so a test system can copy them.
pub const MUSIC_VALUES: [SystemValue; 3] = [
    SystemValue::MusicIsPlaying,
    SystemValue::MusicPosition,
    SystemValue::MusicLength,
];
pub const MUSIC_TEXTS: [SystemValue; 2] = [SystemValue::MusicArtist, SystemValue::MusicTrack];
pub const WEATHER_VALUES: [SystemValue; 7] = [
    SystemValue::WeatherTemperature,
    SystemValue::WeatherCondition,
    SystemValue::WeatherLow,
    SystemValue::WeatherHigh,
    SystemValue::WeatherReceivedAt,
    SystemValue::WeatherSunrise,
    SystemValue::WeatherSunset,
];
pub const WEATHER_TEXTS: [SystemValue; 1] = [SystemValue::WeatherLocation];
pub const CALL_VALUES: [SystemValue; 1] = [SystemValue::CallState];
pub const NAVIGATION_VALUES: [SystemValue; 3] = [
    SystemValue::NavigationTurn,
    SystemValue::NavigationDistance,
    SystemValue::NavigationProgressPercent,
];
pub const NAVIGATION_TEXTS: [SystemValue; 1] = [SystemValue::NavigationInstruction];
pub const CALL_TEXTS: [SystemValue; 1] = [SystemValue::CallCaller];

/// `CallState` with no call under way, which is also its number for an ended call.
const NO_CALL: Value = 2;

fn number_or_none(value: Option<Value>) -> Value {
    value.unwrap_or(NONE)
}

impl PhoneData {
    /// Takes the music, weather and navigation out of the updates, at wall clock time `now`, and says what changed.
    pub fn apply(&mut self, updates: &mut PhoneUpdates, now: Value) -> PhoneChanges {
        let mut changes = PhoneChanges::default();
        // The call stays in the updates, since the shell passes it on to the open app as an event.
        if let Some(call) = &updates.call {
            self.call = Some(call.clone());
            changes.call = true;
        }
        if let Some(info) = updates.music_info.take() {
            self.music.get_or_insert_default().info = *info;
            changes.music = true;
        }
        if let Some(state) = updates.music_state.take() {
            let music = self.music.get_or_insert_default();
            music.is_playing = state.is_playing;
            if let Some(position) = state.position_in_seconds {
                music.position_in_seconds = position;
            }
            changes.music = true;
        }
        if let Some(weather) = updates.weather.take() {
            self.weather = Some(weather);
            self.weather_received_at = now;
            changes.weather = true;
        }
        if let Some(navigation) = updates.navigation.take() {
            self.navigation = (!navigation.has_ended()).then_some(navigation);
            changes.navigation = true;
        }
        if let Some(forecast) = updates.forecast.take() {
            self.forecast = Some(forecast);
            changes.weather = true;
        }
        #[cfg(feature = "single-values")]
        for field in core::mem::take(&mut updates.fields) {
            changes.navigation |= field.is_navigation();
            changes.music |= !field.is_navigation();
            self.apply_field(field);
        }
        changes
    }

    #[cfg(feature = "single-values")]
    fn apply_field(&mut self, field: PhoneField) {
        if field.is_navigation() {
            let navigation = self.navigation.get_or_insert_default();
            match field {
                PhoneField::NavigationTurn(turn) => navigation.turn = Some(turn),
                PhoneField::NavigationInstruction(instruction) => navigation.instruction = instruction,
                PhoneField::NavigationDistanceInMetres(metres) => {
                    navigation.distance_in_metres = Some(metres)
                }
                PhoneField::NavigationProgressPercent(percent) => navigation.progress_percent = Some(percent),
                _ => {}
            }
            return;
        }
        let music = self.music.get_or_insert_default();
        match field {
            PhoneField::MusicArtist(artist) => music.info.artist = artist,
            PhoneField::MusicTrack(track) => music.info.track = track,
            PhoneField::MusicIsPlaying(is_playing) => music.is_playing = is_playing,
            PhoneField::MusicPositionInSeconds(seconds) => music.position_in_seconds = seconds,
            PhoneField::MusicLengthInSeconds(seconds) => music.info.length_in_seconds = seconds,
            _ => {}
        }
    }

    /// A call, music, weather or navigation value, or `None` for any other value and for navigation before a step comes.
    ///
    /// Music and weather the phone never sent read as empty.
    pub fn read(&self, value: SystemValue) -> Option<Value> {
        let music = self.music.as_deref();
        let weather = self.weather.as_deref();
        let navigation = self.navigation.as_deref();
        let seconds = |seconds: Value| seconds.saturating_mul(MILLISECONDS_PER_SECOND);
        Some(match value {
            SystemValue::CallState => self.call.as_deref().map_or(NO_CALL, |call| call.kind as Value),
            SystemValue::MusicIsPlaying => music.is_some_and(|music| music.is_playing) as Value,
            SystemValue::MusicPosition => music.map_or(0, |music| seconds(music.position_in_seconds)),
            SystemValue::MusicLength => music.map_or(0, |music| seconds(music.info.length_in_seconds)),
            SystemValue::WeatherTemperature => number_or_none(weather.and_then(|w| w.temperature_in_celsius)),
            SystemValue::WeatherLow => number_or_none(weather.and_then(|w| w.low_in_celsius)),
            SystemValue::WeatherHigh => number_or_none(weather.and_then(|w| w.high_in_celsius)),
            SystemValue::WeatherCondition => {
                number_or_none(weather.and_then(|w| w.condition).map(|sky| sky as Value))
            }
            SystemValue::WeatherReceivedAt => number_or_none(weather.map(|_| self.weather_received_at)),
            SystemValue::WeatherSunrise => number_or_none(weather.and_then(|w| w.sunrise)),
            SystemValue::WeatherSunset => number_or_none(weather.and_then(|w| w.sunset)),
            SystemValue::WeatherForecastLength => {
                self.forecast.as_ref().map_or(0, |days| days.len() as Value)
            }
            // Without navigation the system's own values stand, as before the phone sent any.
            SystemValue::NavigationTurn => number_or_none(navigation?.turn.map(|turn| turn as Value)),
            SystemValue::NavigationDistance => number_or_none(navigation?.distance_in_metres),
            // Only InfiniTime's navigation service sends progress, so the bar stays empty without it.
            SystemValue::NavigationProgressPercent => navigation?.progress_percent.unwrap_or(0),
            _ => return None,
        })
    }

    /// One coming day's condition, low or high, or `None` for any other value; days past the forecast are empty.
    pub fn read_element(&self, value: SystemValue, index: Value) -> Option<Value> {
        let day = usize::try_from(index)
            .ok()
            .and_then(|place| self.forecast.as_ref()?.get(place));
        Some(match value {
            SystemValue::WeatherForecastCondition => {
                number_or_none(day.and_then(|day| day.condition).map(|sky| sky as Value))
            }
            SystemValue::WeatherForecastLow => number_or_none(day.map(|day| day.low_in_celsius)),
            SystemValue::WeatherForecastHigh => number_or_none(day.map(|day| day.high_in_celsius)),
            _ => return None,
        })
    }

    /// The caller, artist, track, place or navigation instruction, or `None` for any other value.
    pub fn read_text(&self, value: SystemValue) -> Option<&str> {
        let music = self.music.as_deref();
        let weather = self.weather.as_deref();
        Some(match value {
            SystemValue::CallCaller => self.call.as_deref().map_or("", |call| &call.caller),
            SystemValue::MusicArtist => music.map_or("", |music| &music.info.artist),
            SystemValue::MusicTrack => music.map_or("", |music| &music.info.track),
            SystemValue::WeatherLocation => weather.map_or("", |weather| &weather.location),
            SystemValue::NavigationInstruction => &self.navigation.as_deref()?.instruction,
            _ => return None,
        })
    }
}

// ============================================================================
// Lines to the phone
// ============================================================================

/// What the `status` reply reports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WatchStatus {
    pub battery_percent: u8,
    pub battery_millivolts: u16,
    pub steps_today: u32,
    pub is_charging: bool,
}

/// The answer to `status`: `{"t":"status","bat":...,"mv":...,"steps":...,"charging":...}`.
pub fn status_report(status: &WatchStatus) -> ShortLine {
    let mut line = ShortLine::new();
    let _ = writeln!(
        line,
        "{{\"t\":\"status\",\"bat\":{},\"mv\":{},\"steps\":{},\"charging\":{}}}",
        status.battery_percent, status.battery_millivolts, status.steps_today, status.is_charging
    );
    line
}

/// Gadgetbridge's names for `music_control`'s commands, in the order the compiler numbers them.
const MUSIC_COMMANDS: [&str; 6] = ["play", "pause", "next", "previous", "volumeup", "volumedown"];

/// Gadgetbridge's names for `call_control`'s commands: answer, reject and mute.
const CALL_COMMANDS: [&str; 3] = ["ACCEPT", "REJECT", "IGNORE"];

fn command_line(kind: &str, names: &[&str], number: Value) -> Option<ShortLine> {
    let name = usize::try_from(number).ok().and_then(|index| names.get(index))?;
    let mut line = ShortLine::new();
    let _ = writeln!(line, "{{\"t\":\"{kind}\",\"n\":\"{name}\"}}");
    Some(line)
}

/// The `music` line for a `music_control` command by its number, or `None` for a number out of range.
pub fn music_control_line(command: Value) -> Option<ShortLine> {
    command_line("music", &MUSIC_COMMANDS, command)
}

/// The `call` line for a `call_control` command by its number, or `None` for a number out of range.
pub fn call_control_line(command: Value) -> Option<ShortLine> {
    command_line("call", &CALL_COMMANDS, command)
}

/// What `request_music_state` and `request_navigation_state` ask the phone for, as the `n` of a `request` line.
const STATE_REQUESTS: [&str; 2] = ["music", "nav"];

/// The `request` line asking the phone to send its music again, or its navigation step when `navigation` is true.
pub fn state_request_line(navigation: bool) -> ShortLine {
    let mut line = ShortLine::new();
    let _ = writeln!(
        line,
        "{{\"t\":\"request\",\"n\":\"{}\"}}",
        STATE_REQUESTS[navigation as usize]
    );
    line
}

/// The `findPhone` line that starts or stops the phone ringing.
pub fn find_phone_line(is_on: bool) -> ShortLine {
    let mut line = ShortLine::new();
    let _ = writeln!(line, "{{\"t\":\"findPhone\",\"n\":{is_on}}}");
    line
}

/// The longest an app's channel name may be, as the compiler allows.
const LONGEST_APP_CHANNEL: usize = 32;

/// The `app` line for an app's `send_to_phone`: `{"t":"app","ch":"<channel>","d":<data>}`.
///
/// The compiler writes `data` as a JSON object of whole numbers; a channel or data that could break the
/// line, or a line longer than the phone reads, gives `None`.
pub fn app_message_line(channel: &str, data: &str) -> Option<String> {
    let channel_fits = (1..=LONGEST_APP_CHANNEL).contains(&channel.len())
        && channel
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    let data_fits = data.starts_with('{')
        && data.ends_with('}')
        && data.bytes().all(|byte| (b' '..=b'~').contains(&byte));
    if !channel_fits || !data_fits {
        return None;
    }
    let mut line = String::new();
    let _ = writeln!(line, "{{\"t\":\"app\",\"ch\":\"{channel}\",\"d\":{data}}}");
    (line.len() <= crate::MOST_LINE_BYTES).then_some(line)
}

#[cfg(test)]
#[path = "phone_tests.rs"]
mod phone_tests;
