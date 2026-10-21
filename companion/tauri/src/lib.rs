// The BattleTime Companion app, on the desktop and on Android.
//
// It hosts the core's daemon, which keeps the watch linked and does the background work, and answers the
// window's `window.battletime` bridge (bridge.js) from it. D-Bus, note file watching and shell commands
// are the desktop's alone; Android has the phone bridge plugin, whose keep-alive service can start the
// daemon before any window (android.rs).

#[cfg(target_os = "android")]
mod android;
#[cfg(desktop)]
mod autostart;
#[cfg(desktop)]
mod desktop;
#[cfg(desktop)]
mod desktop_events;
#[cfg(desktop)]
mod host_device;
#[cfg(desktop)]
mod notes;
// The phone's device is made only on Android, and tested everywhere.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
mod phone_device;
mod picked_file;
mod plugins;
#[cfg(desktop)]
mod remote;
#[cfg(desktop)]
mod single_instance;
mod sync;

use std::path::PathBuf;
use std::sync::{Arc, OnceLock, Weak};

use battletime_companion_core::ble::{BleError, WatchBle};
use battletime_companion_core::daemon::device::Device;
use battletime_companion_core::daemon::{Daemon, DaemonSetup, Features};
use battletime_companion_core::logging;
use battletime_companion_core::web_request;
use serde::Serialize;
use serde_json::Value;
#[cfg(desktop)]
use tauri::RunEvent;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_phone_bridge::PhoneBridgeExt;

#[cfg(desktop)]
use crate::desktop::DesktopIntegration;
#[cfg(desktop)]
use crate::notes::NoteFileWatches;

// Builds `window.battletime` before the page's own scripts run.
const BRIDGE_SCRIPT: &str = include_str!("bridge.js");

// The bridge script, told whether it runs on Android, where the phone takes the computer's part.
fn bridge_script(on_android: bool) -> String {
    format!("window.__BATTLETIME_ANDROID__ = {on_android};\n{BRIDGE_SCRIPT}")
}

struct Companion {
    daemon: Arc<Daemon>,
    #[cfg(desktop)]
    desktop: DesktopIntegration,
    #[cfg(desktop)]
    notes: NoteFileWatches,
}

impl Companion {
    fn ble(&self) -> &WatchBle {
        self.daemon.ble().expect("the app's daemon runs on Bluetooth")
    }
}

// One daemon for the whole process, which on Android may outlive the window that started it.
static DAEMON: OnceLock<Arc<Daemon>> = OnceLock::new();

type Answer<T> = Result<T, String>;

fn answer<T>(result: Result<T, BleError>) -> Answer<T> {
    result.map_err(|error| error.0)
}

// ============================================================================
// Bluetooth
// ============================================================================

// Android asks the user for nearby devices before the first scan or connection; elsewhere this passes at once.
async fn bluetooth_allowed(app: &AppHandle) -> Answer<()> {
    if app.phone_bridge().request_bluetooth_permissions().await? {
        Ok(())
    } else {
        Err("Allow BattleTime Companion to find and connect to nearby devices.".into())
    }
}

#[tauri::command]
async fn ble_start_scan(app: AppHandle, companion: State<'_, Companion>) -> Answer<()> {
    bluetooth_allowed(&app).await?;
    companion.daemon.set_scanning(true);
    answer(companion.ble().start_scan().await)
}

// A line from the window's own log, such as an error its scripts did not catch.
#[tauri::command]
fn app_log(level: String, message: String) {
    let level = match level.as_str() {
        "error" => log::Level::Error,
        "warn" => log::Level::Warn,
        "info" => log::Level::Info,
        _ => log::Level::Debug,
    };
    log::log!(target: "battletime_companion_lib::window", level, "{message}");
}

#[tauri::command]
async fn ble_stop_scan(companion: State<'_, Companion>) -> Answer<()> {
    companion.daemon.set_scanning(false);
    answer(companion.ble().stop_scan().await)
}

