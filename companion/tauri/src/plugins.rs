// Plugins in the desktop app: the host's link to the watch and the window, its commands, and the
// plugin:// pages that show each plugin's settings.

use std::sync::{Arc, OnceLock};

use neotime_companion_core::ble::WatchBle;
use neotime_companion_core::plugins::host::{HostOutput, PluginHost, PluginStatus};
use neotime_companion_core::protocol::app_image::ImageHeader;
use neotime_companion_core::protocol::launcher::place_new_apps;
use neotime_companion_core::protocol::packages::{install_image, PackagesClient};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::http::{Response, StatusCode};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use crate::Companion;

type Answer<T> = Result<T, String>;

// ============================================================================
// The host's output
// ============================================================================

// Sends plugin messages over the watch link and plugin traffic to the window.
pub struct DesktopOutput {
    pub app: AppHandle,
    // Set once the Bluetooth layer exists, which itself hands text to the host.
    pub ble: Arc<OnceLock<WatchBle>>,
}

impl HostOutput for DesktopOutput {
    fn write_to_watch(&self, line: String) {
        let Some(ble) = self.ble.get().cloned() else {
            return;
        };
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = ble.write(&line).await {
                let _ = app.emit(
                    "plugins:log",
                    json!({ "id": "", "line": format!("error: {error}") }),
                );
            }
        });
    }

    fn send_to_ui(&self, plugin: &str, message: Value) {
        let _ = self
            .app
            .emit("plugins:ui", json!({ "id": plugin, "message": message }));
    }

    fn log(&self, plugin: &str, line: &str) {
        eprintln!("plugin {plugin}: {line}");
        let _ = self
            .app
            .emit("plugins:log", json!({ "id": plugin, "line": line }));
    }
}

// What the Bluetooth layer reports, as the plugin host needs it.
pub fn follow_watch(host: &PluginHost, name: &str, payload: &Value) {
    match (name, payload.as_str()) {
        ("line", Some(text)) => host.on_watch_text(text),
        ("state", Some("connected")) => host.on_connection(true),
        ("state", Some("disconnected")) => host.on_connection(false),
        _ => {}
    }
}

// ============================================================================
// Commands
// ============================================================================

#[derive(Serialize)]
pub struct WatchApp {
    name: String,
    data: Vec<u8>,
}

#[tauri::command]
pub fn plugins_list(companion: State<'_, Companion>) -> Vec<PluginStatus> {
    companion.plugins.list()
}

// Asks for a .ntplugin file and installs it; none when the dialog is cancelled.
#[tauri::command]
pub async fn plugins_install_pick(
    app: AppHandle,
    companion: State<'_, Companion>,
) -> Answer<Option<PluginStatus>> {
    let (chosen_sender, chosen) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Choose a plugin to install")
        .add_filter("NeoTime plugin", &["ntplugin"])
        .pick_file(move |path| {
            let _ = chosen_sender.send(path);
        });
    let Some(path) = chosen.await.ok().flatten().and_then(|path| path.into_path().ok()) else {
        return Ok(None);
    };
    let bytes =
        std::fs::read(&path).map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    companion.plugins.install(&bytes).map(Some)
}

#[tauri::command]
pub fn plugins_install(companion: State<'_, Companion>, data: Vec<u8>) -> Answer<PluginStatus> {
    companion.plugins.install(&data)
}

#[tauri::command]
pub fn plugins_remove(companion: State<'_, Companion>, id: String) -> Answer<()> {
    companion.plugins.remove(&id)
}

#[tauri::command]
pub fn plugins_set_enabled(companion: State<'_, Companion>, id: String, enabled: bool) -> Answer<()> {
    companion.plugins.set_enabled(&id, enabled)
}

// Installs every watch app a plugin carries, over the shared file client, then places them in the launcher.
#[tauri::command]
pub async fn plugins_install_on_watch(
    companion: State<'_, Companion>,
    id: String,
) -> Answer<Vec<ImageHeader>> {
    let images = companion.plugins.watch_apps(&id)?;
    let packages = PackagesClient::open(&companion.ble).await?;
    let files = companion.syncer.files();
    let mut installed = Vec::new();
    for (_, image) in images {
        installed.push(
            install_image(files, &packages, &image)
                .await
                .map_err(|error| error.to_string())?,
        );
    }
    place_new_apps(files, &packages)
        .await
        .map_err(|error| error.to_string())?;
    Ok(installed)
}

