// The device the daemon runs on, a phone or a computer, and what it reports about notifications, calls and media.
//
// Each host supplies one: the desktop reads D-Bus, Android its notification listener and telephony.

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ============================================================================
// What the device reports
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PhoneEvent {
    Notification {
        id: i64,
        #[serde(default)]
        app: String,
        #[serde(default)]
        title: String,
        #[serde(default)]
        body: String,
    },
    NotificationRemoved {
        id: i64,
    },
    Call {
        state: CallState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        number: Option<String>,
    },
    Media(Media),
    // The wearer stopped the SOS from the phone's SOS screen.
    SosStopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CallState {
    Incoming,
    Outgoing,
    Start,
    End,
}

impl CallState {
    pub fn as_str(self) -> &'static str {
        match self {
            CallState::Incoming => "incoming",
            CallState::Outgoing => "outgoing",
            CallState::Start => "start",
            CallState::End => "end",
        }
    }
}

// What is playing, as the window's `nowPlaying` reads it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Media {
    #[serde(default)]
    pub playing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_sec: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position_sec: Option<f64>,
}

impl PhoneEvent {
    // An event as a host sends it, or None when it is not one the daemon knows.
    pub fn from_value(value: &Value) -> Option<PhoneEvent> {
        serde_json::from_value(value.clone()).ok()
    }
}

// ============================================================================
// What the device does
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceKind {
    Phone,
    Computer,
}

// The watch's call screen: pick up, hang up, or only stop the ringer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallCommand {
    Answer,
    Reject,
    Silence,
}

impl CallCommand {
    // The watch's `n` in {"t":"call","n":...}.
    pub fn from_watch(text: &str) -> Option<CallCommand> {
        match text {
            "ACCEPT" => Some(CallCommand::Answer),
            "REJECT" => Some(CallCommand::Reject),
            "IGNORE" => Some(CallCommand::Silence),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default)]
    pub name: String,
}

pub type DeviceAnswer<T> = BoxFuture<'static, Result<T, String>>;

pub trait Device: Send + Sync + 'static {
    fn kind(&self) -> DeviceKind;

    // True whenever the device's interruption filter is anything but "all".
    fn is_do_not_disturb(&self) -> BoxFuture<'static, bool>;

    // play, pause, next, previous, volumeup or volumedown.
    fn media_command(&self, command: &str) -> DeviceAnswer<()>;

    fn call_command(&self, command: CallCommand) -> DeviceAnswer<()>;

    fn locate(&self) -> DeviceAnswer<Location>;

    // The bytes behind a file the user picked, which on Android is a content:// link.
    fn read_file(&self, uri: &str) -> DeviceAnswer<Vec<u8>>;

    // Asks the device to stay running while the daemon keeps a watch, named for its notification; None lets it go.
    fn keep_running(&self, _watch_name: Option<&str>) {}

    // The watch is looking for the phone; a phone rings, and stops when `ringing` is false.
    fn find_phone(&self, _ringing: bool) {}

    // Runs a remote button's action that only this device can do, such as a shell command or an intent.
    fn remote_action(&self, _action: &Value) -> DeviceAnswer<Value> {
        Box::pin(async { Err("This device cannot run that action.".to_string()) })
    }

    // Shows the SOS screen, recording from the cameras when `record` is true, or with `on` false stops recording and closes it.
    fn sos_screen(&self, _on: bool, _record: bool) -> DeviceAnswer<()> {
        Box::pin(async { Err("This device has no SOS screen.".to_string()) })
    }

    // Places a call to the number straight away.
    fn call_number(&self, _number: &str) -> DeviceAnswer<()> {
        Box::pin(async { Err("This device cannot place calls.".to_string()) })
    }

    // Opens the dialer with the number typed in, for a number such as 911 that no app may call itself.
    fn dial_number(&self, _number: &str) -> DeviceAnswer<()> {
        Box::pin(async { Err("This device cannot place calls.".to_string()) })
    }

    fn send_text(&self, _number: &str, _text: &str) -> DeviceAnswer<()> {
        Box::pin(async { Err("This device cannot send texts.".to_string()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn events_read_the_shapes_the_hosts_send() {
        assert_eq!(
            PhoneEvent::from_value(
                &json!({ "type": "notification", "id": 4, "app": "Mail", "title": "Hi", "body": "There" })
            ),
            Some(PhoneEvent::Notification {
                id: 4,
                app: "Mail".into(),
                title: "Hi".into(),
                body: "There".into()
            })
        );
        assert_eq!(
            PhoneEvent::from_value(&json!({ "type": "call", "state": "incoming", "name": "Ann" })),
            Some(PhoneEvent::Call {
                state: CallState::Incoming,
                name: Some("Ann".into()),
                number: None
            })
        );
        assert_eq!(
            PhoneEvent::from_value(&json!({ "type": "media", "playing": true, "durationSec": 180 })),
            Some(PhoneEvent::Media(Media {
                playing: true,
                duration_sec: Some(180.0),
                ..Media::default()
            }))
        );
        assert_eq!(
            PhoneEvent::from_value(&json!({ "type": "call", "state": "ringing" })),
            None
        );
        assert_eq!(
            PhoneEvent::from_value(&json!({ "type": "sosStopped" })),
            Some(PhoneEvent::SosStopped)
        );
        assert_eq!(PhoneEvent::from_value(&json!({ "type": "other" })), None);
    }

    #[test]
    fn media_writes_only_the_fields_it_knows() {
        let media = Media {
            playing: false,
            track: Some("Song".into()),
            ..Media::default()
        };
        assert_eq!(
            serde_json::to_value(&media).unwrap(),
            json!({ "playing": false, "track": "Song" })
        );
    }

    #[test]
    fn the_watch_names_its_call_answers_in_capitals() {
        assert_eq!(CallCommand::from_watch("ACCEPT"), Some(CallCommand::Answer));
        assert_eq!(CallCommand::from_watch("REJECT"), Some(CallCommand::Reject));
        assert_eq!(CallCommand::from_watch("IGNORE"), Some(CallCommand::Silence));
        assert_eq!(CallCommand::from_watch("accept"), None);
    }
}
