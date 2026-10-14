// One sync: every file the watch keeps that the companion mirrors, read into the store.
//
// - `/apps/<id>/state`, decoded, with whether the app is hidden, as `apps/<id>`;
// - every file under `/settings` as `files/settings/<name>`;
// - every file under the logged folders (`/steps`, `/logs`), steps, heart rate and battery days decoded;
//   a past day is read again only when its size changed.

use serde_json::{json, Value};

use super::files::{FileClient, FileError};
use super::state_file;
use super::store::SyncStore;

// Folders kept one file a day: only the newest day and loose counters change in place.
pub const LOGGED_FOLDERS: [&str; 2] = ["/steps", "/logs"];

// What changed in one pass, and what could not be read.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PassReport {
    pub changed: Vec<String>,
    pub errors: Vec<String>,
}

impl PassReport {
    fn put(&mut self, store: &SyncStore, source: &str, value: Value) {
        match store.put(source, &value) {
            Ok(true) => self.changed.push(source.to_string()),
            Ok(false) => {}
            Err(error) => self.errors.push(format!("{source}: {error}")),
        }
    }

    fn failed(&mut self, error: FileError) {
        self.errors.push(error.to_string());
    }
}

pub async fn sync_files(files: &FileClient, store: &SyncStore) -> PassReport {
    let mut report = PassReport::default();
    sync_apps(files, store, &mut report).await;
    sync_settings_files(files, store, &mut report).await;
    for folder in LOGGED_FOLDERS {
        sync_logged(files, store, folder, &mut report).await;
    }
    report
}

// ============================================================================
// Apps
// ============================================================================

async fn sync_apps(files: &FileClient, store: &SyncStore, report: &mut PassReport) {
    let apps = match files.list("/apps").await {
        Ok(apps) => apps,
        Err(FileError::Missing(_)) => return,
        Err(error) => return report.failed(error),
    };
    for app in apps.iter().filter(|entry| entry.folder) {
        let folder = format!("/apps/{}", app.name);
        let inside = match files.list(&folder).await {
            Ok(inside) => inside,
            Err(error) => {
                report.failed(error);
                continue;
            }
        };
        let hidden = inside.iter().any(|entry| entry.name == "disabled");
        let (state, kinds) = if inside.iter().any(|entry| entry.name == "state") {
            match files.read(&format!("{folder}/state")).await {
                Ok(bytes) => match state_file::decode(&bytes) {
                    Ok(entries) => (state_file::to_json(&entries), state_file::kinds_json(&entries)),
                    Err(error) => {
                        report.errors.push(format!("{folder}/state: {error}"));
                        continue;
                    }
                },
                Err(error) => {
                    report.failed(error);
                    continue;
                }
            }
        } else {
            (Value::Null, json!({}))
        };
        report.put(
            store,
            &format!("apps/{}", app.name),
            json!({ "hidden": hidden, "state": state, "kinds": kinds }),
        );
    }
}

// ============================================================================
// Settings files
// ============================================================================

async fn sync_settings_files(files: &FileClient, store: &SyncStore, report: &mut PassReport) {
    let names = match files.list("/settings").await {
        Ok(names) => names,
        Err(FileError::Missing(_)) => return,
        Err(error) => return report.failed(error),
    };
    for entry in names.iter().filter(|entry| !entry.folder) {
        match files.read(&format!("/settings/{}", entry.name)).await {
            Ok(bytes) => {
                let value = settings_file_json(&entry.name, &bytes);
                report.put(store, &format!("files/settings/{}", entry.name), value);
            }
            Err(error) => report.failed(error),
        }
    }
}

// The ring of quick apps is a list of ids; every other file keeps its bytes, which the core reads itself.
fn settings_file_json(name: &str, bytes: &[u8]) -> Value {
    match name {
        "quick_apps" => json!({
            "apps": String::from_utf8_lossy(bytes)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
        }),
        _ => json!({ "bytes": bytes }),
    }
}

// ============================================================================
// Logged folders
// ============================================================================

async fn sync_logged(files: &FileClient, store: &SyncStore, folder: &str, report: &mut PassReport) {
    let mut pending = vec![folder.to_string()];
    while let Some(folder) = pending.pop() {
        let entries = match files.list(&folder).await {
            Ok(entries) => entries,
            Err(FileError::Missing(_)) => continue,
            Err(error) => {
                report.failed(error);
                continue;
            }
        };
        let top = LOGGED_FOLDERS.contains(&folder.as_str());
        let newest = entries
            .iter()
            .filter(|entry| !entry.folder)
            .map(|entry| entry.name.clone())
            .max();
        for entry in entries {
            let path = format!("{folder}/{}", entry.name);
            if entry.folder {
                pending.push(path);
                continue;
            }
            let source = path.trim_start_matches('/').to_string();
            let past_day = !top && Some(&entry.name) != newest.as_ref();
            let unchanged = store
                .get(&source)
                .and_then(|kept| kept["size"].as_u64())
                .is_some_and(|size| size == entry.size as u64);
            if past_day && unchanged {
                continue;
            }
            match files.read(&path).await {
                Ok(bytes) => report.put(store, &source, logged_json(folder.as_str(), &bytes)),
                Err(error) => report.failed(error),
            }
        }
    }
}

