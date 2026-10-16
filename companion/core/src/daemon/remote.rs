// The remote buttons' background job: it keeps the watch's layout matching the buttons set up in the companion and runs a button when the watch reports a tap.
//
// The first send on a connection lists the watch's /remote folder, so a layout removed there is sent
// again, and sends never overlap. Only a tap's index comes from the watch; what the button does is
// always looked up in the companion's own settings. It mirrors src/state/remote-sync.ts,
// src/services/remote-actions.ts and the remote parts of src/state/watch-provider.tsx, and reports to the
// window under the features "remoteStatus" and "remoteLog" in the same shapes.
//
// Actions only the host can run go to `Device::remote_action` as the stored action's JSON. A computer gets
// {"kind":"command","command":"<shell text>"} and answers {"exitCode":<integer or null when killed>,
// "output":"<what it printed>","timedOut":<bool>}. A phone gets {"kind":"android","intent":<intent>}, where
// the intent is {"type":"deeplink","uri"}, {"type":"activity","packageName","className","action"?,"extras"?}
// or {"type":"broadcast","action","packageName"?,"extras"?} with extras an object of strings, and answers
// anything once it has started it. Either answers Err with a message the log shows when it fails.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{broadcast, watch};

use super::device::DeviceKind;
use super::{Daemon, Readiness};
use crate::protocol::remote::{
    read_remote_action, read_remote_buttons, remote_tap_index, sync_remote_layout, AndroidIntent,
    BuiltinAction, RemoteAction, RemoteButton, RemoteOnWatch,
};
use crate::sync::files::FileClient;
use crate::web_request::{self, WebReply, WebRequest};

// ============================================================================
// Limits
// ============================================================================

// How long a web request may take before it counts as failed.
pub const HTTP_TIMEOUT_MS: u64 = 15000;

// The longest result kept for the log line, in characters.
const RESULT_LIMIT: usize = 120;

// How many taps and tests the log keeps, newest first.
pub const REMOTE_LOG_LIMIT: usize = 20;

// ============================================================================
// What the window shows
// ============================================================================

// How sending the layout last went, as src/state/remote-sync.ts's RemoteStatus.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RemoteStatus {
    Idle,
    Sending { buttons: usize },
    Synced { at: i64, buttons: usize },
    Failed { message: String, at: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TapSource {
    Watch,
    Test,
}

// One line in the window's log of recent taps and tests.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RemoteLogEntry {
    pub at: i64,
    pub button: u64,
    pub label: String,
    pub source: TapSource,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionOutcome {
    pub ok: bool,
    pub message: String,
}

// ============================================================================
// The settings it reads
// ============================================================================

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RemoteSettings {
    remote_buttons: Value,
    remote_on_watch: Option<RemoteOnWatch>,
}

impl RemoteSettings {
    fn buttons(&self) -> Vec<RemoteButton> {
        read_remote_buttons(&self.remote_buttons)
    }
}

// ============================================================================
// Names for the window
// ============================================================================

fn describe_intent(intent: &AndroidIntent) -> String {
    match intent {
        AndroidIntent::Deeplink { uri } => format!("Open {uri}"),
        AndroidIntent::Activity {
            package_name,
            class_name,
            ..
        } => format!("Start {package_name}/{class_name}"),
        AndroidIntent::Broadcast { action, .. } => format!("Broadcast {action}"),
    }
}

// A short line saying what an action does.
pub fn describe_action(action: &RemoteAction) -> String {
    match action {
        RemoteAction::Http { method, url, .. } => format!("{} {url}", method.as_str()),
        RemoteAction::Command { command } => format!("Run {command}"),
        RemoteAction::Android { intent } => describe_intent(intent),
        RemoteAction::Builtin { name } => name.label().to_string(),
    }
}

// A button's name for the log: its label, or what it does when it has none.
pub fn button_name(button: &RemoteButton) -> String {
    let label = button.label.trim();
    if label.is_empty() {
        describe_action(&button.action)
    } else {
        label.to_string()
    }
}

// Why a device of this kind cannot run the action, or None when it can.
pub fn unavailable_reason(action: &RemoteAction, kind: DeviceKind) -> Option<&'static str> {
    match (action, kind) {
        (RemoteAction::Command { .. }, DeviceKind::Phone) => {
            Some("Shell commands run only in the desktop app.")
        }
        (RemoteAction::Android { .. }, DeviceKind::Computer) => {
            Some("Android intents run only in the Android app.")
        }
        _ => None,
    }
}

// The text on one line, cut to the log's length.
fn clip(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > RESULT_LIMIT {
        let mut cut: String = flat.chars().take(RESULT_LIMIT - 1).collect();
        cut.push('…');
        cut
    } else {
        flat
    }
}

