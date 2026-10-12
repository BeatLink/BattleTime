//! The watch-wide settings as apps see them: the system values each one shows as, the theme picked
//! by preset number, and telling the open app what changed.

use neotime_runtime::{preset_matching, SystemValue, Theme, Value, PRESET_THEMES};
use neotime_settings::{Setting, Settings};

use crate::{AppHost, PushedEvent, System};

/// The system value apps read a setting through.
pub fn system_value_of(setting: Setting) -> SystemValue {
    match setting {
        Setting::Uses24HourClock => SystemValue::Uses24HourClock,
        Setting::UsesMetricUnits => SystemValue::UsesMetricUnits,
        Setting::StepGoal => SystemValue::StepGoal,
        Setting::Brightness => SystemValue::Brightness,
        Setting::ScreenTimeoutSeconds => SystemValue::ScreenTimeoutSeconds,
        Setting::AlwaysOnScreen => SystemValue::AlwaysOnScreen,
        Setting::Chimes => SystemValue::Chimes,
        Setting::DoNotDisturb => SystemValue::IsDoNotDisturb,
    }
}

/// The value of a system value that a setting holds, or `None` for every other system value.
///
/// `brightness` is left to the system, since an open app may hold the backlight at another level.
pub fn read_setting(settings: &Settings, value: SystemValue) -> Option<Value> {
    Setting::ALL
        .into_iter()
        .filter(|&setting| setting != Setting::Brightness)
        .find(|&setting| system_value_of(setting) == value)
        .map(|setting| settings.get(setting) as Value)
}

/// The name `set_setting` takes to pick a preset theme by number; the theme is kept apart from the store.
pub const THEME_SETTING: &str = "theme";

/// The theme's place in `PRESET_THEMES`, or -1 when its colours match no preset.
pub fn preset_number(theme: &Theme) -> Value {
    preset_matching(theme).map_or(-1, |preset| {
        PRESET_THEMES
            .iter()
            .position(|candidate| candidate.name == preset.name)
            .unwrap_or(0) as Value
    })
}

/// The preset theme with this number, if there is one.
pub fn preset_theme(number: Value) -> Option<Theme> {
    usize::try_from(number)
        .ok()
        .and_then(|index| PRESET_THEMES.get(index))
        .map(|preset| preset.theme)
}

/// Gives the open app a `settings_change` event for each setting in `changed`, which also redraws what shows it.
pub fn tell_app_about_changes(host: &mut AppHost<'_>, changed: u32, now: Value, system: &mut impl System) {
    for setting in Setting::ALL
        .into_iter()
        .filter(|setting| changed & setting.bit() != 0)
    {
        host.deliver(PushedEvent::SettingsChange { key: setting.name() }, now, system);
    }
}
