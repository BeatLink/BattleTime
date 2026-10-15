// Sync in the desktop app: the file service over the watch link, the syncer's output to the plugins and
// the window, and the window's commands.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use futures::future::BoxFuture;
use battletime_companion_core::ble::{subscription_key, WatchBle};
use battletime_companion_core::plugins::host::{PluginHost, WatchData};
use battletime_companion_core::plugins::runtime::DataWrite;
use battletime_companion_core::sync::files::{FileTransport, FILE_SERVICE, FILE_TRANSFER};
use battletime_companion_core::sync::store::SyncSettings;
use battletime_companion_core::sync::{SyncOutput, SyncReport, Syncer};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::OwnedMutexGuard;

use crate::Companion;

type Answer<T> = Result<T, String>;

// The longest the window may hold the file service, in case it never lets go.
const LONGEST_LEASE: Duration = Duration::from_secs(120);

// ============================================================================
// The file service over the watch link
// ============================================================================

pub struct BleFiles {
    pub ble: Arc<OnceLock<WatchBle>>,
}

impl FileTransport for BleFiles {
    fn send(&self, command: Vec<u8>) -> BoxFuture<'static, Result<(), String>> {
        let ble = self.ble.get().cloned();
        Box::pin(async move {
            let ble = ble.ok_or("Not connected")?;
            // The window unsubscribes when it closes its own client, so subscribe again each time.
            ble.gatt_subscribe(FILE_SERVICE, FILE_TRANSFER)
                .await
                .map_err(|error| error.0)?;
            ble.gatt_write(FILE_SERVICE, FILE_TRANSFER, &command, "request")
                .await
                .map_err(|error| error.0)
        })
    }
}

// The window's turn with the file service; while it holds one, the file service's answers go to it.
#[derive(Default)]
pub struct WindowLease {
    guard: Mutex<Option<(u64, OwnedMutexGuard<()>)>>,
    held: AtomicBool,
    next: AtomicU64,
}

impl WindowLease {
    pub fn held(&self) -> bool {
        self.held.load(Ordering::SeqCst)
    }

    fn release(&self, number: Option<u64>) {
        let mut guard = self.guard.lock().unwrap();
        if number.is_none() || guard.as_ref().map(|(held, _)| *held) == number {
            *guard = None;
            self.held.store(false, Ordering::SeqCst);
        }
    }
}

// Where a Bluetooth event goes: the file service's answers to whoever holds it, the rest to the syncer.
// Returns whether the window should hear it too.
pub fn follow_watch(syncer: &Syncer, lease: &WindowLease, name: &str, payload: &Value) -> bool {
    match (name, payload.as_str()) {
        ("line", Some(text)) => syncer.on_watch_text(text),
        ("state", Some("connected")) => syncer.on_connection(true),
        ("state", Some("disconnected")) => syncer.on_connection(false),
        ("notify", _) if payload["key"] == subscription_key(FILE_SERVICE, FILE_TRANSFER) => {
            if lease.held() {
                return true;
            }
            let bytes: Vec<u8> = serde_json::from_value(payload["value"].clone()).unwrap_or_default();
            syncer.files().on_answer(bytes);
            return false;
        }
        _ => {}
    }
    true
}

// ============================================================================
// The syncer's output, and the plugins' way in
// ============================================================================

pub struct DesktopSync {
    pub app: AppHandle,
    pub ble: Arc<OnceLock<WatchBle>>,
    pub plugins: Arc<PluginHost>,
}

impl SyncOutput for DesktopSync {
    fn write_to_watch(&self, line: String) {
        if let Some(ble) = self.ble.get().cloned() {
            tauri::async_runtime::spawn(async move {
                let _ = ble.write(&line).await;
            });
        }
    }

    fn synced(&self, report: &SyncReport) {
        self.plugins.on_synced(&report.changed);
        let _ = self.app.emit("sync:report", report);
    }
}

pub struct PluginData {
    pub syncer: Arc<OnceLock<Arc<Syncer>>>,
}

impl WatchData for PluginData {
    fn read(&self, source: &str) -> Option<Value> {
        self.syncer.get()?.store().get(source)
    }

    fn list(&self, prefix: &str) -> Vec<String> {
        self.syncer
            .get()
            .map_or_else(Vec::new, |syncer| syncer.store().sources(prefix))
    }

