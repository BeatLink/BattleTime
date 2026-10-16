// Keeps the watch's note matching its source while a watch is connected, as the window's NoteSync did.
//
// The note comes from text typed in the app, a chosen file the device reads, or a URL fetched with its
// headers, login and certificate choice. It is checked on connecting, every minute, whenever the source,
// its contents or the font change, and whenever the host says the chosen file was saved. Each check reads
// the source again and sends the note, or its font, only when it changed; the first check on a
// connection also lists the watch's files, so a note removed or replaced there is sent again. Checks
// never overlap, and how the last one went is published as the "noteStatus" feature.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{watch, Notify};
use tokio::time::Instant;

use super::{Daemon, Readiness};
use crate::protocol::notes::{
    fingerprint, note_too_large, remove_note, sync_note, NoteFont, NoteOnWatch, LARGEST_NOTE_BYTES,
};
use crate::web_request::{self, WebAccess, WebReply, WebRequest};

// ============================================================================
// Timing
// ============================================================================

// How often the chosen note is read again while a watch is connected.
pub const NOTE_CHECK_INTERVAL: Duration = Duration::from_secs(60);

// How long fetching the note may take.
const FETCH_TIMEOUT_MS: u64 = 20_000;

// ============================================================================
// What the window shows
// ============================================================================

// How keeping the watch's note up to date last went, in the window's NoteStatus shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum NoteStatus {
    Idle,
    Sending {
        bytes: usize,
    },
    Synced {
        #[serde(rename = "checkedAt")]
        checked_at: i64,
        bytes: usize,
    },
    Failed {
        message: String,
        at: i64,
    },
}

// ============================================================================
// The settings it reads
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoteSource {
    Typed,
    File,
    Url,
}

// A file the user picked: a path on a computer or a content link on a phone.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct NoteFileReference {
    #[serde(default)]
    name: String,
    uri: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct NoteSettings {
    note_source: Option<String>,
    note_text: String,
    note_file: Option<NoteFileReference>,
    note_url: String,
    note_url_access: Value,
    note_font: NoteFont,
    note_on_watch: Option<NoteOnWatch>,
}

impl NoteSettings {
    // The chosen source; settings saved before there was a choice kept only a file.
    fn source(&self) -> NoteSource {
        match self.note_source.as_deref() {
            Some("typed") => NoteSource::Typed,
            Some("file") => NoteSource::File,
            Some("url") => NoteSource::Url,
            _ if self.note_file.is_some() => NoteSource::File,
            _ => NoteSource::Typed,
        }
    }

    // How to read the chosen source, or None when that source has nothing in it yet.
    fn reader(&self) -> Option<NoteReader> {
        match self.source() {
            NoteSource::Typed => {
                (!self.note_text.is_empty()).then(|| NoteReader::Typed(self.note_text.clone()))
            }
            NoteSource::File => self
                .note_file
                .as_ref()
                .map(|file| NoteReader::File(file.uri.clone())),
            NoteSource::Url => {
                let url = self.note_url.trim();
                (!url.is_empty()).then(|| NoteReader::Url {
                    url: url.to_string(),
                    access: WebAccess::read(&self.note_url_access),
                })
            }
        }
    }

    // The settings that start a new round of checks when they change, as the window's note effect keyed it.
    fn session_key(&self, readiness: &Readiness) -> Option<SessionKey> {
        let watch_id = readiness.watch_id.clone()?;
        if !readiness.files() || self.reader().is_none() {
            return None;
        }
        let source = self.source();
        let file_uri = match source {
            NoteSource::File => self.note_file.as_ref().map(|file| file.uri.clone()),
            _ => None,
        };
        Some(SessionKey {
            epoch: readiness.epoch,
            watch_id,
            source,
            file_uri,
        })
    }

