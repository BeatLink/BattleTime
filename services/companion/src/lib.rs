//! The companion app's message channel, as docs/companion-protocol.md describes it.
//!
//! The phone writes newline-ended lines to the Nordic UART service, each `GB({...})` or a bare
//! JSON object, and the watch answers with JSON lines on its notify characteristic. This crate
//! assembles lines from the writes, reads the messages the watch understands, and writes its
//! replies; it knows nothing of Bluetooth, so it runs in tests.

#![no_std]

use core::fmt::Write;

use neotime_runtime::{preset_matching, preset_named, PresetTheme, Theme, ThemeColor, PRESET_THEMES};
use serde::Deserialize;

#[cfg(test)]
mod tests;

/// The longest line either side sends; a longer one is dropped whole.
pub const MOST_LINE_BYTES: usize = 512;

/// What the report calls a theme that matches no preset.
pub const CUSTOM_THEME: &str = "custom";

/// One line the watch sends, newline included.
pub type ReplyLine = heapless::String<MOST_LINE_BYTES>;

// ============================================================================
// Lines
// ============================================================================

/// Gathers written bytes into lines, since one line may take several writes.
#[derive(Default)]
pub struct LineAssembler {
    buffer: heapless::Vec<u8, MOST_LINE_BYTES>,
    overflowed: bool,
}

impl LineAssembler {
    pub fn new() -> LineAssembler {
        LineAssembler::default()
    }

    /// Takes newly written bytes and hands each line they complete to `on_line`, without its newline.
    pub fn push(&mut self, bytes: &[u8], mut on_line: impl FnMut(&[u8])) {
        for &byte in bytes {
            if byte == b'\n' {
                if !self.overflowed {
                    on_line(&self.buffer);
                }
                self.buffer.clear();
                self.overflowed = false;
            } else if self.buffer.push(byte).is_err() {
                self.overflowed = true;
            }
        }
    }

    /// Forgets a half-received line, such as when the phone disconnects.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.overflowed = false;
    }
}

// ============================================================================
// Messages from the phone
// ============================================================================

/// A message the watch has read from a line.
#[derive(Debug, PartialEq, Eq)]
pub enum Message<'line> {
    /// `settings`: the keys it carries change; with none, it asks for the current settings.
    Settings(SettingsChange),
    /// A message this watch does not act on yet, by its `t`.
    Unknown { kind: &'line str },
}

/// The settings a `settings` message changes; anything left out stays as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SettingsChange {
    /// A preset theme picked by name.
    pub preset: Option<&'static PresetTheme>,
    /// Single theme colours, as RGB565, applied after the preset.
    pub colors: [Option<u16>; ThemeColor::COUNT],
    /// The backlight level from 1 to 3.
    pub brightness: Option<u8>,
}

impl SettingsChange {
    pub fn changes_anything(&self) -> bool {
        self.preset.is_some() || self.colors.iter().any(Option::is_some) || self.brightness.is_some()
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageError {
    NotText,
    NotJson,
    UnknownTheme,
    BadColor,
    BadBrightness,
}

#[derive(Deserialize)]
struct Envelope<'line> {
    t: &'line str,
    #[serde(default, borrow)]
    theme: Option<&'line str>,
    #[serde(default, borrow)]
    theme_colors: Option<ThemeColors<'line>>,
    #[serde(default)]
    brightness: Option<u8>,
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

/// Reads one line: `GB({...})` as Gadgetbridge sends it, or a bare JSON object.
pub fn parse_line(line: &[u8]) -> Result<Message<'_>, MessageError> {
    let text = core::str::from_utf8(line).map_err(|_| MessageError::NotText)?.trim();
    let json = text
        .strip_prefix("GB(")
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(text);
    let (envelope, _) =
        serde_json_core::from_str::<Envelope>(json).map_err(|_| MessageError::NotJson)?;
    if envelope.t != "settings" {
        return Ok(Message::Unknown { kind: envelope.t });
    }

    let mut change = SettingsChange::default();
    // `custom` names no preset, so a report sent back unchanged keeps its colours.
    if let Some(name) = envelope.theme.filter(|name| *name != CUSTOM_THEME) {
        change.preset = Some(preset_named(name).ok_or(MessageError::UnknownTheme)?);
    }
    if let Some(colors) = envelope.theme_colors {
        for (slot, value) in change.colors.iter_mut().zip(colors.in_order()) {
            if let Some(value) = value {
                *slot = Some(rgb565_from_hex(value).ok_or(MessageError::BadColor)?);
            }
        }
    }
    if let Some(level) = envelope.brightness {
        if !(1..=3).contains(&level) {
            return Err(MessageError::BadBrightness);
        }
        change.brightness = Some(level);
    }
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
    let _ = write!(text, "#{:02x}{:02x}{:02x}", widen(red, 5), widen(green, 6), widen(blue, 5));
    text
}

// ============================================================================
// Replies to the phone
// ============================================================================

/// The `settings` reply: the theme by name, or `custom`, its colours, the presets and the brightness.
pub fn settings_report(theme: &Theme, brightness: u8) -> ReplyLine {
    let mut line = ReplyLine::new();
    let name = preset_matching(theme).map_or(CUSTOM_THEME, |preset| preset.name);
    let _ = write!(line, "{{\"t\":\"settings\",\"theme\":\"{name}\",\"theme_colors\":{{");
    for (index, color) in ThemeColor::ALL.into_iter().enumerate() {
        let separator = if index == 0 { "" } else { "," };
        let _ = write!(line, "{separator}\"{}\":\"{}\"", color.name(), hex_from_rgb565(theme.color(color)));
    }
    let _ = write!(line, "}},\"themes\":[");
    for (index, preset) in PRESET_THEMES.iter().enumerate() {
        let separator = if index == 0 { "" } else { "," };
        let _ = write!(line, "{separator}\"{}\"", preset.name);
    }
    let _ = writeln!(line, "],\"brightness\":{brightness}}}");
    line
}

/// An `error` reply, such as for a message the watch could not read.
pub fn error_report(message: &str) -> ReplyLine {
    let mut line = ReplyLine::new();
    let _ = writeln!(line, "{{\"t\":\"error\",\"msg\":\"{message}\"}}");
    line
}