    // Plugins call this from a thread of their own, so it may wait for the watch.
    fn write(&self, plugin: &str, write: &DataWrite) -> Result<Value, String> {
        let syncer = self
            .syncer
            .get()
            .cloned()
            .ok_or("The companion is still starting.")?;
        match write {
            DataWrite::AppState(changes) => {
                tauri::async_runtime::block_on(syncer.write_app_state(plugin, changes))
            }
            DataWrite::Settings(changes) => {
                tauri::async_runtime::block_on(syncer.write_settings(changes.clone()))
            }
        }
    }

    fn sync_now(&self) {
        if let Some(syncer) = self.syncer.get() {
            syncer.sync_now();
        }
    }
}

// ============================================================================
// Commands
// ============================================================================

#[tauri::command]
pub fn sync_settings(companion: State<'_, Companion>) -> SyncSettings {
    companion.syncer.settings()
}

#[tauri::command]
pub fn sync_set_settings(companion: State<'_, Companion>, settings: SyncSettings) -> Answer<()> {
    companion.syncer.set_settings(&settings)
}

#[tauri::command]
pub fn sync_now(companion: State<'_, Companion>) {
    companion.syncer.sync_now();
}

#[tauri::command]
pub fn sync_read(companion: State<'_, Companion>, source: String) -> Option<Value> {
    companion.syncer.store().get(&source)
}

#[tauri::command]
pub fn sync_list(companion: State<'_, Companion>, prefix: String) -> Vec<String> {
    companion.syncer.store().sources(&prefix)
}

#[tauri::command]
pub async fn sync_write_app_state(
    companion: State<'_, Companion>,
    id: String,
    changes: Value,
) -> Answer<Value> {
    companion.syncer.write_app_state(&id, &changes).await
}

#[tauri::command]
pub async fn sync_write_settings(companion: State<'_, Companion>, changes: Value) -> Answer<Value> {
    companion.syncer.write_settings(changes).await
}

// The window takes the file service for one operation of its own; the answer names the lease to end.
#[tauri::command]
pub async fn files_begin(companion: State<'_, Companion>) -> Answer<u64> {
    let guard = companion.syncer.files().lease().await;
    let lease = companion.lease.clone();
    let number = lease.next.fetch_add(1, Ordering::SeqCst);
    *lease.guard.lock().unwrap() = Some((number, guard));
    lease.held.store(true, Ordering::SeqCst);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(LONGEST_LEASE).await;
        lease.release(Some(number));
    });
    Ok(number)
}

#[tauri::command]
pub fn files_end(companion: State<'_, Companion>, lease: u64) {
    companion.lease.release(Some(lease));
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use battletime_companion_core::sync::files::FileClient;
    use battletime_companion_core::sync::store::SyncStore;
    use serde_json::json;

    struct Nowhere;

    impl FileTransport for Nowhere {
        fn send(&self, _command: Vec<u8>) -> BoxFuture<'static, Result<(), String>> {
            Box::pin(async { Ok(()) })
        }
    }

    impl SyncOutput for Nowhere {
        fn write_to_watch(&self, _line: String) {}
        fn synced(&self, _report: &SyncReport) {}
    }

    #[test]
    fn file_answers_go_to_the_window_only_while_it_holds_the_service() {
        let root = tempfile::tempdir().unwrap();
        let syncer = Syncer::new(
            Arc::new(FileClient::new(Arc::new(Nowhere))),
            SyncStore::new(root.path()),
            Arc::new(Nowhere),
        );
        let lease = WindowLease::default();
        let answer = json!({ "key": subscription_key(FILE_SERVICE, FILE_TRANSFER), "value": [0x31, 1] });
        assert!(!follow_watch(&syncer, &lease, "notify", &answer));
        lease.held.store(true, Ordering::SeqCst);
        assert!(follow_watch(&syncer, &lease, "notify", &answer));
        assert!(follow_watch(
            &syncer,
            &lease,
            "notify",
            &json!({ "key": "other", "value": [] })
        ));
        assert!(follow_watch(&syncer, &lease, "line", &json!("{\"t\":\"x\"}\n")));
        lease.release(Some(7));
        assert!(lease.held());
        lease.release(None);
        assert!(!lease.held());
    }
}
