// The daemon against a fake link and a fake device, in paused time.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

use futures::future::BoxFuture;
use serde_json::{json, Value};
use uuid::Uuid;

use super::device::{CallCommand, Device, DeviceAnswer, DeviceKind, Location, Media, PhoneEvent};
use super::link::Link;
use super::*;
use crate::ble::{ValueListener, NUS_RX, NUS_SERVICE};
use crate::protocol::battletime::{BATTERY_LEVEL, BATTERY_SERVICE, CURRENT_TIME, CURRENT_TIME_SERVICE};
use crate::protocol::link::WriteMode;
use crate::protocol::notes::{NOTE_FONT_PATH, NOTE_PATH};
use crate::sync::files::fake::FakeWatch;
use crate::web_request::WebReply;
use notes::{NoteKeeper, WebRequester, NOTE_CHECK_INTERVAL};

// ============================================================================
// A fake link
// ============================================================================

#[derive(Default)]
pub(super) struct FakeLink {
    pub(super) daemon: OnceLock<Weak<Daemon>>,
    pub(super) served: Mutex<Vec<(Uuid, Uuid)>>,
    pub(super) values: Mutex<HashMap<Uuid, Vec<u8>>>,
    pub(super) connects: Mutex<Vec<(String, tokio::time::Instant)>>,
    // How many connects fail before one succeeds.
    pub(super) failing_connects: Mutex<u32>,
    pub(super) lines: Mutex<Vec<String>>,
    pub(super) gatt_writes: Mutex<Vec<Uuid>>,
    // Answers to send back for a line containing the key.
    pub(super) answers: Mutex<Vec<(String, String)>>,
    // The watch's files, when it serves the file service.
    pub(super) watch_files: OnceLock<Arc<FakeWatch>>,
}

impl FakeLink {
    fn daemon(&self) -> Arc<Daemon> {
        self.daemon.get().unwrap().upgrade().unwrap()
    }

    fn serve(&self, service: Uuid, characteristic: Uuid) {
        self.served.lock().unwrap().push((service, characteristic));
    }

    pub(super) fn sent(&self) -> Vec<Value> {
        self.lines
            .lock()
            .unwrap()
            .iter()
            .filter_map(|line| {
                line.strip_prefix("GB(")?
                    .trim_end()
                    .strip_suffix(')')
                    .map(str::to_string)
            })
            .filter_map(|json| serde_json::from_str(&json).ok())
            .collect()
    }

    pub(super) fn sent_of(&self, kind: &str) -> Vec<Value> {
        self.sent()
            .into_iter()
            .filter(|message| message["t"] == kind)
            .collect()
    }
}

struct SharedFake(Arc<FakeLink>);

