// The SOS job on a daemon with a fake phone, in paused time.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::future::BoxFuture;
use serde_json::{json, Value};
use tokio::runtime::Handle;
use uuid::Uuid;

use super::*;
use crate::ble::ValueListener;
use crate::daemon::device::{CallCommand, Device, DeviceAnswer, DeviceKind, Location};
use crate::daemon::link::Link;
use crate::daemon::settings::SettingsStore;
use crate::daemon::DaemonSetup;
use crate::protocol::app_message::AppMessage;
use crate::protocol::battletime::Capabilities;
use crate::protocol::link::WriteMode;

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

// ============================================================================
// A fake phone
// ============================================================================

struct FakePhone {
    kind: DeviceKind,
    location: Result<Location, String>,
    // Everything asked of the phone, in order, such as "call 555".
    asked: Mutex<Vec<String>>,
}

struct SharedPhone {
    phone: Arc<FakePhone>,
}

impl SharedPhone {
    fn note(&self, what: String) -> DeviceAnswer<()> {
        self.phone.asked.lock().unwrap().push(what);
        Box::pin(async { Ok(()) })
    }
}

impl Device for SharedPhone {
    fn kind(&self) -> DeviceKind {
        self.phone.kind
    }

    fn is_do_not_disturb(&self) -> BoxFuture<'static, bool> {
        Box::pin(async { false })
    }

    fn media_command(&self, _command: &str) -> DeviceAnswer<()> {
        Box::pin(async { Ok(()) })
    }

    fn call_command(&self, _command: CallCommand) -> DeviceAnswer<()> {
        Box::pin(async { Ok(()) })
    }

    fn locate(&self) -> DeviceAnswer<Location> {
        let location = self.phone.location.clone();
        Box::pin(async move { location })
    }

    fn read_file(&self, _uri: &str) -> DeviceAnswer<Vec<u8>> {
        Box::pin(async { Err("no files".to_string()) })
    }

    fn sos_screen(&self, on: bool, record: bool) -> DeviceAnswer<()> {
        self.note(format!("screen on={on} record={record}"))
    }

    fn call_number(&self, number: &str) -> DeviceAnswer<()> {
        self.note(format!("call {number}"))
    }

    fn dial_number(&self, number: &str) -> DeviceAnswer<()> {
        self.note(format!("dial {number}"))
    }

    fn send_text(&self, number: &str, text: &str) -> DeviceAnswer<()> {
        self.note(format!("text {number}: {text}"))
    }
}

// ============================================================================
// Setting up
// ============================================================================

struct Rig {
    daemon: Arc<Daemon>,
    sos: Arc<Sos>,
    phone: Arc<FakePhone>,
    // Every app message sent to the watch.
    sent: Arc<Mutex<Vec<AppMessage>>>,
    // Whether the next sends fail, as over a link that drops.
    sends_fail: Arc<Mutex<bool>>,
    _folder: tempfile::TempDir,
}

fn rig_on(kind: DeviceKind, location: Result<Location, String>, settings: Value) -> Rig {
    let folder = tempfile::tempdir().unwrap();
    SettingsStore::open(folder.path().join("companion").join("settings.json"))
        .update(&settings)
        .unwrap();
    let phone = Arc::new(FakePhone {
        kind,
        location,
        asked: Mutex::new(Vec::new()),
    });
    let daemon = Daemon::start_with_link(
        DaemonSetup {
            data_dir: folder.path().to_path_buf(),
            device: Arc::new(SharedPhone { phone: phone.clone() }),
            runtime: Handle::current(),
        },
        Arc::new(NoLink),
    );
    let sent = Arc::new(Mutex::new(Vec::new()));
    let sends_fail = Arc::new(Mutex::new(false));
    let (heard, failing) = (sent.clone(), sends_fail.clone());
    let send_message: MessageSender = Arc::new(move |message| {
        let fails = *failing.lock().unwrap();
        if !fails {
            heard.lock().unwrap().push(message);
        }
        Box::pin(async move {
            if fails {
                Err("the link dropped".to_string())
            } else {
                Ok(())
            }
        })
    });
    let sos = Sos::start_with(&daemon, send_message);
    Rig {
        daemon,
        sos,
        phone,
        sent,
        sends_fail,
        _folder: folder,
    }
}

fn here() -> Location {
    Location {
        latitude: 51.5,
        longitude: -0.125,
        name: "Here".into(),
    }
}

fn two_contacts() -> Value {
    json!({ "sosContacts": [{ "name": "Ann", "number": "555 0101" }, { "name": "", "number": "555 0202" }] })
}

fn rig(settings: Value) -> Rig {
    rig_on(DeviceKind::Phone, Ok(here()), settings)
}

fn watch_sends(rig: &Rig, on: i64) {
    let line = json!({ "t": "app", "ch": "sos", "d": { "on": on } }).to_string();
    rig.daemon.on_link_event("line", json!(format!("{line}\n")));
}

fn asked(rig: &Rig) -> Vec<String> {
    rig.phone.asked.lock().unwrap().clone()
}

