// What the companion last read from the watch, kept on disk as one JSON file per source.
//
// A source is named like a path: `settings`, `apps/alarm`, `steps/2026/10-14`, `files/settings/quick_apps`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ============================================================================
// How often to sync
// ============================================================================

pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(5 * 60);
pub const SHORTEST_INTERVAL: Duration = Duration::from_secs(60);
pub const LONGEST_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSettings {
    pub interval_seconds: u64,
}

impl Default for SyncSettings {
    fn default() -> Self {
        SyncSettings {
            interval_seconds: DEFAULT_INTERVAL.as_secs(),
        }
    }
}

impl SyncSettings {
    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_seconds).clamp(SHORTEST_INTERVAL, LONGEST_INTERVAL)
    }
}

// ============================================================================
// The store
// ============================================================================

const SETTINGS_FILE: &str = ".sync-settings.json";

pub struct SyncStore {
    root: PathBuf,
}

impl SyncStore {
    pub fn new(root: impl Into<PathBuf>) -> SyncStore {
        SyncStore { root: root.into() }
    }

    pub fn settings(&self) -> SyncSettings {
        read_json(&self.root.join(SETTINGS_FILE)).unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &SyncSettings) -> Result<(), String> {
        let seconds = settings.interval_seconds;
        if !(SHORTEST_INTERVAL.as_secs()..=LONGEST_INTERVAL.as_secs()).contains(&seconds) {
            return Err(format!(
                "Sync every {} seconds to {} hours.",
                SHORTEST_INTERVAL.as_secs(),
                LONGEST_INTERVAL.as_secs() / 3600
            ));
        }
        write_json(&self.root.join(SETTINGS_FILE), settings)
    }

    pub fn get(&self, source: &str) -> Option<Value> {
        read_json(&self.file(source).ok()?)
    }

    // Returns whether the source changed, so only real changes are announced.
    pub fn put(&self, source: &str, value: &Value) -> Result<bool, String> {
        let file = self.file(source)?;
        if read_json::<Value>(&file).as_ref() == Some(value) {
            return Ok(false);
        }
        write_json(&file, value)?;
        Ok(true)
    }

    // Every source under `prefix`, such as `steps` or `apps`, or every source for "", in name order.
    pub fn sources(&self, prefix: &str) -> Vec<String> {
        let mut found = Vec::new();
        if prefix.is_empty() {
            collect(&self.root, &self.root, &mut found);
        } else if let Ok(folder) = self.file(prefix) {
            collect(&self.root, &folder.with_extension(""), &mut found);
            if folder.is_file() {
                found.push(prefix.to_string());
            }
        }
        found.sort();
        found
    }

    fn file(&self, source: &str) -> Result<PathBuf, String> {
        let fine = !source.is_empty()
            && source.split('/').all(|part| {
                !part.is_empty()
                    && !part.starts_with('.')
                    && part
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
            });
        if !fine {
            return Err(format!("\"{source}\" is not a sync source."));
        }
        Ok(self.root.join(format!("{source}.json")))
    }
}

// ============================================================================
// Files
// ============================================================================

fn collect(root: &Path, folder: &Path, found: &mut Vec<String>) {
    for entry in fs::read_dir(folder).into_iter().flatten().flatten() {
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect(root, &path, found);
        } else if path.extension().is_some_and(|extension| extension == "json") {
            if let Ok(inside) = path.with_extension("").strip_prefix(root) {
                found.push(inside.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

fn read_json<T: for<'a> Deserialize<'a>>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).map_err(|error| error.to_string())?;
    let staged = path.with_extension("json.new");
    fs::write(
        &staged,
        serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::rename(&staged, path).map_err(|error| error.to_string())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_source_reads_back_and_only_a_real_change_counts() {
        let root = tempfile::tempdir().unwrap();
        let store = SyncStore::new(root.path());
        assert_eq!(store.get("apps/alarm"), None);
        assert!(store.put("apps/alarm", &json!({ "a": 1 })).unwrap());
        assert!(!store.put("apps/alarm", &json!({ "a": 1 })).unwrap());
        assert!(store.put("apps/alarm", &json!({ "a": 2 })).unwrap());
        assert_eq!(store.get("apps/alarm"), Some(json!({ "a": 2 })));
    }

    #[test]
    fn sources_are_listed_by_prefix() {
        let root = tempfile::tempdir().unwrap();
        let store = SyncStore::new(root.path());
        for source in ["steps/2026/10-14", "steps/2026/10-13", "apps/timer", "settings"] {
            store.put(source, &json!(1)).unwrap();
        }
        store.save_settings(&SyncSettings::default()).unwrap();
        assert_eq!(
            store.sources("steps"),
            vec!["steps/2026/10-13", "steps/2026/10-14"]
        );
        assert_eq!(store.sources("settings"), vec!["settings"]);
        assert!(store.sources("nothing").is_empty());
        assert_eq!(store.sources("").len(), 4);
    }

    #[test]
    fn names_cannot_leave_the_store() {
        let root = tempfile::tempdir().unwrap();
        let store = SyncStore::new(root.path());
        for bad in [
            "",
            "../x",
            "apps/../x",
            "/abs",
            "apps//x",
            ".sync-settings",
            "a b",
        ] {
            assert!(store.put(bad, &json!(1)).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_interval_starts_at_five_minutes_and_stays_in_range() {
        let root = tempfile::tempdir().unwrap();
        let store = SyncStore::new(root.path());
        assert_eq!(store.settings().interval(), Duration::from_secs(300));
        assert!(store
            .save_settings(&SyncSettings { interval_seconds: 10 })
            .is_err());
        store
            .save_settings(&SyncSettings {
                interval_seconds: 900,
            })
            .unwrap();
        assert_eq!(store.settings().interval_seconds, 900);
        assert_eq!(SyncSettings { interval_seconds: 1 }.interval(), SHORTEST_INTERVAL);
    }
}