// A battery slot with this byte had no reading; otherwise the top bit means charging and the rest is the percent.
const NO_BATTERY_READING: u8 = 0xff;
const CHARGING_BIT: u8 = 0x80;

// A day of steps is 240 six-minute counts, and a day of heart rate or battery 240 bytes; anything else keeps its bytes.
fn logged_json(folder: &str, bytes: &[u8]) -> Value {
    if folder.starts_with("/logs/heart/") {
        return json!({ "size": bytes.len(), "slots": bytes });
    }
    if folder.starts_with("/logs/battery/") {
        let slots: Vec<Option<u8>> = bytes
            .iter()
            .map(|&byte| (byte != NO_BATTERY_READING).then_some(byte & !CHARGING_BIT))
            .collect();
        let charging: Vec<bool> = bytes
            .iter()
            .map(|&byte| byte != NO_BATTERY_READING && byte & CHARGING_BIT != 0)
            .collect();
        return json!({ "size": bytes.len(), "slots": slots, "charging": charging });
    }
    if folder.starts_with("/steps/") {
        let slots: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        return json!({ "size": bytes.len(), "slots": slots });
    }
    if folder == "/steps" && bytes.len() == 4 {
        let trip = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        return json!({ "size": 4, "count": trip });
    }
    json!({ "size": bytes.len(), "bytes": bytes })
}

// ============================================================================
// Writing back
// ============================================================================

