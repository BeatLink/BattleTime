// The remote buttons' job on a daemon with a watch's files in memory and a fake device, in paused time.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::future::BoxFuture;
use serde_json::{json, Value};
use tokio::runtime::Handle;
use uuid::Uuid;

use super::*;
use crate::ble::ValueListener;
use crate::daemon::device::{CallCommand, Device, DeviceAnswer, DeviceKind, Location, Media, PhoneEvent};
use crate::daemon::link::Link;
use crate::daemon::settings::SettingsStore;
use crate::daemon::DaemonSetup;
use crate::protocol::battletime::Capabilities;
use crate::protocol::link::WriteMode;
use crate::protocol::remote::REMOTE_BUTTONS_PATH;
use crate::sync::files::fake::FakeWatch;
use crate::sync::files::FileTransport;

// ============================================================================
// A link that goes nowhere
// ============================================================================

struct NoLink;

fn nothing() -> BoxFuture<'static, Result<(), String>> {
    Box::pin(async { Ok(()) })
}

impl Link for NoLink {
    fn connect(&self, _address: String) -> BoxFuture<'static, Result<(), String>> {
        nothing()
    }

    fn disconnect(&self) -> BoxFuture<'static, Result<(), String>> {
        nothing()
    }

    fn write_text(&self, _text: String) -> BoxFuture<'static, Result<(), String>> {
        nothing()
    }

    fn has_characteristic(&self, _service: Uuid, _characteristic: Uuid) -> bool {
        false
    }

    fn read(&self, _service: Uuid, _characteristic: Uuid) -> BoxFuture<'static, Result<Vec<u8>, String>> {
        Box::pin(async { Err("not served".to_string()) })
    }

    fn write(
        &self,
        _service: Uuid,
        _characteristic: Uuid,
        _data: Vec<u8>,
        _mode: WriteMode,
    ) -> BoxFuture<'static, Result<(), String>> {
        nothing()
    }

    fn listen(
        &self,
        _service: Uuid,
        _characteristic: Uuid,
        _listener: ValueListener,
    ) -> BoxFuture<'static, Result<(), String>> {
        nothing()
    }

    fn subscribe(&self, _service: Uuid, _characteristic: Uuid) -> BoxFuture<'static, Result<(), String>> {
        nothing()
    }
}

// A file service whose every command fails, as over a link that has gone.
struct BrokenFiles;

impl FileTransport for BrokenFiles {
    fn send(&self, _command: Vec<u8>) -> BoxFuture<'static, Result<(), String>> {
        Box::pin(async { Err("The link has gone.".to_string()) })
    }
}

// ============================================================================
// A fake device
// ============================================================================

struct FakeDevice {
    kind: DeviceKind,
    media_commands: Mutex<Vec<String>>,
    ringing: Mutex<Vec<bool>>,
    remote_actions: Mutex<Vec<Value>>,
    remote_answer: Mutex<Result<Value, String>>,
}

impl FakeDevice {
    fn new(kind: DeviceKind) -> Arc<FakeDevice> {
        Arc::new(FakeDevice {
            kind,
            media_commands: Mutex::new(Vec::new()),
            ringing: Mutex::new(Vec::new()),
            remote_actions: Mutex::new(Vec::new()),
            remote_answer: Mutex::new(Ok(Value::Null)),
        })
    }
}

struct SharedDevice {
    device: Arc<FakeDevice>,
}

impl Device for SharedDevice {
    fn kind(&self) -> DeviceKind {
        self.device.kind
    }

    fn is_do_not_disturb(&self) -> BoxFuture<'static, bool> {
        Box::pin(async { false })
    }

    fn media_command(&self, command: &str) -> DeviceAnswer<()> {
        self.device
            .media_commands
            .lock()
            .unwrap()
            .push(command.to_string());
        Box::pin(async { Ok(()) })
    }

    fn call_command(&self, _command: CallCommand) -> DeviceAnswer<()> {
        Box::pin(async { Ok(()) })
    }

    fn locate(&self) -> DeviceAnswer<Location> {
        Box::pin(async { Err("nowhere".to_string()) })
    }

    fn read_file(&self, _uri: &str) -> DeviceAnswer<Vec<u8>> {
        Box::pin(async { Err("no files".to_string()) })
    }

    fn find_phone(&self, ringing: bool) {
        self.device.ringing.lock().unwrap().push(ringing);
    }

    fn remote_action(&self, action: &Value) -> DeviceAnswer<Value> {
        self.device.remote_actions.lock().unwrap().push(action.clone());
        let answer = self.device.remote_answer.lock().unwrap().clone();
        Box::pin(async move { answer })
    }
}