fn with_detail(headline: String, detail: &str) -> String {
    if detail.is_empty() {
        headline
    } else {
        format!("{headline}: {detail}")
    }
}

fn milliseconds_now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

// ============================================================================
// Web requests
// ============================================================================

// Makes a web request; the daemon's own runs core/src/web_request.rs on a blocking thread.
pub type WebRequester = Arc<dyn Fn(WebRequest) -> BoxFuture<'static, Result<WebReply, String>> + Send + Sync>;

fn blocking_web_requester(runtime: tokio::runtime::Handle) -> WebRequester {
    Arc::new(move |request| {
        let task = runtime.spawn_blocking(move || web_request::request(&request));
        Box::pin(async move { task.await.map_err(|error| error.to_string())? })
    })
}

// What a shell command host answers: its exit code (null when killed), what it printed, and whether it ran out of time.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandResult {
    exit_code: Option<i64>,
    #[serde(default)]
    output: String,
    #[serde(default)]
    timed_out: bool,
}

// ============================================================================
// Sending the layout over one connection
// ============================================================================

struct LayoutSender {
    epoch: u64,
    watch_id: String,
    // Held for a whole send, so sends never overlap.
    turn: tokio::sync::Mutex<()>,
    // A send was asked for that no send has started on yet.
    pending: AtomicBool,
    // The next send writes the file even when it looks unchanged.
    force: AtomicBool,
    listed_watch: AtomicBool,
    // What this connection last sent, which the settings hold too once saved.
    sent: Mutex<Option<RemoteOnWatch>>,
}

impl LayoutSender {
    fn new(epoch: u64, watch_id: String) -> LayoutSender {
        LayoutSender {
            epoch,
            watch_id,
            turn: tokio::sync::Mutex::new(()),
            pending: AtomicBool::new(false),
            force: AtomicBool::new(false),
            listed_watch: AtomicBool::new(false),
            sent: Mutex::new(None),
        }
    }

    // Sends the layout if it changed; one asked for while another runs is made once that one ends.
    async fn sync(&self, remote: &Remote, force: bool) {
        if force {
            self.force.store(true, Ordering::SeqCst);
        }
        self.pending.store(true, Ordering::SeqCst);
        let _turn = self.turn.lock().await;
        if self.pending.swap(false, Ordering::SeqCst) {
            self.send_once(remote).await;
        }
    }

    async fn send_once(&self, remote: &Remote) {
        let Some(daemon) = remote.daemon.upgrade() else {
            return;
        };
        let settings: RemoteSettings = daemon.settings().view();
        let buttons = settings.buttons();
        let last = if self.force.swap(false, Ordering::SeqCst) {
            None
        } else {
            self.sent.lock().unwrap().clone().or(settings.remote_on_watch)
        };
        let check = !self.listed_watch.load(Ordering::SeqCst);

        remote.set_status(
            &daemon,
            RemoteStatus::Sending {
                buttons: buttons.len(),
            },
        );
        let files = remote.files(&daemon);
        let result = sync_remote_layout(files, &buttons, &self.watch_id, last.as_ref(), check).await;

        match result {
            Ok(synced) => {
                self.listed_watch.store(true, Ordering::SeqCst);
                if synced.sent {
                    *self.sent.lock().unwrap() = Some(synced.on_watch.clone());
                    let _ = daemon.update_settings(&json!({ "remoteOnWatch": synced.on_watch }));
                }
                remote.set_status(
                    &daemon,
                    RemoteStatus::Synced {
                        at: milliseconds_now(),
                        buttons: buttons.len(),
                    },
                );
            }
            Err(message) => remote.set_status(
                &daemon,
                RemoteStatus::Failed {
                    message,
                    at: milliseconds_now(),
                },
            ),
        }
    }
}

// ============================================================================
// The job
// ============================================================================

pub struct Remote {
    daemon: Weak<Daemon>,
    // A file client in place of the daemon's own, for tests.
    files: Option<Arc<FileClient>>,
    web: WebRequester,
    log: Mutex<Vec<RemoteLogEntry>>,
    // The sender for the connection the layout is kept on, while the remote is in use there.
    sender: Mutex<Option<Arc<LayoutSender>>>,
}

impl Remote {
    // Starts keeping the layout and hearing taps on the daemon's runtime.
    pub fn start(daemon: &Arc<Daemon>) -> Arc<Remote> {
        let web = blocking_web_requester(daemon.runtime().clone());
        Remote::start_with(daemon, None, web)
    }

