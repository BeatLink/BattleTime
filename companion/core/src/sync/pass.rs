// One sync: every file the watch keeps that the companion mirrors, read into the store.
//
// - `/apps/<id>/state`, decoded, with whether the app is hidden, as `apps/<id>`;
// - every file under `/settings` as `files/settings/<name>`;
// - every file under the logged folders (`/steps`, `/logs`), steps, heart rate and battery days decoded;
//   a past day is read again only when its size changed;
// - each day of the blocks newer firmware keeps those logs in (`/day_blocks`), under the same name as the
//   day's own file, merged with it when older firmware left one.

use std::collections::HashSet;

use chrono::Datelike;
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
    let blocks = list_blocks(files, &mut report).await;
    let mut merged_day_files = HashSet::new();
    for folder in LOGGED_FOLDERS {
        sync_logged(files, store, folder, &blocks, &mut merged_day_files, &mut report).await;
    }
    for (log, numbers) in BLOCK_LOGS.iter().zip(&blocks) {
        sync_blocks(files, store, log, numbers, &merged_day_files, &mut report).await;
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

// Mirrors a logged folder, leaving a day whose block exists to `sync_blocks`, noted in `merged_day_files`.
async fn sync_logged(
    files: &FileClient,
    store: &SyncStore,
    folder: &str,
    blocks: &[Vec<i64>],
    merged_day_files: &mut HashSet<String>,
    report: &mut PassReport,
) {
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
            if has_block(&path, blocks) {
                merged_day_files.insert(path);
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
// Blocks of days
// ============================================================================

// A log newer firmware keeps in blocks, one sector each of whole days, the block numbered by its first
// day from 1970-01-01 over the days it holds; each byte is stored flipped so an unwritten slot reads as
// the log's empty value (neotime/services/slot_log/src/lib.rs).
pub struct BlockLog {
    pub folder: &'static str,
    // The folder of the day files it stands for, whose names its days take.
    pub days_folder: &'static str,
    pub bytes_per_slot: usize,
    pub flip: u8,
}

pub const BLOCK_LOGS: [BlockLog; 3] = [
    BlockLog {
        folder: "/day_blocks/steps",
        days_folder: "/steps",
        bytes_per_slot: 2,
        flip: 0xff,
    },
    BlockLog {
        folder: "/day_blocks/heart",
        days_folder: "/logs/heart",
        bytes_per_slot: 1,
        flip: 0xff,
    },
    BlockLog {
        folder: "/day_blocks/battery",
        days_folder: "/logs/battery",
        bytes_per_slot: 1,
        flip: 0,
    },
];

const SLOTS_PER_DAY: usize = 240;
// The bytes of a block: a 4 KB sector less the file's header.
const BLOCK_BYTES: usize = 3872;
// Days from 0001-01-01 to 1970-01-01.
const DAYS_BEFORE_1970: i64 = 719_163;

impl BlockLog {
    fn day_bytes(&self) -> usize {
        SLOTS_PER_DAY * self.bytes_per_slot
    }

    fn days_per_block(&self) -> i64 {
        (BLOCK_BYTES / self.day_bytes()) as i64
    }

    // A day's bytes as its own file would hold them, ending after its last written slot; empty when none is.
    pub fn day_from_block(&self, block: &[u8], day_in_block: usize) -> Vec<u8> {
        let start = (day_in_block * self.day_bytes()).min(block.len());
        let stored = &block[start..(start + self.day_bytes()).min(block.len())];
        let written = stored
            .iter()
            .rposition(|&byte| byte != 0xff)
            .map_or(0, |last| last + 1);
        let length = written.next_multiple_of(self.bytes_per_slot);
        stored[..length.min(stored.len())]
            .iter()
            .map(|&byte| byte ^ self.flip)
            .collect()
    }
}

// The path of a day's own file, such as `/logs/heart/2026/01-05`, for a day counted from 1970-01-01.
pub fn day_file_path(days_folder: &str, day: i64) -> Option<String> {
    let date = chrono::NaiveDate::from_num_days_from_ce_opt(i32::try_from(DAYS_BEFORE_1970 + day).ok()?)?;
    Some(format!("{days_folder}/{}", date.format("%Y/%m-%d")))
}

// A day's own file with its block's slots laid over it; each slot was written into one of them.
fn merged(own_file: &[u8], from_block: &[u8], log: &BlockLog) -> Vec<u8> {
    let mut bytes = own_file.to_vec();
    bytes.resize(own_file.len().max(from_block.len()), 0);
    let empty = log.flip ^ 0xff;
    for (slot, piece) in from_block.chunks(log.bytes_per_slot).enumerate() {
        if piece.iter().any(|&byte| byte != empty) {
            let at = slot * log.bytes_per_slot;
            bytes[at..at + piece.len()].copy_from_slice(piece);
        }
    }
    bytes
}

// The block numbers of each log in `BLOCK_LOGS`, none for a log the watch keeps no blocks of.
async fn list_blocks(files: &FileClient, report: &mut PassReport) -> Vec<Vec<i64>> {
    let mut blocks = Vec::new();
    for log in &BLOCK_LOGS {
        let numbers = match files.list(log.folder).await {
            Ok(entries) => entries
                .iter()
                .filter(|entry| !entry.folder)
                .filter_map(|entry| entry.name.parse().ok())
                .collect(),
            Err(FileError::Missing(_)) => Vec::new(),
            Err(error) => {
                report.failed(error);
                Vec::new()
            }
        };
        blocks.push(numbers);
    }
    blocks
}

// Whether a path is a day's own file whose day also has a block, given each log's block numbers.
fn has_block(path: &str, blocks: &[Vec<i64>]) -> bool {
    BLOCK_LOGS.iter().zip(blocks).any(|(log, numbers)| {
        let Some(name) = path
            .strip_prefix(log.days_folder)
            .and_then(|rest| rest.strip_prefix('/'))
        else {
            return false;
        };
        let Ok(date) = chrono::NaiveDate::parse_from_str(name, "%Y/%m-%d") else {
            return false;
        };
        let day = date.num_days_from_ce() as i64 - DAYS_BEFORE_1970;
        numbers.contains(&day.div_euclid(log.days_per_block()))
    })
}

// Reads each block of a log and puts each day it holds under the name of the day's own file, merged with that
// file when older firmware left one.
//
// A block older than the two newest holds only past days, so once read it is not read again, unless it holds a
// day to merge, which `sync_logged` left for it.
async fn sync_blocks(
    files: &FileClient,
    store: &SyncStore,
    log: &BlockLog,
    blocks: &[i64],
    merged_day_files: &HashSet<String>,
    report: &mut PassReport,
) {
    let newest = blocks.iter().copied().max().unwrap_or(0);
    for &block in blocks {
        let path = format!("{}/{block}", log.folder);
        let source = path.trim_start_matches('/').to_string();
        let days = block * log.days_per_block()..(block + 1) * log.days_per_block();
        let day_paths: Vec<Option<String>> = days.map(|day| day_file_path(log.days_folder, day)).collect();
        let merges = day_paths
            .iter()
            .flatten()
            .any(|day_path| merged_day_files.contains(day_path));
        if block + 1 < newest && !merges && store.get(&source).is_some() {
            continue;
        }
        let bytes = match files.read(&path).await {
            Ok(bytes) => bytes,
            Err(error) => {
                report.failed(error);
                continue;
            }
        };
        for (day_in_block, day_path) in day_paths.iter().enumerate() {
            let Some(day_path) = day_path else {
                continue;
            };
            let from_block = log.day_from_block(&bytes, day_in_block);
            let day_bytes = if merged_day_files.contains(day_path) {
                match files.read(day_path).await {
                    Ok(own_file) => merged(&own_file, &from_block, log),
                    Err(error) => {
                        report.failed(error);
                        continue;
                    }
                }
            } else {
                from_block
            };
            if day_bytes.is_empty() {
                continue;
            }
            let folder = day_path.rsplit_once('/').map_or("", |(folder, _)| folder);
            report.put(
                store,
                day_path.trim_start_matches('/'),
                logged_json(folder, &day_bytes),
            );
        }
        // Remembers the block was read, so a past one is not read again.
        let _ = store.put(&source, &json!({ "size": bytes.len() }));
    }
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

    // The day number of a date, counted from 1970-01-01.
    fn day_of(date: &str) -> i64 {
        let date = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap();
        date.num_days_from_ce() as i64 - DAYS_BEFORE_1970
    }

    // A block of a log with values written into some slots of some of its days, stored flipped.
    fn block(log: &BlockLog, slots: &[(usize, usize, u16)]) -> Vec<u8> {
        let mut bytes = vec![0xff; BLOCK_BYTES];
        for &(day_in_block, slot, value) in slots {
            let at = day_in_block * log.day_bytes() + slot * log.bytes_per_slot;
            for (index, byte) in value.to_le_bytes()[..log.bytes_per_slot].iter().enumerate() {
                bytes[at + index] = byte ^ log.flip;
            }
        }
        bytes
    }

    #[test]
    fn a_day_from_a_block_reads_as_its_own_file_would() {
        let [steps, heart, battery] = &BLOCK_LOGS;
        assert_eq!((steps.days_per_block(), heart.days_per_block()), (8, 16));
        let steps_block = block(steps, &[(1, 0, 300), (1, 2, 5), (7, 239, 1)]);
        assert_eq!(steps.day_from_block(&steps_block, 1), [44, 1, 0, 0, 5, 0]);
        assert!(steps.day_from_block(&steps_block, 0).is_empty());
        assert_eq!(steps.day_from_block(&steps_block, 7).len(), 480);
        let battery_block = block(battery, &[(15, 1, 0x80 | 40)]);
        assert_eq!(battery.day_from_block(&battery_block, 15), [0xff, 0x80 | 40]);
        assert_eq!(
            heart.day_from_block(&block(heart, &[(0, 3, 255)]), 0),
            [0, 0, 0, 255]
        );
        assert_eq!(
            day_file_path("/logs/heart", day_of("2026-01-05")).unwrap(),
            "/logs/heart/2026/01-05"
        );
        assert_eq!(day_file_path("/steps", 0).unwrap(), "/steps/1970/01-01");
    }

    #[tokio::test]
    async fn days_in_blocks_sync_under_their_own_names_merged_with_an_older_days_file() {
        let root = tempfile::tempdir().unwrap();
        let store = SyncStore::new(root.path());
        let [steps, heart, _] = &BLOCK_LOGS;
        let today = day_of("2026-10-14");
        let (steps_block, today_in_steps) = (today.div_euclid(8), today.rem_euclid(8) as usize);
        let heart_block = today.div_euclid(16);
        let old_heart_block = heart_block - 3;
        let own_file: Vec<u8> = [11u16, 0, 0].iter().flat_map(|n| n.to_le_bytes()).collect();
        let watch = FakeWatch::with(&[
            ("/steps/2026/10-14", &own_file),
            ("/steps/trip", &7u32.to_le_bytes()),
            (
                &format!("/day_blocks/steps/{steps_block}"),
                &block(steps, &[(today_in_steps, 3, 40)]),
            ),
            (
                &format!("/day_blocks/heart/{heart_block}"),
                &block(heart, &[(today.rem_euclid(16) as usize, 1, 70)]),
            ),
            (
                &format!("/day_blocks/heart/{old_heart_block}"),
                &block(heart, &[(0, 0, 60)]),
            ),
        ]);
        let client = watch.client();

        let report = sync_files(&client, &store).await;

        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(
            store.get("steps/2026/10-14").unwrap()["slots"],
            json!([11, 0, 0, 40])
        );
        assert_eq!(
            store.get("logs/heart/2026/10-14").unwrap()["slots"],
            json!([0, 70])
        );
        let old_day = day_file_path("logs/heart", old_heart_block * 16).unwrap();
        assert_eq!(store.get(&old_day).unwrap()["slots"], json!([60]));
        assert_eq!(store.get("steps/trip").unwrap()["count"], 7);

        watch.commands.lock().unwrap().clear();
        let report = sync_files(&client, &store).await;
        assert!(report.changed.is_empty(), "{:?}", report.changed);
        let reads = watch
            .commands
            .lock()
            .unwrap()
            .iter()
            .filter(|&&c| c == 0x10)
            .count();
        // The trip, the newest heart block, and the steps block with its day's own file to merge; the past block is not.
        assert_eq!(reads, 4);
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