// ============================================================================
// Setting up
// ============================================================================

struct Rig {
    daemon: Arc<Daemon>,
    remote: Arc<Remote>,
    watch: Arc<FakeWatch>,
    device: Arc<FakeDevice>,
    requests: Arc<Mutex<Vec<WebRequest>>>,
    _folder: tempfile::TempDir,
}

struct RigSetup {
    settings: Value,
    kind: DeviceKind,
    broken_files: bool,
    reply: WebReply,
}

impl Default for RigSetup {
    fn default() -> Self {
        RigSetup {
            settings: Value::Null,
            kind: DeviceKind::Phone,
            broken_files: false,
            reply: WebReply {
                status: 200,
                status_text: "OK".into(),
                body: Vec::new(),
            },
        }
    }
}

fn rig_with(setup: RigSetup) -> Rig {
    let folder = tempfile::tempdir().unwrap();
    if setup.settings != Value::Null {
        SettingsStore::open(folder.path().join("companion").join("settings.json"))
            .update(&setup.settings)
            .unwrap();
    }
    let device = FakeDevice::new(setup.kind);
    let daemon = Daemon::start_with_link(
        DaemonSetup {
            data_dir: folder.path().to_path_buf(),
            device: Arc::new(SharedDevice {
                device: device.clone(),
            }),
            runtime: Handle::current(),
        },
        Arc::new(NoLink),
    );
    let watch = FakeWatch::with(&[]);
    let files = if setup.broken_files {
        Arc::new(FileClient::new(Arc::new(BrokenFiles)))
    } else {
        watch.client()
    };
    let requests = Arc::new(Mutex::new(Vec::new()));
    let heard = requests.clone();
    let reply = setup.reply;
    let web: WebRequester = Arc::new(move |request| {
        heard.lock().unwrap().push(request);
        let reply = reply.clone();
        Box::pin(async move { Ok(reply) })
    });
    let remote = Remote::start_with(&daemon, Some(files), web);
    Rig {
        daemon,
        remote,
        watch,
        device,
        requests,
        _folder: folder,
    }
}

fn rig_with_buttons(buttons: Value) -> Rig {
    rig_with(RigSetup {
        settings: json!({ "remoteButtons": buttons }),
        ..RigSetup::default()
    })
}

fn builtin(icon: &str, name: &str) -> Value {
    json!({ "icon": icon, "label": "", "action": { "kind": "builtin", "name": name } })
}

fn two_buttons() -> Value {
    json!([builtin("alarm", "next"), builtin("bell", "playpause")])
}

// Says a watch with the file service is ready, on a new connection.
fn connect(rig: &Rig, watch_id: &str, files: bool) {
    rig.daemon.readiness.send_modify(|readiness| {
        readiness.epoch += 1;
        readiness.watch_id = Some(watch_id.to_string());
        readiness.capabilities = Some(Capabilities {
            files,
            ..Capabilities::default()
        });
    });
}

fn disconnect(rig: &Rig) {
    rig.daemon.readiness.send_modify(|readiness| {
        readiness.epoch += 1;
        readiness.watch_id = None;
        readiness.capabilities = None;
    });
}

fn tap(rig: &Rig, index: Value) {
    let line = json!({ "t": "app", "ch": "remote", "d": { "button": index } }).to_string();
    rig.daemon.on_link_event("line", json!(format!("{line}\n")));
}

fn layout_on_watch(rig: &Rig) -> Option<Vec<u8>> {
    rig.watch.files.lock().unwrap().get(REMOTE_BUTTONS_PATH).cloned()
}

fn layout_writes(rig: &Rig) -> usize {
    const WRITE: u8 = 0x20;
    rig.watch
        .commands
        .lock()
        .unwrap()
        .iter()
        .filter(|&&command| command == WRITE)
        .count()
}

fn feature(rig: &Rig, key: &str) -> Value {
    rig.daemon.state()["features"][key].clone()
}

async fn pass(duration: Duration) {
    tokio::time::sleep(duration).await;
}

async fn settle() {
    pass(Duration::from_millis(50)).await;
}

// ============================================================================
// Keeping the layout
// ============================================================================