    pub(crate) fn start_with(
        daemon: &Arc<Daemon>,
        files: Option<Arc<FileClient>>,
        web: WebRequester,
    ) -> Arc<Remote> {
        let remote = Arc::new(Remote {
            daemon: Arc::downgrade(daemon),
            files,
            web,
            log: Mutex::new(Vec::new()),
            sender: Mutex::new(None),
        });
        remote.set_status(daemon, RemoteStatus::Idle);
        daemon.set_feature("remoteLog", json!([]));
        daemon.runtime().spawn(
            remote
                .clone()
                .keep_layout(daemon.readiness(), daemon.settings().subscribe()),
        );
        daemon
            .runtime()
            .spawn(remote.clone().hear_taps(daemon.watch_messages()));
        remote
    }

    // ========================================================================
    // For the window
    // ========================================================================

    // Writes the layout to the watch now, even if it looks unchanged.
    pub async fn send_layout(&self) -> Result<(), String> {
        let sender = self
            .sender
            .lock()
            .unwrap()
            .clone()
            .ok_or("Connect to a watch that serves files first.")?;
        sender.sync(self, true).await;
        Ok(())
    }

    // Runs a button as a tap would, and logs it as a test; an index with no button does nothing.
    pub async fn test_button(&self, index: usize) {
        let Some(daemon) = self.daemon.upgrade() else {
            return;
        };
        let Some(button) = daemon
            .settings()
            .view::<RemoteSettings>()
            .buttons()
            .into_iter()
            .nth(index)
        else {
            return;
        };
        let outcome = self.run_action(&daemon, &button.action).await;
        self.add_log(
            &daemon,
            RemoteLogEntry {
                at: milliseconds_now(),
                button: index as u64,
                label: button_name(&button),
                source: TapSource::Test,
                ok: outcome.ok,
                message: outcome.message,
            },
        );
    }

    pub fn clear_log(&self) {
        self.log.lock().unwrap().clear();
        if let Some(daemon) = self.daemon.upgrade() {
            daemon.set_feature("remoteLog", json!([]));
        }
    }

