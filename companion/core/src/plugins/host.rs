// Every installed plugin, kept running and joined to the watch.
//
// Text from the watch's UART comes in as it arrives; an app message on a plugin's channel goes to that
// plugin, and the plugins hear when the watch connects and disconnects. What they send goes out as
// GB lines.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{Map, Value};

use super::package::Package;
use super::runtime::{DataWrite, Event, FetchRequest, FetchResponse, PluginServices, RunningPlugin};
use super::store::{InstalledPlugin, PluginStore};
use crate::gadgetbridge::{decode_watch_line, encode_for_watch, LineBuffer};

// ============================================================================
// What the shell provides
// ============================================================================

// Where the host's output goes: the watch link, the window showing a plugin's settings page, the log.
pub trait HostOutput: Send + Sync + 'static {
    fn write_to_watch(&self, line: String);
    fn send_to_ui(&self, plugin: &str, message: Value);
    fn log(&self, plugin: &str, line: &str);
}

// Fetching is a seam of its own so tests need no network.
pub type Fetcher = Arc<dyn Fn(&FetchRequest) -> Result<FetchResponse, String> + Send + Sync>;

// The synced watch data, which the shell keeps (see `crate::sync`).
pub trait WatchData: Send + Sync + 'static {
    fn read(&self, source: &str) -> Option<Value>;
    fn list(&self, prefix: &str) -> Vec<String>;
    // May block until the watch has answered.
    fn write(&self, plugin: &str, write: &DataWrite) -> Result<Value, String>;
    fn sync_now(&self);
}

struct Services {
    store: Arc<PluginStore>,
    output: Arc<dyn HostOutput>,
    fetcher: Fetcher,
    data: OnceLock<Arc<dyn WatchData>>,
}

impl PluginServices for Services {
    fn send_to_watch(&self, _plugin: &str, message: Value) {
        self.output.write_to_watch(encode_for_watch(&message));
    }
    fn send_to_ui(&self, plugin: &str, message: Value) {
        self.output.send_to_ui(plugin, message);
    }
    fn log(&self, plugin: &str, line: &str) {
        self.output.log(plugin, line);
    }
    fn save_settings(&self, plugin: &str, settings: &Map<String, Value>) {
        if let Err(error) = self.store.save_settings(plugin, settings) {
            self.output
                .log(plugin, &format!("error: settings were not saved: {error}"));
        }
    }
    fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, String> {
        (self.fetcher)(request)
    }
    fn read_data(&self, source: &str) -> Option<Value> {
        self.data.get()?.read(source)
    }
    fn list_data(&self, prefix: &str) -> Vec<String> {
        self.data.get().map_or_else(Vec::new, |data| data.list(prefix))
    }
    fn write_data(&self, plugin: &str, write: &DataWrite) -> Result<Value, String> {
        match self.data.get() {
            Some(data) => data.write(plugin, write),
            None => Err("This companion keeps no watch data.".into()),
        }
    }
    fn sync_now(&self) {
        if let Some(data) = self.data.get() {
            data.sync_now();
        }
    }
}

// ============================================================================
// The host
// ============================================================================

pub struct PluginHost {
    store: Arc<PluginStore>,
    services: Arc<Services>,
    running: Mutex<BTreeMap<String, RunningPlugin>>,
    lines: Mutex<LineBuffer>,
    connected: AtomicBool,
}

// An installed plugin as the window lists it, with whether its logic is running.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginStatus {
    #[serde(flatten)]
    pub installed: InstalledPlugin,
    pub running: bool,
}

impl PluginHost {
    pub fn new(store: PluginStore, output: Arc<dyn HostOutput>, fetcher: Fetcher) -> PluginHost {
        let store = Arc::new(store);
        PluginHost {
            services: Arc::new(Services {
                store: store.clone(),
                output,
                fetcher,
                data: OnceLock::new(),
            }),
            store,
            running: Mutex::new(BTreeMap::new()),
            lines: Mutex::new(LineBuffer::default()),
            connected: AtomicBool::new(false),
        }
    }

