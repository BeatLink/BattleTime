// The text file shown on the watch's notes app: read from disk and watched for saves.
//
// Editors often save by writing a new file and renaming it over the old one, which ends a watch on
// the file itself, so the folder is watched and its events are filtered by name.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

// Saves arrive as several events; they are answered once things are quiet for this long.
pub const SETTLE: Duration = Duration::from_millis(300);

pub fn read_note_file(path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|error| format!("Could not read {path}: {error}"))
}

// ============================================================================
// Watching files
// ============================================================================

// Keeps the window's watches by number, so the window can stop one it no longer needs.
pub struct NoteFileWatches {
    on_change: Arc<dyn Fn(u64) + Send + Sync>,
    settle: Duration,
    next_id: Mutex<u64>,
    watchers: Mutex<HashMap<u64, RecommendedWatcher>>,
}

impl NoteFileWatches {
    pub fn new(on_change: Arc<dyn Fn(u64) + Send + Sync>, settle: Duration) -> Self {
        NoteFileWatches {
            on_change,
            settle,
            next_id: Mutex::new(1),
            watchers: Mutex::new(HashMap::new()),
        }
    }

    pub fn start(&self, path: &str) -> Result<u64, String> {
        let path = PathBuf::from(path);
        let folder = path
            .parent()
            .filter(|folder| !folder.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let name = path
            .file_name()
            .ok_or_else(|| format!("{} names no file.", path.display()))?
            .to_owned();

        let id = {
            let mut next_id = self.next_id.lock().unwrap();
            let id = *next_id;
            *next_id += 1;
            id
        };

        // Each event restarts the quiet period; only the last one in a burst reports the change.
        let generation = Arc::new(Mutex::new(0u64));
        let on_change = self.on_change.clone();
        let settle = self.settle;
        let runtime = tokio::runtime::Handle::current();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let Ok(event) = event else { return };
            if !event
                .paths
                .iter()
                .any(|changed| changed.file_name() == Some(name.as_os_str()))
            {
                return;
            }
            let mine = {
                let mut generation = generation.lock().unwrap();
                *generation += 1;
                *generation
            };
            let generation = generation.clone();
            let on_change = on_change.clone();
            runtime.spawn(async move {
                tokio::time::sleep(settle).await;
                if *generation.lock().unwrap() == mine {
                    on_change(id);
                }
            });
        })
        .map_err(|error| format!("Could not watch {}: {error}", folder.display()))?;
        watcher
            .watch(&folder, RecursiveMode::NonRecursive)
            .map_err(|error| format!("Could not watch {}: {error}", folder.display()))?;

        self.watchers.lock().unwrap().insert(id, watcher);
        Ok(id)
    }

    pub fn stop(&self, id: u64) {
        self.watchers.lock().unwrap().remove(&id);
    }

    pub fn stop_all(&self) {
        self.watchers.lock().unwrap().clear();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn counting_watches() -> (NoteFileWatches, Arc<Mutex<Vec<u64>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = calls.clone();
        let watches = NoteFileWatches::new(
            Arc::new(move |id| recorded.lock().unwrap().push(id)),
            Duration::from_millis(100),
        );
        (watches, calls)
    }

    #[test]
    fn reading_a_missing_file_names_it() {
        assert!(read_note_file("/nonexistent/note.txt")
            .unwrap_err()
            .contains("/nonexistent/note.txt"));
    }

    #[tokio::test]
    async fn a_save_by_rename_reports_one_change() {
        let folder = tempfile::tempdir().unwrap();
        let note = folder.path().join("note.txt");
        std::fs::write(&note, "one").unwrap();
        let (watches, calls) = counting_watches();
        let id = watches.start(note.to_str().unwrap()).unwrap();

        let staged = folder.path().join(".note.txt.swp");
        std::fs::write(&staged, "two").unwrap();
        std::fs::rename(&staged, &note).unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;

        assert_eq!(*calls.lock().unwrap(), vec![id]);
        assert_eq!(read_note_file(note.to_str().unwrap()).unwrap(), b"two");
    }

    #[tokio::test]
    async fn other_files_in_the_folder_are_ignored_and_a_stopped_watch_is_quiet() {
        let folder = tempfile::tempdir().unwrap();
        let note = folder.path().join("note.txt");
        std::fs::write(&note, "one").unwrap();
        let (watches, calls) = counting_watches();
        let id = watches.start(note.to_str().unwrap()).unwrap();

        std::fs::write(folder.path().join("other.txt"), "x").unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        watches.stop(id);
        std::fs::write(&note, "two").unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;

        assert!(calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn each_watch_gets_its_own_number() {
        let folder = tempfile::tempdir().unwrap();
        let note = folder.path().join("note.txt");
        std::fs::write(&note, "one").unwrap();
        let (watches, _calls) = counting_watches();
        let first = watches.start(note.to_str().unwrap()).unwrap();
        let second = watches.start(note.to_str().unwrap()).unwrap();
        assert_ne!(first, second);
        watches.stop_all();
    }
}
