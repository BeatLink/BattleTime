// The phone as the daemon's device: each thing the daemon asks of it is a request to the phone bridge's DaemonHost.kt.
//
// A request is an action name and a JSON argument, answered with JSON or a reason it failed; android.rs
// carries them over JNI. Each runs on a blocking thread with a time limit, so a stuck phone never holds
// the daemon up.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use battletime_companion_core::daemon::device::{CallCommand, Device, DeviceAnswer, DeviceKind, Location};
use futures::future::BoxFuture;
use serde_json::{json, Value};

// ============================================================================
// Timing
// ============================================================================

// How long a quick request, such as a media key, may take.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);

// A location fix can take a while; DaemonHost.kt gives up after 30 seconds.
const LOCATE_TIMEOUT: Duration = Duration::from_secs(40);

const READ_FILE_TIMEOUT: Duration = Duration::from_secs(30);

// ============================================================================
// The device
// ============================================================================

// Sends one request to the phone and waits for its answer.
pub type AskPhone = Arc<dyn Fn(&str, &Value) -> Result<Value, String> + Send + Sync>;

pub struct PhoneDevice {
    ask: AskPhone,
    // Requests nobody waits for, sent one at a time so a later one never overtakes an earlier.
    told: Mutex<mpsc::Sender<(&'static str, Value)>>,
}

impl PhoneDevice {
    pub fn new(ask: AskPhone) -> PhoneDevice {
        let (told, queue) = mpsc::channel::<(&'static str, Value)>();
        let teller = ask.clone();
        std::thread::spawn(move || {
            for (action, argument) in queue {
                let _ = teller(action, &argument);
            }
        });
        PhoneDevice {
            ask,
            told: Mutex::new(told),
        }
    }

    fn ask_later(&self, action: &'static str, argument: Value, timeout: Duration) -> DeviceAnswer<Value> {
        let ask = self.ask.clone();
        Box::pin(async move {
            let asking = tauri::async_runtime::spawn_blocking(move || ask(action, &argument));
            match tokio::time::timeout(timeout, asking).await {
                Ok(Ok(answer)) => answer,
                Ok(Err(error)) => Err(error.to_string()),
                Err(_) => Err("The phone took too long to answer.".to_string()),
            }
        })
    }

    fn ask_for_nothing(&self, action: &'static str, argument: Value) -> DeviceAnswer<()> {
        let answer = self.ask_later(action, argument, ANSWER_TIMEOUT);
        Box::pin(async move { answer.await.map(|_| ()) })
    }

    // Sends a request whose answer nobody waits for.
    fn tell(&self, action: &'static str, argument: Value) {
        let _ = self.told.lock().unwrap().send((action, argument));
    }
}

impl Device for PhoneDevice {
    fn kind(&self) -> DeviceKind {
        DeviceKind::Phone
    }

    fn is_do_not_disturb(&self) -> BoxFuture<'static, bool> {
        let answer = self.ask_later("doNotDisturb", json!({}), ANSWER_TIMEOUT);
        Box::pin(async move { answer.await.ok().and_then(|on| on.as_bool()).unwrap_or(false) })
    }

    fn media_command(&self, command: &str) -> DeviceAnswer<()> {
        self.ask_for_nothing("media", json!({ "command": command }))
    }

    fn call_command(&self, command: CallCommand) -> DeviceAnswer<()> {
        let action = match command {
            CallCommand::Answer => "answerCall",
            CallCommand::Reject => "rejectCall",
            CallCommand::Silence => "silenceCall",
        };
        self.ask_for_nothing(action, json!({}))
    }

    fn locate(&self) -> DeviceAnswer<Location> {
        let answer = self.ask_later("locate", json!({}), LOCATE_TIMEOUT);
        Box::pin(async move { read_location(answer.await?) })
    }

    fn read_file(&self, uri: &str) -> DeviceAnswer<Vec<u8>> {
        let answer = self.ask_later("readFile", json!({ "uri": uri }), READ_FILE_TIMEOUT);
        Box::pin(async move { read_file_bytes(&answer.await?) })
    }

    fn keep_running(&self, watch_name: Option<&str>) {
        self.tell("keepRunning", json!({ "watchName": watch_name }));
    }

    fn find_phone(&self, ringing: bool) {
        self.tell("findPhone", json!({ "ringing": ringing }));
    }

    // A phone runs only Android intents for the remote; see core/src/daemon/remote.rs.
    fn remote_action(&self, action: &Value) -> DeviceAnswer<Value> {
        if action["kind"] != "android" || !action["intent"].is_object() {
            return Box::pin(async {
                Err("This phone runs only Android intents for the remote.".to_string())
            });
        }
        let started = self.ask_later("intent", json!({ "intent": action["intent"] }), ANSWER_TIMEOUT);
        Box::pin(async move { started.await.map(|_| json!({})) })
    }

    fn sos_screen(&self, on: bool, record: bool) -> DeviceAnswer<()> {
        self.ask_for_nothing("sosScreen", json!({ "on": on, "record": record }))
    }

    fn call_number(&self, number: &str) -> DeviceAnswer<()> {
        self.ask_for_nothing("callNumber", json!({ "number": number }))
    }

    fn dial_number(&self, number: &str) -> DeviceAnswer<()> {
        self.ask_for_nothing("dialNumber", json!({ "number": number }))
    }