#[tokio::test(start_paused = true)]
async fn the_layout_is_sent_when_a_watch_serving_files_connects() {
    let rig = rig_with_buttons(two_buttons());
    assert_eq!(feature(&rig, "remoteStatus"), json!({ "kind": "idle" }));
    assert_eq!(feature(&rig, "remoteLog"), json!([]));

    connect(&rig, "AA", true);
    settle().await;

    assert_eq!(layout_on_watch(&rig), Some(vec![1, 2, 0, 3]));
    let status = feature(&rig, "remoteStatus");
    assert_eq!(status["kind"], "synced");
    assert_eq!(status["buttons"], 2);
    assert!(status["at"].is_i64());
    assert_eq!(
        rig.daemon.settings_json()["remoteOnWatch"],
        json!({ "watchId": "AA", "fingerprint": crate::protocol::remote::fingerprint(&[1, 2, 0, 3]) })
    );
}

#[tokio::test(start_paused = true)]
async fn nothing_is_sent_without_buttons_or_without_the_file_service() {
    let rig = rig_with(RigSetup::default());
    connect(&rig, "AA", true);
    settle().await;
    assert_eq!(layout_writes(&rig), 0);

    let rig = rig_with_buttons(two_buttons());
    connect(&rig, "AA", false);
    settle().await;
    assert_eq!(layout_writes(&rig), 0);
    assert_eq!(feature(&rig, "remoteStatus"), json!({ "kind": "idle" }));
}

#[tokio::test(start_paused = true)]
async fn changed_buttons_are_sent_again_but_other_settings_are_not() {
    let rig = rig_with_buttons(two_buttons());
    connect(&rig, "AA", true);
    settle().await;
    assert_eq!(layout_writes(&rig), 1);

    rig.daemon
        .update_settings(&json!({ "forwardMusic": false }))
        .unwrap();
    settle().await;
    assert_eq!(layout_writes(&rig), 1);

    rig.daemon
        .update_settings(&json!({ "remoteButtons": [builtin("dice", "play")] }))
        .unwrap();
    settle().await;
    assert_eq!(layout_writes(&rig), 2);
    assert_eq!(layout_on_watch(&rig), Some(vec![1, 1, 13]));
}

#[tokio::test(start_paused = true)]
async fn removing_every_button_clears_the_layout_on_the_watch() {
    let rig = rig_with_buttons(two_buttons());
    connect(&rig, "AA", true);
    settle().await;

    rig.daemon
        .update_settings(&json!({ "remoteButtons": [] }))
        .unwrap();
    settle().await;

    assert_eq!(layout_on_watch(&rig), Some(vec![1, 0]));
    assert_eq!(feature(&rig, "remoteStatus")["buttons"], 0);
}

#[tokio::test(start_paused = true)]
async fn a_new_connection_checks_the_watch_and_resends_a_lost_layout() {
    let rig = rig_with_buttons(two_buttons());
    connect(&rig, "AA", true);
    settle().await;
    disconnect(&rig);
    settle().await;

    connect(&rig, "AA", true);
    settle().await;
    assert_eq!(layout_writes(&rig), 1);

    rig.watch.files.lock().unwrap().clear();
    disconnect(&rig);
    connect(&rig, "AA", true);
    settle().await;
    assert_eq!(layout_writes(&rig), 2);
    assert_eq!(layout_on_watch(&rig), Some(vec![1, 2, 0, 3]));
}

#[tokio::test(start_paused = true)]
async fn a_different_watch_is_sent_the_layout_too() {
    let rig = rig_with_buttons(two_buttons());
    connect(&rig, "AA", true);
    settle().await;

    connect(&rig, "BB", true);
    settle().await;

    assert_eq!(layout_writes(&rig), 2);
    assert_eq!(rig.daemon.settings_json()["remoteOnWatch"]["watchId"], "BB");
}

#[tokio::test(start_paused = true)]
async fn sending_by_hand_writes_even_an_unchanged_layout() {
    let rig = rig_with_buttons(two_buttons());
    assert_eq!(
        rig.remote.send_layout().await.unwrap_err(),
        "Connect to a watch that serves files first."
    );
    connect(&rig, "AA", true);
    settle().await;

    rig.remote.send_layout().await.unwrap();

    assert_eq!(layout_writes(&rig), 2);
    assert_eq!(feature(&rig, "remoteStatus")["kind"], "synced");
}

#[tokio::test(start_paused = true)]
async fn a_failed_send_says_why() {
    let rig = rig_with(RigSetup {
        settings: json!({ "remoteButtons": two_buttons() }),
        broken_files: true,
        ..RigSetup::default()
    });

    connect(&rig, "AA", true);
    settle().await;

    let status = feature(&rig, "remoteStatus");
    assert_eq!(status["kind"], "failed");
    assert_eq!(status["message"], "The link has gone.");
    assert!(status["at"].is_i64());
    assert!(rig.daemon.settings_json().get("remoteOnWatch").is_none());
}

