//! Runs an app image in the simulator window through `battletime-app-host`, with the mouse as the
//! touch panel, the computer's clock as the watch's, and effects written to the terminal.

use std::{
    cell::Cell,
    error::Error,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use battletime_app_host::{
    folder::FolderListing, settings, AppEffect, AppFailure, AppHost, EventResult, PushedEvent, System, Touch,
    STRIP_PIXELS,
};
use battletime_app_testing::{
    starting_step_hours, HOURS_PER_DAY, STARTING_DAY_STEPS, STARTING_TRIP_STEPS, STEP_HISTORY_DAYS,
};
use battletime_heart_rate::monitor::Status;
use battletime_runtime::{
    events::SwipeDirection, resources::ImageReference, AppImage, EffectKind, SystemValue, SystemValues,
    Theme, Value, DEFAULT_THEME, NONE,
};
use battletime_settings::{Setting, Settings};
use battletime_vibrator::{Step, TAP};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use embedded_graphics_simulator::{sdl2::Keycode, OutputSettings, SimulatorDisplay, SimulatorEvent, Window};

use super::{copy_panel_to_screen, start_vibrator, WatchDisplay, SWIPE_DISTANCE_IN_PIXELS};
use crate::{
    heart_rate::{start_heart_rate_sensor, HeartRateLink, Update},
    panel::SimulatedPanel,
};

// ============================================================================
// Settings
// ============================================================================

const FRAME_PAUSE: Duration = Duration::from_millis(16);
const LONG_PRESS: Duration = Duration::from_millis(500);
const DRAG_DISTANCE_IN_PIXELS: i32 = 4;
const MINUTE_IN_MILLISECONDS: Value = 60_000;

/// The coming days the simulated phone forecasts: condition number, low and high in °C.
const SIMULATED_FORECAST: [(Value, Value, Value); 3] = [(0, 12, 22), (2, 10, 17), (5, 8, 14)];
const HOUR_IN_MILLISECONDS: Value = 60 * MINUTE_IN_MILLISECONDS;
const SIMULATED_FIRMWARE_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+simulator");
const SIMULATED_FREE_STORAGE_KILOBYTES: Value = 2840;
/// The simulator runs as one task on the computer, so the task page lists just that.
const SIMULATED_TASK_NAME: &str = "simulator";
/// What a PineTime's touch panel and accelerometer usually report: a CST816S and a BMA425.
const SIMULATED_TOUCH_CHIP_ID: Value = 0xB4;
const SIMULATED_ACCELEROMETER_CHIP_ID: Value = 0x13;

/// A light theme, for trying `set_theme` with the T key.
pub const LIGHT_THEME: Theme = Theme::new([0xFFFF, 0xDEFB, 0x0000, 0x6B4D, 0x04B3, 0x2589, 0xFC40, 0xD8A7]);

/// Where to touch before a screenshot, so it can show a screen after a click.
#[derive(Clone, Copy, Debug)]
pub struct Click {
    pub x: i32,
    pub y: i32,
}

pub fn parse_click(text: &str) -> Result<Click, String> {
    let (x, y) = text.split_once(',').ok_or(format!("{text} is not X,Y"))?;
    let number = |text: &str| text.trim().parse().map_err(|_| format!("{text} is not a number"));
    Ok(Click {
        x: number(x)?,
        y: number(y)?,
    })
}

pub fn wall_clock_in_milliseconds() -> Value {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as Value)
}

// ============================================================================
// The simulated system
// ============================================================================

