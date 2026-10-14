// Keeps a copy of every setting and record on the watch, read again on one interval for the whole companion.
//
// Each pass asks the watch for its settings over the UART and reads its files with the file service; see
// docs/sync.md. Changes made from the companion go back the same two ways.

pub mod files;
pub mod pass;
pub mod state_file;
pub mod store;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};
use tokio::sync::{watch, Notify};

use crate::gadgetbridge::{decode_watch_line, encode_for_watch, LineBuffer};
use files::FileClient;
use store::{SyncSettings, SyncStore};

// How long the watch may take to answer a `settings` message.
const SETTINGS_TIMEOUT: Duration = Duration::from_secs(5);

// ============================================================================
// What the shell provides
// ============================================================================

pub trait SyncOutput: Send + Sync + 'static {
    // A GB line for the watch's UART.
    fn write_to_watch(&self, line: String);
    // A pass finished; plugins and the window hear what changed.
    fn synced(&self, report: &SyncReport);
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub changed: Vec<String>,
    pub errors: Vec<String>,
    pub finished_at: u64,
}

// ============================================================================
// The syncer
// ============================================================================

pub struct Syncer {
    files: Arc<FileClient>,
    store: SyncStore,
    output: Arc<dyn SyncOutput>,
    lines: Mutex<LineBuffer>,
    settings_reply: watch::Sender<Option<Value>>,
    connected: AtomicBool,
    wake: Notify,
    // Set by connecting and by asking; a wake without it only recounts the interval.
    requested: AtomicBool,
    // One pass or write at a time, so a write never lands between a pass's read and its store.
    turn: tokio::sync::Mutex<()>,
}

impl Syncer {
    pub fn new(files: Arc<FileClient>, store: SyncStore, output: Arc<dyn SyncOutput>) -> Syncer {
        Syncer {
            files,
            store,
            output,
            lines: Mutex::new(LineBuffer::default()),
            settings_reply: watch::channel(None).0,
            connected: AtomicBool::new(false),
            wake: Notify::new(),
            requested: AtomicBool::new(false),
            turn: tokio::sync::Mutex::new(()),
        }
    }

    pub fn store(&self) -> &SyncStore {
        &self.store
    }

    pub fn files(&self) -> &FileClient {
        &self.files
    }

    // ========================================================================
    // Following the watch
    // ========================================================================

    pub fn on_watch_text(&self, chunk: &str) {
        let lines = self.lines.lock().unwrap().push(chunk);
        for message in lines.iter().filter_map(|line| decode_watch_line(line)) {
            if message["t"] == "settings" && message.get("err").is_none() {
                self.settings_reply.send_replace(Some(message));
            }
        }
    }

    // A pass starts as soon as the watch connects.
    pub fn on_connection(&self, connected: bool) {
        if self.connected.swap(connected, Ordering::SeqCst) == connected {
            return;
        }
        if connected {
            self.request_pass();
        } else {
            self.lines.lock().unwrap().reset();
        }
    }

    pub fn sync_now(&self) {
        self.request_pass();
    }

    fn request_pass(&self) {
        self.requested.store(true, Ordering::SeqCst);
        self.wake.notify_one();
    }

    pub fn settings(&self) -> SyncSettings {
        self.store.settings()
    }

    // The new interval counts from now.
    pub fn set_settings(&self, settings: &SyncSettings) -> Result<(), String> {
        self.store.save_settings(settings)?;
        self.wake.notify_one();
        Ok(())
    }

    // ========================================================================
    // Passes
    // ========================================================================

