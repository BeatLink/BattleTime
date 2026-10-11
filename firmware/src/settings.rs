//! The wearer's settings that the companion app changes: the theme and the backlight level.
//!
//! Each is kept in its own file under `/settings` on the external flash, and read back at start.

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use neotime_companion::{ReplyLine, SettingsChange};
use neotime_runtime::{Theme, ThemeColor, DEFAULT_THEME};

use crate::{backlight, storage};

/// Changes the companion app asked for, from the Bluetooth task to the shell.
pub static CHANGES: Channel<CriticalSectionRawMutex, SettingsChange, 2> = Channel::new();

/// Replies for the companion app, from the shell to the Bluetooth task.
pub static REPLIES: Channel<CriticalSectionRawMutex, ReplyLine, 2> = Channel::new();

const THEME_PATH: &str = "/settings/theme";
const BRIGHTNESS_PATH: &str = "/settings/brightness";

#[derive(Clone, Copy)]
pub struct Settings {
    pub theme: Theme,
    /// The backlight level from 1 to 3 when no app has asked for another.
    pub brightness: u8,
}

impl Settings {
    /// The saved settings, or the defaults for any that were never saved.
    pub fn load(files: &storage::Files) -> Settings {
        let theme = files
            .read(THEME_PATH)
            .and_then(|bytes| theme_from_bytes(&bytes))
            .unwrap_or(DEFAULT_THEME);
        let brightness = files
            .read(BRIGHTNESS_PATH)
            .and_then(|bytes| bytes.first().copied())
            .filter(|level| (1..=3).contains(level))
            .unwrap_or(backlight::DEFAULT_LEVEL);
        Settings { theme, brightness }
    }

    /// Applies a change and saves whatever it changed.
    pub fn apply(&mut self, change: &SettingsChange, files: &storage::Files) {
        let theme = change.applied_to(self.theme);
        if theme != self.theme {
            self.theme = theme;
            files.write(THEME_PATH, &theme_to_bytes(&theme));
        }
        if let Some(level) = change.brightness.filter(|level| *level != self.brightness) {
            self.brightness = level;
            files.write(BRIGHTNESS_PATH, &[level]);
        }
    }
}

// ============================================================================
// The theme file: eight RGB565 colours, little-endian, in `ThemeColor::ALL` order
// ============================================================================

const THEME_BYTES: usize = 2 * ThemeColor::COUNT;

fn theme_to_bytes(theme: &Theme) -> [u8; THEME_BYTES] {
    let mut bytes = [0; THEME_BYTES];
    for (color, pair) in theme.colors().iter().zip(bytes.as_chunks_mut::<2>().0) {
        *pair = color.to_le_bytes();
    }
    bytes
}

fn theme_from_bytes(bytes: &[u8]) -> Option<Theme> {
    let bytes: &[u8; THEME_BYTES] = bytes.try_into().ok()?;
    let mut colors = [0; ThemeColor::COUNT];
    for (color, pair) in colors.iter_mut().zip(bytes.as_chunks::<2>().0) {
        *color = u16::from_le_bytes(*pair);
    }
    Some(Theme::new(colors))
}