    // The settings whose change asks for one more check.
    fn content_key(&self) -> ContentKey {
        ContentKey {
            font: self.note_font,
            text: self.note_text.clone(),
            url: self.note_url.clone(),
            access: WebAccess::read(&self.note_url_access),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionKey {
    epoch: u64,
    watch_id: String,
    source: NoteSource,
    file_uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ContentKey {
    font: NoteFont,
    text: String,
    url: String,
    access: WebAccess,
}

// One round of checks on one connection.
struct Session {
    key: SessionKey,
    listed_watch: bool,
    // What this round last sent, which may be newer than what the settings hold.
    sent: Option<NoteOnWatch>,
}

// ============================================================================
// Reading the source
// ============================================================================

// Makes a web request; the daemon's own runs it on a blocking thread, and tests give a fake.
pub type WebRequester = Arc<dyn Fn(WebRequest) -> BoxFuture<'static, Result<WebReply, String>> + Send + Sync>;

pub fn web_requester() -> WebRequester {
    Arc::new(|request: WebRequest| {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || web_request::request(&request))
                .await
                .map_err(|error| error.to_string())?
        })
    })
}

enum NoteReader {
    Typed(String),
    File(String),
    Url { url: String, access: WebAccess },
}

impl NoteReader {
    async fn read(&self, daemon: &Daemon, requester: &WebRequester) -> Result<Vec<u8>, String> {
        match self {
            NoteReader::Typed(text) => Ok(text.as_bytes().to_vec()),
            NoteReader::File(uri) => daemon.device().read_file(uri).await,
            NoteReader::Url { url, access } => fetch_note(url, access, requester).await,
        }
    }
}

// Fetches the note's text from an http or https URL, refusing an error reply or one too big for the watch.
async fn fetch_note(url: &str, access: &WebAccess, requester: &WebRequester) -> Result<Vec<u8>, String> {
    let lowercase = url.to_ascii_lowercase();
    if !lowercase.starts_with("http://") && !lowercase.starts_with("https://") {
        return Err(format!("{url} is not an http or https URL."));
    }
    let reply = match access.request("GET", url, None, FETCH_TIMEOUT_MS) {
        Ok(mut request) => {
            // The access settings' own headers win over this default.
            request
                .headers
                .entry("Accept".to_string())
                .or_insert_with(|| "text/plain, */*".to_string());
            requester(request).await
        }
        Err(error) => Err(error),
    }
    .map_err(|error| format!("Could not fetch {url}: {error}"))?;
    if !(200..=299).contains(&reply.status) {
        let reason = if reply.status_text.is_empty() {
            String::new()
        } else {
            format!(" {}", reply.status_text)
        };
        return Err(format!("{url} answered {}{reason}.", reply.status));
    }
    if reply.body.len() > LARGEST_NOTE_BYTES {
        return Err(note_too_large(reply.body.len()));
    }
    Ok(reply.body)
}

// ============================================================================
// The keeper
// ============================================================================

pub struct NoteKeeper {
    daemon: Arc<Daemon>,
    requester: WebRequester,
    wake: Notify,
    // Counts every ask for a check, and the asks a finished check has answered.
    asked: AtomicU64,
    answered: watch::Sender<u64>,
}

impl NoteKeeper {
    pub fn start(daemon: &Arc<Daemon>) -> Arc<NoteKeeper> {
        NoteKeeper::start_with_requester(daemon, web_requester())
    }

    pub fn start_with_requester(daemon: &Arc<Daemon>, requester: WebRequester) -> Arc<NoteKeeper> {
        let keeper = Arc::new(NoteKeeper {
            daemon: daemon.clone(),
            requester,
            wake: Notify::new(),
            asked: AtomicU64::new(0),
            answered: watch::channel(0).0,
        });
        keeper.set_status(NoteStatus::Idle);
        daemon.runtime().spawn(keeper.clone().run());
        keeper
    }

    // Reads the note's source now and sends it if it changed, returning once that check is done.
    pub async fn sync_now(&self) {
        let ask = self.ask();
        let mut answered = self.answered.subscribe();
        let _ = answered.wait_for(|answered| *answered >= ask).await;
    }

    // The host saw the chosen file saved, so the note is checked again.
    pub fn file_changed(&self) {
        self.ask();
    }

    // Clears the chosen source, stops keeping a note on the watch and takes it off.
    pub async fn forget(&self) -> Result<(), String> {
        let settings: NoteSettings = self.daemon.settings().view();
        let mut patch = match settings.source() {
            NoteSource::Typed => json!({ "noteText": "" }),
            NoteSource::File => json!({ "noteFile": null }),
            NoteSource::Url => json!({ "noteUrl": "" }),
        };
        patch["noteOnWatch"] = Value::Null;
        self.daemon.update_settings(&patch)?;
        self.set_status(NoteStatus::Idle);
        if self.daemon.readiness().borrow().files() {
            remove_note(self.daemon.syncer().files()).await?;
        }
        Ok(())
    }