// ============================================================================
// Taps
// ============================================================================

#[tokio::test(start_paused = true)]
async fn a_tap_runs_the_stored_button_and_logs_it() {
    let rig = rig_with_buttons(two_buttons());

    tap(&rig, json!(0));
    settle().await;

    assert_eq!(
        *rig.device.media_commands.lock().unwrap(),
        vec!["next".to_string()]
    );
    let entry = &feature(&rig, "remoteLog")[0];
    assert_eq!(entry["button"], 0);
    assert_eq!(entry["label"], "Next track");
    assert_eq!(entry["source"], "watch");
    assert_eq!(entry["ok"], true);
    assert_eq!(entry["message"], "Sent next to the media player.");
}

#[tokio::test(start_paused = true)]
async fn a_tap_on_a_missing_button_is_ignored_and_a_malformed_one_unheard() {
    let rig = rig_with_buttons(two_buttons());

    tap(&rig, json!(4));
    tap(&rig, json!(1.5));
    settle().await;

    let log = feature(&rig, "remoteLog");
    assert_eq!(log.as_array().unwrap().len(), 1);
    assert_eq!(log[0]["label"], "Button 5");
    assert_eq!(log[0]["ok"], false);
    assert_eq!(log[0]["message"], "Ignored: there is no button 5.");
    assert!(rig.device.media_commands.lock().unwrap().is_empty());
}

#[tokio::test(start_paused = true)]
async fn play_or_pause_follows_what_is_playing() {
    let rig = rig_with_buttons(two_buttons());

    tap(&rig, json!(1));
    settle().await;
    rig.daemon.phone_event(PhoneEvent::Media(Media {
        playing: true,
        ..Media::default()
    }));
    settle().await;
    tap(&rig, json!(1));
    settle().await;

    assert_eq!(
        *rig.device.media_commands.lock().unwrap(),
        vec!["play".to_string(), "pause".to_string()]
    );
}

#[tokio::test(start_paused = true)]
async fn find_this_device_shows_the_watch_is_looking() {
    let rig = rig_with_buttons(json!([builtin("flag", "findphone")]));

    tap(&rig, json!(0));
    settle().await;

    assert_eq!(rig.daemon.state()["findPhone"], true);
    assert_eq!(*rig.device.ringing.lock().unwrap(), vec![true]);
    assert_eq!(
        feature(&rig, "remoteLog")[0]["message"],
        "Showing that the watch is looking for this device."
    );
}

#[tokio::test(start_paused = true)]
async fn a_web_request_sends_the_buttons_access_and_reports_the_reply() {
    let rig = rig_with(RigSetup {
        settings: json!({ "remoteButtons": [
            { "icon": "bell", "label": " Lights ", "action": {
                "kind": "http", "method": "POST", "url": "https://hub/on", "body": "on",
                "username": "me", "password": "secret" } },
            { "icon": "bell", "label": "", "action": {
                "kind": "http", "method": "GET", "url": "http://hub/state", "body": "ignored" } },
        ] }),
        reply: WebReply {
            status: 201,
            status_text: "Created".into(),
            body: b"  all\n  done ".to_vec(),
        },
        ..RigSetup::default()
    });

    tap(&rig, json!(0));
    settle().await;
    tap(&rig, json!(1));
    settle().await;

    let requests = rig.requests.lock().unwrap().clone();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].body.as_deref(), Some("on"));
    assert_eq!(requests[0].timeout_ms, HTTP_TIMEOUT_MS);
    assert_eq!(requests[0].headers["Authorization"], "Basic bWU6c2VjcmV0");
    assert_eq!(requests[1].body, None);
    let log = feature(&rig, "remoteLog");
    assert_eq!(log[1]["label"], "Lights");
    assert_eq!(log[1]["message"], "HTTP 201: all done");
    assert_eq!(log[1]["ok"], true);
    assert_eq!(log[0]["label"], "GET http://hub/state");
}

#[tokio::test(start_paused = true)]
async fn a_reply_outside_success_counts_as_failed() {
    let rig = rig_with(RigSetup {
        settings: json!({ "remoteButtons": [
            { "icon": "bell", "action": { "kind": "http", "method": "GET", "url": "http://hub" } },
            { "icon": "bell", "action": { "kind": "http", "method": "GET", "url": "hub" } },
        ] }),
        reply: WebReply {
            status: 404,
            status_text: "Not Found".into(),
            body: Vec::new(),
        },
        ..RigSetup::default()
    });

    rig.remote.test_button(0).await;
    rig.remote.test_button(1).await;

    let log = feature(&rig, "remoteLog");
    assert_eq!(log[1]["message"], "HTTP 404");
    assert_eq!(log[1]["ok"], false);
    assert_eq!(log[0]["message"], "hub is not an http or https URL.");
}