impl Link for SharedFake {
    fn connect(&self, address: String) -> BoxFuture<'static, Result<(), String>> {
        let fake = self.0.clone();
        Box::pin(async move {
            fake.connects
                .lock()
                .unwrap()
                .push((address, tokio::time::Instant::now()));
            let daemon = fake.daemon();
            daemon.on_link_event("state", json!("connecting"));
            {
                let mut failing = fake.failing_connects.lock().unwrap();
                if *failing > 0 {
                    *failing -= 1;
                    drop(failing);
                    daemon.on_link_event("state", json!("disconnected"));
                    return Err("No watch in range.".into());
                }
            }
            daemon.on_link_event("mode", json!("application"));
            daemon.on_link_event("state", json!("connected"));
            Ok(())
        })
    }

    fn disconnect(&self) -> BoxFuture<'static, Result<(), String>> {
        let fake = self.0.clone();
        Box::pin(async move {
            fake.daemon().on_link_event("state", json!("disconnected"));
            Ok(())
        })
    }

    fn write_text(&self, text: String) -> BoxFuture<'static, Result<(), String>> {
        let fake = self.0.clone();
        Box::pin(async move {
            fake.lines.lock().unwrap().push(text.clone());
            let answers: Vec<String> = fake
                .answers
                .lock()
                .unwrap()
                .iter()
                .filter(|(key, _)| text.contains(key.as_str()))
                .map(|(_, answer)| answer.clone())
                .collect();
            for answer in answers {
                fake.daemon().on_link_event("line", json!(answer));
            }
            Ok(())
        })
    }

    fn has_characteristic(&self, service: Uuid, characteristic: Uuid) -> bool {
        self.0.served.lock().unwrap().contains(&(service, characteristic))
    }

    fn read(&self, _service: Uuid, characteristic: Uuid) -> BoxFuture<'static, Result<Vec<u8>, String>> {
        let value = self.0.values.lock().unwrap().get(&characteristic).cloned();
        Box::pin(async move { value.ok_or_else(|| "not served".to_string()) })
    }

    fn write(
        &self,
        _service: Uuid,
        characteristic: Uuid,
        data: Vec<u8>,
        _mode: WriteMode,
    ) -> BoxFuture<'static, Result<(), String>> {
        self.0.gatt_writes.lock().unwrap().push(characteristic);
        if let Some(watch) = self
            .0
            .watch_files
            .get()
            .filter(|_| characteristic == FILE_TRANSFER)
        {
            for answer in watch.answers(&data) {
                self.0
                    .daemon()
                    .on_link_event("notify", json!({ "key": file_service_key(), "value": answer }));
            }
        }
        Box::pin(async { Ok(()) })
    }

    fn listen(
        &self,
        _service: Uuid,
        _characteristic: Uuid,
        _listener: ValueListener,
    ) -> BoxFuture<'static, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }

    fn subscribe(&self, _service: Uuid, _characteristic: Uuid) -> BoxFuture<'static, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
}

// ============================================================================
// A fake device
// ============================================================================

#[derive(Default)]
pub(super) struct FakeDevice {
    pub(super) quiet: Mutex<bool>,
    pub(super) media_commands: Mutex<Vec<String>>,
    pub(super) call_commands: Mutex<Vec<CallCommand>>,
    pub(super) kept_running: Mutex<Vec<Option<String>>>,
    pub(super) ringing: Mutex<Vec<bool>>,
    // Where locate() says the device is; None makes it fail.
    pub(super) location: Mutex<Option<Location>>,
    pub(super) picked_files: Mutex<HashMap<String, Vec<u8>>>,
}

struct SharedDevice(Arc<FakeDevice>);

impl Device for SharedDevice {
    fn kind(&self) -> DeviceKind {
        DeviceKind::Phone
    }

    fn is_do_not_disturb(&self) -> BoxFuture<'static, bool> {
        let quiet = *self.0.quiet.lock().unwrap();
        Box::pin(async move { quiet })
    }

    fn media_command(&self, command: &str) -> DeviceAnswer<()> {
        self.0.media_commands.lock().unwrap().push(command.to_string());
        Box::pin(async { Ok(()) })
    }

    fn call_command(&self, command: CallCommand) -> DeviceAnswer<()> {
        self.0.call_commands.lock().unwrap().push(command);
        Box::pin(async { Ok(()) })
    }

    fn locate(&self) -> DeviceAnswer<Location> {
        let location = self.0.location.lock().unwrap().clone();
        Box::pin(async move { location.ok_or_else(|| "nowhere".to_string()) })
    }

    fn read_file(&self, uri: &str) -> DeviceAnswer<Vec<u8>> {
        let contents = self.0.picked_files.lock().unwrap().get(uri).cloned();
        Box::pin(async move { contents.ok_or_else(|| "no such file".to_string()) })
    }

    fn keep_running(&self, watch_name: Option<&str>) {
        self.0
            .kept_running
            .lock()
            .unwrap()
            .push(watch_name.map(str::to_string));
    }

    fn find_phone(&self, ringing: bool) {
        self.0.ringing.lock().unwrap().push(ringing);
    }
}

// ============================================================================
// Setting up
// ============================================================================

pub(super) struct Rig {
    pub(super) daemon: Arc<Daemon>,
    pub(super) link: Arc<FakeLink>,
    pub(super) device: Arc<FakeDevice>,
    pub(super) events: Arc<Mutex<Vec<(String, Value)>>>,
    pub(super) _folder: tempfile::TempDir,
}