    fn ask(&self) -> u64 {
        let ask = self.asked.fetch_add(1, Ordering::SeqCst) + 1;
        self.wake.notify_one();
        ask
    }

    fn set_status(&self, status: NoteStatus) {
        self.daemon
            .set_feature("noteStatus", serde_json::to_value(status).unwrap_or_default());
    }

    // ========================================================================
    // Checking
    // ========================================================================

    // Runs for the daemon's life, checking whenever something asks for it; asks made during a check get one more.
    async fn run(self: Arc<Self>) {
        let mut readiness = self.daemon.readiness();
        let mut settings_changes = self.daemon.settings().subscribe();
        let mut session: Option<Session> = None;
        let mut content: Option<ContentKey> = None;
        let mut next_check = Instant::now() + NOTE_CHECK_INTERVAL;
        let mut nudged = false;
        loop {
            settings_changes.borrow_and_update();
            let ready = readiness.borrow_and_update().clone();
            let settings: NoteSettings = self.daemon.settings().view();
            let mut wanted = nudged;
            nudged = false;

            let key = settings.session_key(&ready);
            if key != session.as_ref().map(|session| session.key.clone()) {
                wanted = true;
                next_check = Instant::now() + NOTE_CHECK_INTERVAL;
                session = key.map(|key| Session {
                    key,
                    listed_watch: false,
                    sent: None,
                });
            }
            let new_content = settings.content_key();
            if content.as_ref() != Some(&new_content) {
                wanted = true;
                content = Some(new_content);
            }

            let answering = self.asked.load(Ordering::SeqCst);
            if let (true, Some(session)) = (wanted, session.as_mut()) {
                self.check_once(session).await;
            }
            self.answered.send_if_modified(|answered| {
                let newer = answering > *answered;
                if newer {
                    *answered = answering;
                }
                newer
            });

            tokio::select! {
                _ = self.wake.notified() => nudged = true,
                changed = readiness.changed() => if changed.is_err() { return },
                changed = settings_changes.changed() => if changed.is_err() { return },
                _ = tokio::time::sleep_until(next_check), if session.is_some() => {
                    nudged = true;
                    next_check = Instant::now() + NOTE_CHECK_INTERVAL;
                }
            }
        }
    }

    async fn check_once(&self, session: &mut Session) {
        let settings: NoteSettings = self.daemon.settings().view();
        let Some(reader) = settings.reader() else {
            self.set_status(NoteStatus::Idle);
            return;
        };
        let on_watch = session.sent.clone().or(settings.note_on_watch.clone());
        let status = match self.send(session, &reader, settings.note_font, on_watch).await {
            Ok(bytes) => NoteStatus::Synced {
                checked_at: now_in_milliseconds(),
                bytes,
            },
            Err(message) => NoteStatus::Failed {
                message,
                at: now_in_milliseconds(),
            },
        };
        self.set_status(status);
    }