// Reads the app's state from the watch, applies the changes, and writes it back in the same layout.
pub async fn write_app_state(
    files: &FileClient,
    store: &SyncStore,
    id: &str,
    changes: &Value,
) -> Result<Value, String> {
    if !crate::plugins::manifest::is_plugin_id(id) {
        return Err(format!("\"{id}\" is not an app id."));
    }
    let path = format!("/apps/{id}/state");
    let bytes = files.read(&path).await.map_err(|error| error.to_string())?;
    let entries = state_file::decode(&bytes).map_err(|error| format!("{path}: {error}"))?;
    let changed = state_file::apply_json(&entries, changes)?;
    files
        .write(&path, &state_file::encode(&changed)?)
        .await
        .map_err(|error| error.to_string())?;
    let state = state_file::to_json(&changed);
    let mut kept = store
        .get(&format!("apps/{id}"))
        .unwrap_or(json!({ "hidden": false }));
    kept["state"] = state.clone();
    kept["kinds"] = state_file::kinds_json(&changed);
    store.put(&format!("apps/{id}"), &kept)?;
    Ok(state)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::files::fake::FakeWatch;
    use crate::sync::state_file::tests::alarm_state;

    fn watch() -> std::sync::Arc<FakeWatch> {
        let state = state_file::encode(&alarm_state()).unwrap();
        let day: Vec<u8> = [0u16, 12, 340].iter().flat_map(|n| n.to_le_bytes()).collect();
        FakeWatch::with(&[
            ("/apps/alarm/state", &state),
            ("/apps/alarm/app.ntapp", b"x"),
            ("/apps/hello/app.ntapp", b"x"),
            ("/apps/hello/disabled", b""),
            ("/settings/quick_apps", b"flashlight\ntimer\n"),
            ("/settings/system", b"\x01\x00\x01\x00\x00\x00"),
            ("/steps/2026/10-13", &day),
            ("/steps/2026/10-14", &day),
            ("/steps/trip", &1234u32.to_le_bytes()),
        ])
    }

    #[test]
    fn heart_rate_and_battery_days_read_as_slots() {
        assert_eq!(
            logged_json("/logs/heart/2026", &[0, 64, 71]),
            json!({ "size": 3, "slots": [0, 64, 71] })
        );
        assert_eq!(
            logged_json("/logs/battery/2026", &[0xff, 90, 0x80 | 55]),
            json!({ "size": 3, "slots": [null, 90, 55], "charging": [false, false, true] })
        );
        assert_eq!(
            logged_json("/logs/other/2026", &[1]),
            json!({ "size": 1, "bytes": [1] })
        );
    }

    #[tokio::test]
    async fn past_heart_rate_and_battery_days_are_skipped_until_their_size_changes() {
        let root = tempfile::tempdir().unwrap();
        let store = SyncStore::new(root.path());
        let watch = FakeWatch::with(&[
            ("/logs/heart/2026/10-13", &[0u8, 70][..]),
            ("/logs/heart/2026/10-14", &[65u8][..]),
            ("/logs/battery/2026/10-13", &[80u8, 79][..]),
            ("/logs/battery/2026/10-14", &[0x80u8 | 50][..]),
        ]);
        let client = watch.client();
        let report = sync_files(&client, &store).await;
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(
            store.get("logs/heart/2026/10-13").unwrap()["slots"],
            json!([0, 70])
        );
        assert_eq!(
            store.get("logs/battery/2026/10-14").unwrap()["charging"],
            json!([true])
        );

        watch.commands.lock().unwrap().clear();
        watch
            .files
            .lock()
            .unwrap()
            .insert("/logs/battery/2026/10-13".into(), vec![80, 79, 78]);
        sync_files(&client, &store).await;
        let reads = watch
            .commands
            .lock()
            .unwrap()
            .iter()
            .filter(|&&c| c == 0x10)
            .count();
        // Both newest days, and the past battery day whose size changed.
        assert_eq!(reads, 3);
        assert_eq!(
            store.get("logs/battery/2026/10-13").unwrap()["slots"],
            json!([80, 79, 78])
        );
    }

    #[tokio::test]
    async fn a_pass_mirrors_apps_settings_and_steps() {
        let root = tempfile::tempdir().unwrap();
        let store = SyncStore::new(root.path());
        let report = sync_files(&watch().client(), &store).await;
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(
            store.get("apps/alarm").unwrap()["state"]["alarms"]["hour"],
            json!([7, 9])
        );
        assert_eq!(
            store.get("apps/hello").unwrap(),
            json!({ "hidden": true, "state": null, "kinds": {} })
        );
        assert_eq!(
            store.get("apps/alarm").unwrap()["kinds"],
            json!({ "next_ring_at": "time" })
        );
        assert_eq!(
            store.get("files/settings/quick_apps").unwrap(),
            json!({ "apps": ["flashlight", "timer"] })
        );
        assert_eq!(store.get("files/settings/system").unwrap()["bytes"][0], 1);
        assert_eq!(
            store.get("steps/2026/10-14").unwrap()["slots"],
            json!([0, 12, 340])
        );
        assert_eq!(store.get("steps/trip").unwrap()["count"], 1234);
        assert_eq!(report.changed.len(), 7);
    }

    #[tokio::test]
    async fn a_second_pass_reports_only_changes_and_skips_unchanged_days() {
        let root = tempfile::tempdir().unwrap();
        let store = SyncStore::new(root.path());
        let watch = watch();
        let client = watch.client();
        sync_files(&client, &store).await;
        watch.commands.lock().unwrap().clear();
        let report = sync_files(&client, &store).await;
        assert!(report.changed.is_empty());
        // Everything but the past day is read again, since the rest can change in place.
        let reads = watch
            .commands
            .lock()
            .unwrap()
            .iter()
            .filter(|&&c| c == 0x10)
            .count();
        assert_eq!(reads, 5);
    }

    #[tokio::test]
    async fn a_watch_without_the_folders_syncs_nothing() {
        let root = tempfile::tempdir().unwrap();
        let report = sync_files(&FakeWatch::with(&[]).client(), &SyncStore::new(root.path())).await;
        assert_eq!(report, PassReport::default());
    }

    #[tokio::test]
    async fn unreadable_state_is_reported_and_the_rest_still_syncs() {
        let root = tempfile::tempdir().unwrap();
        let store = SyncStore::new(root.path());
        let watch = FakeWatch::with(&[("/apps/broken/state", b"junk"), ("/settings/quick_apps", b"x")]);
        let report = sync_files(&watch.client(), &store).await;
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("/apps/broken/state"));
        assert!(store.get("files/settings/quick_apps").is_some());
    }

    #[tokio::test]
    async fn written_state_lands_on_the_watch_in_its_own_layout() {
        let root = tempfile::tempdir().unwrap();
        let store = SyncStore::new(root.path());
        let watch = watch();
        let client = watch.client();
        let state = write_app_state(&client, &store, "alarm", &json!({ "alarms": { "hour": [6, 9] } }))
            .await
            .unwrap();
        assert_eq!(state["alarms"]["hour"], json!([6, 9]));
        let written = state_file::decode(&watch.files.lock().unwrap()["/apps/alarm/state"]).unwrap();
        assert_eq!(written[1].values, vec![6, 9]);
        assert_eq!(written[1].width, 1);
        assert_eq!(
            store.get("apps/alarm").unwrap()["state"]["alarms"]["hour"],
            json!([6, 9])
        );
        assert!(write_app_state(&client, &store, "../x", &json!({}))
            .await
            .is_err());
        assert!(write_app_state(&client, &store, "timer", &json!({}))
            .await
            .is_err());
    }
}