#[tauri::command]
async fn ble_connect(app: AppHandle, companion: State<'_, Companion>, id: String) -> Answer<()> {
    bluetooth_allowed(&app).await?;
    answer(companion.ble().connect(&id).await)
}

#[tauri::command]
async fn ble_reconnect(app: AppHandle, companion: State<'_, Companion>, id: String) -> Answer<()> {
    bluetooth_allowed(&app).await?;
    answer(companion.ble().reconnect(&id).await)
}

#[tauri::command]
async fn ble_disconnect(companion: State<'_, Companion>) -> Answer<()> {
    answer(companion.ble().disconnect().await)
}

#[tauri::command]
async fn ble_write(companion: State<'_, Companion>, text: String) -> Answer<()> {
    answer(companion.ble().write(&text).await)
}

#[tauri::command]
async fn ble_write_bytes(companion: State<'_, Companion>, data: Vec<u8>) -> Answer<()> {
    answer(companion.ble().write_bytes(&data).await)
}

#[tauri::command]
fn ble_gatt_has(companion: State<'_, Companion>, service: String, characteristic: String) -> bool {
    companion.ble().gatt_has(&service, &characteristic)
}

#[tauri::command]
async fn ble_gatt_read(
    companion: State<'_, Companion>,
    service: String,
    characteristic: String,
) -> Answer<Vec<u8>> {
    answer(companion.ble().gatt_read(&service, &characteristic).await)
}

#[tauri::command]
async fn ble_gatt_write(
    companion: State<'_, Companion>,
    service: String,
    characteristic: String,
    data: Vec<u8>,
    mode: String,
) -> Answer<()> {
    answer(
        companion
            .ble()
            .gatt_write(&service, &characteristic, &data, &mode)
            .await,
    )
}

#[tauri::command]
async fn ble_gatt_subscribe(
    companion: State<'_, Companion>,
    service: String,
    characteristic: String,
) -> Answer<()> {
    answer(companion.ble().gatt_subscribe(&service, &characteristic).await)
}

#[tauri::command]
async fn ble_gatt_unsubscribe(
    companion: State<'_, Companion>,
    service: String,
    characteristic: String,
) -> Answer<()> {
    answer(companion.ble().gatt_unsubscribe(&service, &characteristic).await)
}

// ============================================================================
// The daemon
// ============================================================================

#[tauri::command]
fn daemon_state(companion: State<'_, Companion>) -> Value {
    companion.daemon.state()
}

#[tauri::command]
fn daemon_settings(companion: State<'_, Companion>) -> Value {
    companion.daemon.settings_json()
}

#[tauri::command]
fn daemon_update_settings(companion: State<'_, Companion>, patch: Value) -> Answer<Value> {
    companion.daemon.update_settings(&patch)
}

// The window hands over the settings it kept before the daemon, which takes them only once.
#[tauri::command]
fn daemon_import_settings(companion: State<'_, Companion>, settings: Value) -> Answer<bool> {
    companion.daemon.import_settings(&settings)
}

#[tauri::command]
async fn daemon_connect(
    app: AppHandle,
    companion: State<'_, Companion>,
    id: String,
    name: String,
) -> Answer<()> {
    bluetooth_allowed(&app).await?;
    companion.daemon.connect(&id, &name).await
}

#[tauri::command]
async fn daemon_disconnect(companion: State<'_, Companion>) -> Answer<()> {
    companion.daemon.disconnect().await
}

// A firmware update holds the daemon back from reconnecting while it moves the link itself.
#[tauri::command]
fn daemon_hold(companion: State<'_, Companion>, held: bool) {
    companion.daemon.hold(held);
}

#[tauri::command]
async fn daemon_refresh_status(companion: State<'_, Companion>) -> Answer<()> {
    companion.daemon.refresh_status().await
}

#[tauri::command]
async fn daemon_send_weather(companion: State<'_, Companion>) -> Answer<()> {
    features(&companion)?.weather.send_now().await
}

