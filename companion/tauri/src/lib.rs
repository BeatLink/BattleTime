// The BattleTime Companion app, on the desktop and on Android.
//
// It runs the same interface as the phone app, from the web export, and answers the window's
// `window.battletime` bridge (bridge.js) with Bluetooth, D-Bus, note files, remote commands, plugins and sync here.
// D-Bus, note file watching and shell commands are the desktop's alone; Android has the phone bridge plugin.

#[cfg(desktop)]
mod desktop;
#[cfg(desktop)]
mod notes;
mod picked_file;
mod plugins;
#[cfg(desktop)]
mod remote;
mod sync;

use std::sync::{Arc, OnceLock};

use battletime_companion_core::ble::{BleError, WatchBle};
use battletime_companion_core::plugins::host::{Fetcher, PluginHost};
use battletime_companion_core::plugins::http;
use battletime_companion_core::plugins::store::PluginStore;
use battletime_companion_core::sync::files::FileClient;
use battletime_companion_core::sync::store::SyncStore;
use battletime_companion_core::sync::Syncer;
use battletime_companion_core::web_request;
use serde::Serialize;
use serde_json::json;
#[cfg(desktop)]
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder};
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
    ble: WatchBle,
    #[cfg(desktop)]
    desktop: DesktopIntegration,
    #[cfg(desktop)]
    notes: NoteFileWatches,
    plugins: Arc<PluginHost>,
    syncer: Arc<Syncer>,
    lease: Arc<sync::WindowLease>,
}

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
    answer(companion.ble.start_scan().await)
}

#[tauri::command]
async fn ble_stop_scan(companion: State<'_, Companion>) -> Answer<()> {
    answer(companion.ble.stop_scan().await)
}

#[tauri::command]
async fn ble_connect(app: AppHandle, companion: State<'_, Companion>, id: String) -> Answer<()> {
    bluetooth_allowed(&app).await?;
    answer(companion.ble.connect(&id).await)
}

#[tauri::command]
async fn ble_reconnect(app: AppHandle, companion: State<'_, Companion>, id: String) -> Answer<()> {
    bluetooth_allowed(&app).await?;
    answer(companion.ble.reconnect(&id).await)
}

#[tauri::command]
async fn ble_disconnect(companion: State<'_, Companion>) -> Answer<()> {
    answer(companion.ble.disconnect().await)
}

#[tauri::command]
async fn ble_write(companion: State<'_, Companion>, text: String) -> Answer<()> {
    answer(companion.ble.write(&text).await)
}

#[tauri::command]
async fn ble_write_bytes(companion: State<'_, Companion>, data: Vec<u8>) -> Answer<()> {
    answer(companion.ble.write_bytes(&data).await)
}

#[tauri::command]
fn ble_gatt_has(companion: State<'_, Companion>, service: String, characteristic: String) -> bool {
    companion.ble.gatt_has(&service, &characteristic)
}

#[tauri::command]
async fn ble_gatt_read(
    companion: State<'_, Companion>,
    service: String,
    characteristic: String,
) -> Answer<Vec<u8>> {
    answer(companion.ble.gatt_read(&service, &characteristic).await)
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
            .ble
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
    answer(companion.ble.gatt_subscribe(&service, &characteristic).await)
}

#[tauri::command]
async fn ble_gatt_unsubscribe(
    companion: State<'_, Companion>,
    service: String,
    characteristic: String,
) -> Answer<()> {
    answer(companion.ble.gatt_unsubscribe(&service, &characteristic).await)
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

fn companion_for(app: &AppHandle) -> Result<Companion, Box<dyn std::error::Error>> {
    let ble_app = app.clone();

    // The plugins write to the watch through the Bluetooth layer, which hands them the watch's text in turn.
    let ble_cell = Arc::new(OnceLock::new());
    let output = plugins::DesktopOutput {
        app: app.clone(),
        ble: ble_cell.clone(),
    };
    let fetcher: Fetcher = Arc::new(http::fetch);
    let store = PluginStore::new(app.path().app_data_dir()?.join("plugins"));
    let plugin_host = Arc::new(PluginHost::new(store, Arc::new(output), fetcher));

    // The syncer reads the watch's files through the same link, and the plugins read what it keeps.
    let files = Arc::new(FileClient::new(Arc::new(sync::BleFiles {
        ble: ble_cell.clone(),
    })));
    let syncer = Arc::new(Syncer::new(
        files,
        SyncStore::new(app.path().app_data_dir()?.join("sync")),
        Arc::new(sync::DesktopSync {
            app: app.clone(),
            ble: ble_cell.clone(),
            plugins: plugin_host.clone(),
        }),
    ));
    let syncer_cell = Arc::new(OnceLock::new());
    let _ = syncer_cell.set(syncer.clone());
    plugin_host.set_watch_data(Arc::new(sync::PluginData { syncer: syncer_cell }));
    let lease = Arc::new(sync::WindowLease::default());

    let (followed, synced, leased) = (plugin_host.clone(), syncer.clone(), lease.clone());
    let ble = WatchBle::new(Arc::new(move |name, payload| {
        plugins::follow_watch(&followed, name, &payload);
        if sync::follow_watch(&synced, &leased, name, &payload) {
            let _ = ble_app.emit("ble:event", json!({ "name": name, "payload": payload }));
        }
    }));
    let _ = ble_cell.set(ble.clone());

    Ok(Companion {
        ble,
        plugins: plugin_host,
        syncer,
        lease,
        #[cfg(desktop)]
        desktop: {
            let desktop_app = app.clone();
            DesktopIntegration::new(Arc::new(move |payload| {
                let _ = desktop_app.emit("desktop:event", payload);
            }))
        },
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_phone_bridge::init())
        .register_uri_scheme_protocol("plugin", |context, request| {
            let companion = context.app_handle().state::<Companion>();
            plugins::serve_ui_file(&companion.plugins, request.uri())
        })
        .setup(|app| {
            let companion = companion_for(app.handle())?;
            #[cfg(desktop)]
            {
                let desktop = companion.desktop.clone();
                tauri::async_runtime::spawn(async move { desktop.start().await });
            }
            let plugin_host = companion.plugins.clone();
            let syncer = companion.syncer.clone();
            app.manage(companion);
            tauri::async_runtime::spawn(syncer.run());
            std::thread::spawn(move || plugin_host.start_all());

            // In development BATTLETIME_DEV_URL points at the Vite dev server (`npm run dev`), so the interface reloads.
            let url = match std::env::var("BATTLETIME_DEV_URL") {
                Ok(dev_url) => WebviewUrl::External(dev_url.parse()?),
                Err(_) => WebviewUrl::App("index.html".into()),
            };
            WebviewWindowBuilder::new(app, "main", url)
                .title("BattleTime Companion")
                .inner_size(480.0, 860.0)
                .background_color(tauri::window::Color(0, 0, 0, 255))
                .initialization_script(bridge_script(cfg!(target_os = "android")))
                .build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
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

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            let companion = app.state::<Companion>();
            #[cfg(desktop)]
            companion.notes.stop_all();
            companion.plugins.stop_all();
            tauri::async_runtime::block_on(companion.ble.destroy());
        }
    });
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
