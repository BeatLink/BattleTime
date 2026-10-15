//! The companion app's message channel, as docs/companion-protocol.md describes it.
//!
//! The phone writes newline-ended lines to the Nordic UART service, each `GB({...})` or a bare
//! JSON object, and the watch answers with JSON lines on its notify characteristic. This crate
//! assembles lines from the writes, reads the messages the watch understands, and writes its
//! replies; it knows nothing of Bluetooth, so it runs in tests.

#![no_std]

extern crate alloc;

use core::fmt::Write;

use neotime_runtime::{preset_matching, preset_named, PresetTheme, Theme, ThemeColor, PRESET_THEMES};
use neotime_settings::{allows_updates, Setting, Settings};
use serde::Deserialize;
use serde_json_core::str::{EscapedStr, EscapedStringFragment};

pub mod advertising;
mod alert;
#[cfg(test)]
mod alert_tests;
mod device_information;
#[cfg(test)]
mod device_information_tests;
mod heart_rate_measurement;
#[cfg(test)]
mod heart_rate_measurement_tests;
pub mod link_pace;
#[cfg(test)]
mod link_pace_tests;
mod phone;
mod slots;
#[cfg(test)]
mod slots_tests;
#[cfg(test)]
mod tests;

pub use alert::{parse_new_alert, parse_phone_new_alert, MOST_NEW_ALERT_BYTES};
pub use device_information::{
    serial_number, HARDWARE_REVISION, MANUFACTURER_NAME, MODEL_NUMBER, SOFTWARE_REVISION,
};
pub use heart_rate_measurement::heart_rate_measurement;
pub use slots::{slots_refusal, slots_report, SlotRange, SlotsReport, SlotsRequest};

pub use phone::{
    app_message_line, call_control_line, find_phone_line, metres_from_text, music_control_line,
    state_request_line, status_report, CallKind, MusicInfo, MusicState, Navigation, NavigationTurn,
    PhoneCall, PhoneChanges, PhoneData, PhoneUpdates, ShortLine, WatchStatus, Weather, WeatherCondition,
    CALLER_BYTES, CALL_TEXTS, CALL_VALUES, MOST_PHONE_BUZZES, MUSIC_ARTIST_BYTES, MUSIC_TEXTS,
    MUSIC_TRACK_BYTES, MUSIC_VALUES, NAVIGATION_INSTRUCTION_BYTES, NAVIGATION_TEXTS, NAVIGATION_VALUES,
    SHORT_LINE_BYTES, WEATHER_LOCATION_BYTES, WEATHER_TEXTS, WEATHER_VALUES,
};

/// The longest line either side sends; a longer one is dropped whole.
pub const MOST_LINE_BYTES: usize = 640;

/// What the report calls a theme that matches no preset.
pub const CUSTOM_THEME: &str = "custom";

/// The longest line the watch sends, newline included, which leaves room for the settings reply to grow.
pub const MOST_REPLY_BYTES: usize = 704;

/// One line the watch sends, newline included.
pub type ReplyLine = heapless::String<MOST_REPLY_BYTES>;

// ============================================================================
// Lines
// ============================================================================

/// Gathers written bytes into lines, since one line may take several writes.
///
/// A half-received line is kept on the heap, which is freed once the line is complete.
#[derive(Default)]
pub struct LineAssembler {
    buffer: alloc::vec::Vec<u8>,
    overflowed: bool,
}

impl LineAssembler {
    pub fn new() -> LineAssembler {
        LineAssembler::default()
    }

    /// Takes newly written bytes and hands each line they complete to `on_line`, without its newline.
    ///
    /// A JSON line has its numbers' fractions rounded first, as `with_fractions_rounded` describes.
    pub fn push(&mut self, bytes: &[u8], mut on_line: impl FnMut(&[u8])) {
        for &byte in bytes {
            if byte == b'\n' {
                if !self.overflowed {
                    match with_fractions_rounded(&self.buffer) {
                        Some(rounded) => on_line(&rounded),
                        None => on_line(&self.buffer),
                    }
                }
                self.clear();
            } else if self.buffer.len() < MOST_LINE_BYTES {
                self.buffer.push(byte);
            } else {
                self.overflowed = true;
            }
        }
    }

    /// Forgets a half-received line, such as when the phone disconnects.
    pub fn clear(&mut self) {
        self.buffer = alloc::vec::Vec::new();
        self.overflowed = false;
    }
}