pub(super) fn rig_with(settings: Value, prepare: impl FnOnce(&FakeLink)) -> Rig {
    let folder = tempfile::tempdir().unwrap();
    if settings != Value::Null {
        SettingsStore::open(folder.path().join("companion").join("settings.json"))
            .update(&settings)
            .unwrap();
    }
    let link = Arc::new(FakeLink::default());
    prepare(&link);
    let device = Arc::new(FakeDevice::default());
    let setup = DaemonSetup {
        data_dir: folder.path().to_path_buf(),
        device: Arc::new(SharedDevice(device.clone())),
        runtime: Handle::current(),
    };
    let daemon = Daemon::start_with_link(setup, Arc::new(SharedFake(link.clone())));
    link.daemon.set(Arc::downgrade(&daemon)).ok();
    let events = Arc::new(Mutex::new(Vec::new()));
    let heard = events.clone();
    daemon.set_host(Arc::new(move |name, payload| {
        heard.lock().unwrap().push((name.to_string(), payload))
    }));
    Rig {
        daemon,
        link,
        device,
        events,
        _folder: folder,
    }
}

pub(super) fn a_battletime_watch(link: &FakeLink) {
    link.serve(NUS_SERVICE, NUS_RX);
    link.serve(CURRENT_TIME_SERVICE, CURRENT_TIME);
    link.answers.lock().unwrap().push((
        "\"t\":\"status\"".into(),
        "{\"t\":\"status\",\"bat\":81,\"mv\":3900,\"charging\":false,\"steps\":1200}\n".into(),
    ));
}

pub(super) async fn pass(duration: Duration) {
    tokio::time::sleep(duration).await;
}

// ============================================================================
// Connecting and keeping the watch
// ============================================================================

#[tokio::test(start_paused = true)]
async fn connecting_remembers_the_watch_sets_its_clock_and_reads_its_status() {
    let rig = rig_with(Value::Null, a_battletime_watch);

    rig.daemon.connect("AA:BB", "BattleTime").await.unwrap();
    pass(Duration::from_secs(3)).await;

    let settings = rig.daemon.settings_json();
    assert_eq!(settings["lastWatchId"], "AA:BB");
    assert_eq!(settings["lastWatchName"], "BattleTime");
    assert!(rig.link.gatt_writes.lock().unwrap().contains(&CURRENT_TIME));
    let state = rig.daemon.state();
    assert_eq!(state["connection"], "connected");
    assert_eq!(state["capabilities"]["uart"], true);
    assert_eq!(state["status"]["bat"], 81);
    assert_eq!(state["watch"]["name"], "BattleTime");
    assert!(rig.daemon.readiness().borrow().uart());
}

#[tokio::test(start_paused = true)]
async fn a_watch_without_the_uart_reports_its_battery_level() {
    let rig = rig_with(Value::Null, |link| {
        link.serve(BATTERY_SERVICE, BATTERY_LEVEL);
        link.values.lock().unwrap().insert(BATTERY_LEVEL, vec![64]);
    });

    rig.daemon.connect("AA", "InfiniTime").await.unwrap();
    pass(Duration::from_secs(3)).await;

    assert_eq!(rig.daemon.state()["batteryLevel"], 64);
    assert!(rig.link.sent_of("status").is_empty());
}

#[tokio::test(start_paused = true)]
async fn the_remembered_watch_is_connected_at_start_and_again_after_it_drops() {
    let rig = rig_with(
        json!({ "lastWatchId": "AA", "lastWatchName": "BattleTime" }),
        a_battletime_watch,
    );

    pass(Duration::from_secs(3)).await;
    assert_eq!(rig.link.connects.lock().unwrap().len(), 1);
    assert_eq!(rig.daemon.state()["connection"], "connected");

    rig.daemon.on_link_event("state", json!("disconnected"));
    assert_eq!(rig.daemon.state()["reconnecting"], true);
    pass(Duration::from_secs(3)).await;

    assert_eq!(rig.link.connects.lock().unwrap().len(), 2);
    assert_eq!(rig.daemon.state()["reconnecting"], false);
}