fn calls_asked(rig: &Rig) -> Vec<String> {
    asked(rig)
        .into_iter()
        .filter(|what| what.starts_with("call "))
        .collect()
}

fn status(rig: &Rig) -> Value {
    rig.daemon.state()["features"]["sosStatus"].clone()
}

fn log_messages(rig: &Rig) -> Vec<String> {
    status(rig)["log"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["message"].as_str().unwrap().to_string())
        .collect()
}

// Says a watch with the message channel is connected, on a new connection.
fn connect(rig: &Rig) {
    rig.daemon.readiness.send_modify(|readiness| {
        readiness.epoch += 1;
        readiness.watch_id = Some("watch".to_string());
        readiness.capabilities = Some(Capabilities {
            uart: true,
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

fn sent(rig: &Rig) -> Vec<AppMessage> {
    rig.sent.lock().unwrap().clone()
}

fn stopped_on_watch() -> AppMessage {
    AppMessage::new("sos").with_number("on", 0)
}

async fn pass(duration: Duration) {
    tokio::time::sleep(duration).await;
}

async fn settle() {
    pass(Duration::from_millis(50)).await;
}

// ============================================================================
// Reading the watch and writing the text
// ============================================================================

#[test]
fn only_sos_lines_with_a_number_start_or_stop_it() {
    assert_eq!(
        sos_request(&AppMessage::new("sos").with_number("on", 1)),
        Some(true)
    );
    assert_eq!(sos_request(&stop_message()), Some(false));
    assert_eq!(sos_request(&AppMessage::new("sos")), None);
    assert_eq!(sos_request(&AppMessage::new("remote").with_number("on", 1)), None);
}

#[test]
fn the_text_carries_a_map_link_when_the_phone_knows_where_it_is() {
    assert!(
        sos_text(Some((51.5, -0.125))).ends_with("I am here: https://maps.google.com/?q=51.50000,-0.12500")
    );
    assert!(sos_text(None).ends_with("My phone could not find where I am."));
}

#[test]
fn settings_default_to_calling_the_contacts_and_doing_everything() {
    let settings = SosSettings::default();
    assert_eq!(settings.sos_call, SosCall::Contacts);
    assert_eq!(settings.sos_emergency_number, "911");
    assert!(settings.sos_text_location && settings.sos_record);
}

// ============================================================================
// An SOS
// ============================================================================

#[tokio::test(start_paused = true)]
async fn it_shows_the_screen_texts_everyone_and_calls_until_someone_answers() {
    let rig = rig(two_contacts());
    watch_sends(&rig, 1);
    settle().await;

    let text = sos_text(Some((51.5, -0.125)));
    let asked_now = asked(&rig);
    assert!(asked_now.contains(&"screen on=true record=true".to_string()));
    assert!(asked_now.contains(&format!("text 555 0101: {text}")));
    assert!(asked_now.contains(&format!("text 555 0202: {text}")));
    assert_eq!(calls_asked(&rig), ["call 555 0101"]);
    assert_eq!(status(&rig)["active"], true);

    rig.sos.on_call(CallState::Outgoing);
    pass(Duration::from_secs(10)).await;
    rig.sos.on_call(CallState::End);
    settle().await;
    assert_eq!(calls_asked(&rig), ["call 555 0101", "call 555 0202"]);
    assert!(log_messages(&rig).contains(&"Ann did not answer.".to_string()));

    rig.sos.on_call(CallState::Outgoing);
    pass(ANSWERED_AFTER).await;
    rig.sos.on_call(CallState::End);
    settle().await;
    assert_eq!(calls_asked(&rig).len(), 2);
    assert_eq!(
        log_messages(&rig)[0],
        "The call to 555 0202 ended after it was answered."
    );
}

#[tokio::test(start_paused = true)]
async fn nobody_answering_ends_after_one_round() {
    let rig = rig(two_contacts());
    watch_sends(&rig, 1);
    for _ in 0..2 {
        settle().await;
        rig.sos.on_call(CallState::Outgoing);
        pass(Duration::from_secs(5)).await;
        rig.sos.on_call(CallState::End);
    }
    settle().await;
    assert_eq!(calls_asked(&rig).len(), 2);
    assert_eq!(log_messages(&rig)[0], "Nobody answered.");
}

#[tokio::test(start_paused = true)]
async fn a_call_that_never_starts_moves_on_to_the_next_contact() {
    let rig = rig(two_contacts());
    watch_sends(&rig, 1);
    settle().await;
    pass(DIAL_TIMEOUT).await;
    settle().await;
    assert_eq!(calls_asked(&rig), ["call 555 0101", "call 555 0202"]);
    assert!(log_messages(&rig).contains(&"The call to Ann never started.".to_string()));
}

#[tokio::test(start_paused = true)]
async fn the_emergency_choice_only_opens_the_dialer() {
    let mut settings = two_contacts();
    settings["sosCall"] = json!("emergency");
    settings["sosEmergencyNumber"] = json!("112");
    let rig = rig(settings);
    watch_sends(&rig, 1);
    settle().await;
    assert!(asked(&rig).contains(&"dial 112".to_string()));
    assert!(calls_asked(&rig).is_empty());
}

#[tokio::test(start_paused = true)]
async fn the_switches_leave_out_the_texts_and_the_recording() {
    let mut settings = two_contacts();
    settings["sosTextLocation"] = json!(false);
    settings["sosRecord"] = json!(false);
    let rig = rig(settings);
    watch_sends(&rig, 1);
    settle().await;
    assert!(asked(&rig).contains(&"screen on=true record=false".to_string()));
    assert!(!asked(&rig).iter().any(|what| what.starts_with("text ")));
}

#[tokio::test(start_paused = true)]
async fn without_a_location_the_texts_still_go() {
    let rig = rig_on(DeviceKind::Phone, Err("no fix".into()), two_contacts());
    watch_sends(&rig, 1);
    settle().await;
    assert!(asked(&rig).contains(&format!("text 555 0101: {}", sos_text(None))));
}

#[tokio::test(start_paused = true)]
async fn stopping_closes_the_screen_and_calls_nobody_else() {
    let rig = rig(two_contacts());
    watch_sends(&rig, 1);
    settle().await;
    rig.sos.on_call(CallState::Outgoing);
    watch_sends(&rig, 0);
    settle().await;
    rig.sos.on_call(CallState::End);
    pass(DIAL_TIMEOUT).await;
    assert_eq!(calls_asked(&rig), ["call 555 0101"]);
    assert!(asked(&rig).contains(&"screen on=false record=false".to_string()));
    assert_eq!(status(&rig)["active"], false);
}

#[tokio::test(start_paused = true)]
async fn a_second_start_while_on_changes_nothing() {
    let rig = rig(two_contacts());
    watch_sends(&rig, 1);
    settle().await;
    watch_sends(&rig, 1);
    settle().await;
    assert_eq!(calls_asked(&rig).len(), 1);
    assert_eq!(
        log_messages(&rig)[0],
        "The watch asked again; the SOS is already on."
    );
}

#[tokio::test(start_paused = true)]
async fn no_contacts_says_nobody_was_called() {
    let rig = rig(json!({}));
    watch_sends(&rig, 1);
    settle().await;
    assert!(log_messages(&rig).contains(&"No emergency contacts are set, so nobody was called.".to_string()));
}

#[tokio::test(start_paused = true)]
async fn a_computer_does_nothing_but_say_it_needs_a_phone() {
    let rig = rig_on(DeviceKind::Computer, Ok(here()), two_contacts());
    watch_sends(&rig, 1);
    settle().await;
    assert!(asked(&rig).is_empty());
    assert_eq!(status(&rig)["active"], false);
    assert_eq!(
        log_messages(&rig),
        ["SOS needs the companion on an Android phone."]
    );
}

// ============================================================================
// Telling the watch the phone stopped it
// ============================================================================

#[tokio::test(start_paused = true)]
async fn stopping_on_the_phone_tells_the_watch_app_to_stop() {
    let rig = rig(two_contacts());
    connect(&rig);
    watch_sends(&rig, 1);
    settle().await;
    rig.sos.end_from_phone();
    settle().await;
    assert_eq!(sent(&rig), [stopped_on_watch()]);
    assert_eq!(log_messages(&rig)[0], "Told the watch to stop signalling.");
    assert!(asked(&rig).contains(&"screen on=false record=false".to_string()));
}

#[tokio::test(start_paused = true)]
async fn stopping_on_the_watch_sends_nothing_back() {
    let rig = rig(two_contacts());
    connect(&rig);
    watch_sends(&rig, 1);
    settle().await;
    watch_sends(&rig, 0);
    settle().await;
    assert!(sent(&rig).is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_watch_that_is_away_is_told_when_it_connects() {
    let rig = rig(two_contacts());
    watch_sends(&rig, 1);
    settle().await;
    rig.sos.end_from_phone();
    settle().await;
    assert!(sent(&rig).is_empty());
    assert!(log_messages(&rig)[0].starts_with("The watch is away"));

    connect(&rig);
    settle().await;
    assert_eq!(sent(&rig), [stopped_on_watch()]);
    disconnect(&rig);
    connect(&rig);
    settle().await;
    assert_eq!(sent(&rig).len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_failed_telling_is_tried_again_on_the_next_connection() {
    let rig = rig(two_contacts());
    connect(&rig);
    *rig.sends_fail.lock().unwrap() = true;
    rig.sos.end_from_phone();
    settle().await;
    assert!(log_messages(&rig)[0].starts_with("Could not tell the watch to stop"));

    *rig.sends_fail.lock().unwrap() = false;
    disconnect(&rig);
    connect(&rig);
    settle().await;
    assert_eq!(sent(&rig), [stopped_on_watch()]);
}

#[tokio::test(start_paused = true)]
async fn a_new_sos_from_the_watch_cancels_an_untold_stop() {
    let rig = rig(two_contacts());
    rig.sos.end_from_phone();
    watch_sends(&rig, 1);
    settle().await;
    connect(&rig);
    settle().await;
    assert!(sent(&rig).is_empty());
}
