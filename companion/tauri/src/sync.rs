// Sync in the app: the window's commands for the daemon's copy of the watch, and its turns with the file service.

use battletime_companion_core::sync::store::SyncSettings;
use serde_json::Value;
use tauri::State;

use crate::Companion;

type Answer<T> = Result<T, String>;

// ============================================================================
// Commands
// ============================================================================

#[tauri::command]
pub fn sync_settings(companion: State<'_, Companion>) -> SyncSettings {
    companion.daemon.syncer().settings()
}

#[tauri::command]
pub fn sync_set_settings(companion: State<'_, Companion>, settings: SyncSettings) -> Answer<()> {
    companion.daemon.syncer().set_settings(&settings)
}

#[tauri::command]
pub fn sync_now(companion: State<'_, Companion>) {
    companion.daemon.syncer().sync_now();
}

#[tauri::command]
pub fn sync_read(companion: State<'_, Companion>, source: String) -> Option<Value> {
    companion.daemon.syncer().store().get(&source)
}

#[tauri::command]
pub fn sync_list(companion: State<'_, Companion>, prefix: String) -> Vec<String> {
    companion.daemon.syncer().store().sources(&prefix)
}

#[tauri::command]
pub async fn sync_write_app_state(
    companion: State<'_, Companion>,
    id: String,
    changes: Value,
) -> Answer<Value> {
    companion.daemon.syncer().write_app_state(&id, &changes).await
}

#[tauri::command]
pub async fn sync_write_settings(companion: State<'_, Companion>, changes: Value) -> Answer<Value> {
    companion.daemon.syncer().write_settings(changes).await
}

// The window takes the file service for one operation of its own; the answer names the lease to end.
#[tauri::command]
pub async fn files_begin(companion: State<'_, Companion>) -> Answer<u64> {
    Ok(companion.daemon.begin_files_turn().await)
}

#[tauri::command]
pub fn files_end(companion: State<'_, Companion>, lease: u64) {
    companion.daemon.lease().release(Some(lease));
}