    fn send_text(&self, number: &str, text: &str) -> DeviceAnswer<()> {
        self.ask_for_nothing("sendText", json!({ "number": number, "text": text }))
    }
}

// ============================================================================
// Reading the answers
// ============================================================================

fn read_location(answer: Value) -> Result<Location, String> {
    let mut location: Location =
        serde_json::from_value(answer).map_err(|_| "The phone gave no usable location.".to_string())?;
    if location.name.is_empty() {
        location.name = "Here".into();
    }
    Ok(location)
}

fn read_file_bytes(answer: &Value) -> Result<Vec<u8>, String> {
    let text = answer["base64"].as_str().ok_or("The phone sent no file.")?;
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every request the fake phone heard, as its action and argument.
    type Heard = Arc<Mutex<Vec<(String, Value)>>>;

    // A phone that records every request and answers from a table.
    fn phone(answers: Value) -> (PhoneDevice, Heard) {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let recorded = heard.clone();
        let device = PhoneDevice::new(Arc::new(move |action, argument| {
            recorded
                .lock()
                .unwrap()
                .push((action.to_string(), argument.clone()));
            match &answers[action] {
                Value::Null => Ok(Value::Null),
                Value::String(reason) if reason.starts_with("fail:") => Err(reason[5..].to_string()),
                answer => Ok(answer.clone()),
            }
        }));
        (device, heard)
    }

    #[tokio::test]
    async fn calls_and_media_become_their_requests() {
        let (device, heard) = phone(json!({}));

        device.media_command("next").await.unwrap();
        device.call_command(CallCommand::Answer).await.unwrap();
        device.call_command(CallCommand::Reject).await.unwrap();
        device.call_command(CallCommand::Silence).await.unwrap();

        let heard = heard.lock().unwrap().clone();
        let actions: Vec<&str> = heard.iter().map(|(action, _)| action.as_str()).collect();
        assert_eq!(actions, ["media", "answerCall", "rejectCall", "silenceCall"]);
        assert_eq!(heard[0].1, json!({ "command": "next" }));
    }

    #[tokio::test]
    async fn a_failed_request_gives_the_phones_reason() {
        let (device, _) = phone(json!({ "answerCall": "fail:Allow the app to answer calls first" }));

        let answered = device.call_command(CallCommand::Answer).await;

        assert_eq!(answered, Err("Allow the app to answer calls first".to_string()));
    }

    #[tokio::test]
    async fn do_not_disturb_counts_as_off_when_the_phone_cannot_say() {
        let (quiet, _) = phone(json!({ "doNotDisturb": true }));
        let (broken, _) = phone(json!({ "doNotDisturb": "fail:no" }));

        assert!(quiet.is_do_not_disturb().await);
        assert!(!broken.is_do_not_disturb().await);
    }

    #[tokio::test]
    async fn a_location_without_a_town_is_here() {
        let (device, _) = phone(json!({ "locate": { "latitude": 51.5, "longitude": -0.1, "name": "" } }));

        let found = device.locate().await.unwrap();

        assert_eq!(
            (found.latitude, found.longitude, found.name.as_str()),
            (51.5, -0.1, "Here")
        );
    }

    #[tokio::test]
    async fn a_file_arrives_as_base64() {
        let (device, heard) = phone(json!({ "readFile": { "base64": "aGVsbG8=" } }));

        let bytes = device.read_file("content://notes/1").await.unwrap();

        assert_eq!(bytes, b"hello");
        assert_eq!(heard.lock().unwrap()[0].1, json!({ "uri": "content://notes/1" }));
    }

    #[tokio::test]
    async fn the_remote_starts_only_android_intents() {
        let (device, heard) = phone(json!({}));
        let intent = json!({ "type": "deeplink", "uri": "geo:0,0" });

        let started = device
            .remote_action(&json!({ "kind": "android", "intent": intent }))
            .await;
        let refused = device
            .remote_action(&json!({ "kind": "command", "command": "ls" }))
            .await;

        assert_eq!(started, Ok(json!({})));
        assert!(refused.is_err());
        assert_eq!(
            heard.lock().unwrap().clone(),
            [("intent".to_string(), json!({ "intent": intent }))]
        );
    }

    #[tokio::test]
    async fn the_sos_steps_reach_the_phone_by_name() {
        let (device, heard) = phone(json!({}));

        device.sos_screen(true, false).await.unwrap();
        device.call_number("555 0101").await.unwrap();
        device.dial_number("911").await.unwrap();
        device.send_text("555 0101", "Help").await.unwrap();

        assert_eq!(
            heard.lock().unwrap().clone(),
            [
                ("sosScreen".to_string(), json!({ "on": true, "record": false })),
                ("callNumber".to_string(), json!({ "number": "555 0101" })),
                ("dialNumber".to_string(), json!({ "number": "911" })),
                (
                    "sendText".to_string(),
                    json!({ "number": "555 0101", "text": "Help" })
                ),
            ]
        );
    }

    #[tokio::test]
    async fn keeping_running_and_ringing_are_sent_in_order_without_waiting() {
        let (device, heard) = phone(json!({}));

        device.keep_running(Some("Pine"));
        device.find_phone(true);
        device.keep_running(None);
        for _ in 0..100 {
            if heard.lock().unwrap().len() == 3 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        assert_eq!(
            heard.lock().unwrap().clone(),
            [
                ("keepRunning".to_string(), json!({ "watchName": "Pine" })),
                ("findPhone".to_string(), json!({ "ringing": true })),
                ("keepRunning".to_string(), json!({ "watchName": null })),
            ]
        );
    }
}