/// A JSON line with each number outside quotes that has a fraction rounded to a whole one, halves away
/// from zero, since the watch reads whole numbers only; `None` for a line with none, or one not JSON.
///
/// A phone may send a temperature such as `291.6`. Lines that are not JSON, such as Gadgetbridge's
/// `setTime(...);E.setTimeZone(5.5)`, are left alone.
pub fn with_fractions_rounded(line: &[u8]) -> Option<alloc::vec::Vec<u8>> {
    let start = line
        .iter()
        .position(|&byte| byte != GADGETBRIDGE_LINE_START && !byte.is_ascii_whitespace())?;
    if !matches!(line[start], b'{' | b'G') || !line.contains(&b'.') {
        return None;
    }
    let mut out = alloc::vec::Vec::with_capacity(line.len() + 1);
    let mut in_text = false;
    let mut after_backslash = false;
    // Where the digits of the number being copied start, and while skipping its fraction, whether it rounds up.
    let mut digits_start = None;
    let mut fraction: Option<Option<bool>> = None;
    for &byte in line {
        if in_text {
            out.push(byte);
            in_text = after_backslash || byte != b'"';
            after_backslash = !after_backslash && byte == b'\\';
            continue;
        }
        if let Some(rounds_up) = fraction {
            if byte.is_ascii_digit() {
                fraction = Some(Some(rounds_up.unwrap_or(byte >= b'5')));
                continue;
            }
            round_up_if(rounds_up == Some(true), &mut out, digits_start);
            fraction = None;
            digits_start = None;
        }
        match byte {
            b'0'..=b'9' => {
                digits_start.get_or_insert(out.len());
                out.push(byte);
            }
            b'.' if digits_start.is_some() => fraction = Some(None),
            _ => {
                digits_start = None;
                in_text = byte == b'"';
                out.push(byte);
            }
        }
    }
    round_up_if(fraction == Some(Some(true)), &mut out, digits_start);
    Some(out)
}

/// The byte Gadgetbridge puts before each line, an Espruino convention.
const GADGETBRIDGE_LINE_START: u8 = 0x10;

/// Adds one to the whole number whose digits run from `digits_start` to the end of `out`.
fn round_up_if(rounds_up: bool, out: &mut alloc::vec::Vec<u8>, digits_start: Option<usize>) {
    let Some(start) = digits_start.filter(|_| rounds_up) else {
        return;
    };
    for digit in out[start..].iter_mut().rev() {
        if *digit != b'9' {
            *digit += 1;
            return;
        }
        *digit = b'0';
    }
    out.insert(start, b'1');
}

// ============================================================================
// Messages from the phone
// ============================================================================

/// A message the watch has read from a line.
#[derive(Debug, PartialEq, Eq)]
pub enum Message<'line> {
    /// `settings`: the keys it carries change; with none, it asks for the current settings.
    Settings(SettingsChange),
    /// `notify`: a notification arrived on the phone.
    Notification(PhoneNotification),
    /// `call`: a call started, was answered or ended.
    Call(PhoneCall),
    /// `musicinfo`: the track the phone's player has.
    MusicInfo(MusicInfo),
    /// `musicstate`: whether it plays, and where in the track.
    MusicState(MusicState),
    /// `weather`: the phone's forecast.
    Weather(Weather),
    /// `nav`: the next step of the phone's navigation, or its end.
    Navigation(Navigation),
    /// `find`: start or stop ringing the watch.
    Find { is_on: bool },
    /// `vibrate`: buzz this many times.
    Vibrate { times: u8 },
    /// `status`: the phone asks for the battery and steps.
    StatusRequest,
    /// `notify-`: the notification with this id was dismissed on the phone.
    NotificationDismissed { id: i64 },
    /// `slots`: where the update and recovery slots are, or a request to move them.
    Slots(SlotsRequest),
    /// `state`: the phone holds, rewrote or lets go of this app's saved state.
    AppState { app: &'line str, step: StateStep },
    /// A message this watch does not act on yet, by its `t`.
    Unknown { kind: &'line str },
}

/// Where the phone is in rewriting an app's saved state, from a `state` message's `hold`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateStep {
    /// `"hold":true`: the phone is about to read and rewrite the state, so the watch leaves it alone.
    Hold,
    /// No `hold`: the phone rewrote the state, so an open app loads it again.
    Written,
    /// `"hold":false`: the phone gave up without writing, so the watch may save the state again.
    Released,
}