#[tokio::test(start_paused = true)]
async fn failed_tries_wait_longer_each_time() {
    let rig = rig_with(json!({ "lastWatchId": "AA" }), |link| {
        *link.failing_connects.lock().unwrap() = 3
    });

    pass(Duration::from_secs(60)).await;

    let connects = rig.link.connects.lock().unwrap().clone();
    let gaps: Vec<u64> = connects
        .windows(2)
        .map(|pair| (pair[1].1 - pair[0].1).as_secs())
        .collect();
    assert_eq!(gaps, vec![5, 10, 20]);
    assert_eq!(rig.daemon.state()["connection"], "connected");
}

#[tokio::test(start_paused = true)]
async fn a_user_disconnect_keeps_the_watch_away_until_they_connect_again() {
    let rig = rig_with(json!({ "lastWatchId": "AA" }), a_battletime_watch);
    pass(Duration::from_secs(3)).await;

    rig.daemon.disconnect().await.unwrap();
    pass(Duration::from_secs(600)).await;
    assert_eq!(rig.link.connects.lock().unwrap().len(), 1);
    assert_eq!(rig.daemon.state()["reconnecting"], false);

    rig.daemon.connect("AA", "BattleTime").await.unwrap();
    rig.daemon.on_link_event("state", json!("disconnected"));
    pass(Duration::from_secs(3)).await;
    assert_eq!(rig.link.connects.lock().unwrap().len(), 3);
}

#[tokio::test(start_paused = true)]
async fn an_update_or_a_scan_holds_the_keeper_back() {
    let rig = rig_with(json!({ "lastWatchId": "AA" }), |link| {
        *link.failing_connects.lock().unwrap() = 100
    });
    rig.daemon.hold(true);
    pass(Duration::from_secs(60)).await;
    let held = rig.link.connects.lock().unwrap().len();

    rig.daemon.hold(false);
    rig.daemon.set_scanning(true);
    pass(Duration::from_secs(60)).await;
    let scanning = rig.link.connects.lock().unwrap().len();

    rig.daemon.set_scanning(false);
    pass(Duration::from_secs(3)).await;

    assert!(held <= 1);
    assert_eq!(scanning, held);
    assert!(rig.link.connects.lock().unwrap().len() > scanning);
}

#[tokio::test(start_paused = true)]
async fn the_device_stays_running_while_a_watch_is_kept() {
    let rig = rig_with(
        json!({ "lastWatchId": "AA", "lastWatchName": "Pine" }),
        a_battletime_watch,
    );
    pass(Duration::from_secs(3)).await;

    rig.daemon
        .update_settings(&json!({ "autoReconnect": false }))
        .unwrap();
    rig.daemon.disconnect().await.unwrap();
    pass(Duration::from_secs(1)).await;

    let kept = rig.device.kept_running.lock().unwrap().clone();
    assert_eq!(kept.first(), Some(&Some("Pine".to_string())));
    assert_eq!(kept.last(), Some(&None));
}

#[tokio::test(start_paused = true)]
async fn a_host_can_ask_at_once_which_watch_keeps_it_running() {
    let remembering = rig_with(json!({ "lastWatchId": "AA", "lastWatchName": "Pine" }), |_| {});
    let forgetting = rig_with(json!({ "lastWatchId": "AA", "autoReconnect": false }), |_| {});

    assert_eq!(remembering.daemon.kept_running_for(), Some("Pine".to_string()));
    assert_eq!(forgetting.daemon.kept_running_for(), None);
}

// ============================================================================
// The watch's messages
// ============================================================================