/// Fixed system values, the real clock, changeable settings, a heart rate sensor, and effects printed to the terminal.
pub struct SimulatedSystem {
    vibrator_requests: Option<mpsc::Sender<&'static [Step]>>,
    random_state: Cell<u32>,
    /// Each installed app's name and id, for a launcher.
    installed_apps: Vec<(String, String)>,
    settings: Settings,
    /// `Setting::bit`s of the settings changed since the app was last told.
    changed_settings: u32,
    /// The preset number of the theme on show, as `settings.theme` reads.
    theme_preset: Value,
    /// A preset theme the app picked, shown once the host call returns.
    theme_request: Option<Theme>,
    /// When the simulator started, as `started_at` reads.
    started_at: Value,
    /// The folder standing in for the watch's storage, if one was given.
    storage_folder: Option<PathBuf>,
    /// The folder of the storage the app last opened with `open_folder`.
    folder: FolderListing,
    /// Set when the app opened a folder, so it is told once the host call returns.
    folder_changed: bool,
    /// The simulated heart rate sensor's thread, when the simulator runs in a window.
    heart_rate_link: Option<HeartRateLink>,
    /// The sensor's latest reading and when it came, or none before the first.
    latest_heart_rate: Option<HeartRateReading>,
    heart_rate_status: Status,
}

#[derive(Clone, Copy)]
struct HeartRateReading {
    beats_per_minute: Value,
    measured_at: Value,
}

impl SimulatedSystem {
    pub fn new(
        vibrator_requests: Option<mpsc::Sender<&'static [Step]>>,
        heart_rate_link: Option<HeartRateLink>,
        random_seed: u32,
        files: &AppFiles,
    ) -> Self {
        let installed_apps = files
            .installed_apps
            .iter()
            .filter_map(|bytes| AppImage::parse(bytes).ok())
            .map(|image| {
                let text = |string| image.string(string).unwrap_or("").to_string();
                (text(image.name_string), text(image.id_string))
            })
            .collect();
        SimulatedSystem {
            vibrator_requests,
            random_state: Cell::new(random_seed),
            installed_apps,
            settings: Settings::default(),
            changed_settings: 0,
            theme_preset: 0,
            theme_request: None,
            started_at: wall_clock_in_milliseconds(),
            storage_folder: files.storage_folder.map(Path::to_path_buf),
            folder: FolderListing::new(),
            folder_changed: false,
            heart_rate_link,
            latest_heart_rate: None,
            heart_rate_status: Status::Stopped,
        }
    }

    fn change_setting(&mut self, name: &str, value: Value) {
        if name == settings::THEME_SETTING {
            self.theme_request = settings::preset_theme(value);
            return;
        }
        match self.settings.set_by_name(name, value) {
            Ok(setting) => self.changed_settings |= setting.bit(),
            Err(error) => println!("  the watch would refuse it: {error:?}"),
        }
    }

    /// Tells the app about settings its own handlers changed, once the host call has returned.
    pub fn tell_app_about_settings(&mut self, host: &mut AppHost<'_>, now: Value) {
        let changed = core::mem::take(&mut self.changed_settings);
        if changed != 0 {
            settings::tell_app_about_changes(host, changed, now, self);
        }
        if core::mem::take(&mut self.folder_changed) {
            host.report_system_value_changes(SystemValue::FolderPath.bit(), now, self);
        }
        if let Some(theme) = self.theme_request.take() {
            self.theme_preset = settings::preset_number(&theme);
            host.set_theme(theme);
            let key = settings::THEME_SETTING;
            host.deliver(PushedEvent::SettingsChange { key }, now, self);
        }
    }

    /// Passes the simulated sensor's readings to the app, and tells it when the sensor's status changes.
    pub fn take_heart_rate_updates(&mut self, host: &mut AppHost<'_>, now: Value) {
        let updates: Vec<Update> = self
            .heart_rate_link
            .as_ref()
            .map_or_else(Vec::new, |link| link.updates.try_iter().collect());
        for update in updates {
            let status_before = self.heart_rate_status;
            match update {
                Update::Status(status) => self.heart_rate_status = status,
                Update::Reading(reading) => {
                    self.heart_rate_status = Status::Measuring;
                    self.latest_heart_rate = Some(HeartRateReading {
                        beats_per_minute: reading.beats_per_minute as Value,
                        measured_at: now,
                    });
                    let event = PushedEvent::HeartRate {
                        beats_per_minute: reading.beats_per_minute as Value,
                        confidence: reading.confidence_percent as Value,
                    };
                    host.deliver(event, now, self);
                }
            }
            if self.heart_rate_status != status_before {
                let changed = SystemValue::HeartRateStatus.bit() | SystemValue::IsHeartRateRunning.bit();
                host.report_system_value_changes(changed, now, self);
            }
        }
    }
}