/// The settings a `settings` message changes; anything left out stays as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SettingsChange {
    /// A preset theme picked by name.
    pub preset: Option<&'static PresetTheme>,
    /// Single theme colours, as RGB565, applied after the preset.
    pub colors: [Option<u16>; ThemeColor::COUNT],
    /// New values for the watch-wide settings, in `Setting::ALL` order, each inside its limits.
    pub values: [Option<i32>; Setting::COUNT],
}

impl SettingsChange {
    pub fn changes_anything(&self) -> bool {
        self.preset.is_some()
            || self.colors.iter().any(Option::is_some)
            || self.values.iter().any(Option::is_some)
    }

    pub fn value(&self, setting: Setting) -> Option<i32> {
        self.values[setting.index()]
    }

    /// The theme after this change.
    pub fn applied_to(&self, theme: Theme) -> Theme {
        let mut theme = self.preset.map_or(theme, |preset| preset.theme);
        for (color, value) in ThemeColor::ALL.into_iter().zip(self.colors) {
            if let Some(value) = value {
                theme.set_color(color, value);
            }
        }
        theme
    }
}

/// The longest app id, as `neotime-packages` allows.
pub const MOST_APP_ID_BYTES: usize = 32;

/// The longest app name, title and body a notification keeps; longer text is cut short.
pub const NOTIFICATION_SOURCE_BYTES: usize = 24;
pub const NOTIFICATION_TITLE_BYTES: usize = 32;
pub const NOTIFICATION_BODY_BYTES: usize = 120;

/// A notification from the phone, its text unescaped and cut to fit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PhoneNotification {
    pub id: i64,
    /// The phone app it came from, such as `Messages`.
    pub source: heapless::String<NOTIFICATION_SOURCE_BYTES>,
    pub title: heapless::String<NOTIFICATION_TITLE_BYTES>,
    pub body: heapless::String<NOTIFICATION_BODY_BYTES>,
}

