// The watch's remote app: up to nine buttons the user sets up in the companion, which the watch shows and reports taps on.
//
// The watch only holds each button's icon, in the file /remote/buttons, and sends
// {"t":"app","ch":"remote","d":{"button":<index>}} when one is tapped. What a button does lives only in the
// companion's settings, so a tap can never carry a command, a URL or an intent from the watch. This mirrors
// src/protocol/remote.ts.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::sync::files::FileClient;
use crate::web_request::WebAccess;

// ============================================================================
// Buttons and their actions
// ============================================================================

pub const MOST_REMOTE_BUTTONS: usize = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
}

impl HttpMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Delete => "DELETE",
        }
    }

    fn read(text: &str) -> Option<HttpMethod> {
        match text {
            "GET" => Some(HttpMethod::Get),
            "POST" => Some(HttpMethod::Post),
            "PUT" => Some(HttpMethod::Put),
            "DELETE" => Some(HttpMethod::Delete),
            _ => None,
        }
    }
}

// Ready-made actions the companion already knows how to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BuiltinAction {
    PlayPause,
    Play,
    Pause,
    Next,
    Previous,
    VolumeUp,
    VolumeDown,
    FindPhone,
}

impl BuiltinAction {
    pub fn as_str(self) -> &'static str {
        match self {
            BuiltinAction::PlayPause => "playpause",
            BuiltinAction::Play => "play",
            BuiltinAction::Pause => "pause",
            BuiltinAction::Next => "next",
            BuiltinAction::Previous => "previous",
            BuiltinAction::VolumeUp => "volumeup",
            BuiltinAction::VolumeDown => "volumedown",
            BuiltinAction::FindPhone => "findphone",
        }
    }

    // The name the screen shows for it.
    pub fn label(self) -> &'static str {
        match self {
            BuiltinAction::PlayPause => "Play or pause",
            BuiltinAction::Play => "Play",
            BuiltinAction::Pause => "Pause",
            BuiltinAction::Next => "Next track",
            BuiltinAction::Previous => "Previous track",
            BuiltinAction::VolumeUp => "Volume up",
            BuiltinAction::VolumeDown => "Volume down",
            BuiltinAction::FindPhone => "Find this device",
        }
    }

    fn read(text: &str) -> Option<BuiltinAction> {
        [
            BuiltinAction::PlayPause,
            BuiltinAction::Play,
            BuiltinAction::Pause,
            BuiltinAction::Next,
            BuiltinAction::Previous,
            BuiltinAction::VolumeUp,
            BuiltinAction::VolumeDown,
            BuiltinAction::FindPhone,
        ]
        .into_iter()
        .find(|action| action.as_str() == text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum AndroidIntent {
    Deeplink {
        uri: String,
    },
    Activity {
        package_name: String,
        class_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        action: Option<String>,
        #[serde(skip_serializing_if = "BTreeMap::is_empty")]
        extras: BTreeMap<String, String>,
    },
    Broadcast {
        action: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        package_name: Option<String>,
        #[serde(skip_serializing_if = "BTreeMap::is_empty")]
        extras: BTreeMap<String, String>,
    },
}

// What a tap does; it serializes to the same JSON the window stores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RemoteAction {
    // A web request, with the headers, login and certificate choice in its access part.
    Http {
        method: HttpMethod,
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        #[serde(flatten)]
        access: WebAccess,
    },
    Command {
        command: String,
    },
    Android {
        intent: AndroidIntent,
    },
    Builtin {
        name: BuiltinAction,
    },
}

impl RemoteAction {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

// One button: the library icon the watch shows, a label only the companion shows, and what a tap does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteButton {
    pub icon: String,
    pub label: String,
    pub action: RemoteAction,
}

// ============================================================================
// Reading buttons back from storage
// ============================================================================

fn text_at<'value>(object: &'value Map<String, Value>, key: &str) -> Option<&'value str> {
    object.get(key).and_then(Value::as_str)
}