impl SystemValues for SimulatedSystem {
    fn read(&self, value: SystemValue) -> Value {
        use SystemValue::*;
        let now = wall_clock_in_milliseconds();
        if let Some(setting) = settings::read_setting(&self.settings, value) {
            return setting;
        }
        if let Some(number) = self.folder.read(value) {
            return number;
        }
        if value == ThemePreset {
            return self.theme_preset;
        }
        match value {
            Now => now,
            BatteryPercent => 87,
            BatteryMillivolts => 4056,
            StepsToday => 4321,
            Brightness => self.settings.get(Setting::Brightness) as Value,
            HeartRate => self
                .latest_heart_rate
                .map_or(64, |reading| reading.beats_per_minute),
            HeartRateMeasuredAt => self
                .latest_heart_rate
                .map_or(now - 5 * MINUTE_IN_MILLISECONDS, |reading| reading.measured_at),
            HeartRateStatus => self.heart_rate_status as Value,
            IsHeartRateRunning => (self.heart_rate_status != Status::Stopped) as Value,
            IsPhoneConnected | MusicIsPlaying => 1,
            WeatherTemperature => 18,
            WeatherLow => 11,
            WeatherHigh => 21,
            WeatherReceivedAt => now - 20 * MINUTE_IN_MILLISECONDS,
            WeatherSunrise => now - 6 * HOUR_IN_MILLISECONDS,
            WeatherSunset => now + 6 * HOUR_IN_MILLISECONDS,
            MusicPosition => 72_000,
            MusicLength => 215_000,
            NavigationDistance => 350,
            NavigationProgressPercent => 40,
            NextAlarm => NONE,
            WeatherForecastLength => SIMULATED_FORECAST.len() as Value,
            InstalledAppCount => self.installed_apps.len() as Value,
            StepTrip => STARTING_TRIP_STEPS,
            StepDayCount => STEP_HISTORY_DAYS as Value,
            StepHourCount => (STEP_HISTORY_DAYS * HOURS_PER_DAY) as Value,
            StartedAt => self.started_at,
            FreeStorageKilobytes => SIMULATED_FREE_STORAGE_KILOBYTES,
            // No call has come.
            CallState => 2,
            TouchChipId => SIMULATED_TOUCH_CHIP_ID,
            AccelerometerChipId => SIMULATED_ACCELEROMETER_CHIP_ID,
            TaskCount => 1,
            BootloaderCrc => NONE,
            _ => 0,
        }
    }

    fn read_element_text(&self, value: SystemValue, index: Value) -> &str {
        if let Some(text) = self.folder.read_element_text(value, index) {
            return text;
        }
        if value == SystemValue::TaskName {
            return if index == 0 { SIMULATED_TASK_NAME } else { "" };
        }
        let Some((name, id)) = usize::try_from(index)
            .ok()
            .and_then(|place| self.installed_apps.get(place))
        else {
            return "";
        };
        match value {
            SystemValue::InstalledAppName => name,
            SystemValue::InstalledAppId => id,
            _ => "",
        }
    }

    /// A three-day forecast: each day's condition, low and high; the installed apps' icons; and the test watch's steps.
    fn read_element(&self, value: SystemValue, index: Value) -> Value {
        if let Some(number) = self.folder.read_element(value, index) {
            return number;
        }
        if let Some(steps) = simulated_steps(value, index) {
            return steps;
        }
        if value == SystemValue::InstalledAppIcon {
            return match u16::try_from(index) {
                Ok(place) if (place as usize) < self.installed_apps.len() => {
                    ImageReference::InstalledApp(place).to_value() as Value
                }
                _ => NONE,
            };
        }
        let Some(&(condition, low, high)) = usize::try_from(index)
            .ok()
            .and_then(|day| SIMULATED_FORECAST.get(day))
        else {
            return NONE;
        };
        match value {
            SystemValue::WeatherForecastCondition => condition,
            SystemValue::WeatherForecastLow => low,
            SystemValue::WeatherForecastHigh => high,
            _ => NONE,
        }
    }