#[tauri::command]
pub fn plugins_watch_apps(companion: State<'_, Companion>, id: String) -> Answer<Vec<WatchApp>> {
    Ok(companion
        .plugins
        .watch_apps(&id)?
        .into_iter()
        .map(|(name, data)| WatchApp { name, data })
        .collect())
}

#[tauri::command]
pub fn plugins_ui_message(companion: State<'_, Companion>, id: String, message: Value) -> Answer<()> {
    companion.plugins.on_ui_message(&id, message)
}

// ============================================================================
// Settings pages
// ============================================================================

// Answers plugin://<id>/<path> with that file of the plugin's settings page.
pub fn serve_ui_file(host: &PluginHost, uri: &tauri::http::Uri) -> Response<Vec<u8>> {
    let id = uri.host().unwrap_or_default();
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match host.ui_file(id, path) {
        Ok(contents) => Response::builder()
            .header("Content-Type", content_type(path))
            // The page runs in a sandboxed frame and reaches nothing but its own files.
            .header(
                "Content-Security-Policy",
                "default-src plugin: 'unsafe-inline'; img-src plugin: data:",
            )
            .body(contents)
            .unwrap_or_default(),
        Err(error) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(error.into_bytes())
            .unwrap_or_default(),
    }
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use neotime_companion_core::plugins::host::Fetcher;
    use neotime_companion_core::plugins::store::PluginStore;
    use std::io::Write;

    struct Silent;

    impl HostOutput for Silent {
        fn write_to_watch(&self, _line: String) {}
        fn send_to_ui(&self, _plugin: &str, _message: Value) {}
        fn log(&self, _plugin: &str, _line: &str) {}
    }

    fn host_with_page() -> (PluginHost, tempfile::TempDir) {
        let folder = tempfile::tempdir().unwrap();
        let fetcher: Fetcher = Arc::new(|_| Err("offline".into()));
        let host = PluginHost::new(PluginStore::new(folder.path()), Arc::new(Silent), fetcher);
        let mut bytes = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut bytes);
        for (name, contents) in [
            (
                "manifest.json",
                r#"{"id":"demo","name":"Demo","version":"1","ui":"ui/index.html"}"#,
            ),
            ("ui/index.html", "<p>page</p>"),
            ("ui/app.js", "1"),
        ] {
            writer
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(contents.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
        host.install(&bytes.into_inner()).unwrap();
        (host, folder)
    }

    #[test]
    fn a_plugin_page_is_served_with_its_type_and_a_missing_one_is_not_found() {
        let (host, _folder) = host_with_page();
        let page = serve_ui_file(&host, &"plugin://demo/".parse().unwrap());
        assert_eq!(page.status(), StatusCode::OK);
        assert_eq!(page.body(), b"<p>page</p>");
        assert_eq!(page.headers()["Content-Type"], "text/html; charset=utf-8");
        let script = serve_ui_file(&host, &"plugin://demo/app.js".parse().unwrap());
        assert_eq!(script.headers()["Content-Type"], "text/javascript; charset=utf-8");
        let missing = serve_ui_file(&host, &"plugin://demo/../manifest.json".parse().unwrap());
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            serve_ui_file(&host, &"plugin://nobody/".parse().unwrap()).status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn watch_text_and_connection_reach_the_host() {
        let (host, _folder) = host_with_page();
        follow_watch(&host, "state", &json!("connected"));
        follow_watch(
            &host,
            "line",
            &json!("{\"t\":\"app\",\"ch\":\"demo\",\"d\":{}}\n"),
        );
        follow_watch(&host, "mode", &json!("application"));
        follow_watch(&host, "state", &json!("disconnected"));
    }

    #[test]
    fn content_types_follow_the_extension() {
        assert_eq!(content_type("a.css"), "text/css; charset=utf-8");
        assert_eq!(content_type("a.svg"), "image/svg+xml");
        assert_eq!(content_type("noextension"), "application/octet-stream");
    }
}