/// A JSON string's text with its escapes undone, cut at the last whole character that fits.
fn unescaped<const BYTES: usize>(text: Option<EscapedStr<'_>>) -> heapless::String<BYTES> {
    let mut out = heapless::String::new();
    for fragment in text.iter().flat_map(EscapedStr::fragments) {
        // A bad escape, such as half of a surrogate pair, ends the text there.
        let Ok(fragment) = fragment else {
            break;
        };
        let mut encoded = [0; 4];
        let piece = match fragment {
            EscapedStringFragment::NotEscaped(piece) => piece,
            EscapedStringFragment::Escaped(character) => character.encode_utf8(&mut encoded),
        };
        for character in piece.chars() {
            if out.push(character).is_err() {
                return out;
            }
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageError {
    NotText,
    NotJson,
    UnknownTheme,
    BadColor,
    /// A setting's value outside its limits, or `units` other than `Metric` or `Imperial`.
    BadSetting,
}

// ============================================================================
// The watch-wide settings' keys
// ============================================================================

/// How a setting is written in a message.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Number,
    TrueOrFalse,
    /// `"Metric"` for 1 and `"Imperial"` for 0, as the wasp-os companion writes units.
    Units,
    /// Whether updates are on, then a second key saying whether only until the next restart.
    Updates,
}

/// The key that turns updates on only until the next restart, written after `updates`.
const UPDATES_UNTIL_RESTART: &str = "updates_until_restart";

/// Each setting's key, the same names the wasp-os companion's settings screen uses where it has one.
const KEYS: [(Setting, &str, Shape); Setting::COUNT] = [
    (Setting::Uses24HourClock, "clock_24h", Shape::TrueOrFalse),
    (Setting::UsesMetricUnits, "units", Shape::Units),
    (Setting::StepGoal, "step_goal", Shape::Number),
    (Setting::Brightness, "brightness", Shape::Number),
    (Setting::ScreenTimeoutSeconds, "blank_after", Shape::Number),
    (Setting::AlwaysOnScreen, "always_on", Shape::TrueOrFalse),
    (Setting::Chimes, "chimes", Shape::Number),
    (Setting::DoNotDisturb, "do_not_disturb", Shape::TrueOrFalse),
    (Setting::Bluetooth, "bluetooth", Shape::TrueOrFalse),
    (Setting::AllowsUpdates, "updates", Shape::Updates),
    (Setting::WakeOnTouch, "wake_on_touch", Shape::Number),
    (Setting::AlarmsInDoNotDisturb, "alarms_in_dnd", Shape::TrueOrFalse),
    (Setting::HeartRateInterval, "heart_rate_interval", Shape::Number),
    (Setting::RaiseToWake, "raise_to_wake", Shape::TrueOrFalse),
    (Setting::LowerToSleep, "lower_to_sleep", Shape::TrueOrFalse),
    (Setting::ShakeToWake, "shake_to_wake", Shape::TrueOrFalse),
    (Setting::ShakeThreshold, "shake_threshold", Shape::Number),
    (Setting::SleepMode, "sleep_mode", Shape::TrueOrFalse),
    (Setting::WatchFace, "watch_face", Shape::Number),
    (Setting::GadgetbridgeName, "gadgetbridge_name", Shape::TrueOrFalse),
];

const METRIC: &str = "Metric";
const IMPERIAL: &str = "Imperial";

#[derive(Deserialize)]
struct Envelope<'line> {
    t: &'line str,
    #[serde(default, borrow)]
    app: Option<&'line str>,
    #[serde(default)]
    hold: Option<bool>,
    #[serde(default, borrow)]
    theme: Option<&'line str>,
    #[serde(default, borrow)]
    theme_colors: Option<ThemeColors<'line>>,
    #[serde(default)]
    brightness: Option<i64>,
    #[serde(default)]
    blank_after: Option<i64>,
    #[serde(default)]
    clock_24h: Option<bool>,
    #[serde(default, borrow)]
    units: Option<&'line str>,
    #[serde(default)]
    step_goal: Option<i64>,
    #[serde(default)]
    always_on: Option<bool>,
    #[serde(default)]
    chimes: Option<i64>,
    #[serde(default)]
    do_not_disturb: Option<bool>,
    #[serde(default)]
    bluetooth: Option<bool>,
    #[serde(default)]
    updates: Option<bool>,
    #[serde(default)]
    updates_until_restart: Option<bool>,
    #[serde(default)]
    wake_on_touch: Option<i64>,
    #[serde(default)]
    alarms_in_dnd: Option<bool>,
    #[serde(default)]
    heart_rate_interval: Option<i64>,
    #[serde(default)]
    raise_to_wake: Option<bool>,
    #[serde(default)]
    lower_to_sleep: Option<bool>,
    #[serde(default)]
    shake_to_wake: Option<bool>,
    #[serde(default)]
    shake_threshold: Option<i64>,
    #[serde(default)]
    sleep_mode: Option<bool>,
    #[serde(default)]
    watch_face: Option<i64>,
    #[serde(default)]
    gadgetbridge_name: Option<bool>,
    #[serde(default)]
    id: Option<i64>,
    #[serde(default, borrow)]
    src: Option<EscapedStr<'line>>,
    #[serde(default, borrow)]
    title: Option<EscapedStr<'line>>,
    #[serde(default, borrow)]
    body: Option<EscapedStr<'line>>,
}

impl Envelope<'_> {
    /// Each setting the message carries, as the number the store holds, or an error for a bad one.
    fn setting_values(&self) -> Result<[Option<i32>; Setting::COUNT], MessageError> {
        let number = |value: Option<i64>| value.map(|value| i32::try_from(value).unwrap_or(i32::MIN));
        let units = match self.units {
            None => None,
            Some(METRIC) => Some(1),
            Some(IMPERIAL) => Some(0),
            Some(_) => return Err(MessageError::BadSetting),
        };
        let mut values = [None; Setting::COUNT];
        for setting in Setting::ALL {
            values[setting.index()] = match setting {
                Setting::Uses24HourClock => self.clock_24h.map(i32::from),
                Setting::UsesMetricUnits => units,
                Setting::StepGoal => number(self.step_goal),
                Setting::Brightness => number(self.brightness),
                Setting::ScreenTimeoutSeconds => number(self.blank_after),
                Setting::AlwaysOnScreen => self.always_on.map(i32::from),
                Setting::Chimes => number(self.chimes),
                Setting::DoNotDisturb => self.do_not_disturb.map(i32::from),
                Setting::Bluetooth => self.bluetooth.map(i32::from),
                Setting::AllowsUpdates => match self.updates_until_restart {
                    Some(true) => Some(allows_updates::UNTIL_RESTART),
                    _ => self.updates.map(i32::from),
                },
                Setting::WakeOnTouch => number(self.wake_on_touch),
                Setting::AlarmsInDoNotDisturb => self.alarms_in_dnd.map(i32::from),
                Setting::HeartRateInterval => number(self.heart_rate_interval),
                Setting::RaiseToWake => self.raise_to_wake.map(i32::from),
                Setting::LowerToSleep => self.lower_to_sleep.map(i32::from),
                Setting::ShakeToWake => self.shake_to_wake.map(i32::from),
                Setting::ShakeThreshold => number(self.shake_threshold),
                Setting::SleepMode => self.sleep_mode.map(i32::from),
                Setting::WatchFace => number(self.watch_face),
                Setting::GadgetbridgeName => self.gadgetbridge_name.map(i32::from),
            };
        }
        let mut checked = Settings::default();
        for (setting, value) in Setting::ALL.into_iter().zip(values) {
            if let Some(value) = value {
                checked
                    .set(setting, value)
                    .map_err(|_| MessageError::BadSetting)?;
            }
        }
        Ok(values)
    }
}