    // Runs for the life of the app: a pass on connecting, on asking, and every interval while connected.
    pub async fn run(self: Arc<Self>) {
        let mut last_pass: Option<tokio::time::Instant> = None;
        loop {
            let interval = self.store.settings().interval();
            if !self.connected.load(Ordering::SeqCst) {
                // Connecting asks for a pass, so there is nothing to count while the watch is away.
                self.wake.notified().await;
                continue;
            }
            let asked = self.requested.swap(false, Ordering::SeqCst);
            if asked || last_pass.is_none_or(|at| at.elapsed() >= interval) {
                let report = self.pass().await;
                self.output.synced(&report);
                last_pass = Some(tokio::time::Instant::now());
            }
            let wait = last_pass.map_or(interval, |at| interval.saturating_sub(at.elapsed()));
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = self.wake.notified() => {}
            }
        }
    }

    pub async fn pass(&self) -> SyncReport {
        let _turn = self.turn.lock().await;
        let mut report = SyncReport::default();
        match self.ask_settings(json!({})).await {
            Ok(settings) => match self.store.put("settings", &settings) {
                Ok(true) => report.changed.push("settings".into()),
                Ok(false) => {}
                Err(error) => report.errors.push(format!("settings: {error}")),
            },
            Err(error) => report.errors.push(error),
        }
        let files = pass::sync_files(&self.files, &self.store).await;
        report.changed.extend(files.changed);
        report.errors.extend(files.errors);
        report.finished_at = now_ms();
        report
    }

    // Sends a `settings` message and waits for the watch's answer, which lists every setting as it now is.
    async fn ask_settings(&self, changes: Value) -> Result<Value, String> {
        let mut message = Map::new();
        message.insert("t".into(), json!("settings"));
        if let Value::Object(changes) = changes {
            message.extend(changes.into_iter().filter(|(key, _)| key != "t"));
        }
        let mut replies = self.settings_reply.subscribe();
        replies.mark_unchanged();
        self.output
            .write_to_watch(encode_for_watch(&Value::Object(message)));
        tokio::time::timeout(SETTINGS_TIMEOUT, replies.changed())
            .await
            .map_err(|_| "settings: the watch did not answer.".to_string())?
            .map_err(|_| "settings: the link closed.".to_string())?;
        let reply = replies.borrow().clone().unwrap_or(Value::Null);
        let mut reply = reply;
        if let Some(fields) = reply.as_object_mut() {
            fields.remove("t");
        }
        Ok(reply)
    }

    // ========================================================================
    // Changes from the companion
    // ========================================================================

    pub async fn write_settings(&self, changes: Value) -> Result<Value, String> {
        let _turn = self.turn.lock().await;
        let settings = self.ask_settings(changes).await?;
        if self.store.put("settings", &settings)? {
            self.announce(vec!["settings".into()]);
        }
        Ok(settings)
    }

    // Writes the app's state file, then tells the watch so an open app takes it up instead of saving over it.
    pub async fn write_app_state(&self, id: &str, changes: &Value) -> Result<Value, String> {
        let _turn = self.turn.lock().await;
        let state = pass::write_app_state(&self.files, &self.store, id, changes).await?;
        self.output
            .write_to_watch(encode_for_watch(&json!({ "t": "state", "app": id })));
        self.announce(vec![format!("apps/{id}")]);
        Ok(state)
    }

    fn announce(&self, changed: Vec<String>) {
        self.output.synced(&SyncReport {
            changed,
            errors: Vec::new(),
            finished_at: now_ms(),
        });
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::files::fake::FakeWatch;
    use super::*;

    // Answers each settings message as the watch would, from settings it keeps.
    struct FakeLink {
        syncer: std::sync::OnceLock<std::sync::Weak<Syncer>>,
        settings: Mutex<Value>,
        lines: Mutex<Vec<String>>,
        reports: Mutex<Vec<SyncReport>>,
        answers: AtomicBool,
    }

    impl SyncOutput for FakeLink {
        fn write_to_watch(&self, line: String) {
            self.lines.lock().unwrap().push(line.clone());
            let message = decode_watch_line(line.trim_start_matches("GB(").trim_end().trim_end_matches(')'));
            let Some(message) = message.filter(|message| message["t"] == "settings") else {
                return;
            };
            let reply = {
                let mut settings = self.settings.lock().unwrap();
                for (key, value) in message.as_object().unwrap() {
                    if key != "t" {
                        settings[key] = value.clone();
                    }
                }
                let mut reply = settings.clone();
                reply["t"] = json!("settings");
                reply
            };
            if !self.answers.load(Ordering::SeqCst) {
                return;
            }
            if let Some(syncer) = self.syncer.get().and_then(std::sync::Weak::upgrade) {
                let text = format!("{reply}\n");
                // The reply arrives in notification-sized pieces.
                for piece in text.as_bytes().chunks(20) {
                    syncer.on_watch_text(std::str::from_utf8(piece).unwrap());
                }
            }
        }
        fn synced(&self, report: &SyncReport) {
            self.reports.lock().unwrap().push(report.clone());
        }
    }

    fn syncer(root: &std::path::Path, watch: &Arc<FakeWatch>) -> (Arc<Syncer>, Arc<FakeLink>) {
        let link = Arc::new(FakeLink {
            syncer: std::sync::OnceLock::new(),
            settings: Mutex::new(json!({ "brightness": 2, "clock_24h": true })),
            lines: Mutex::new(Vec::new()),
            reports: Mutex::new(Vec::new()),
            answers: AtomicBool::new(true),
        });
        let syncer = Arc::new(Syncer::new(watch.client(), SyncStore::new(root), link.clone()));
        let _ = link.syncer.set(Arc::downgrade(&syncer));
        (syncer, link)
    }

    #[tokio::test]
    async fn a_pass_keeps_the_watch_settings_and_its_files() {
        let root = tempfile::tempdir().unwrap();
        let watch = FakeWatch::with(&[("/settings/quick_apps", b"timer")]);
        let (syncer, _) = syncer(root.path(), &watch);
        let report = syncer.pass().await;
        assert_eq!(report.changed, vec!["settings", "files/settings/quick_apps"]);
        assert_eq!(
            syncer.store().get("settings").unwrap(),
            json!({ "brightness": 2, "clock_24h": true })
        );
        assert!(syncer.pass().await.changed.is_empty());
    }

    #[tokio::test]
    async fn a_silent_watch_is_an_error_but_files_still_sync() {
        let root = tempfile::tempdir().unwrap();
        let watch = FakeWatch::with(&[("/settings/quick_apps", b"timer")]);
        let (syncer, link) = syncer(root.path(), &watch);
        link.answers.store(false, Ordering::SeqCst);
        tokio::time::pause();
        let report = syncer.pass().await;
        assert_eq!(report.errors, vec!["settings: the watch did not answer."]);
        assert_eq!(report.changed, vec!["files/settings/quick_apps"]);
    }

    #[tokio::test]
    async fn settings_changed_here_go_to_the_watch_and_are_kept() {
        let root = tempfile::tempdir().unwrap();
        let (syncer, link) = syncer(root.path(), &FakeWatch::with(&[]));
        let settings = syncer
            .write_settings(json!({ "brightness": 3, "t": "x" }))
            .await
            .unwrap();
        assert_eq!(settings["brightness"], 3);
        assert_eq!(syncer.store().get("settings").unwrap()["brightness"], 3);
        assert_eq!(link.reports.lock().unwrap()[0].changed, vec!["settings"]);
    }

    #[tokio::test]
    async fn written_app_state_tells_the_watch_to_take_it_up() {
        let root = tempfile::tempdir().unwrap();
        let state = state_file::encode(&state_file::tests::alarm_state()).unwrap();
        let watch = FakeWatch::with(&[("/apps/alarm/state", &state)]);
        let (syncer, link) = syncer(root.path(), &watch);
        syncer
            .write_app_state("alarm", &json!({ "alarm_count": 1 }))
            .await
            .unwrap();
        assert_eq!(
            link.lines.lock().unwrap()[0],
            "GB({\"app\":\"alarm\",\"t\":\"state\"})\n"
        );
        assert_eq!(link.reports.lock().unwrap()[0].changed, vec!["apps/alarm"]);
    }

    #[tokio::test(start_paused = true)]
    async fn passes_run_on_connecting_and_then_every_interval() {
        let root = tempfile::tempdir().unwrap();
        let (syncer, link) = syncer(root.path(), &FakeWatch::with(&[]));
        syncer
            .set_settings(&SyncSettings { interval_seconds: 60 })
            .unwrap();
        tokio::spawn(syncer.clone().run());
        tokio::time::sleep(Duration::from_secs(600)).await;
        assert!(
            link.reports.lock().unwrap().is_empty(),
            "no pass while disconnected"
        );

        syncer.on_connection(true);
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(link.reports.lock().unwrap().len(), 1);
        tokio::time::sleep(Duration::from_secs(60)).await;
        assert_eq!(link.reports.lock().unwrap().len(), 2);

        syncer.on_connection(false);
        tokio::time::sleep(Duration::from_secs(600)).await;
        assert_eq!(link.reports.lock().unwrap().len(), 2);
    }
}