    fn read_text(&self, value: SystemValue) -> &str {
        use SystemValue::*;
        if let Some(text) = self.folder.read_text(value) {
            return text;
        }
        match value {
            WeatherLocation => "Simulated City",
            MusicArtist => "The Simulators",
            MusicTrack => "Test Pattern",
            NavigationInstruction => "Turn left onto Main Street",
            FirmwareVersion => SIMULATED_FIRMWARE_VERSION,
            ResetReason => "power on",
            _ => "",
        }
    }

    fn read_file(&self, path: &str, offset: u32, out: &mut [u8]) -> Option<usize> {
        read_storage_file(self.storage_folder.as_deref()?, path, offset, out)
    }

    fn random_number(&self) -> u32 {
        let mut state = self.random_state.get();
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        self.random_state.set(state);
        state
    }
}

impl SimulatedSystem {
    /// Lists a folder of the storage folder at once, where the watch lists it once the handler returns.
    pub(crate) fn list_folder(&mut self, path: &str) {
        if self.folder.begin(path) {
            if let Some(storage) = &self.storage_folder {
                for_each_storage_file(storage, "", &mut |file_path, length| {
                    self.folder.add_file(file_path, length)
                });
            }
        }
        self.folder_changed = true;
    }
}

/// Gives `visit` every file under a folder of the computer as the watch's path and its length, such as `/remote/buttons`.
pub fn for_each_storage_file(folder: &Path, watch_path: &str, visit: &mut dyn FnMut(&str, usize)) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let inner_path = format!("{watch_path}/{name}");
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            for_each_storage_file(&entry.path(), &inner_path, visit);
        } else if let Ok(metadata) = entry.metadata() {
            visit(&inner_path, metadata.len() as usize);
        }
    }
}

/// Reads part of a watch file from the folder standing in for the watch's storage, as `read_file` does.
pub fn read_storage_file(folder: &Path, path: &str, offset: u32, out: &mut [u8]) -> Option<usize> {
    let relative = path.strip_prefix('/')?;
    if relative
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return None;
    }
    let mut file = File::open(folder.join(relative)).ok()?;
    file.seek(SeekFrom::Start(offset as u64)).ok()?;
    let mut filled = 0;
    while filled < out.len() {
        match file.read(&mut out[filled..]).ok()? {
            0 => break,
            read => filled += read,
        }
    }
    Some(filled)
}

/// The step history the test watch starts with, by day or by hour.
fn simulated_steps(value: SystemValue, index: Value) -> Option<Value> {
    let place = usize::try_from(index).ok();
    match value {
        SystemValue::StepDayTotal => Some(
            place
                .and_then(|day| STARTING_DAY_STEPS.get(day).copied())
                .unwrap_or(NONE),
        ),
        SystemValue::StepHourSteps => Some(
            place
                .and_then(|hour| starting_step_hours().get(hour).copied())
                .unwrap_or(NONE),
        ),
        _ => None,
    }
}

/// Which of an effect's arguments are text, so they print as text rather than as numbers.
fn text_arguments(kind: EffectKind) -> &'static [usize] {
    use EffectKind::*;
    match kind {
        Go | OpenApp | ShowToast | Log | SetSetting | PrefetchFile | OpenFolder => &[0],
        Notify | SendToPhone => &[0, 1],
        _ => &[],
    }
}