fn filled_text_at(object: &Map<String, Value>, key: &str) -> Option<String> {
    text_at(object, key)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn read_extras(value: Option<&Value>) -> BTreeMap<String, String> {
    value
        .and_then(Value::as_object)
        .map(|extras| {
            extras
                .iter()
                .filter_map(|(key, item)| Some((key.clone(), item.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

fn read_intent(intent: &Map<String, Value>) -> Option<AndroidIntent> {
    match text_at(intent, "type")? {
        "deeplink" => Some(AndroidIntent::Deeplink {
            uri: text_at(intent, "uri")?.to_string(),
        }),
        "activity" => Some(AndroidIntent::Activity {
            package_name: text_at(intent, "packageName")?.to_string(),
            class_name: text_at(intent, "className")?.to_string(),
            action: filled_text_at(intent, "action"),
            extras: read_extras(intent.get("extras")),
        }),
        "broadcast" => Some(AndroidIntent::Broadcast {
            action: text_at(intent, "action")?.to_string(),
            package_name: filled_text_at(intent, "packageName"),
            extras: read_extras(intent.get("extras")),
        }),
        _ => None,
    }
}

// One action as stored, or None when it is not one the companion knows.
pub fn read_remote_action(value: &Value) -> Option<RemoteAction> {
    let action = value.as_object()?;
    match text_at(action, "kind")? {
        "http" => Some(RemoteAction::Http {
            method: HttpMethod::read(text_at(action, "method")?)?,
            url: text_at(action, "url")?.to_string(),
            body: filled_text_at(action, "body"),
            access: WebAccess::read(value),
        }),
        "command" => Some(RemoteAction::Command {
            command: text_at(action, "command")?.to_string(),
        }),
        "android" => Some(RemoteAction::Android {
            intent: read_intent(action.get("intent")?.as_object()?)?,
        }),
        "builtin" => Some(RemoteAction::Builtin {
            name: BuiltinAction::read(text_at(action, "name")?)?,
        }),
        _ => None,
    }
}

// The buttons in a stored list, dropping any that are malformed and any past the ninth.
pub fn read_remote_buttons(value: &Value) -> Vec<RemoteButton> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let button = item.as_object()?;
            let icon = text_at(button, "icon").filter(|icon| is_library_icon(icon))?;
            let action = read_remote_action(button.get("action")?)?;
            Some(RemoteButton {
                icon: icon.to_string(),
                label: text_at(button, "label").unwrap_or_default().to_string(),
                action,
            })
        })
        .take(MOST_REMOTE_BUTTONS)
        .collect()
}

// ============================================================================
// The shared library's icons
// ============================================================================

// The window's generated list, so the two never disagree on how the watch numbers its icons.
const ICON_NAMES_SOURCE: &str = include_str!("../../../src/bundles/icon-names.ts");

// The shared library's icons, in the order the watch numbers them.
pub fn library_icon_names() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let list = ICON_NAMES_SOURCE
            .split_once("LIBRARY_ICON_NAMES = [")
            .and_then(|(_, rest)| rest.split_once(']'))
            .map_or("", |(list, _)| list);
        list.split(',')
            .filter_map(|item| item.trim().strip_prefix('\'')?.strip_suffix('\''))
            .map(str::to_string)
            .collect()
    })
}

pub fn is_library_icon(name: &str) -> bool {
    library_icon_names().iter().any(|known| known == name)
}

// ============================================================================
// The layout file
// ============================================================================

pub const REMOTE_DIRECTORY: &str = "/remote";
pub const REMOTE_BUTTONS_PATH: &str = "/remote/buttons";
pub const REMOTE_BUTTONS_FILE_NAME: &str = "buttons";
pub const REMOTE_LAYOUT_VERSION: u8 = 1;

// The bytes before the icons: the format version and the button count.
const LAYOUT_HEADER_BYTES: usize = 2;

// Byte 0 is the format version, byte 1 the button count, then each button's icon as its place in the library.
pub fn encode_remote_layout(icons: &[&str]) -> Result<Vec<u8>, String> {
    if icons.len() > MOST_REMOTE_BUTTONS {
        return Err(format!(
            "The watch shows up to {MOST_REMOTE_BUTTONS} buttons, not {}.",
            icons.len()
        ));
    }
    let mut bytes = vec![REMOTE_LAYOUT_VERSION, icons.len() as u8];
    for icon in icons {
        let number = library_icon_names()
            .iter()
            .position(|known| known == icon)
            .ok_or_else(|| format!("The shared library has no icon called {icon}."))?;
        bytes.push(number as u8);
    }
    Ok(bytes)
}

// The icon names in a layout file, for checking what the watch holds.
pub fn decode_remote_layout(bytes: &[u8]) -> Result<Vec<String>, String> {
    if bytes.len() < LAYOUT_HEADER_BYTES || bytes[0] != REMOTE_LAYOUT_VERSION {
        return Err("That is not a version 1 remote layout.".into());
    }
    let count = bytes[1] as usize;
    if count > MOST_REMOTE_BUTTONS || bytes.len() != LAYOUT_HEADER_BYTES + count {
        return Err("The remote layout is the wrong length for its button count.".into());
    }
    bytes[LAYOUT_HEADER_BYTES..]
        .iter()
        .map(|&number| {
            library_icon_names().get(number as usize).cloned().ok_or_else(|| {
                format!("The remote layout names icon {number}, which the library does not have.")
            })
        })
        .collect()
}

// The length and FNV-1a hash of some bytes, written as the window's notes.ts fingerprint writes it.
pub fn fingerprint(bytes: &[u8]) -> String {
    const FNV_OFFSET_BASIS: u32 = 0x811c_9dc5;
    const FNV_PRIME: u32 = 0x0100_0193;
    let hash = bytes.iter().fold(FNV_OFFSET_BASIS, |hash, &byte| {
        (hash ^ byte as u32).wrapping_mul(FNV_PRIME)
    });
    format!("{}:{hash:08x}", bytes.len())
}

// ============================================================================
// Sending the layout
// ============================================================================

// What was last sent to a watch, so an unchanged layout is not sent again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteOnWatch {
    pub watch_id: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutSync {
    pub on_watch: RemoteOnWatch,
    pub sent: bool,
}

// Writes the layout unless the watch already holds it; with `check`, the watch's folder is listed first.
pub async fn sync_remote_layout(
    files: &FileClient,
    buttons: &[RemoteButton],
    watch_id: &str,
    last: Option<&RemoteOnWatch>,
    check: bool,
) -> Result<LayoutSync, String> {
    let icons: Vec<&str> = buttons.iter().map(|button| button.icon.as_str()).collect();
    let layout = encode_remote_layout(&icons)?;
    let print = fingerprint(&layout);
    let mut there = last.is_some_and(|last| last.watch_id == watch_id && last.fingerprint == print);
    if there && check {
        there = match files.list(REMOTE_DIRECTORY).await {
            Ok(entries) => entries.iter().any(|entry| {
                entry.name == REMOTE_BUTTONS_FILE_NAME && !entry.folder && entry.size as usize == layout.len()
            }),
            Err(_) => false,
        };
    }
    if !there {
        files
            .write(REMOTE_BUTTONS_PATH, &layout)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(LayoutSync {
        on_watch: RemoteOnWatch {
            watch_id: watch_id.to_string(),
            fingerprint: print,
        },
        sent: !there,
    })
}

// ============================================================================
// Taps from the watch
// ============================================================================

pub const REMOTE_CHANNEL: &str = "remote";

// The index of the tapped button, or None when the message is not a well-formed remote tap.
pub fn remote_tap_index(message: &Value) -> Option<u64> {
    if message["t"] != "app" || message["ch"] != REMOTE_CHANNEL {
        return None;
    }
    let button = message["d"].as_object()?.get("button")?.as_f64()?;
    let whole = button.fract() == 0.0 && button.is_finite();
    (whole && button >= 0.0).then_some(button as u64)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::files::fake::FakeWatch;
    use serde_json::json;

    fn button(icon: &str) -> RemoteButton {
        RemoteButton {
            icon: icon.into(),
            label: String::new(),
            action: RemoteAction::Builtin {
                name: BuiltinAction::Play,
            },
        }
    }

    #[test]
    fn the_icon_list_is_read_from_the_windows_generated_file() {
        let names = library_icon_names();

        assert_eq!(names[0], "alarm");
        assert_eq!(names[1], "backspace");
        assert!(names.len() > 20);
        assert!(is_library_icon("bell"));
        assert!(!is_library_icon("nonsense"));
    }

    #[test]
    fn a_layout_round_trips_through_its_bytes() {
        let bytes = encode_remote_layout(&["alarm", "bell"]).unwrap();

        assert_eq!(bytes, vec![1, 2, 0, 3]);
        assert_eq!(decode_remote_layout(&bytes).unwrap(), vec!["alarm", "bell"]);
        assert_eq!(
            encode_remote_layout(&["nonsense"]).unwrap_err(),
            "The shared library has no icon called nonsense."
        );
        assert_eq!(
            encode_remote_layout(&["alarm"; 10]).unwrap_err(),
            "The watch shows up to 9 buttons, not 10."
        );
        assert!(decode_remote_layout(&[2, 0]).is_err());
        assert!(decode_remote_layout(&[1, 2, 0]).is_err());
        assert!(decode_remote_layout(&[1, 1, 255]).is_err());
    }

    #[test]
    fn the_fingerprint_matches_the_windows() {
        assert_eq!(fingerprint(&[]), "0:811c9dc5");
        assert_eq!(fingerprint(b"a"), "1:e40c292c");
    }

    #[test]
    fn stored_buttons_keep_only_the_well_formed_and_the_first_nine() {
        let stored = json!([
            { "icon": "bell", "label": "Lights", "action": {
                "kind": "http", "method": "POST", "url": "http://hub/on", "body": "",
                "username": "me", "certificate": "any" } },
            { "icon": "nonsense", "action": { "kind": "builtin", "name": "play" } },
            { "icon": "alarm", "action": { "kind": "builtin", "name": "dance" } },
            { "icon": "alarm", "action": { "kind": "command", "command": "ls" } },
            { "icon": "alarm", "action": { "kind": "android", "intent": {
                "type": "activity", "packageName": "a.b", "className": "a.b.C", "action": "",
                "extras": { "k": "v", "n": 1 } } } },
            { "icon": "alarm", "action": { "kind": "android", "intent": { "type": "broadcast" } } },
            "not a button",
        ]);

        let buttons = read_remote_buttons(&stored);

        assert_eq!(buttons.len(), 3);
        assert_eq!(buttons[0].label, "Lights");
        assert_eq!(
            buttons[0].action.to_value(),
            json!({ "kind": "http", "method": "POST", "url": "http://hub/on", "username": "me", "certificate": "any" })
        );
        assert_eq!(buttons[1].label, "");
        assert_eq!(
            buttons[2].action.to_value(),
            json!({ "kind": "android", "intent": {
                "type": "activity", "packageName": "a.b", "className": "a.b.C", "extras": { "k": "v" } } })
        );
        let many = json!(vec![
            json!({ "icon": "bell", "action": { "kind": "builtin", "name": "next" } });
            12
        ]);
        assert_eq!(read_remote_buttons(&many).len(), MOST_REMOTE_BUTTONS);
        assert!(read_remote_buttons(&json!({})).is_empty());
    }

    #[test]
    fn only_a_well_formed_remote_message_is_a_tap() {
        let tap = |data: Value| json!({ "t": "app", "ch": "remote", "d": data });

        assert_eq!(remote_tap_index(&tap(json!({ "button": 2 }))), Some(2));
        assert_eq!(remote_tap_index(&tap(json!({ "button": 2.0 }))), Some(2));
        assert_eq!(remote_tap_index(&tap(json!({ "button": 1.5 }))), None);
        assert_eq!(remote_tap_index(&tap(json!({ "button": -1 }))), None);
        assert_eq!(remote_tap_index(&tap(json!({ "button": "2" }))), None);
        assert_eq!(remote_tap_index(&tap(json!(null))), None);
        assert_eq!(
            remote_tap_index(&json!({ "t": "app", "ch": "other", "d": { "button": 1 } })),
            None
        );
    }

    #[tokio::test]
    async fn an_unchanged_layout_is_sent_only_when_the_watch_lost_it() {
        let watch = FakeWatch::with(&[]);
        let client = watch.client();
        let buttons = vec![button("alarm"), button("bell")];

        let first = sync_remote_layout(&client, &buttons, "AA", None, true)
            .await
            .unwrap();
        assert!(first.sent);
        assert_eq!(watch.files.lock().unwrap()[REMOTE_BUTTONS_PATH], vec![1, 2, 0, 3]);

        let again = sync_remote_layout(&client, &buttons, "AA", Some(&first.on_watch), true)
            .await
            .unwrap();
        assert!(!again.sent);

        watch.files.lock().unwrap().clear();
        let unchecked = sync_remote_layout(&client, &buttons, "AA", Some(&first.on_watch), false)
            .await
            .unwrap();
        assert!(!unchecked.sent);
        let checked = sync_remote_layout(&client, &buttons, "AA", Some(&first.on_watch), true)
            .await
            .unwrap();
        assert!(checked.sent);

        let other_watch = sync_remote_layout(&client, &buttons, "BB", Some(&first.on_watch), false)
            .await
            .unwrap();
        assert!(other_watch.sent);
    }
}