#[tokio::test(start_paused = true)]
async fn the_watch_can_ring_the_phone_control_music_and_answer_calls() {
    let rig = rig_with(Value::Null, a_battletime_watch);
    rig.daemon.connect("AA", "BattleTime").await.unwrap();
    pass(Duration::from_secs(1)).await;

    rig.daemon
        .on_link_event("line", json!("{\"t\":\"findPhone\",\"n\":\"true\"}\n"));
    rig.daemon.on_link_event(
        "line",
        json!("{\"t\":\"music\",\"n\":\"next\"}\n{\"t\":\"call\",\"n\":\"REJECT\"}\n"),
    );
    rig.daemon
        .on_link_event("line", json!("{\"t\":\"error\",\"msg\":\"bad\"}\n"));
    pass(Duration::from_millis(10)).await;

    assert_eq!(*rig.device.ringing.lock().unwrap(), vec![true]);
    assert_eq!(rig.daemon.state()["findPhone"], true);
    assert_eq!(
        *rig.device.media_commands.lock().unwrap(),
        vec!["next".to_string()]
    );
    assert_eq!(
        *rig.device.call_commands.lock().unwrap(),
        vec![CallCommand::Reject]
    );
    assert_eq!(rig.daemon.state()["lastError"], "bad");

    rig.daemon.stop_finding_phone();
    assert_eq!(rig.daemon.state()["findPhone"], false);
}

#[tokio::test(start_paused = true)]
async fn a_watch_app_asking_for_the_music_gets_what_plays() {
    let rig = rig_with(Value::Null, a_battletime_watch);
    rig.daemon.connect("AA", "BattleTime").await.unwrap();
    pass(Duration::from_secs(1)).await;
    rig.daemon.phone_event(PhoneEvent::Media(Media {
        playing: true,
        track: Some("Song".into()),
        ..Media::default()
    }));
    pass(Duration::from_millis(10)).await;
    rig.link.lines.lock().unwrap().clear();

    rig.daemon
        .on_link_event("line", json!("{\"t\":\"request\",\"n\":\"music\"}\n"));
    pass(Duration::from_millis(10)).await;

    assert_eq!(rig.link.sent_of("musicinfo")[0]["track"], "Song");
    assert_eq!(rig.daemon.state()["nowPlaying"]["track"], "Song");
}

#[tokio::test(start_paused = true)]
async fn messages_no_request_took_reach_the_background_jobs() {
    let rig = rig_with(Value::Null, a_battletime_watch);
    let mut messages = rig.daemon.watch_messages();

    rig.daemon.on_link_event(
        "line",
        json!("{\"t\":\"app\",\"ch\":\"remote\",\"d\":{\"button\":2}}\n"),
    );

    assert_eq!(messages.recv().await.unwrap()["ch"], "remote");
}

#[tokio::test(start_paused = true)]
async fn app_messages_reach_their_topic_and_go_out_only_to_a_connected_watch() {
    use crate::protocol::app_message::AppMessage;
    let rig = rig_with(Value::Null, a_battletime_watch);
    let mut heard = rig.daemon.subscribe_topic("demo");

    rig.daemon.on_link_event(
        "line",
        json!("{\"t\":\"app\",\"ch\":\"other\",\"d\":{}}\n{\"t\":\"app\",\"ch\":\"demo\",\"d\":{\"n\":4}}\n"),
    );
    assert_eq!(
        heard.recv().await.unwrap(),
        AppMessage::new("demo").with_number("n", 4)
    );

    let message = AppMessage::new("demo").with_text("note", "hi");
    assert!(rig
        .daemon
        .send_app_message(&message)
        .await
        .unwrap_err()
        .contains("Connect to a watch"));
    rig.daemon.connect("AA:BB", "BattleTime").await.unwrap();
    pass(Duration::from_secs(3)).await;
    rig.daemon.send_app_message(&message).await.unwrap();
    assert_eq!(
        rig.link.sent_of("app"),
        [json!({ "t": "app", "ch": "demo", "d": { "note": "hi" } })]
    );
    assert!(rig
        .daemon
        .send_app_message(&AppMessage::new("bad topic"))
        .await
        .unwrap_err()
        .contains("is not a topic"));
}

// ============================================================================
// The device's events
// ============================================================================