/// The name an effect's first argument stands for, where the compiler numbers names.
pub fn named_argument(kind: EffectKind, number: Value) -> Option<&'static str> {
    const PATTERNS: [&str; 6] = ["tap", "double_tap", "notification", "alarm", "success", "failure"];
    const MUSIC_COMMANDS: [&str; 6] = ["play", "pause", "next", "previous", "volume_up", "volume_down"];
    const CALL_COMMANDS: [&str; 3] = ["answer", "reject", "mute"];
    let names: &[&str] = match kind {
        EffectKind::VibratePattern => &PATTERNS,
        EffectKind::MusicControl => &MUSIC_COMMANDS,
        EffectKind::CallControl => &CALL_COMMANDS,
        _ => return None,
    };
    usize::try_from(number)
        .ok()
        .and_then(|index| names.get(index).copied())
}

/// An effect written the way an app calls it, such as `ShowToast("Saved")`.
fn describe(effect: &AppEffect) -> String {
    let texts = text_arguments(effect.kind);
    let arguments: Vec<String> = (0..effect.kind.argument_count())
        .map(|position| {
            let number = effect.number(position);
            if texts.contains(&position) {
                if let Some(text) = effect.text(position) {
                    return format!("{text:?}");
                }
            }
            match (position, named_argument(effect.kind, number)) {
                (0, Some(name)) => name.to_string(),
                _ => number.to_string(),
            }
        })
        .collect();
    format!("{:?}({})", effect.kind, arguments.join(", "))
}

impl System for SimulatedSystem {
    fn carry_out(&mut self, effect: &AppEffect) {
        println!("effect: {}", describe(effect));
        if matches!(effect.kind, EffectKind::Vibrate | EffectKind::VibratePattern) {
            if let Some(requests) = &self.vibrator_requests {
                let _ = requests.send(TAP);
            }
        }
        match effect.kind {
            EffectKind::Back | EffectKind::Close => println!("  the watch would leave the app now"),
            EffectKind::RequestNavigationState | EffectKind::RequestMusicState => {
                println!("  the watch would ask the phone; the simulator's values stay as they are")
            }
            EffectKind::SetClock => {
                println!("  the watch would set its clock; the simulator keeps the computer's")
            }
            EffectKind::SetSetting => {
                let name = effect.text(0).unwrap_or("").to_string();
                self.change_setting(&name, effect.number(1));
            }
            EffectKind::SetDoNotDisturb => {
                self.change_setting(Setting::DoNotDisturb.name(), effect.number(0))
            }
            EffectKind::SetBluetooth => self.change_setting(Setting::Bluetooth.name(), effect.number(0)),
            EffectKind::OpenFolder => self.list_folder(effect.text(0).unwrap_or("")),
            EffectKind::ConfirmFirmware => println!("  the simulated firmware is always confirmed"),
            EffectKind::ChooseWatchFace => println!("  the watch would open the face chooser"),
            EffectKind::RollBackFirmware => {
                println!("  the watch would restart into the firmware before this one")
            }
            EffectKind::StartHeartRate | EffectKind::StopHeartRate | EffectKind::MeasureHeartRate => {
                if let Some(link) = &self.heart_rate_link {
                    match effect.kind {
                        EffectKind::StartHeartRate => link.start(),
                        EffectKind::StopHeartRate => link.stop(),
                        _ => link.measure_once(),
                    }
                }
            }
            _ => {}
        }
    }

    fn app_failed(&mut self, failure: AppFailure) {
        eprintln!("app failed, so the watch would show its crash screen: {failure:?}");
    }
}

/// A simulated system with no motor, linked to a heart rate sensor the test steps itself.
#[cfg(test)]
pub fn test_system_with_heart_rate(link: HeartRateLink) -> SimulatedSystem {
    SimulatedSystem {
        heart_rate_link: Some(link),
        ..test_system()
    }
}

/// A simulated system with no motor whose storage is a folder of the computer, for tests.
#[cfg(test)]
pub fn test_system_with_storage(folder: &Path) -> SimulatedSystem {
    SimulatedSystem {
        storage_folder: Some(folder.to_path_buf()),
        ..test_system()
    }
}

