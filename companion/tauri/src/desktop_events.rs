// Turns the raw facts desktop.rs overhears on D-Bus into the phone events the daemon forwards, as src/services/phone/desktop.ts does for the window.

use std::collections::VecDeque;

use battletime_companion_core::daemon::device::{Media, PhoneEvent};
use serde_json::Value;

// ============================================================================
// Notifications
// ============================================================================

// The desktop app's own notifications never go back to the watch.
const OWN_APP_NAMES: [&str; 2] = ["BattleTime Companion", "battletime-companion"];

// Notifications waiting for the server to say which id it gave them; old ones are dropped past this many.
const PENDING_LIMIT: usize = 100;

const MICROSECONDS_PER_SECOND: f64 = 1_000_000.0;

// Desktop notification bodies may hold simple markup, which the watch would show as text.
pub fn strip_markup(text: &str) -> String {
    let mut plain = String::new();
    let mut in_tag = false;
    for character in text.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => plain.push(character),
            _ => {}
        }
    }
    plain
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
        .trim()
        .to_string()
}

fn notification_event(id: i64, raw: &Value) -> PhoneEvent {
    let app_name = raw["appName"].as_str().unwrap_or_default();
    let desktop_entry = raw["hints"]["desktop-entry"].as_str().unwrap_or_default();
    PhoneEvent::Notification {
        id,
        app: if app_name.is_empty() {
            desktop_entry
        } else {
            app_name
        }
        .to_string(),
        title: strip_markup(raw["summary"].as_str().unwrap_or_default()),
        body: strip_markup(raw["body"].as_str().unwrap_or_default()),
    }
}

// ============================================================================
// Media players
// ============================================================================

fn text(value: &Value) -> Option<String> {
    let joined = match value {
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        Value::String(text) => text.clone(),
        _ => String::new(),
    };
    (!joined.is_empty()).then_some(joined)
}

fn seconds(microseconds: &Value) -> Option<f64> {
    microseconds
        .as_f64()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| (value / MICROSECONDS_PER_SECOND).round())
}

// Prefers a player that is playing, then the one already followed, then any that is paused.
pub fn choose_player<'players>(players: &'players [Value], current: Option<&str>) -> Option<&'players Value> {
    players
        .iter()
        .find(|player| player["status"] == "Playing")
        .or_else(|| {
            players
                .iter()
                .find(|player| player["name"].as_str() == current && player["status"] != "Stopped")
        })
        .or_else(|| players.iter().find(|player| player["status"] == "Paused"))
}

fn media_of(player: Option<&Value>) -> Media {
    let Some(player) = player else {
        return Media::default();
    };
    let metadata = &player["metadata"];
    Media {
        playing: player["status"] == "Playing",
        artist: text(&metadata["xesam:artist"]),
        album: text(&metadata["xesam:album"]),
        track: text(&metadata["xesam:title"]),
        duration_sec: seconds(&metadata["mpris:length"]),
        position_sec: seconds(&player["position"]),
    }
}

// ============================================================================
// The tracker
// ============================================================================

// Remembers just enough to pair a notification with the id the server gave it, and to send media only when it changes.
#[derive(Default)]
pub struct DesktopEventTracker {
    pending: VecDeque<(String, Value)>,
    last_media: Option<Media>,
    pub current_player: Option<String>,
}

impl DesktopEventTracker {
    pub fn handle(&mut self, raw: &Value) -> Option<PhoneEvent> {
        match raw["kind"].as_str()? {
            "notify" => self.on_notify(raw),
            "notify-reply" => self.on_notify_reply(raw),
            "closed" => Some(PhoneEvent::NotificationRemoved {
                id: raw["id"].as_i64()?,
            }),
            "players" => self.on_players(raw["players"].as_array()?),
            _ => None,
        }
    }

    fn on_notify(&mut self, raw: &Value) -> Option<PhoneEvent> {
        if OWN_APP_NAMES.contains(&raw["appName"].as_str().unwrap_or_default()) {
            return None;
        }
        // A replacement keeps the id of the notification it replaces, so it needs no answer from the server.
        let replaces = raw["replacesId"].as_i64().unwrap_or(0);
        if replaces > 0 {
            return Some(notification_event(replaces, raw));
        }
        let key = format!("{}/{}", raw["sender"].as_str().unwrap_or_default(), raw["serial"]);
        self.pending.push_back((key, raw.clone()));
        if self.pending.len() > PENDING_LIMIT {
            self.pending.pop_front();
        }
        None
    }