#[tokio::test(start_paused = true)]
async fn notifications_wait_for_the_watch_and_respect_do_not_disturb() {
    let rig = rig_with(Value::Null, a_battletime_watch);
    let notification = |id| PhoneEvent::Notification {
        id,
        app: "Mail".into(),
        title: "Hi".into(),
        body: "".into(),
    };

    rig.daemon.phone_event(notification(1));
    pass(Duration::from_millis(10)).await;
    rig.daemon.connect("AA", "BattleTime").await.unwrap();
    pass(Duration::from_secs(1)).await;
    rig.daemon.phone_event(notification(2));
    pass(Duration::from_millis(10)).await;
    *rig.device.quiet.lock().unwrap() = true;
    rig.daemon.phone_event(notification(3));
    pass(Duration::from_millis(10)).await;

    let sent: Vec<Value> = rig
        .link
        .sent_of("notify")
        .iter()
        .map(|message| message["id"].clone())
        .collect();
    assert_eq!(sent, vec![json!(2)]);
}

// ============================================================================
// Settings and the window
// ============================================================================

#[tokio::test(start_paused = true)]
async fn the_windows_old_settings_are_taken_only_once() {
    let rig = rig_with(Value::Null, |_| {});

    assert_eq!(
        rig.daemon.import_settings(&json!({ "forwardMusic": false })),
        Ok(true)
    );
    assert_eq!(
        rig.daemon.import_settings(&json!({ "forwardMusic": true })),
        Ok(false)
    );

    assert_eq!(rig.daemon.settings_json()["forwardMusic"], false);
    assert!(rig
        .events
        .lock()
        .unwrap()
        .iter()
        .any(|(name, _)| name == "daemon:settings"));
}

#[tokio::test(start_paused = true)]
async fn the_sync_interval_follows_the_companions_setting() {
    let rig = rig_with(Value::Null, |_| {});

    rig.daemon
        .update_settings(&json!({ "syncIntervalSeconds": 900 }))
        .unwrap();
    assert_eq!(rig.daemon.syncer().settings().interval_seconds, 900);

    rig.daemon
        .update_settings(&json!({ "syncIntervalSeconds": 5 }))
        .unwrap();
    assert_eq!(rig.daemon.syncer().settings().interval_seconds, 300);
}

#[tokio::test(start_paused = true)]
async fn file_answers_go_to_the_window_only_while_it_holds_the_service() {
    let rig = rig_with(Value::Null, |_| {});
    let answer = json!({ "key": file_service_key(), "value": [0x31, 1] });
    let heard = |rig: &Rig| {
        rig.events
            .lock()
            .unwrap()
            .iter()
            .filter(|(name, _)| name == "ble:event")
            .count()
    };

    rig.daemon.on_link_event("notify", answer.clone());
    assert_eq!(heard(&rig), 0);

    let turn = rig.daemon.syncer().files().lease().await;
    let number = rig.daemon.lease().hold(turn);
    rig.daemon.on_link_event("notify", answer);
    assert_eq!(heard(&rig), 1);
    rig.daemon.lease().release(Some(number));
}

#[tokio::test(start_paused = true)]
async fn the_window_hears_state_changes_and_what_the_daemon_sends() {
    let rig = rig_with(Value::Null, a_battletime_watch);

    rig.daemon.connect("AA", "BattleTime").await.unwrap();
    pass(Duration::from_secs(3)).await;

    let events = rig.events.lock().unwrap();
    assert!(events
        .iter()
        .any(|(name, payload)| name == "daemon:state" && payload["connection"] == "connected"));
    assert!(events
        .iter()
        .any(|(name, payload)| name == "daemon:sent" && payload == "GB({\"t\":\"status\"})"));
    assert!(events
        .iter()
        .any(|(name, payload)| name == "ble:event" && payload["name"] == "state"));
}

// ============================================================================
// The watch's note
// ============================================================================

fn a_watch_with_files(link: &FakeLink) {
    a_battletime_watch(link);
    link.serve(FILE_TRANSFER_SERVICE, FILE_TRANSFER);
    link.watch_files.set(FakeWatch::with(&[])).ok();
}