fn features<'state>(companion: &'state State<'_, Companion>) -> Answer<&'state Features> {
    companion
        .daemon
        .features()
        .ok_or_else(|| "The daemon is still starting.".to_string())
}

#[tauri::command]
async fn daemon_sync_note(companion: State<'_, Companion>) -> Answer<()> {
    features(&companion)?.notes.sync_now().await;
    Ok(())
}

#[tauri::command]
async fn daemon_forget_note(companion: State<'_, Companion>) -> Answer<()> {
    features(&companion)?.notes.forget().await
}

#[tauri::command]
async fn daemon_send_remote_layout(companion: State<'_, Companion>) -> Answer<()> {
    features(&companion)?.remote.send_layout().await
}

#[tauri::command]
async fn daemon_test_remote_button(companion: State<'_, Companion>, index: usize) -> Answer<()> {
    features(&companion)?.remote.test_button(index).await;
    Ok(())
}

#[tauri::command]
fn daemon_clear_remote_log(companion: State<'_, Companion>) -> Answer<()> {
    features(&companion)?.remote.clear_log();
    Ok(())
}

#[tauri::command]
fn daemon_stop_sos(companion: State<'_, Companion>) -> Answer<()> {
    features(&companion)?.sos.end_from_phone();
    Ok(())
}

#[tauri::command]
fn daemon_stop_finding_phone(companion: State<'_, Companion>) {
    companion.daemon.stop_finding_phone();
}

// ============================================================================
// This computer in place of a phone
// ============================================================================

#[cfg(desktop)]
#[tauri::command]
fn desktop_notification_access(companion: State<'_, Companion>) -> bool {
    companion.desktop.has_notification_access()
}

#[cfg(desktop)]
#[tauri::command]
async fn desktop_do_not_disturb(companion: State<'_, Companion>) -> Answer<bool> {
    Ok(companion.desktop.is_do_not_disturb().await)
}

#[cfg(desktop)]
#[tauri::command]
async fn desktop_media_command(
    companion: State<'_, Companion>,
    player: String,
    command: String,
) -> Answer<()> {
    companion.desktop.media_command(&player, &command).await
}

#[cfg(desktop)]
#[tauri::command]
async fn desktop_locate(companion: State<'_, Companion>) -> Answer<Value> {
    companion.desktop.locate().await
}

// The window asks for the players once it is listening, since it may have missed the first report.
#[cfg(desktop)]
#[tauri::command]
fn desktop_players(companion: State<'_, Companion>) {
    companion.desktop.emit_players();
}

// ============================================================================
// Notes
// ============================================================================

#[derive(Serialize)]
struct NoteFileRef {
    name: String,
    uri: String,
}

#[tauri::command]
async fn notes_pick(app: AppHandle) -> Answer<Option<NoteFileRef>> {
    let text = picked_file::FileKind {
        name: "Text",
        extensions: &["txt", "md", "text"],
    };
    let any = picked_file::FileKind {
        name: "All files",
        extensions: &["*"],
    };
    let Some(file) = picked_file::pick_file(&app, "Choose a note for the watch", &[text, any]).await else {
        return Ok(None);
    };
    Ok(Some(NoteFileRef {
        name: picked_file::file_name(&app, &file),
        uri: file.to_string(),
    }))
}

#[tauri::command]
fn notes_read(app: AppHandle, path: String) -> Answer<Vec<u8>> {
    picked_file::read_file(&app, &path)
}

// Android has no folder to watch behind a content:// link, so a phone reads the note again on each check.
#[cfg(desktop)]
#[tauri::command]
async fn notes_watch(companion: State<'_, Companion>, path: String) -> Answer<u64> {
    companion.notes.start(&path)
}

#[cfg(desktop)]
#[tauri::command]
fn notes_unwatch(companion: State<'_, Companion>, id: u64) {
    companion.notes.stop(id);
}

// ============================================================================
// Remote buttons
// ============================================================================