#[derive(Deserialize)]
struct ThemeColors<'line> {
    #[serde(default, borrow)]
    background: Option<&'line str>,
    #[serde(default, borrow)]
    surface: Option<&'line str>,
    #[serde(default, borrow)]
    text: Option<&'line str>,
    #[serde(default, borrow)]
    muted: Option<&'line str>,
    #[serde(default, borrow)]
    accent: Option<&'line str>,
    #[serde(default, borrow)]
    success: Option<&'line str>,
    #[serde(default, borrow)]
    warning: Option<&'line str>,
    #[serde(default, borrow)]
    danger: Option<&'line str>,
}

impl<'line> ThemeColors<'line> {
    /// The colours in `ThemeColor::ALL` order.
    fn in_order(&self) -> [Option<&'line str>; ThemeColor::COUNT] {
        [
            self.background,
            self.surface,
            self.text,
            self.muted,
            self.accent,
            self.success,
            self.warning,
            self.danger,
        ]
    }
}

/// The JSON object in a line, with Gadgetbridge's `GB(...)` around it taken off.
#[inline(always)]
fn json_of(line: &[u8]) -> Result<&str, MessageError> {
    let text = core::str::from_utf8(line)
        .map_err(|_| MessageError::NotText)?
        .trim();
    Ok(text
        .strip_prefix("GB(")
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(text))
}

/// Only the `t` of a line, for the recovery firmware, which answers `status` and `slots` and nothing else.
#[derive(Deserialize)]
struct KindOnly<'line> {
    t: &'line str,
}

/// Reads only a line's kind, its `t`, without the parsers for each kind that `parse_line` brings in.
pub fn message_kind(line: &[u8]) -> Result<&str, MessageError> {
    let (envelope, _) =
        serde_json_core::from_str::<KindOnly>(json_of(line)?).map_err(|_| MessageError::NotJson)?;
    Ok(envelope.t)
}

