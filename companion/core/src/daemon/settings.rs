// The companion's own settings, kept by the daemon in one JSON file so they outlast the window.
//
// The window's `src/state/settings.ts` names every key and its default; each part of the daemon reads
// only the keys it needs, through a serde view with the same defaults.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{Map, Value};
use tokio::sync::watch;

// ============================================================================
// The store
// ============================================================================

pub struct SettingsStore {
    path: PathBuf,
    saved: Mutex<Option<Map<String, Value>>>,
    // Counts every change, so a part of the daemon can wait for the next one.
    changes: watch::Sender<u64>,
}

impl SettingsStore {
    pub fn open(path: impl Into<PathBuf>) -> SettingsStore {
        let path = path.into();
        let saved = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .and_then(|value| value.as_object().cloned());
        SettingsStore {
            path,
            saved: Mutex::new(saved),
            changes: watch::channel(0).0,
        }
    }

    // Whether anything has been saved yet, so the window knows to hand over what it kept before the daemon.
    pub fn has_saved(&self) -> bool {
        self.saved.lock().unwrap().is_some()
    }

    // Every saved key, without defaults, which the window fills in itself.
    pub fn all(&self) -> Value {
        Value::Object(self.saved.lock().unwrap().clone().unwrap_or_default())
    }

    // The settings seen through one part's view, with its defaults for anything not saved or not readable.
    pub fn view<View: DeserializeOwned + Default>(&self) -> View {
        let saved = self.saved.lock().unwrap().clone().unwrap_or_default();
        if let Ok(view) = serde_json::from_value(Value::Object(saved.clone())) {
            return view;
        }
        let mut readable = Map::new();
        for (key, value) in saved {
            readable.insert(key.clone(), value);
            if serde_json::from_value::<View>(Value::Object(readable.clone())).is_err() {
                readable.remove(&key);
            }
        }
        serde_json::from_value(Value::Object(readable)).unwrap_or_default()
    }

    // Merges `patch` over what is saved, writes it out and tells whoever waits.
    pub fn update(&self, patch: &Value) -> Result<Value, String> {
        let Some(patch) = patch.as_object() else {
            return Err("Settings must be an object.".into());
        };
        let all = {
            let mut saved = self.saved.lock().unwrap();
            let map = saved.get_or_insert_with(Map::new);
            for (key, value) in patch {
                map.insert(key.clone(), value.clone());
            }
            Value::Object(map.clone())
        };
        if let Some(folder) = self.path.parent() {
            std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        }
        let temporary = self.path.with_extension("json.new");
        std::fs::write(&temporary, serde_json::to_vec_pretty(&all).unwrap_or_default())
            .and_then(|()| std::fs::rename(&temporary, &self.path))
            .map_err(|error| error.to_string())?;
        self.changes.send_modify(|count| *count += 1);
        Ok(all)
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }
}

// ============================================================================
// The daemon's own view
// ============================================================================

// The keys the link keeper, the forwarding and the watch's requests read.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CoreSettings {
    pub last_watch_id: Option<String>,
    pub last_watch_name: Option<String>,
    pub auto_reconnect: bool,
    pub forward_notifications: bool,
    pub forward_music: bool,
    pub forward_calls: bool,
    pub respect_do_not_disturb: bool,
    pub sync_interval_seconds: Option<f64>,
}

impl Default for CoreSettings {
    fn default() -> Self {
        CoreSettings {
            last_watch_id: None,
            last_watch_name: None,
            auto_reconnect: true,
            forward_notifications: true,
            forward_music: true,
            forward_calls: true,
            respect_do_not_disturb: true,
            sync_interval_seconds: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_new_store_has_nothing_saved_and_reads_the_defaults() {
        let folder = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(folder.path().join("settings.json"));

        assert!(!store.has_saved());
        assert_eq!(store.all(), json!({}));
        assert_eq!(store.view::<CoreSettings>(), CoreSettings::default());
    }

    #[test]
    fn an_update_merges_saves_and_counts_as_a_change() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("companion").join("settings.json");
        let store = SettingsStore::open(&path);
        let changes = store.subscribe();

        store
            .update(&json!({ "lastWatchId": "AA", "autoReconnect": false }))
            .unwrap();
        store.update(&json!({ "lastWatchName": "BattleTime" })).unwrap();

        assert_eq!(*changes.borrow(), 2);
        let reopened = SettingsStore::open(&path);
        assert!(reopened.has_saved());
        let view = reopened.view::<CoreSettings>();
        assert_eq!(view.last_watch_id.as_deref(), Some("AA"));
        assert_eq!(view.last_watch_name.as_deref(), Some("BattleTime"));
        assert!(!view.auto_reconnect);
        assert!(view.forward_notifications);
    }

    #[test]
    fn a_patch_that_is_not_an_object_is_refused() {
        let folder = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(folder.path().join("settings.json"));
        assert!(store.update(&json!(3)).is_err());
        assert!(!store.has_saved());
    }

    #[test]
    fn a_wrongly_typed_key_falls_back_to_its_default_alone() {
        let folder = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(folder.path().join("settings.json"));
        store
            .update(&json!({ "autoReconnect": "yes", "forwardMusic": false }))
            .unwrap();
        let view = store.view::<CoreSettings>();
        assert!(view.auto_reconnect);
        assert!(!view.forward_music);
    }
}