fn note_on(rig: &Rig) -> Option<Vec<u8>> {
    rig.link
        .watch_files
        .get()
        .unwrap()
        .files
        .lock()
        .unwrap()
        .get(NOTE_PATH)
        .cloned()
}

fn note_status(rig: &Rig) -> Value {
    rig.daemon.state()["features"]["noteStatus"].clone()
}

fn no_web() -> WebRequester {
    Arc::new(|_| Box::pin(async { Err("offline".to_string()) }))
}

#[tokio::test(start_paused = true)]
async fn a_typed_note_is_sent_on_connecting_and_remembered() {
    let rig = rig_with(
        json!({ "noteText": "hello", "noteFont": "large" }),
        a_watch_with_files,
    );
    let _keeper = NoteKeeper::start_with_requester(&rig.daemon, no_web());
    assert_eq!(note_status(&rig), json!({ "kind": "idle" }));

    rig.daemon.connect("AA", "BattleTime").await.unwrap();
    pass(Duration::from_secs(3)).await;

    assert_eq!(note_on(&rig), Some(b"hello".to_vec()));
    let files = rig.link.watch_files.get().unwrap().files.lock().unwrap().clone();
    assert_eq!(files[NOTE_FONT_PATH], b"large");
    assert_eq!(rig.daemon.settings_json()["noteOnWatch"]["watchId"], "AA");
    assert_eq!(rig.daemon.settings_json()["noteOnWatch"]["font"], "large");
    assert_eq!(note_status(&rig)["kind"], "synced");
    assert_eq!(note_status(&rig)["bytes"], 5);
}

#[tokio::test(start_paused = true)]
async fn changing_the_text_or_font_sends_the_note_again() {
    let rig = rig_with(json!({ "noteText": "hello" }), a_watch_with_files);
    let _keeper = NoteKeeper::start_with_requester(&rig.daemon, no_web());
    rig.daemon.connect("AA", "BattleTime").await.unwrap();
    pass(Duration::from_secs(3)).await;

    rig.daemon
        .update_settings(&json!({ "noteText": "goodbye" }))
        .unwrap();
    pass(Duration::from_secs(1)).await;
    assert_eq!(note_on(&rig), Some(b"goodbye".to_vec()));

    rig.daemon
        .update_settings(&json!({ "noteFont": "small" }))
        .unwrap();
    pass(Duration::from_secs(1)).await;
    let files = rig.link.watch_files.get().unwrap().files.lock().unwrap().clone();
    assert_eq!(files[NOTE_FONT_PATH], b"small");
}

#[tokio::test(start_paused = true)]
async fn a_note_already_on_the_watch_is_not_sent_again() {
    let rig = rig_with(json!({ "noteText": "hello" }), a_watch_with_files);
    let keeper = NoteKeeper::start_with_requester(&rig.daemon, no_web());
    rig.daemon.connect("AA", "BattleTime").await.unwrap();
    pass(Duration::from_secs(3)).await;
    let writes = rig.link.gatt_writes.lock().unwrap().len();

    keeper.sync_now().await;

    assert_eq!(rig.link.gatt_writes.lock().unwrap().len(), writes);
    assert_eq!(note_status(&rig)["kind"], "synced");
}

#[tokio::test(start_paused = true)]
async fn a_picked_file_is_read_again_every_minute_and_when_it_is_saved() {
    let rig = rig_with(
        json!({ "noteSource": "file", "noteFile": { "name": "todo.txt", "uri": "/home/todo.txt" } }),
        a_watch_with_files,
    );
    let picked = |contents: &[u8]| {
        rig.device
            .picked_files
            .lock()
            .unwrap()
            .insert("/home/todo.txt".into(), contents.to_vec())
    };
    picked(b"one");
    let keeper = NoteKeeper::start_with_requester(&rig.daemon, no_web());
    rig.daemon.connect("AA", "BattleTime").await.unwrap();
    pass(Duration::from_secs(3)).await;
    assert_eq!(note_on(&rig), Some(b"one".to_vec()));

    picked(b"two");
    pass(Duration::from_secs(30)).await;
    assert_eq!(note_on(&rig), Some(b"one".to_vec()));
    pass(NOTE_CHECK_INTERVAL).await;
    assert_eq!(note_on(&rig), Some(b"two".to_vec()));

    picked(b"three");
    keeper.file_changed();
    pass(Duration::from_secs(1)).await;
    assert_eq!(note_on(&rig), Some(b"three".to_vec()));
}

