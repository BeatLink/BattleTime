// The NeoTime Companion desktop app.
//
// It runs the same interface as the phone app, from the web export, and answers the window's
// `window.neotime` bridge (bridge.js) with Bluetooth, D-Bus, note files and remote commands here.

mod desktop;
mod notes;
mod remote;

use std::sync::Arc;

use neotime_companion_core::ble::{BleError, WatchBle};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;

use crate::desktop::DesktopIntegration;
use crate::notes::NoteFileWatches;

// Builds `window.neotime` before the page's own scripts run.
const BRIDGE_SCRIPT: &str = include_str!("bridge.js");

struct Companion {
    ble: WatchBle,
    desktop: DesktopIntegration,
    notes: NoteFileWatches,
}

type Answer<T> = Result<T, String>;

fn answer<T>(result: Result<T, BleError>) -> Answer<T> {
    result.map_err(|error| error.0)
}

// ============================================================================
// Bluetooth
// ============================================================================

#[tauri::command]
async fn ble_start_scan(companion: State<'_, Companion>) -> Answer<()> {
    answer(companion.ble.start_scan().await)
}

#[tauri::command]
async fn ble_stop_scan(companion: State<'_, Companion>) -> Answer<()> {
    answer(companion.ble.stop_scan().await)
}

#[tauri::command]
async fn ble_connect(companion: State<'_, Companion>, id: String) -> Answer<()> {
    answer(companion.ble.connect(&id).await)
}

#[tauri::command]
async fn ble_reconnect(companion: State<'_, Companion>, id: String) -> Answer<()> {
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

#[tauri::command]
fn desktop_notification_access(companion: State<'_, Companion>) -> bool {
    companion.desktop.has_notification_access()
}

#[tauri::command]
async fn desktop_do_not_disturb(companion: State<'_, Companion>) -> Answer<bool> {
    Ok(companion.desktop.is_do_not_disturb().await)
}

#[tauri::command]
async fn desktop_media_command(
    companion: State<'_, Companion>,
    player: String,
    command: String,
) -> Answer<()> {
    companion.desktop.media_command(&player, &command).await
}

#[tauri::command]
async fn desktop_locate(companion: State<'_, Companion>) -> Answer<Value> {
    companion.desktop.locate().await
}

// The window asks for the players once it is listening, since it may have missed the first report.
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
    let (chosen_sender, chosen) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Choose a note for the watch")
        .add_filter("Text", &["txt", "md", "text"])
        .add_filter("All files", &["*"])
        .pick_file(move |path| {
            let _ = chosen_sender.send(path);
        });
    let Some(path) = chosen.await.ok().flatten().and_then(|path| path.into_path().ok()) else {
        return Ok(None);
    };
    Ok(Some(NoteFileRef {
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        uri: path.to_string_lossy().into_owned(),
    }))
}

#[tauri::command]
fn notes_read(path: String) -> Answer<Vec<u8>> {
    notes::read_note_file(&path)
}

#[tauri::command]
async fn notes_watch(companion: State<'_, Companion>, path: String) -> Answer<u64> {
    companion.notes.start(&path)
}

#[tauri::command]
fn notes_unwatch(companion: State<'_, Companion>, id: u64) {
    companion.notes.stop(id);
}

// ============================================================================
// Remote buttons
// ============================================================================

#[tauri::command]
async fn remote_run_command(command: String) -> Answer<remote::CommandResult> {
    remote::run_shell_command(&command, remote::COMMAND_TIMEOUT).await
}

// ============================================================================
// Starting up
// ============================================================================

fn companion_for(app: &AppHandle) -> Companion {
    let ble_app = app.clone();
    let desktop_app = app.clone();
    let notes_app = app.clone();
    Companion {
        ble: WatchBle::new(Arc::new(move |name, payload| {
            let _ = ble_app.emit("ble:event", json!({ "name": name, "payload": payload }));
        })),
        desktop: DesktopIntegration::new(Arc::new(move |payload| {
            let _ = desktop_app.emit("desktop:event", payload);
        })),
        notes: NoteFileWatches::new(
            Arc::new(move |id| {
                let _ = notes_app.emit("notes:changed", id);
            }),
            notes::SETTLE,
        ),
    }
}

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let companion = companion_for(app.handle());
            let desktop = companion.desktop.clone();
            app.manage(companion);
            tauri::async_runtime::spawn(async move { desktop.start().await });

            // In development NEOTIME_DEV_URL points at the Expo dev server, so the interface reloads.
            let url = match std::env::var("NEOTIME_DEV_URL") {
                Ok(dev_url) => WebviewUrl::External(dev_url.parse()?),
                Err(_) => WebviewUrl::App("index.html".into()),
            };
            WebviewWindowBuilder::new(app, "main", url)
                .title("NeoTime Companion")
                .inner_size(480.0, 860.0)
                .background_color(tauri::window::Color(0, 0, 0, 255))
                .initialization_script(BRIDGE_SCRIPT)
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
            desktop_notification_access,
            desktop_do_not_disturb,
            desktop_media_command,
            desktop_locate,
            desktop_players,
            notes_pick,
            notes_read,
            notes_watch,
            notes_unwatch,
            remote_run_command,
        ])
        .build(tauri::generate_context!())
        .expect("the desktop app could not start");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            let companion = app.state::<Companion>();
            companion.notes.stop_all();
            tauri::async_runtime::block_on(companion.ble.destroy());
        }
    });
}