    fn on_notify_reply(&mut self, reply: &Value) -> Option<PhoneEvent> {
        let key = format!(
            "{}/{}",
            reply["destination"].as_str().unwrap_or_default(),
            reply["replySerial"]
        );
        let found = self.pending.iter().position(|(pending, _)| *pending == key)?;
        let (_, raw) = self.pending.remove(found)?;
        Some(notification_event(reply["id"].as_i64()?, &raw))
    }

    fn on_players(&mut self, players: &[Value]) -> Option<PhoneEvent> {
        let player = choose_player(players, self.current_player.as_deref());
        self.current_player = player
            .and_then(|player| player["name"].as_str())
            .map(str::to_string);
        let media = media_of(player);
        if self.last_media.as_ref() == Some(&media) {
            return None;
        }
        self.last_media = Some(media.clone());
        Some(PhoneEvent::Media(media))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn notify(serial: u64, replaces: u32, app: &str) -> Value {
        json!({
            "kind": "notify", "sender": ":1.5", "serial": serial, "appName": app, "replacesId": replaces,
            "summary": "<b>Hi</b>", "body": "Tom &amp; Ann", "hints": { "desktop-entry": "mail" },
        })
    }

    #[test]
    fn markup_is_stripped_and_entities_read() {
        assert_eq!(
            strip_markup(" <b>Bold</b> &lt;x&gt; &amp; &quot;q&quot; "),
            "Bold <x> & \"q\""
        );
    }

    #[test]
    fn a_notification_is_sent_once_the_server_names_its_id() {
        let mut tracker = DesktopEventTracker::default();

        assert_eq!(tracker.handle(&notify(9, 0, "Mail")), None);
        let event = tracker
            .handle(&json!({ "kind": "notify-reply", "destination": ":1.5", "replySerial": 9, "id": 41 }));

        assert_eq!(
            event,
            Some(PhoneEvent::Notification {
                id: 41,
                app: "Mail".into(),
                title: "Hi".into(),
                body: "Tom & Ann".into()
            })
        );
        assert_eq!(
            tracker.handle(
                &json!({ "kind": "notify-reply", "destination": ":1.5", "replySerial": 9, "id": 41 })
            ),
            None
        );
    }

    #[test]
    fn a_replacement_keeps_its_id_and_an_empty_app_name_uses_the_desktop_entry() {
        let mut tracker = DesktopEventTracker::default();
        let event = tracker.handle(&notify(3, 12, "")).unwrap();
        assert!(matches!(event, PhoneEvent::Notification { id: 12, ref app, .. } if app == "mail"));
    }

    #[test]
    fn the_apps_own_notifications_and_closed_ones() {
        let mut tracker = DesktopEventTracker::default();
        assert_eq!(tracker.handle(&notify(3, 12, "BattleTime Companion")), None);
        assert_eq!(
            tracker.handle(&json!({ "kind": "closed", "id": 12 })),
            Some(PhoneEvent::NotificationRemoved { id: 12 })
        );
    }

    #[test]
    fn the_playing_player_wins_and_unchanged_media_is_not_sent_again() {
        let mut tracker = DesktopEventTracker::default();
        let players = json!({ "kind": "players", "players": [
            { "name": "vlc", "status": "Paused", "metadata": {}, "position": null },
            { "name": "spotify", "status": "Playing", "position": 3_000_000,
              "metadata": { "xesam:artist": ["A", "B"], "xesam:title": "Song", "mpris:length": 180_000_000 } },
        ]});

        let event = tracker.handle(&players);

        assert_eq!(
            event,
            Some(PhoneEvent::Media(Media {
                playing: true,
                artist: Some("A, B".into()),
                track: Some("Song".into()),
                duration_sec: Some(180.0),
                position_sec: Some(3.0),
                album: None,
            }))
        );
        assert_eq!(tracker.current_player.as_deref(), Some("spotify"));
        assert_eq!(tracker.handle(&players), None);
        assert_eq!(
            tracker.handle(&json!({ "kind": "players", "players": [] })),
            Some(PhoneEvent::Media(Media::default()))
        );
        assert_eq!(tracker.current_player, None);
    }
}