#[cfg(desktop)]
#[tauri::command]
async fn remote_run_command(command: String) -> Answer<remote::CommandResult> {
    remote::run_shell_command(&command, remote::COMMAND_TIMEOUT).await
}

// A web request for the notes URL or a remote button, which may trust a self-signed certificate.
#[tauri::command]
async fn web_request(request: web_request::WebRequest) -> Answer<web_request::WebReply> {
    tauri::async_runtime::spawn_blocking(move || web_request::request(&request))
        .await
        .map_err(|error| error.to_string())?
}

// ============================================================================
// Starting up
// ============================================================================

// The process's daemon, made by whichever comes first: the window's setup or, on Android, the keep-alive service.
fn process_daemon(data_dir: PathBuf, device: Arc<dyn Device>) -> Arc<Daemon> {
    DAEMON
        .get_or_init(|| {
            logging::start_with_file(&data_dir);
            let runtime = tauri::async_runtime::handle().inner().clone();
            let _entered = runtime.enter();
            Daemon::start(DaemonSetup {
                data_dir,
                device,
                runtime,
            })
        })
        .clone()
}

// The process's daemon, with this app's data folder, which on Android is the folder the service passes too.
fn daemon_for(app: &AppHandle, device: Arc<dyn Device>) -> Result<Arc<Daemon>, Box<dyn std::error::Error>> {
    if let Some(daemon) = DAEMON.get() {
        return Ok(daemon.clone());
    }
    Ok(process_daemon(app.path().app_data_dir()?, device))
}

fn companion_for(app: &AppHandle) -> Result<Companion, Box<dyn std::error::Error>> {
    // The device hears the daemon's events only once the daemon exists, so it is handed over afterwards.
    let daemon_cell: Arc<OnceLock<Weak<Daemon>>> = Arc::new(OnceLock::new());
    #[cfg(desktop)]
    let device = Arc::new(host_device::DesktopDevice::new(app, daemon_cell.clone()));
    #[cfg(desktop)]
    let desktop = device.integration.clone();
    #[cfg(target_os = "android")]
    let device = Arc::new(android::phone_device());
    let daemon = daemon_for(app, device)?;
    let _ = daemon_cell.set(Arc::downgrade(&daemon));

    let window = app.clone();
    daemon.set_host(Arc::new(move |event, payload| {
        let _ = window.emit(event, payload);
    }));

    Ok(Companion {
        daemon,
        #[cfg(desktop)]
        desktop,
        #[cfg(desktop)]
        notes: {
            let notes_app = app.clone();
            NoteFileWatches::new(
                Arc::new(move |id| {
                    let _ = notes_app.emit("notes:changed", id);
                }),
                notes::SETTLE,
            )
        },
    })
}

// The note's picked file, watched for saves so the daemon sends it again at once rather than at its next check.
#[cfg(desktop)]
async fn follow_note_file(daemon: Arc<Daemon>) {
    #[derive(serde::Deserialize, Default, PartialEq)]
    #[serde(default, rename_all = "camelCase")]
    struct NoteFileSettings {
        note_source: String,
        note_file: Option<Value>,
    }

    let saved = daemon.clone();
    let watches = NoteFileWatches::new(
        Arc::new(move |_| {
            if let Some(features) = saved.features() {
                features.notes.file_changed();
            }
        }),
        notes::SETTLE,
    );
    let mut changes = daemon.settings().subscribe();
    let mut watched: Option<(String, u64)> = None;
    loop {
        let view: NoteFileSettings = daemon.settings().view();
        let wanted = (view.note_source == "file")
            .then(|| {
                view.note_file
                    .and_then(|file| file["uri"].as_str().map(str::to_string))
            })
            .flatten();
        if watched.as_ref().map(|(path, _)| path) != wanted.as_ref() {
            if let Some((_, id)) = watched.take() {
                watches.stop(id);
            }
            if let Some(path) = wanted {
                watched = watches.start(&path).ok().map(|id| (path, id));
            }
        }
        if changes.changed().await.is_err() {
            return;
        }
    }
}