    // Joins the plugins to the synced watch data; until then they read nothing.
    pub fn set_watch_data(&self, data: Arc<dyn WatchData>) {
        let _ = self.services.data.set(data);
    }

    // A sync changed these sources; each plugin hears about those it may read.
    pub fn on_synced(&self, changed: &[String]) {
        for (id, plugin) in self.running.lock().unwrap().iter() {
            let Ok(installed) = self.store.installed(id) else {
                continue;
            };
            let readable: Vec<String> = changed
                .iter()
                .filter(|source| installed.manifest.may_read(source))
                .cloned()
                .collect();
            if !readable.is_empty() {
                plugin.send(Event::Synced(readable));
            }
        }
    }

    // Starts every plugin that is turned on; one that fails to start is logged and left stopped.
    pub fn start_all(&self) {
        for plugin in self.store.list() {
            if plugin.enabled {
                if let Err(error) = self.start(&plugin.manifest.id) {
                    self.services
                        .output
                        .log(&plugin.manifest.id, &format!("error: {error}"));
                }
            }
        }
    }

    pub fn stop_all(&self) {
        let stopping = std::mem::take(&mut *self.running.lock().unwrap());
        for (_, plugin) in stopping {
            plugin.stop();
        }
    }

    fn start(&self, id: &str) -> Result<(), String> {
        self.stop(id);
        let package = self.store.package(id)?;
        let Some(source) = package.main_source() else {
            return Ok(());
        };
        let settings = self.store.settings(id)?;
        let plugin = RunningPlugin::start(package.manifest, source, settings, self.services.clone())?;
        if self.connected.load(Ordering::SeqCst) {
            plugin.send(Event::Connected);
        }
        self.running.lock().unwrap().insert(id.to_string(), plugin);
        Ok(())
    }

    fn stop(&self, id: &str) {
        let stopping = self.running.lock().unwrap().remove(id);
        if let Some(plugin) = stopping {
            plugin.stop();
        }
    }

    // ========================================================================
    // Managing plugins
    // ========================================================================

    pub fn list(&self) -> Vec<PluginStatus> {
        let running = self.running.lock().unwrap();
        self.store
            .list()
            .into_iter()
            .map(|installed| PluginStatus {
                running: running.contains_key(&installed.manifest.id),
                installed,
            })
            .collect()
    }

    // Installs or updates a plugin from its .ntplugin bytes and starts it; the shell then installs its watch apps.
    pub fn install(&self, bytes: &[u8]) -> Result<PluginStatus, String> {
        let package = Package::read(bytes)?;
        let id = package.manifest.id.clone();
        self.stop(&id);
        let installed = self.store.install(&package)?;
        let started = self.start(&id);
        if let Err(error) = &started {
            self.services.output.log(&id, &format!("error: {error}"));
        }
        Ok(PluginStatus {
            installed,
            running: self.running.lock().unwrap().contains_key(&id),
        })
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        self.stop(id);
        self.store.remove(id)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        self.store.set_enabled(id, enabled)?;
        if enabled {
            self.start(id)
        } else {
            self.stop(id);
            Ok(())
        }
    }

    pub fn watch_apps(&self, id: &str) -> Result<Vec<(String, Vec<u8>)>, String> {
        Ok(self.store.package(id)?.watch_apps())
    }

    pub fn ui_file(&self, id: &str, path: &str) -> Result<Vec<u8>, String> {
        self.store.ui_file(id, path)
    }

    // ========================================================================
    // Traffic
    // ========================================================================

    // Text from the watch's UART, in whatever pieces it arrives.
    pub fn on_watch_text(&self, chunk: &str) {
        let lines = self.lines.lock().unwrap().push(chunk);
        for line in lines {
            let Some(message) = decode_watch_line(&line) else {
                continue;
            };
            if message["t"] != "app" {
                continue;
            }
            let Some(channel) = message["ch"].as_str() else {
                continue;
            };
            if let Some(plugin) = self.running.lock().unwrap().get(channel) {
                plugin.send(Event::Message(
                    message.get("d").cloned().unwrap_or(Value::Object(Map::new())),
                ));
            }
        }
    }