/// A simulated system with no motor, for tests.
#[cfg(test)]
pub fn test_system() -> SimulatedSystem {
    SimulatedSystem {
        vibrator_requests: None,
        random_state: Cell::new(1),
        installed_apps: Vec::new(),
        settings: Settings::default(),
        changed_settings: 0,
        theme_preset: 0,
        theme_request: None,
        started_at: 0,
        storage_folder: None,
        folder: FolderListing::new(),
        folder_changed: false,
        heart_rate_link: None,
        latest_heart_rate: None,
        heart_rate_status: Status::Stopped,
    }
}

// ============================================================================
// Running an app
// ============================================================================

/// Opens the app, applies `clicks`, and saves one frame to `screenshot_path`.
pub fn save_screenshot(
    files: AppFiles,
    panel: &SimulatedPanel,
    display: &mut WatchDisplay,
    screen: &mut SimulatorDisplay<Rgb565>,
    output_settings: &OutputSettings,
    clicks: &[Click],
    screenshot_path: &str,
) -> Result<(), Box<dyn Error>> {
    let mut system = SimulatedSystem::new(None, None, 1, &files);
    let now = wall_clock_in_milliseconds();
    let mut host = open(files, now, &mut system)?;
    for click in clicks {
        host.touch(
            Touch::Click {
                x: click.x,
                y: click.y,
            },
            now,
            &mut system,
        );
        system.tell_app_about_settings(&mut host, now);
    }
    host.draw(display).unwrap_or_else(|never| match never {});

    copy_panel_to_screen(panel, screen, false);
    screen
        .to_rgb_output_image(output_settings)
        .save_png(screenshot_path)?;
    println!("saved {screenshot_path}");
    Ok(())
}

/// An app image, and the shared library's bytes or nothing.
#[derive(Clone, Copy)]
pub struct AppFiles<'image> {
    pub image: &'image [u8],
    pub library: &'image [u8],
    /// Other apps' images, which a launcher lists.
    pub installed_apps: &'image [&'image [u8]],
    /// Each of those apps' icon, or empty for none.
    pub installed_app_icons: &'image [&'image [u8]],
    /// A folder on the computer standing in for the watch's storage, so `/remote/buttons` is `<folder>/remote/buttons`.
    pub storage_folder: Option<&'image Path>,
}

/// Opens the app, with the shared library when one was given.
pub fn open<'image>(
    files: AppFiles<'image>,
    now: Value,
    system: &mut SimulatedSystem,
) -> Result<AppHost<'image>, Box<dyn Error>> {
    let mut host = AppHost::open(files.image, DEFAULT_THEME, now, system)
        .map_err(|error| format!("the app did not open: {error:?}"))?;
    if !files.library.is_empty() {
        host.set_library(files.library, now, system)
            .map_err(|error| format!("the shared library did not open: {error:?}"))?;
    }
    host.set_installed_app_icons(files.installed_app_icons);
    Ok(host)
}