    // Reads the note and sends what changed, giving the note's size.
    async fn send(
        &self,
        session: &mut Session,
        reader: &NoteReader,
        font: NoteFont,
        on_watch: Option<NoteOnWatch>,
    ) -> Result<usize, String> {
        let note = reader.read(&self.daemon, &self.requester).await?;
        let watch_id = session.key.watch_id.clone();
        let changed = on_watch.as_ref().is_none_or(|on_watch| {
            on_watch.watch_id != watch_id
                || on_watch.fingerprint != fingerprint(&note)
                || on_watch.font != font
        });
        if changed {
            self.set_status(NoteStatus::Sending { bytes: note.len() });
        }
        let result = sync_note(
            self.daemon.syncer().files(),
            &note,
            font,
            &watch_id,
            on_watch.as_ref(),
            !session.listed_watch,
        )
        .await?;
        session.listed_watch = true;
        if result.sent_note || result.sent_font {
            session.sent = Some(result.on_watch.clone());
            let remembered = serde_json::to_value(&result.on_watch).unwrap_or_default();
            let _ = self.daemon.update_settings(&json!({ "noteOnWatch": remembered }));
        }
        Ok(note.len())
    }
}

fn now_in_milliseconds() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(value: Value) -> NoteSettings {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn statuses_take_the_windows_shapes() {
        let shape = |status| serde_json::to_value(status).unwrap();

        assert_eq!(shape(NoteStatus::Idle), json!({ "kind": "idle" }));
        assert_eq!(
            shape(NoteStatus::Sending { bytes: 5 }),
            json!({ "kind": "sending", "bytes": 5 })
        );
        assert_eq!(
            shape(NoteStatus::Synced {
                checked_at: 7,
                bytes: 5
            }),
            json!({ "kind": "synced", "checkedAt": 7, "bytes": 5 })
        );
        assert_eq!(
            shape(NoteStatus::Failed {
                message: "no".into(),
                at: 7
            }),
            json!({ "kind": "failed", "message": "no", "at": 7 })
        );
    }

    #[test]
    fn settings_saved_before_the_choice_of_source_read_a_kept_file() {
        let old = settings(json!({ "noteFile": { "name": "a.txt", "uri": "/home/a.txt" } }));
        let bad = settings(json!({ "noteSource": "paper", "noteText": "hi" }));

        assert_eq!(old.source(), NoteSource::File);
        assert_eq!(bad.source(), NoteSource::Typed);
    }

    #[test]
    fn an_empty_source_has_nothing_to_read() {
        assert!(settings(json!({ "noteSource": "typed" })).reader().is_none());
        assert!(settings(json!({ "noteSource": "file" })).reader().is_none());
        assert!(settings(json!({ "noteSource": "url", "noteUrl": "  " }))
            .reader()
            .is_none());
        assert!(
            settings(json!({ "noteSource": "url", "noteUrl": " https://a/ " }))
                .reader()
                .is_some()
        );
    }

    #[test]
    fn notes_wait_for_a_watch_that_serves_files() {
        let typed = settings(json!({ "noteText": "hi" }));
        let away = Readiness::default();
        let without_files = Readiness {
            epoch: 1,
            watch_id: Some("AA".into()),
            capabilities: Some(Default::default()),
        };

        assert_eq!(typed.session_key(&away), None);
        assert_eq!(typed.session_key(&without_files), None);
    }

    fn replying(
        status: u16,
        status_text: &str,
        body: &[u8],
    ) -> (WebRequester, Arc<std::sync::Mutex<Vec<WebRequest>>>) {
        let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = requests.clone();
        let reply = WebReply {
            status,
            status_text: status_text.into(),
            body: body.to_vec(),
        };
        let requester: WebRequester = Arc::new(move |request| {
            seen.lock().unwrap().push(request);
            let reply = reply.clone();
            Box::pin(async move { Ok(reply) })
        });
        (requester, requests)
    }

    #[tokio::test]
    async fn a_fetched_note_asks_for_text_with_its_login() {
        let (requester, requests) = replying(200, "OK", b"hello");
        let access = WebAccess::read(&json!({ "username": "me", "password": "secret" }));

        let note = fetch_note("https://example.org/note", &access, &requester)
            .await
            .unwrap();

        assert_eq!(note, b"hello");
        let request = requests.lock().unwrap()[0].clone();
        assert_eq!(request.method, "GET");
        assert_eq!(request.headers["Accept"], "text/plain, */*");
        assert_eq!(request.headers["Authorization"], "Basic bWU6c2VjcmV0");
        assert_eq!(request.timeout_ms, FETCH_TIMEOUT_MS);
    }

    #[tokio::test]
    async fn a_fetch_refuses_error_replies_big_notes_and_other_addresses() {
        let (missing, _) = replying(404, "Not Found", b"");
        let (huge, _) = replying(200, "OK", &vec![b'x'; LARGEST_NOTE_BYTES + 1]);
        let (unnamed, _) = replying(500, "", b"");
        let failing: WebRequester =
            Arc::new(|_| Box::pin(async { Err("No answer within 20 s.".to_string()) }));
        let access = WebAccess::default();

        assert_eq!(
            fetch_note("https://a/n", &access, &missing).await.unwrap_err(),
            "https://a/n answered 404 Not Found."
        );
        assert_eq!(
            fetch_note("https://a/n", &access, &unnamed).await.unwrap_err(),
            "https://a/n answered 500."
        );
        assert_eq!(
            fetch_note("https://a/n", &access, &huge).await.unwrap_err(),
            note_too_large(LARGEST_NOTE_BYTES + 1)
        );
        assert_eq!(
            fetch_note("ftp://a/n", &access, &missing).await.unwrap_err(),
            "ftp://a/n is not an http or https URL."
        );
        assert_eq!(
            fetch_note("https://a/n", &access, &failing).await.unwrap_err(),
            "Could not fetch https://a/n: No answer within 20 s."
        );
    }
}