    pub fn on_connection(&self, connected: bool) {
        if self.connected.swap(connected, Ordering::SeqCst) == connected {
            return;
        }
        if !connected {
            self.lines.lock().unwrap().reset();
        }
        let event = if connected {
            Event::Connected
        } else {
            Event::Disconnected
        };
        for plugin in self.running.lock().unwrap().values() {
            plugin.send(event.clone());
        }
    }

    // A message from a plugin's settings page.
    pub fn on_ui_message(&self, id: &str, message: Value) -> Result<(), String> {
        match self.running.lock().unwrap().get(id) {
            Some(plugin) => {
                plugin.send(Event::Ui(message));
                Ok(())
            }
            None => Err(format!("The plugin \"{id}\" is not running.")),
        }
    }
}

impl Drop for PluginHost {
    fn drop(&mut self) {
        self.stop_all();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::package::tests::zipped;
    use crate::plugins::runtime::tests::wait_for;
    use serde_json::json;

    #[derive(Default)]
    struct Shell {
        watch: Mutex<Vec<String>>,
        ui: Mutex<Vec<(String, Value)>>,
        log: Mutex<Vec<String>>,
    }

    impl HostOutput for Shell {
        fn write_to_watch(&self, line: String) {
            self.watch.lock().unwrap().push(line);
        }
        fn send_to_ui(&self, plugin: &str, message: Value) {
            self.ui.lock().unwrap().push((plugin.to_string(), message));
        }
        fn log(&self, plugin: &str, line: &str) {
            self.log.lock().unwrap().push(format!("{plugin}: {line}"));
        }
    }

    const ECHO: &str = r#"
        battletime.on('message', (d) => battletime.send({ t: 'vibrate', n: d.button + 1 }));
        battletime.on('connect', () => console.log('connected'));
        battletime.on('disconnect', () => console.log('disconnected'));
        battletime.on('ui', (m) => battletime.ui.send({ echo: m }));
    "#;

    fn package(id: &str, main: &str) -> Vec<u8> {
        let manifest = json!({
            "id": id, "name": "Echo", "version": "1", "main": "main.js", "watch": ["echo.ntapp"],
            "permissions": { "messages": ["vibrate"] }
        })
        .to_string();
        zipped(&[
            ("manifest.json", manifest.as_bytes()),
            ("main.js", main.as_bytes()),
            ("echo.ntapp", b"NTAP"),
        ])
    }

    fn host() -> (PluginHost, Arc<Shell>, tempfile::TempDir) {
        let folder = tempfile::tempdir().unwrap();
        let shell = Arc::new(Shell::default());
        let fetcher: Fetcher = Arc::new(|_| Err("offline".into()));
        (
            PluginHost::new(PluginStore::new(folder.path()), shell.clone(), fetcher),
            shell,
            folder,
        )
    }

    #[test]
    fn an_app_message_reaches_its_plugin_and_the_answer_goes_out_as_a_gb_line() {
        let (host, shell, _folder) = host();
        assert!(host.install(&package("echo", ECHO)).unwrap().running);
        host.on_watch_text("{\"t\":\"app\",\"ch\":\"echo\",");
        host.on_watch_text("\"d\":{\"button\":2}}\n{\"t\":\"app\",\"ch\":\"other\",\"d\":{}}\nconsole\n");
        wait_for(|| shell.watch.lock().unwrap().len() == 1);
        assert_eq!(
            shell.watch.lock().unwrap()[0],
            "GB({\"n\":3,\"t\":\"vibrate\"})\n"
        );
    }

    #[test]
    fn plugins_hear_the_connection_once_per_change_even_when_started_later() {
        let (host, shell, _folder) = host();
        host.on_connection(true);
        host.install(&package("echo", ECHO)).unwrap();
        host.on_connection(true);
        host.on_connection(false);
        wait_for(|| shell.log.lock().unwrap().len() == 2);
        assert_eq!(
            shell.log.lock().unwrap().as_slice(),
            ["echo: connected", "echo: disconnected"]
        );
    }

    #[test]
    fn the_settings_page_talks_to_a_running_plugin_only() {
        let (host, shell, _folder) = host();
        host.install(&package("echo", ECHO)).unwrap();
        host.on_ui_message("echo", json!({"a": 1})).unwrap();
        wait_for(|| shell.ui.lock().unwrap().len() == 1);
        assert_eq!(
            shell.ui.lock().unwrap()[0],
            ("echo".to_string(), json!({"echo": {"a": 1}}))
        );
        host.set_enabled("echo", false).unwrap();
        assert!(host
            .on_ui_message("echo", json!({}))
            .unwrap_err()
            .contains("not running"));
        assert!(!host.list()[0].running);
        host.set_enabled("echo", true).unwrap();
        assert!(host.list()[0].running);
    }

    struct Kept;

    impl WatchData for Kept {
        fn read(&self, source: &str) -> Option<Value> {
            Some(json!(source))
        }
        fn list(&self, _prefix: &str) -> Vec<String> {
            vec!["apps/echo".into()]
        }
        fn write(&self, plugin: &str, _write: &DataWrite) -> Result<Value, String> {
            Ok(json!(plugin))
        }
        fn sync_now(&self) {}
    }

    #[test]
    fn plugins_hear_only_the_synced_sources_they_may_read() {
        let (host, shell, _folder) = host();
        let main = "battletime.on('sync', (changed) => battletime.ui.send({ changed, own: battletime.data.get(changed[0]) }));";
        host.install(&package("echo", main)).unwrap();
        host.on_synced(&["settings".into(), "apps/echo".into()]);
        wait_for(|| shell.ui.lock().unwrap().len() == 1);
        // No data is joined yet, so the plugin reads nothing.
        assert_eq!(shell.ui.lock().unwrap()[0].1, json!({ "changed": ["apps/echo"] }));
        host.set_watch_data(Arc::new(Kept));
        host.on_synced(&["apps/other".into()]);
        host.on_synced(&["apps/echo".into()]);
        wait_for(|| shell.ui.lock().unwrap().len() == 2);
        assert_eq!(
            shell.ui.lock().unwrap()[1].1,
            json!({ "changed": ["apps/echo"], "own": "apps/echo" })
        );
    }

    #[test]
    fn a_plugin_whose_script_breaks_installs_but_does_not_run() {
        let (host, shell, _folder) = host();
        let status = host.install(&package("broken", "syntax error here")).unwrap();
        assert!(!status.running && status.installed.enabled);
        assert!(
            shell.log.lock().unwrap()[0].starts_with("broken: error: The plugin's script failed to start")
        );
    }

    #[test]
    fn start_all_runs_what_is_turned_on_and_removal_stops_it() {
        let folder = tempfile::tempdir().unwrap();
        let shell = Arc::new(Shell::default());
        let fetcher: Fetcher = Arc::new(|_| Err("offline".into()));
        {
            let first = PluginHost::new(PluginStore::new(folder.path()), shell.clone(), fetcher.clone());
            first.install(&package("echo", ECHO)).unwrap();
            first.install(&package("quiet", ECHO)).unwrap();
            first.set_enabled("quiet", false).unwrap();
        }
        let host = PluginHost::new(PluginStore::new(folder.path()), shell, fetcher);
        host.start_all();
        let running: Vec<(String, bool)> = host
            .list()
            .into_iter()
            .map(|status| (status.installed.manifest.id, status.running))
            .collect();
        assert_eq!(
            running,
            [("echo".to_string(), true), ("quiet".to_string(), false)]
        );
        assert_eq!(
            host.watch_apps("echo").unwrap(),
            vec![("echo.ntapp".to_string(), b"NTAP".to_vec())]
        );
        host.remove("echo").unwrap();
        assert_eq!(host.list().len(), 1);
        assert!(host
            .ui_file("quiet", "index.html")
            .unwrap_err()
            .contains("no settings page"));
    }
}
