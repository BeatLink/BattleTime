//! The watch-wide settings and the theme, shared by the shell and the Bluetooth task.
//!
//! The settings live in one store checked by `battletime-settings`. Apps change them with
//! `set_setting`, and the companion app with its `settings` message, which also carries the theme.
//! Every change wakes the shell, which saves the settings to `/settings/system` and tells the open
//! app. The theme's colours are kept apart, in `/settings/theme`.

use core::{
    cell::Cell,
    sync::atomic::{AtomicU32, Ordering},
};

use embassy_sync::{
    blocking_mutex::{raw::CriticalSectionRawMutex, Mutex},
    channel::Channel,
    signal::Signal,
};
extern crate alloc;

use alloc::boxed::Box;
use battletime_app_host::settings::system_value_of;
use battletime_companion::SettingsChange;
use battletime_runtime::{Theme, ThemeColor, Value, DEFAULT_THEME};
use battletime_settings::{Setting, Settings, FILE_BYTES};

use crate::{storage, system_values};

/// Changes from the companion app for the shell; the companion waits for each reply before the next.
pub static CHANGES: Channel<CriticalSectionRawMutex, SettingsChange, 1> = Channel::new();

/// Replies for the companion app, from the shell to the Bluetooth task, kept on the heap only while they wait.
pub static REPLIES: Channel<CriticalSectionRawMutex, Box<str>, 2> = Channel::new();

/// Queues a reply line for the companion app; it is dropped if two are already waiting.
pub fn send_reply(line: &str) {
    let _ = REPLIES.try_send(Box::from(line));
}

/// Raised when the Bluetooth setting or the advertised name changes, so the Bluetooth task drops the phone or advertises again.
pub static BLUETOOTH_SWITCHED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// The settings that change what the Bluetooth task does, as `Setting::bit`s.
fn bluetooth_bits() -> u32 {
    Setting::Bluetooth.bit() | Setting::GadgetbridgeName.bit()
}

pub fn is_bluetooth_on() -> bool {
    current().is_on(Setting::Bluetooth)
}

/// The two names the watch advertises, in statics so the GAP service can borrow them.
static OWN_NAME: &str = crate::DEVICE_NAME;
static GADGETBRIDGE_NAME: &str = battletime_companion::advertising::GADGETBRIDGE_NAME;

/// The name the watch advertises and gives in its GAP service: "InfiniTime" for Gadgetbridge, or its own.
pub fn advertised_name() -> &'static &'static str {
    match current().is_on(Setting::GadgetbridgeName) {
        true => &GADGETBRIDGE_NAME,
        false => &OWN_NAME,
    }
}

const SETTINGS_PATH: &str = "/settings/system";
const THEME_PATH: &str = "/settings/theme";

// ============================================================================
// The store
// ============================================================================

static SETTINGS: Mutex<CriticalSectionRawMutex, Cell<Settings>> = Mutex::new(Cell::new(Settings::DEFAULT));

/// `Setting::bit`s of the changes the shell has not yet saved and passed on.
static UNSAVED_CHANGES: AtomicU32 = AtomicU32::new(0);

pub fn current() -> Settings {
    SETTINGS.lock(Cell::get)
}

/// Reads the saved settings at start-up; a missing or unreadable file leaves the defaults.
pub fn load(files: &storage::Files) {
    if let Some(bytes) = files.read_now(SETTINGS_PATH) {
        let loaded = Settings::from_file(&bytes);
        let before = SETTINGS.lock(|settings| settings.replace(loaded));
        // Advertising started before the file was read, so a saved name starts it again.
        if loaded.differences_from(&before) & bluetooth_bits() != 0 {
            BLUETOOTH_SWITCHED.signal(());
        }
    }
}

pub fn save(files: &storage::Files) {
    let mut buffer = [0; FILE_BYTES];
    let length = current().write_file(&mut buffer);
    files.write_now(SETTINGS_PATH, &buffer[..length]);
}

/// Changes one setting by the name an app gave `set_setting`; unknown names and bad values do nothing.
pub fn set_by_name(name: &str, value: Value) {
    change(|settings| match settings.set_by_name(name, value) {
        Ok(setting) => setting.bit(),
        Err(_) => 0,
    });
}

/// Applies the values a companion message carried, already checked against their limits.
pub fn set_values(values: &[Option<i32>; Setting::COUNT]) {
    change(|settings| {
        let mut changed = 0;
        for (setting, value) in Setting::ALL.into_iter().zip(values) {
            if let Some(Ok(true)) = value.map(|value| settings.set(setting, value)) {
                changed |= setting.bit();
            }
        }
        changed
    });
}

/// The changes since the last call, as `Setting::bit`s.
pub fn take_changes() -> u32 {
    UNSAVED_CHANGES.swap(0, Ordering::Relaxed)
}

/// Runs a change on the store, then wakes the shell for every setting whose value changed.
fn change(apply: impl FnOnce(&mut Settings) -> u32) {
    let changed = SETTINGS.lock(|cell| {
        let mut settings = cell.get();
        let before = settings;
        let changed = apply(&mut settings);
        cell.set(settings);
        if settings == before {
            0
        } else {
            changed
        }
    });
    if changed == 0 {
        return;
    }
    UNSAVED_CHANGES.fetch_or(changed, Ordering::Relaxed);
    if changed & bluetooth_bits() != 0 {
        BLUETOOTH_SWITCHED.signal(());
    }
    for setting in Setting::ALL
        .into_iter()
        .filter(|setting| changed & setting.bit() != 0)
    {
        system_values::changed(&[system_value_of(setting)]);
    }
}

// ============================================================================
// The theme file: eight RGB565 colours, little-endian, in `ThemeColor::ALL` order
// ============================================================================

const THEME_BYTES: usize = 2 * ThemeColor::COUNT;

/// The saved theme, or the default when none was saved.
pub fn load_theme(files: &storage::Files) -> Theme {
    files
        .read_now(THEME_PATH)
        .and_then(|bytes| theme_from_bytes(&bytes))
        .unwrap_or(DEFAULT_THEME)
}

pub fn save_theme(files: &storage::Files, theme: &Theme) {
    files.write_now(THEME_PATH, &theme_to_bytes(theme));
}

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