    // Why this device cannot run a stored action, or None when it can or the action is not one the companion knows.
    pub fn unavailable(&self, action: &Value) -> Option<&'static str> {
        let daemon = self.daemon.upgrade()?;
        unavailable_reason(&read_remote_action(action)?, daemon.device().kind())
    }

    // ========================================================================
    // Keeping the layout
    // ========================================================================

    // Starts a sender for each connection that serves files while buttons are set up, and sends again whenever they change.
    async fn keep_layout(
        self: Arc<Self>,
        mut readiness: watch::Receiver<Readiness>,
        mut settings: watch::Receiver<u64>,
    ) {
        let mut seen_buttons: Option<Vec<RemoteButton>> = None;
        loop {
            let Some(daemon) = self.daemon.upgrade() else {
                return;
            };
            let ready = readiness.borrow_and_update().clone();
            settings.borrow_and_update();
            let view: RemoteSettings = daemon.settings().view();
            let buttons = view.buttons();
            let buttons_changed = seen_buttons.as_ref() != Some(&buttons);
            let in_use = !buttons.is_empty() || view.remote_on_watch.is_some();
            seen_buttons = Some(buttons);

            let serves_files = ready.files();
            let to_send = match ready.watch_id.filter(|_| serves_files && in_use) {
                Some(watch_id) => {
                    let mut current = self.sender.lock().unwrap();
                    let fresh = current
                        .as_ref()
                        .is_none_or(|sender| sender.epoch != ready.epoch || sender.watch_id != watch_id);
                    if fresh {
                        *current = Some(Arc::new(LayoutSender::new(ready.epoch, watch_id)));
                    }
                    current.clone().filter(|_| fresh || buttons_changed)
                }
                None => {
                    *self.sender.lock().unwrap() = None;
                    None
                }
            };
            if let Some(sender) = to_send {
                let remote = self.clone();
                daemon
                    .runtime()
                    .spawn(async move { sender.sync(&remote, false).await });
            }
            drop(daemon);

            tokio::select! {
                changed = readiness.changed() => if changed.is_err() { return },
                changed = settings.changed() => if changed.is_err() { return },
            }
        }
    }

    fn files<'client>(&'client self, daemon: &'client Daemon) -> &'client FileClient {
        self.files.as_deref().unwrap_or_else(|| daemon.syncer().files())
    }

    fn set_status(&self, daemon: &Daemon, status: RemoteStatus) {
        daemon.set_feature("remoteStatus", serde_json::to_value(status).unwrap_or_default());
    }

    // ========================================================================
    // Taps
    // ========================================================================

    async fn hear_taps(self: Arc<Self>, mut messages: broadcast::Receiver<Value>) {
        loop {
            let message = match messages.recv().await {
                Ok(message) => message,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return,
            };
            let Some(index) = remote_tap_index(&message) else {
                continue;
            };
            let Some(daemon) = self.daemon.upgrade() else {
                return;
            };
            daemon.runtime().spawn(self.clone().on_tap(index));
        }
    }

    // Runs the stored button the watch named, or logs that there is none.
    async fn on_tap(self: Arc<Self>, index: u64) {
        let Some(daemon) = self.daemon.upgrade() else {
            return;
        };
        let buttons = daemon.settings().view::<RemoteSettings>().buttons();
        let entry = match usize::try_from(index).ok().and_then(|index| buttons.get(index)) {
            None => RemoteLogEntry {
                at: milliseconds_now(),
                button: index,
                label: format!("Button {}", index.saturating_add(1)),
                source: TapSource::Watch,
                ok: false,
                message: format!("Ignored: there is no button {}.", index.saturating_add(1)),
            },
            Some(button) => {
                let outcome = self.run_action(&daemon, &button.action).await;
                RemoteLogEntry {
                    at: milliseconds_now(),
                    button: index,
                    label: button_name(button),
                    source: TapSource::Watch,
                    ok: outcome.ok,
                    message: outcome.message,
                }
            }
        };
        self.add_log(&daemon, entry);
    }

    fn add_log(&self, daemon: &Daemon, entry: RemoteLogEntry) {
        let entries = {
            let mut log = self.log.lock().unwrap();
            log.insert(0, entry);
            log.truncate(REMOTE_LOG_LIMIT);
            serde_json::to_value(&*log).unwrap_or_default()
        };
        daemon.set_feature("remoteLog", entries);
    }

    // ========================================================================
    // Running an action
    // ========================================================================

    // Runs one action and says how it went; an action this device cannot run is skipped with the reason.
    pub async fn run_action(&self, daemon: &Daemon, action: &RemoteAction) -> ActionOutcome {
        if let Some(reason) = unavailable_reason(action, daemon.device().kind()) {
            return ActionOutcome {
                ok: false,
                message: format!("Skipped: {reason}"),
            };
        }
        let outcome =
            match action {
                RemoteAction::Http {
                    method,
                    url,
                    body,
                    access,
                } => {
                    let body = if *method == crate::protocol::remote::HttpMethod::Get {
                        None
                    } else {
                        body.clone()
                    };
                    match access.request(method.as_str(), url, body, HTTP_TIMEOUT_MS) {
                        Ok(request) => self.run_http(request).await,
                        Err(error) => Err(error),
                    }
                }
                RemoteAction::Command { .. } => self.run_command(daemon, action).await,
                RemoteAction::Android { intent } => daemon
                    .device()
                    .remote_action(&action.to_value())
                    .await
                    .map(|_| ActionOutcome {
                        ok: true,
                        message: format!("{}: done.", describe_intent(intent)),
                    }),
                RemoteAction::Builtin { name } => self.run_builtin(daemon, *name).await,
            };
        outcome.unwrap_or_else(|message| ActionOutcome { ok: false, message })
    }

    async fn run_http(&self, request: WebRequest) -> Result<ActionOutcome, String> {
        let reply = (self.web)(request).await?;
        let text = clip(&String::from_utf8_lossy(&reply.body));
        Ok(ActionOutcome {
            ok: (200..300).contains(&reply.status),
            message: with_detail(format!("HTTP {}", reply.status), &text),
        })
    }

    async fn run_command(&self, daemon: &Daemon, action: &RemoteAction) -> Result<ActionOutcome, String> {
        let answer = daemon.device().remote_action(&action.to_value()).await?;
        let result: CommandResult = serde_json::from_value(answer)
            .map_err(|_| "The desktop app could not run the command.".to_string())?;
        let output = clip(&result.output);
        if result.timed_out {
            return Ok(ActionOutcome {
                ok: false,
                message: with_detail("Stopped after running too long".into(), &output),
            });
        }
        let exit = result
            .exit_code
            .map_or_else(|| "killed".to_string(), |code| code.to_string());
        Ok(ActionOutcome {
            ok: result.exit_code == Some(0),
            message: with_detail(format!("Exit {exit}"), &output),
        })
    }

    async fn run_builtin(&self, daemon: &Daemon, name: BuiltinAction) -> Result<ActionOutcome, String> {
        if name == BuiltinAction::FindPhone {
            daemon.start_finding_phone();
            return Ok(ActionOutcome {
                ok: true,
                message: "Showing that the watch is looking for this device.".into(),
            });
        }
        let command = match name {
            BuiltinAction::PlayPause if daemon.state()["nowPlaying"]["playing"] == true => "pause",
            BuiltinAction::PlayPause => "play",
            other => other.as_str(),
        };
        daemon.device().media_command(command).await?;
        Ok(ActionOutcome {
            ok: true,
            message: format!("Sent {command} to the media player."),
        })
    }
}

#[cfg(test)]
mod tests;