#[tokio::test(start_paused = true)]
async fn a_shell_command_runs_through_the_computer() {
    let rig = rig_with(RigSetup {
        settings: json!({ "remoteButtons": [
            { "icon": "bell", "action": { "kind": "command", "command": "lights on" } },
            { "icon": "bell", "action": { "kind": "android", "intent": { "type": "deeplink", "uri": "x:y" } } },
        ] }),
        kind: DeviceKind::Computer,
        ..RigSetup::default()
    });
    *rig.device.remote_answer.lock().unwrap() =
        Ok(json!({ "exitCode": 0, "output": "fine\n", "timedOut": false }));

    rig.remote.test_button(0).await;
    *rig.device.remote_answer.lock().unwrap() =
        Ok(json!({ "exitCode": null, "output": "", "timedOut": false }));
    rig.remote.test_button(0).await;
    *rig.device.remote_answer.lock().unwrap() =
        Ok(json!({ "exitCode": null, "output": "half", "timedOut": true }));
    rig.remote.test_button(0).await;
    rig.remote.test_button(1).await;

    assert_eq!(
        rig.device.remote_actions.lock().unwrap()[0],
        json!({ "kind": "command", "command": "lights on" })
    );
    let log = feature(&rig, "remoteLog");
    assert_eq!(log[3]["message"], "Exit 0: fine");
    assert_eq!(log[3]["ok"], true);
    assert_eq!(log[3]["source"], "test");
    assert_eq!(log[3]["label"], "Run lights on");
    assert_eq!(log[2]["message"], "Exit killed");
    assert_eq!(log[1]["message"], "Stopped after running too long: half");
    assert_eq!(
        log[0]["message"],
        "Skipped: Android intents run only in the Android app."
    );
    assert_eq!(rig.device.remote_actions.lock().unwrap().len(), 3);
}

#[tokio::test(start_paused = true)]
async fn an_android_intent_runs_through_the_phone() {
    let rig = rig_with(RigSetup {
        settings: json!({ "remoteButtons": [
            { "icon": "bell", "action": { "kind": "android", "intent": {
                "type": "broadcast", "action": "a.LIGHTS", "extras": { "on": "1" } } } },
            { "icon": "bell", "action": { "kind": "command", "command": "ls" } },
        ] }),
        ..RigSetup::default()
    });

    rig.remote.test_button(0).await;
    *rig.device.remote_answer.lock().unwrap() = Err("No app takes that.".into());
    rig.remote.test_button(0).await;
    rig.remote.test_button(1).await;

    assert_eq!(
        rig.device.remote_actions.lock().unwrap()[0],
        json!({ "kind": "android", "intent": { "type": "broadcast", "action": "a.LIGHTS", "extras": { "on": "1" } } })
    );
    let log = feature(&rig, "remoteLog");
    assert_eq!(log[2]["message"], "Broadcast a.LIGHTS: done.");
    assert_eq!(log[1]["message"], "No app takes that.");
    assert_eq!(log[1]["ok"], false);
    assert_eq!(
        log[0]["message"],
        "Skipped: Shell commands run only in the desktop app."
    );
    assert_eq!(
        rig.remote
            .unavailable(&json!({ "kind": "command", "command": "ls" })),
        Some("Shell commands run only in the desktop app.")
    );
}

// ============================================================================
// The log
// ============================================================================

#[tokio::test(start_paused = true)]
async fn the_log_keeps_the_newest_entries_and_can_be_cleared() {
    let rig = rig_with_buttons(two_buttons());

    for _ in 0..REMOTE_LOG_LIMIT + 5 {
        rig.remote.test_button(0).await;
    }
    rig.remote.test_button(7).await;
    assert_eq!(
        feature(&rig, "remoteLog").as_array().unwrap().len(),
        REMOTE_LOG_LIMIT
    );

    rig.remote.clear_log();
    assert_eq!(feature(&rig, "remoteLog"), json!([]));
}

#[test]
fn a_long_result_is_cut_to_one_line() {
    let long = "word ".repeat(40);

    let clipped = clip(&long);

    assert_eq!(clipped.chars().count(), RESULT_LIMIT);
    assert!(clipped.ends_with('…'));
    assert_eq!(clip(" a \n\t b "), "a b");
}
