// Plugins in the app: the host's link to the watch and the window, its commands, and the
// plugin:// pages that show each plugin's settings.

use crate::picked_file::{self, FileKind};
use crate::Companion;
use battletime_companion_core::plugins::host::{PluginHost, PluginStatus};
use battletime_companion_core::protocol::app_image::ImageHeader;
use battletime_companion_core::protocol::launcher::{place_new_apps, regrouped_apps};
use battletime_companion_core::protocol::packages::{install_image, PackagesClient};
use serde::Serialize;
use serde_json::Value;
use tauri::http::{Response, StatusCode};
use tauri::{AppHandle, State};

type Answer<T> = Result<T, String>;

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
    companion.daemon.plugins().list()
}

// Asks for a .ntplugin file and installs it; none when the dialog is cancelled.
#[tauri::command]
pub async fn plugins_install_pick(
    app: AppHandle,
    companion: State<'_, Companion>,
) -> Answer<Option<PluginStatus>> {
    let plugin_files = FileKind {
        name: "BattleTime plugin",
        extensions: &["ntplugin"],
    };
    let Some(file) = picked_file::pick_file(&app, "Choose a plugin to install", &[plugin_files]).await else {
        return Ok(None);
    };
    let bytes = picked_file::read_file(&app, &file.to_string())?;
    companion.daemon.plugins().install(&bytes).map(Some)
}

#[tauri::command]
pub fn plugins_install(companion: State<'_, Companion>, data: Vec<u8>) -> Answer<PluginStatus> {
    companion.daemon.plugins().install(&data)
}

#[tauri::command]
pub fn plugins_remove(companion: State<'_, Companion>, id: String) -> Answer<()> {
    companion.daemon.plugins().remove(&id)
}

#[tauri::command]
pub fn plugins_set_enabled(companion: State<'_, Companion>, id: String, enabled: bool) -> Answer<()> {
    companion.daemon.plugins().set_enabled(&id, enabled)
}

// Installs every watch app a plugin carries, over the shared file client, then places them in the launcher.
#[tauri::command]
pub async fn plugins_install_on_watch(
    companion: State<'_, Companion>,
    id: String,
) -> Answer<Vec<ImageHeader>> {
    let images = companion.daemon.plugins().watch_apps(&id)?;
    let link = companion.daemon.shared_link();
    let packages = PackagesClient::open(&link).await?;
    let files = companion.daemon.syncer().files();
    let before = packages.listing().await.map_err(|error| error.to_string())?.apps;
    let mut installed = Vec::new();
    for (_, image) in images {
        installed.push(
            install_image(files, &packages, &image)
                .await
                .map_err(|error| error.to_string())?,
        );
    }
    place_new_apps(files, &packages, &regrouped_apps(&before, &installed))
        .await
        .map_err(|error| error.to_string())?;
    Ok(installed)
}

#[tauri::command]
pub fn plugins_watch_apps(companion: State<'_, Companion>, id: String) -> Answer<Vec<WatchApp>> {
    Ok(companion
        .daemon
        .plugins()
        .watch_apps(&id)?
        .into_iter()
        .map(|(name, data)| WatchApp { name, data })
        .collect())
}

#[tauri::command]
pub fn plugins_ui_message(companion: State<'_, Companion>, id: String, message: Value) -> Answer<()> {
    companion.daemon.plugins().on_ui_message(&id, message)
}

// ============================================================================
// Settings pages
// ============================================================================

// Android serves the plugin:// scheme at this host, with the plugin's id as the first folder.
const ANDROID_PAGE_HOST: &str = "plugin.localhost";

// Splits a page address into the plugin's id and the file asked for.
fn page_file(uri: &tauri::http::Uri) -> (&str, &str) {
    let host = uri.host().unwrap_or_default();
    let path = uri.path().trim_start_matches('/');
    let (id, path) = if host == ANDROID_PAGE_HOST {
        path.split_once('/').unwrap_or((path, ""))
    } else {
        (host, path)
    };
    (id, if path.is_empty() { "index.html" } else { path })
}

// Answers plugin://<id>/<path>, or http://plugin.localhost/<id>/<path> on Android, with that file of the plugin's settings page.
pub fn serve_ui_file(host: &PluginHost, uri: &tauri::http::Uri) -> Response<Vec<u8>> {
    let (id, path) = page_file(uri);
    match host.ui_file(id, path) {
        Ok(contents) => Response::builder()
            .header("Content-Type", content_type(path))
            // The page runs in a sandboxed frame and reaches nothing but its own files.
            .header(
                "Content-Security-Policy",
                "default-src plugin: http://plugin.localhost 'unsafe-inline'; img-src plugin: http://plugin.localhost data:",
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
    use battletime_companion_core::plugins::host::{Fetcher, HostOutput};
    use battletime_companion_core::plugins::store::PluginStore;
    use std::io::Write;
    use std::sync::Arc;

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
    fn android_page_addresses_carry_the_id_in_the_path() {
        let (host, _folder) = host_with_page();
        let page = serve_ui_file(&host, &"http://plugin.localhost/demo/".parse().unwrap());
        assert_eq!(page.status(), StatusCode::OK);
        let script = serve_ui_file(&host, &"http://plugin.localhost/demo/app.js".parse().unwrap());
        assert_eq!(script.status(), StatusCode::OK);
        assert_eq!(
            serve_ui_file(&host, &"http://plugin.localhost/".parse().unwrap()).status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn content_types_follow_the_extension() {
        assert_eq!(content_type("a.css"), "text/css; charset=utf-8");
        assert_eq!(content_type("a.svg"), "image/svg+xml");
        assert_eq!(content_type("noextension"), "application/octet-stream");
    }
}