#[tokio::test(start_paused = true)]
async fn a_url_note_is_fetched_and_a_failure_is_reported() {
    let rig = rig_with(
        json!({ "noteSource": "url", "noteUrl": "https://example.org/note", "noteUrlAccess": { "headers": { "X-Key": "k" } } }),
        a_watch_with_files,
    );
    let status = Arc::new(Mutex::new(200u16));
    let answer = status.clone();
    let requester: WebRequester = Arc::new(move |request| {
        let status = *answer.lock().unwrap();
        Box::pin(async move {
            assert_eq!(request.headers["X-Key"], "k");
            Ok(WebReply {
                status,
                status_text: "Not Found".into(),
                body: b"fetched".to_vec(),
            })
        })
    });
    let keeper = NoteKeeper::start_with_requester(&rig.daemon, requester);
    rig.daemon.connect("AA", "BattleTime").await.unwrap();
    pass(Duration::from_secs(3)).await;
    assert_eq!(note_on(&rig), Some(b"fetched".to_vec()));

    *status.lock().unwrap() = 404;
    keeper.sync_now().await;

    let failed = note_status(&rig);
    assert_eq!(failed["kind"], "failed");
    assert_eq!(
        failed["message"],
        "https://example.org/note answered 404 Not Found."
    );
    assert!(failed["at"].is_i64());
}

#[tokio::test(start_paused = true)]
async fn forgetting_the_note_clears_its_source_and_takes_it_off_the_watch() {
    let rig = rig_with(json!({ "noteText": "hello" }), a_watch_with_files);
    let keeper = NoteKeeper::start_with_requester(&rig.daemon, no_web());
    rig.daemon.connect("AA", "BattleTime").await.unwrap();
    pass(Duration::from_secs(3)).await;

    keeper.forget().await.unwrap();
    pass(NOTE_CHECK_INTERVAL * 2).await;

    assert_eq!(note_on(&rig), None);
    let settings = rig.daemon.settings_json();
    assert_eq!(settings["noteText"], "");
    assert_eq!(settings["noteOnWatch"], Value::Null);
    assert_eq!(note_status(&rig), json!({ "kind": "idle" }));
}

#[tokio::test(start_paused = true)]
async fn no_note_is_sent_to_a_watch_without_the_file_service() {
    let rig = rig_with(json!({ "noteText": "hello" }), a_battletime_watch);
    let keeper = NoteKeeper::start_with_requester(&rig.daemon, no_web());
    rig.daemon.connect("AA", "BattleTime").await.unwrap();
    pass(Duration::from_secs(3)).await;

    keeper.sync_now().await;

    assert!(!rig.link.gatt_writes.lock().unwrap().contains(&FILE_TRANSFER));
    assert_eq!(note_status(&rig), json!({ "kind": "idle" }));
    assert_eq!(rig.daemon.settings_json().get("noteOnWatch"), None);
}

#[tokio::test(start_paused = true)]
async fn a_new_connection_lists_the_watch_and_restores_a_removed_note() {
    let rig = rig_with(json!({ "noteText": "hello" }), a_watch_with_files);
    let _keeper = NoteKeeper::start_with_requester(&rig.daemon, no_web());
    rig.daemon.connect("AA", "BattleTime").await.unwrap();
    pass(Duration::from_secs(3)).await;

    rig.link
        .watch_files
        .get()
        .unwrap()
        .files
        .lock()
        .unwrap()
        .remove(NOTE_PATH);
    rig.daemon.on_link_event("state", json!("disconnected"));
    rig.daemon.connect("AA", "BattleTime").await.unwrap();
    pass(Duration::from_secs(3)).await;

    assert_eq!(note_on(&rig), Some(b"hello".to_vec()));
}