/// Runs the app in the window until it is closed.
pub fn run(
    files: AppFiles,
    panel: SimulatedPanel,
    mut display: WatchDisplay,
    mut screen: SimulatorDisplay<Rgb565>,
    output_settings: OutputSettings,
) -> Result<(), Box<dyn Error>> {
    let motor_running = Arc::new(AtomicBool::new(false));
    let mut system = SimulatedSystem::new(
        Some(start_vibrator(motor_running.clone())),
        Some(start_heart_rate_sensor()),
        wall_clock_in_milliseconds() as u32 | 1,
        &files,
    );
    let mut host = open(files, wall_clock_in_milliseconds(), &mut system)?;
    let mut window = Window::new("BattleTime simulator", &output_settings);
    let mut touch = TouchTracker::default();
    let mut is_light = false;

    let mut strip = vec![0u16; STRIP_PIXELS];

    print_app_controls(host.image().string(host.image().name_string).unwrap_or("app"));
    loop {
        let now = wall_clock_in_milliseconds();
        host.tick(now, &mut system);
        system.take_heart_rate_updates(&mut host, now);
        // A transition draws one step a frame, paced by the clock as it is on the watch.
        if host.needs_drawing() {
            host.draw_step_at(&mut display, &mut strip, now as u64)
                .unwrap_or_else(|never| match never {});
        }
        copy_panel_to_screen(&panel, &mut screen, motor_running.load(Ordering::Relaxed));
        window.update(&screen);

        let events: Vec<SimulatorEvent> = window.events().collect();
        for event in events {
            let now = wall_clock_in_milliseconds();
            match event {
                SimulatorEvent::Quit => return Ok(()),
                SimulatorEvent::KeyDown {
                    keycode,
                    repeat: false,
                    ..
                } => match keycode {
                    Keycode::Q | Keycode::Escape => return Ok(()),
                    Keycode::Space => report(host.go_back_a_screen(now, &mut system), "button"),
                    Keycode::T => {
                        is_light = !is_light;
                        host.set_theme(if is_light { LIGHT_THEME } else { DEFAULT_THEME });
                    }
                    _ => {}
                },
                SimulatorEvent::MouseButtonDown { point, .. } => touch.press(point),
                SimulatorEvent::MouseMove { point } => {
                    if let Some(drag) = touch.drag(point) {
                        host.touch(drag, now, &mut system);
                    }
                }
                SimulatorEvent::MouseButtonUp { point, .. } => {
                    if let Some(gesture) = touch.release(point) {
                        report(host.touch(gesture, now, &mut system), "touch");
                    }
                }
                _ => {}
            }
            system.tell_app_about_settings(&mut host, now);
        }

        thread::sleep(FRAME_PAUSE);
    }
}

pub fn report(result: EventResult, what: &str) {
    if result == EventResult::NotHandled {
        println!("{what}: the app does not take it, so the system's own meaning would apply");
    }
}

fn print_app_controls(name: &str) {
    println!("BattleTime simulator running {name}");
    println!("  click, hold or drag      touch the screen");
    println!("  swipe with the mouse     swipe");
    println!("  space                    press the side button, which goes back");
    println!("  t                        switch between the default and a light theme");
    println!("  q or Escape              quit");
}

// ============================================================================
// Mouse gestures
// ============================================================================

/// Turns mouse presses and moves into clicks, long presses, drags and swipes.
#[derive(Default)]
pub struct TouchTracker {
    start: Option<(Point, Instant)>,
    has_dragged: bool,
}

impl TouchTracker {
    pub fn press(&mut self, point: Point) {
        self.start = Some((point, Instant::now()));
        self.has_dragged = false;
    }

    pub fn drag(&mut self, point: Point) -> Option<Touch> {
        let (start, _) = self.start?;
        let distance = (point - start).x.abs().max((point - start).y.abs());
        if distance < DRAG_DISTANCE_IN_PIXELS && !self.has_dragged {
            return None;
        }
        self.has_dragged = true;
        Some(Touch::Drag {
            x: point.x,
            y: point.y,
            start_x: start.x,
            start_y: start.y,
        })
    }

    pub fn release(&mut self, point: Point) -> Option<Touch> {
        let (start, pressed_at) = self.start.take()?;
        let moved = point - start;
        if moved.x.abs().max(moved.y.abs()) >= SWIPE_DISTANCE_IN_PIXELS {
            let direction = match (moved.x.abs() > moved.y.abs(), moved.x > 0, moved.y > 0) {
                (true, true, _) => SwipeDirection::Right,
                (true, false, _) => SwipeDirection::Left,
                (false, _, true) => SwipeDirection::Down,
                (false, _, false) => SwipeDirection::Up,
            };
            return Some(Touch::Swipe(direction));
        }
        if self.has_dragged {
            return None;
        }
        Some(match pressed_at.elapsed() >= LONG_PRESS {
            true => Touch::LongPress {
                x: point.x,
                y: point.y,
            },
            false => Touch::Click {
                x: point.x,
                y: point.y,
            },
        })
    }
}