// ============================================================================
// Running in the background on the desktop
// ============================================================================

// The desktop's own settings, which the window's Settings screen changes.
#[cfg(desktop)]
#[derive(serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct DesktopSettings {
    run_in_background: bool,
    start_with_computer: bool,
}

#[cfg(desktop)]
impl Default for DesktopSettings {
    fn default() -> Self {
        DesktopSettings {
            run_in_background: true,
            start_with_computer: false,
        }
    }
}

#[cfg(desktop)]
fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

// Keeps the autostart entry matching the setting, for the life of the app.
#[cfg(desktop)]
async fn follow_autostart(daemon: Arc<Daemon>) {
    let mut changes = daemon.settings().subscribe();
    loop {
        let wanted = daemon.settings().view::<DesktopSettings>().start_with_computer;
        if let Some(folder) = autostart::autostart_folder() {
            if let Err(error) = autostart::set_autostart(&folder, wanted, &autostart::program()) {
                log::warn!("Could not change the autostart entry: {error}");
            }
        }
        if changes.changed().await.is_err() {
            return;
        }
    }
}

// The tray icon, with a way back to the window and a way to quit; a desktop without a tray goes without.
#[cfg(desktop)]
fn add_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    let open = MenuItem::with_id(app, "open", "Open BattleTime Companion", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("BattleTime Companion")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_window(app),
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[cfg(desktop)]
#[tauri::command]
fn app_quit(app: AppHandle) {
    app.exit(0);
}

// Lets the command line tool use this app's link to the watch rather than reaching for it through BlueZ.
#[cfg(desktop)]
async fn serve_command_line(daemon: Arc<Daemon>, data_dir: std::path::PathBuf) {
    use battletime_companion_core::daemon::socket;

    let path = socket::default_socket_path(&data_dir);
    if let Err(error) = socket::serve_daemon(&daemon, &path).await {
        log::warn!(
            "The command line socket at {} could not start: {error}",
            path.display()
        );
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::start();
    log::info!("BattleTime Companion {} starting", env!("CARGO_PKG_VERSION"));
    // A second launch shows the running companion's window instead of starting another.
    #[cfg(desktop)]
    let launches = {
        let path = single_instance::socket_path(
            std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from),
            &std::env::temp_dir(),
        );
        match single_instance::claim(&path) {
            Ok(single_instance::Claim::AlreadyRunning) => return,
            Ok(single_instance::Claim::First(listener)) => Some(listener),
            Err(error) => {
                log::warn!("Could not check for a running companion: {error}");
                None
            }
        }
    };
    // Started with the computer, the companion keeps the watch without opening its window.
    #[cfg(desktop)]
    let background = std::env::args().any(|argument| argument == "--background");

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_phone_bridge::init())
        .register_uri_scheme_protocol("plugin", |context, request| {
            let companion = context.app_handle().state::<Companion>();
            plugins::serve_ui_file(companion.daemon.plugins(), request.uri())
        })
        .setup(move |app| {
            let companion = companion_for(app.handle())?;
            #[cfg(desktop)]
            {
                let desktop = companion.desktop.clone();
                tauri::async_runtime::spawn(async move { desktop.start().await });
                tauri::async_runtime::spawn(follow_note_file(companion.daemon.clone()));
                tauri::async_runtime::spawn(serve_command_line(
                    companion.daemon.clone(),
                    app.path().app_data_dir()?,
                ));
            }
            app.manage(companion);

            // In development BATTLETIME_DEV_URL points at the Vite dev server (`npm run dev`), so the interface reloads.
            let url = match std::env::var("BATTLETIME_DEV_URL") {
                Ok(dev_url) => WebviewUrl::External(dev_url.parse()?),
                Err(_) => WebviewUrl::App("index.html".into()),
            };
            let window = WebviewWindowBuilder::new(app, "main", url)
                .title("BattleTime Companion")
                .inner_size(480.0, 860.0)
                .background_color(tauri::window::Color(0, 0, 0, 255))
                .initialization_script(bridge_script(cfg!(target_os = "android")));
            #[cfg(desktop)]
            let window = window.visible(!background);
            window.build()?;

            #[cfg(desktop)]
            {
                if let Some(listener) = launches {
                    let handle = app.handle().clone();
                    single_instance::follow_launches(listener, move || show_window(&handle));
                }
                let daemon = app.state::<Companion>().daemon.clone();
                tauri::async_runtime::spawn(follow_autostart(daemon));
                // The tray library may be missing on a desktop with no tray, which it reports by panicking.
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| add_tray(app))) {
                    Ok(Err(error)) => log::warn!("Could not add the tray icon: {error}"),
                    Err(_) => {
                        log::info!("This desktop has no tray, so the companion runs without an icon there.")
                    }
                    Ok(Ok(())) => {}
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window leaves the daemon keeping the watch, unless the user turned that off.
            #[cfg(desktop)]
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let companion = window.app_handle().state::<Companion>();
                if companion
                    .daemon
                    .settings()
                    .view::<DesktopSettings>()
                    .run_in_background
                {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            #[cfg(mobile)]
            let _ = (window, event);
        })
        .invoke_handler(tauri::generate_handler![
            app_log,
            ble_start_scan,
            ble_stop_scan,
            ble_connect,
            ble_reconnect,
            ble_disconnect,
            ble_write,
            ble_write_bytes,
            ble_gatt_has,
            ble_gatt_read,
            ble_gatt_write,
            ble_gatt_subscribe,
            ble_gatt_unsubscribe,
            daemon_state,
            daemon_settings,
            daemon_update_settings,
            daemon_import_settings,
            daemon_connect,
            daemon_disconnect,
            daemon_hold,
            daemon_refresh_status,
            daemon_stop_finding_phone,
            daemon_send_weather,
            daemon_sync_note,
            daemon_forget_note,
            daemon_send_remote_layout,
            daemon_test_remote_button,
            daemon_clear_remote_log,
            daemon_stop_sos,
            #[cfg(desktop)]
            desktop_notification_access,
            #[cfg(desktop)]
            desktop_do_not_disturb,
            #[cfg(desktop)]
            desktop_media_command,
            #[cfg(desktop)]
            desktop_locate,
            #[cfg(desktop)]
            desktop_players,
            notes_pick,
            notes_read,
            #[cfg(desktop)]
            notes_watch,
            #[cfg(desktop)]
            notes_unwatch,
            #[cfg(desktop)]
            remote_run_command,
            #[cfg(desktop)]
            app_quit,
            web_request,
            plugins::plugins_list,
            plugins::plugins_install_pick,
            plugins::plugins_install,
            plugins::plugins_remove,
            plugins::plugins_set_enabled,
            plugins::plugins_watch_apps,
            plugins::plugins_install_on_watch,
            plugins::plugins_ui_message,
            sync::sync_settings,
            sync::sync_set_settings,
            sync::sync_now,
            sync::sync_read,
            sync::sync_list,
            sync::sync_write_app_state,
            sync::sync_write_settings,
            sync::files_begin,
            sync::files_end,
        ])
        .build(tauri::generate_context!())
        .expect("the companion app could not start");

    #[cfg(desktop)]
    app.run(|app, event| {
        if let RunEvent::Exit = event {
            let companion = app.state::<Companion>();
            companion.notes.stop_all();
            tauri::async_runtime::block_on(companion.daemon.shut_down());
        }
    });
    // On Android the daemon outlives the window, for the keep-alive service, and ends with the process.
    #[cfg(mobile)]
    app.run(|_, _| {});
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bridge_script_says_whether_it_runs_on_android() {
        assert!(bridge_script(true).starts_with("window.__BATTLETIME_ANDROID__ = true;\n"));
        assert!(bridge_script(false).starts_with("window.__BATTLETIME_ANDROID__ = false;\n"));
        assert!(bridge_script(false).ends_with(BRIDGE_SCRIPT));
    }
}