/// Reads one line: `GB({...})` as Gadgetbridge sends it, or a bare JSON object.
// Kept out of line so it is built for size with this crate, never inlined into the firmware's own code.
#[inline(never)]
pub fn parse_line(line: &[u8]) -> Result<Message<'_>, MessageError> {
    let json = json_of(line)?;
    let (envelope, _) = serde_json_core::from_str::<Envelope>(json).map_err(|_| MessageError::NotJson)?;
    match envelope.t {
        "settings" => {}
        "notify" => {
            return Ok(Message::Notification(PhoneNotification {
                id: envelope.id.unwrap_or(0),
                source: unescaped(envelope.src),
                title: unescaped(envelope.title),
                body: unescaped(envelope.body),
            }))
        }
        "call" | "musicinfo" | "musicstate" | "weather" | "nav" => {
            return phone::parse_phone_message(envelope.t, json)
        }
        "find" => return phone::parse_find(json),
        "vibrate" => return phone::parse_vibrate(json),
        "status" => return Ok(Message::StatusRequest),
        "slots" => return slots::parse_slots(json),
        "state" => {
            return match envelope.app {
                Some(app) if !app.is_empty() && app.len() <= MOST_APP_ID_BYTES => {
                    let step = match envelope.hold {
                        Some(true) => StateStep::Hold,
                        Some(false) => StateStep::Released,
                        None => StateStep::Written,
                    };
                    Ok(Message::AppState { app, step })
                }
                _ => Err(MessageError::NotJson),
            }
        }
        "notify-" => {
            return Ok(Message::NotificationDismissed {
                id: envelope.id.unwrap_or(0),
            })
        }
        kind => return Ok(Message::Unknown { kind }),
    }

    let mut change = SettingsChange::default();
    // `custom` names no preset, so a report sent back unchanged keeps its colours.
    if let Some(name) = envelope.theme.filter(|name| *name != CUSTOM_THEME) {
        change.preset = Some(preset_named(name).ok_or(MessageError::UnknownTheme)?);
    }
    if let Some(colors) = &envelope.theme_colors {
        for (slot, value) in change.colors.iter_mut().zip(colors.in_order()) {
            if let Some(value) = value {
                *slot = Some(rgb565_from_hex(value).ok_or(MessageError::BadColor)?);
            }
        }
    }
    change.values = envelope.setting_values()?;
    Ok(Message::Settings(change))
}

/// A colour written `#rrggbb`, as RGB565.
fn rgb565_from_hex(text: &str) -> Option<u16> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    let [_, red, green, blue] = value.to_be_bytes();
    Some(neotime_runtime::theme::rgb565(red, green, blue))
}

/// An RGB565 colour written `#rrggbb`, each part widened back to 8 bits.
fn hex_from_rgb565(color: u16) -> heapless::String<7> {
    let red = ((color >> 11) & 0x1F) as u32;
    let green = ((color >> 5) & 0x3F) as u32;
    let blue = (color & 0x1F) as u32;
    let widen = |part: u32, bits: u32| (part << (8 - bits)) | (part >> (2 * bits - 8));
    let mut text = heapless::String::new();
    let _ = write!(
        text,
        "#{:02x}{:02x}{:02x}",
        widen(red, 5),
        widen(green, 6),
        widen(blue, 5)
    );
    text
}

// ============================================================================
// Replies to the phone
// ============================================================================

/// The `settings` reply: the theme by name, or `custom`, its colours, the presets and every setting.
pub fn settings_report(theme: &Theme, settings: &Settings) -> ReplyLine {
    let mut line = ReplyLine::new();
    let name = preset_matching(theme).map_or(CUSTOM_THEME, |preset| preset.name);
    let _ = write!(
        line,
        "{{\"t\":\"settings\",\"theme\":\"{name}\",\"theme_colors\":{{"
    );
    for (index, color) in ThemeColor::ALL.into_iter().enumerate() {
        let separator = if index == 0 { "" } else { "," };
        let _ = write!(
            line,
            "{separator}\"{}\":\"{}\"",
            color.name(),
            hex_from_rgb565(theme.color(color))
        );
    }
    let _ = write!(line, "}},\"themes\":[");
    for (index, preset) in PRESET_THEMES.iter().enumerate() {
        let separator = if index == 0 { "" } else { "," };
        let _ = write!(line, "{separator}\"{}\"", preset.name);
    }
    let _ = write!(line, "]");
    for (setting, key, shape) in KEYS {
        let value = settings.get(setting);
        let _ = match shape {
            Shape::Number => write!(line, ",\"{key}\":{value}"),
            Shape::TrueOrFalse => write!(line, ",\"{key}\":{}", value != 0),
            Shape::Updates => write!(
                line,
                ",\"{key}\":{},\"{UPDATES_UNTIL_RESTART}\":{}",
                value != allows_updates::OFF,
                value == allows_updates::UNTIL_RESTART
            ),
            Shape::Units => write!(
                line,
                ",\"{key}\":\"{}\"",
                if value != 0 { METRIC } else { IMPERIAL }
            ),
        };
    }
    let _ = writeln!(line, "}}");
    line
}

/// An `error` reply, such as for a message the watch could not read.
pub fn error_report(message: &str) -> ReplyLine {
    let mut line = ReplyLine::new();
    let _ = writeln!(line, "{{\"t\":\"error\",\"msg\":\"{message}\"}}");
    line
}
