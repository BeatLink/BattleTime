//! The phone's calls, music, weather and other Gadgetbridge messages besides settings and notifications.
//!
//! Each message is read into a small struct whose text is unescaped and cut at a whole character.
//! `PhoneUpdates` keeps the latest of each kind until the shell takes them, and `PhoneData` keeps
//! the music and weather the apps read as system values. The watch's own lines to the phone, such
//! as the `status` reply and music commands, are written here too.

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
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

/// The fields of `call`, `musicinfo`, `musicstate` and `weather`, read by one parser to keep the
/// firmware small; no two of these messages use a key in different ways.
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
}

/// Reads a `call`, `musicinfo`, `musicstate` or `weather` message, by its `t`.
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
        "weather" => weather_from(fields),
        _ => Message::Unknown { kind },
    })
}

// ============================================================================
// Calls
// ============================================================================

/// The longest caller name kept; a longer one is cut short.
pub const CALLER_BYTES: usize = 32;

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

fn call_from(fields: PhoneFields<'_>) -> Message<'_> {
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

fn music_info_from(fields: PhoneFields<'_>) -> Message<'_> {
    Message::MusicInfo(MusicInfo {
        artist: unescaped(fields.artist),
        track: unescaped(fields.track),
        length_in_seconds: fields.dur.unwrap_or(0).max(0),
    })
}

fn music_state_from(fields: PhoneFields<'_>) -> Message<'_> {
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
}

fn celsius_from_kelvin(kelvin: Option<i64>) -> Option<Value> {
    kelvin.map(|kelvin| kelvin.saturating_sub(KELVIN_AT_ZERO_CELSIUS))
}

fn weather_from(fields: PhoneFields<'_>) -> Message<'_> {
    Message::Weather(Weather {
        temperature_in_celsius: celsius_from_kelvin(fields.temp),
        low_in_celsius: celsius_from_kelvin(fields.lo),
        high_in_celsius: celsius_from_kelvin(fields.hi),
        condition: fields.code.and_then(WeatherCondition::from_code),
        location: unescaped(fields.loc),
    })
}

// ============================================================================
// Finding the watch and buzzing it
// ============================================================================

/// `vibrate` buzzes at least once and at most this many times.
pub const MOST_PHONE_BUZZES: u8 = 5;

#[derive(Deserialize)]
struct FindFields {
    #[serde(default)]
    n: Option<bool>,
}

#[derive(Deserialize)]
struct VibrateFields {
    #[serde(default)]
    n: Option<i64>,
}

#[inline(never)]
pub(crate) fn parse_find(json: &str) -> Result<Message<'_>, MessageError> {
    let (fields, _) = serde_json_core::from_str::<FindFields>(json).map_err(|_| MessageError::NotJson)?;
    Ok(Message::Find {
        is_on: fields.n.unwrap_or(false),
    })
}

#[inline(never)]
pub(crate) fn parse_vibrate(json: &str) -> Result<Message<'_>, MessageError> {
    let (fields, _) = serde_json_core::from_str::<VibrateFields>(json).map_err(|_| MessageError::NotJson)?;
    let times = fields.n.unwrap_or(1).clamp(1, MOST_PHONE_BUZZES as i64) as u8;
    Ok(Message::Vibrate { times })
}

// ============================================================================
// Waiting for the shell
// ============================================================================

/// The phone's messages the shell has not yet taken, only the latest of each kind.
///
/// The larger ones wait on the heap, so an empty set costs a few words.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PhoneUpdates {
    pub call: Option<Box<PhoneCall>>,
    pub music_info: Option<Box<MusicInfo>>,
    pub music_state: Option<MusicState>,
    pub weather: Option<Box<Weather>>,
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
            find: None,
            buzzes: None,
        }
    }

    /// Keeps a call, music, weather, find or vibrate message, replacing one of its kind; false for any other.
    pub fn record(&mut self, message: Message<'_>) -> bool {
        match message {
            Message::Call(call) => self.call = Some(Box::new(call)),
            Message::MusicInfo(info) => self.music_info = Some(Box::new(info)),
            Message::MusicState(state) => self.music_state = Some(state),
            Message::Weather(weather) => self.weather = Some(Box::new(weather)),
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
}

#[derive(Debug, Default)]
struct Music {
    info: MusicInfo,
    is_playing: bool,
    position_in_seconds: Value,
}

/// The latest music and weather the phone sent, kept on the heap once the phone sends any.
#[derive(Debug, Default)]
pub struct PhoneData {
    music: Option<Box<Music>>,
    weather: Option<Box<Weather>>,
    weather_received_at: Value,
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

fn number_or_none(value: Option<Value>) -> Value {
    value.unwrap_or(NONE)
}

impl PhoneData {
    /// Takes the music and weather out of the updates, at wall clock time `now`, and says what changed.
    pub fn apply(&mut self, updates: &mut PhoneUpdates, now: Value) -> PhoneChanges {
        let mut changes = PhoneChanges::default();
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
        changes
    }

    /// A music or weather value, or `None` for any other value; values the phone never sent are empty.
    pub fn read(&self, value: SystemValue) -> Option<Value> {
        let music = self.music.as_deref();
        let weather = self.weather.as_deref();
        let seconds = |seconds: Value| seconds.saturating_mul(MILLISECONDS_PER_SECOND);
        Some(match value {
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
            SystemValue::WeatherSunrise | SystemValue::WeatherSunset => NONE,
            SystemValue::WeatherForecastLength => 0,
            _ => return None,
        })
    }

    /// The artist, track or place, or `None` for any other value.
    pub fn read_text(&self, value: SystemValue) -> Option<&str> {
        let music = self.music.as_deref();
        let weather = self.weather.as_deref();
        Some(match value {
            SystemValue::MusicArtist => music.map_or("", |music| &music.info.artist),
            SystemValue::MusicTrack => music.map_or("", |music| &music.info.track),
            SystemValue::WeatherLocation => weather.map_or("", |weather| &weather.location),
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
