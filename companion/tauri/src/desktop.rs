// The computer's side of what a phone offers the watch: desktop notifications, media players, Do Not Disturb and location.
//
// It only gathers raw facts over D-Bus and carries out commands; src/services/phone/desktop.ts decides what reaches the watch.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::StreamExt;
use serde_json::{json, Map, Value};
use zbus::message::Type as MessageType;
use zbus::zvariant::{OwnedValue, Value as DbusValue};
use zbus::{Connection, MatchRule, MessageStream, Proxy};

// ============================================================================
// Names on the bus
// ============================================================================

const NOTIFICATIONS_NAME: &str = "org.freedesktop.Notifications";
const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";

const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";
const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";
const MPRIS_PLAYER: &str = "org.mpris.MediaPlayer2.Player";

const GEOCLUE_NAME: &str = "org.freedesktop.GeoClue2";
const GEOCLUE_MANAGER_PATH: &str = "/org/freedesktop/GeoClue2/Manager";
const GEOCLUE_CLIENT: &str = "org.freedesktop.GeoClue2.Client";
const GEOCLUE_LOCATION: &str = "org.freedesktop.GeoClue2.Location";

// GeoClue lets an app in only under the name of its desktop file.
const DESKTOP_ID: &str = "neotime-companion";

// GeoClue's accuracy levels; city level is enough for the weather.
const CITY_ACCURACY: u32 = 4;

// ============================================================================
// Timing
// ============================================================================

// How long to wait for GeoClue to find where the computer is.
const LOCATE_TIMEOUT: Duration = Duration::from_secs(20);

// How long a Do Not Disturb check may take before it counts as off.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(3);

// ============================================================================
// D-Bus values as JSON
// ============================================================================

// Turns a D-Bus value into plain JSON that can cross to the window.
pub fn plain(value: &DbusValue<'_>) -> Value {
    match value {
        DbusValue::U8(number) => json!(number),
        DbusValue::Bool(flag) => json!(flag),
        DbusValue::I16(number) => json!(number),
        DbusValue::U16(number) => json!(number),
        DbusValue::I32(number) => json!(number),
        DbusValue::U32(number) => json!(number),
        DbusValue::I64(number) => json!(number),
        DbusValue::U64(number) => json!(number),
        DbusValue::F64(number) => json!(number),
        DbusValue::Str(text) => json!(text.as_str()),
        DbusValue::Signature(signature) => json!(signature.to_string()),
        DbusValue::ObjectPath(path) => json!(path.as_str()),
        DbusValue::Value(inner) => plain(inner),
        DbusValue::Array(array) => Value::Array(array.iter().map(plain).collect()),
        DbusValue::Dict(dict) => Value::Object(
            dict.iter()
                .map(|(key, item)| (key_text(key), plain(item)))
                .collect(),
        ),
        DbusValue::Structure(structure) => Value::Array(structure.fields().iter().map(plain).collect()),
        #[allow(unreachable_patterns)]
        _ => Value::Null,
    }
}

fn key_text(key: &DbusValue<'_>) -> String {
    match plain(key) {
        Value::String(text) => text,
        other => other.to_string(),
    }
}

fn plain_map(values: &HashMap<String, OwnedValue>) -> Map<String, Value> {
    values
        .iter()
        .map(|(name, value)| (name.clone(), plain(value)))
        .collect()
}

// ============================================================================
// Media players
// ============================================================================

#[derive(Clone, Default)]
struct Player {
    state: Map<String, Value>,
}

// The window's view of one player, as node/desktop.js reported it.
fn player_entry(name: &str, state: &Map<String, Value>) -> Value {
    json!({
        "name": name,
        "status": state.get("PlaybackStatus").cloned().unwrap_or(json!("Stopped")),
        "metadata": state.get("Metadata").cloned().unwrap_or(json!({})),
        "position": state.get("Position").cloned().unwrap_or(Value::Null),
        "volume": state.get("Volume").cloned().unwrap_or(Value::Null),
    })
}

// The MPRIS method behind each media command other than the volume.
fn media_method(command: &str) -> Option<&'static str> {
    match command {
        "play" => Some("Play"),
        "pause" => Some("Pause"),
        "next" => Some("Next"),
        "previous" => Some("Previous"),
        _ => None,
    }
}

// The volume after one press of volume up or down, kept between silent and full.
fn stepped_volume(current: f64, command: &str) -> f64 {
    let step = if command == "volumeup" { 0.1 } else { -0.1 };
    (current + step).clamp(0.0, 1.0)
}

// ============================================================================
// Desktop integration
// ============================================================================

pub type DesktopSink = Arc<dyn Fn(Value) + Send + Sync>;

#[derive(Clone)]
pub struct DesktopIntegration {
    emit: DesktopSink,
    bus: Arc<tokio::sync::OnceCell<Connection>>,
    notifications_watched: Arc<Mutex<bool>>,
    players: Arc<Mutex<HashMap<String, Player>>>,
}

impl DesktopIntegration {
    pub fn new(emit: DesktopSink) -> Self {
        DesktopIntegration {
            emit,
            bus: Arc::new(tokio::sync::OnceCell::new()),
            notifications_watched: Arc::new(Mutex::new(false)),
            players: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn bus(&self) -> zbus::Result<Connection> {
        self.bus.get_or_try_init(Connection::session).await.cloned()
    }

    // Starts both watches; either may fail alone on a desktop that lacks it.
    pub async fn start(&self) {
        let (_notifications, _players) = tokio::join!(self.watch_notifications(), self.watch_players());
    }

    // ========================================================================
    // Notifications
    // ========================================================================

    // Listens in on what apps send the notification server, which is the only way to read them on Linux.
    async fn watch_notifications(&self) -> zbus::Result<()> {
        let bus = self.bus().await?;
        let server = zbus::fdo::DBusProxy::new(&bus)
            .await?
            .get_name_owner(NOTIFICATIONS_NAME.try_into()?)
            .await?
            .to_string();

        // A monitor connection can never send again, so it is a connection of its own.
        let monitor = Connection::session().await?;
        let rules = [
            format!("type='method_call',interface='{NOTIFICATIONS_NAME}',member='Notify'"),
            format!("type='method_call',interface='{NOTIFICATIONS_NAME}',member='CloseNotification'"),
            format!("type='method_return',sender='{server}'"),
            format!("type='signal',interface='{NOTIFICATIONS_NAME}',member='NotificationClosed'"),
        ];
        let rules: Vec<MatchRule<'_>> = rules
            .iter()
            .map(|rule| MatchRule::try_from(rule.as_str()))
            .collect::<Result<_, _>>()?;
        let mut messages = MessageStream::from(&monitor);
        zbus::fdo::MonitoringProxy::new(&monitor)
            .await?
            .become_monitor(&rules, 0)
            .await?;
        *self.notifications_watched.lock().unwrap() = true;

        let emit = self.emit.clone();
        tokio::spawn(async move {
            // Keeps the monitor connection open for as long as messages are read from it.
            let _monitor = monitor;
            while let Some(Ok(message)) = messages.next().await {
                if let Some(event) = notification_event(&message, &server) {
                    emit(event);
                }
            }
        });
        Ok(())
    }

    // True once notifications are being read.
    pub fn has_notification_access(&self) -> bool {
        *self.notifications_watched.lock().unwrap()
    }

    // ========================================================================
    // Media players
    // ========================================================================

    // Follows every MPRIS player, and tells the window whenever one changes.
    async fn watch_players(&self) -> zbus::Result<()> {
        let bus = self.bus().await?;
        let dbus = zbus::fdo::DBusProxy::new(&bus).await?;
        let mut owner_changes = dbus.receive_name_owner_changed().await?;

        for name in dbus.list_names().await? {
            if name.starts_with(MPRIS_PREFIX) {
                let _ = self.add_player(name.to_string()).await;
            }
        }
        self.emit_players();

        let desktop = self.clone();
        tokio::spawn(async move {
            while let Some(change) = owner_changes.next().await {
                let Ok(arguments) = change.args() else { continue };
                let name = arguments.name().to_string();
                if !name.starts_with(MPRIS_PREFIX) {
                    continue;
                }
                if arguments.new_owner().is_some() {
                    let _ = desktop.add_player(name).await;
                } else {
                    desktop.players.lock().unwrap().remove(&name);
                    desktop.emit_players();
                }
            }
        });
        Ok(())
    }

    async fn player_properties(&self, name: &str) -> zbus::Result<zbus::fdo::PropertiesProxy<'static>> {
        zbus::fdo::PropertiesProxy::builder(&self.bus().await?)
            .destination(name.to_owned())?
            .path(MPRIS_PATH)?
            .build()
            .await
    }

    async fn add_player(&self, name: String) -> zbus::Result<()> {
        let properties = self.player_properties(&name).await?;
        let mut changes = properties.receive_properties_changed().await?;
        let state = plain_map(&properties.get_all(MPRIS_PLAYER.try_into()?).await?);
        self.players
            .lock()
            .unwrap()
            .insert(name.clone(), Player { state });
        self.emit_players();

        let desktop = self.clone();
        tokio::spawn(async move {
            while let Some(change) = changes.next().await {
                let Ok(arguments) = change.args() else { continue };
                if arguments.interface_name().as_str() != MPRIS_PLAYER {
                    continue;
                }
                let changed: Map<String, Value> = arguments
                    .changed_properties()
                    .iter()
                    .map(|(key, value)| (key.to_string(), plain(value)))
                    .collect();
                // Players do not announce the position as it moves, so it is read whenever anything else changes.
                let position = properties
                    .get(MPRIS_PLAYER.try_into().unwrap(), "Position")
                    .await
                    .ok()
                    .map(|value| plain(&value));
                {
                    let mut players = desktop.players.lock().unwrap();
                    let Some(player) = players.get_mut(&name) else {
                        break;
                    };
                    player.state.extend(changed);
                    if let Some(position) = position {
                        player.state.insert("Position".into(), position);
                    }
                }
                desktop.emit_players();
            }
        });
        Ok(())
    }

    pub fn emit_players(&self) {
        let players: Vec<Value> = {
            let players = self.players.lock().unwrap();
            let mut names: Vec<&String> = players.keys().collect();
            names.sort();
            names
                .into_iter()
                .map(|name| player_entry(name, &players[name].state))
                .collect()
        };
        (self.emit)(json!({ "kind": "players", "players": players }));
    }

    pub async fn media_command(&self, player: &str, command: &str) -> Result<(), String> {
        let state = self
            .players
            .lock()
            .unwrap()
            .get(player)
            .map(|found| found.state.clone())
            .ok_or("No media player is running.")?;
        let failed = |error: zbus::Error| error.to_string();

        if command == "volumeup" || command == "volumedown" {
            let current = state.get("Volume").and_then(Value::as_f64).unwrap_or(0.5);
            let properties = self.player_properties(player).await.map_err(failed)?;
            properties
                .set(
                    MPRIS_PLAYER.try_into().unwrap(),
                    "Volume",
                    DbusValue::F64(stepped_volume(current, command)),
                )
                .await
                .map_err(|error| error.to_string())?;
            return Ok(());
        }

        let method = media_method(command).ok_or_else(|| format!("Unknown media command {command}."))?;
        let bus = self.bus().await.map_err(failed)?;
        bus.call_method(Some(player), MPRIS_PATH, Some(MPRIS_PLAYER), method, &())
            .await
            .map_err(failed)?;
        Ok(())
    }

    // ========================================================================
    // Do Not Disturb
    // ========================================================================

    // Each desktop keeps Do Not Disturb its own way, so ask every one that might be running.
    pub async fn is_do_not_disturb(&self) -> bool {
        if let Some(inhibited) = self.notification_server_inhibited().await {
            return inhibited;
        }
        let checks: [(&str, &[&str], &str); 3] = [
            (
                "gsettings",
                &[
                    "get",
                    "org.cinnamon.desktop.notifications",
                    "display-notifications",
                ],
                "false",
            ),
            (
                "gsettings",
                &["get", "org.gnome.desktop.notifications", "show-banners"],
                "false",
            ),
            ("dunstctl", &["is-paused"], "true"),
        ];
        for (command, arguments, quiet_answer) in checks {
            if run_command(command, arguments).await.as_deref() == Some(quiet_answer) {
                return true;
            }
        }
        false
    }

    // KDE and some others say whether notifications are held back on the notification server itself.
    async fn notification_server_inhibited(&self) -> Option<bool> {
        let bus = self.bus().await.ok()?;
        let server = Proxy::new(&bus, NOTIFICATIONS_NAME, NOTIFICATIONS_PATH, NOTIFICATIONS_NAME)
            .await
            .ok()?;
        server.get_property::<bool>("Inhibited").await.ok()
    }

    // ========================================================================
    // Location
    // ========================================================================

    // Asks GeoClue, the desktop's location service, where the computer is.
    pub async fn locate(&self) -> Result<Value, String> {
        match locate_with_geoclue().await {
            Ok(location) => Ok(location),
            Err(error) => {
                let message = error.to_string();
                if ["not activatable", "ServiceUnknown", "was not provided"]
                    .iter()
                    .any(|text| message.contains(text))
                {
                    Err("This computer has no location service (GeoClue) running; choose a place by name instead.".into())
                } else {
                    Err(format!("The location service did not answer: {message}"))
                }
            }
        }
    }
}

// The raw event for one message the notification monitor overheard, or none for anything else.
fn notification_event(message: &zbus::Message, server: &str) -> Option<Value> {
    let header = message.header();
    let member = header.member().map(|member| member.as_str());
    let sender = header.sender().map(|sender| sender.as_str());
    match message.message_type() {
        MessageType::MethodCall if member == Some("Notify") => {
            type Notify = (
                String,
                u32,
                String,
                String,
                String,
                Vec<String>,
                HashMap<String, OwnedValue>,
                i32,
            );
            let (app_name, replaces_id, _icon, summary, body, _actions, hints, _timeout): Notify =
                message.body().deserialize().ok()?;
            Some(json!({
                "kind": "notify",
                "sender": sender,
                "serial": header.primary().serial_num().get(),
                "appName": app_name,
                "replacesId": replaces_id,
                "summary": summary,
                "body": body,
                "hints": plain_map(&hints),
            }))
        }
        MessageType::MethodReturn if sender == Some(server) => {
            let id: u32 = message.body().deserialize().ok()?;
            Some(json!({
                "kind": "notify-reply",
                "destination": header.destination().map(|destination| destination.to_string()),
                "replySerial": header.reply_serial().map(|serial| serial.get()),
                "id": id,
            }))
        }
        MessageType::Signal if member == Some("NotificationClosed") => {
            let (id, _reason): (u32, u32) = message.body().deserialize().ok()?;
            Some(json!({ "kind": "closed", "id": id }))
        }
        MessageType::MethodCall if member == Some("CloseNotification") => {
            let id: u32 = message.body().deserialize().ok()?;
            Some(json!({ "kind": "closed", "id": id }))
        }
        _ => None,
    }
}

async fn locate_with_geoclue() -> zbus::Result<Value> {
    let bus = Connection::system().await?;
    let manager = Proxy::new(
        &bus,
        GEOCLUE_NAME,
        GEOCLUE_MANAGER_PATH,
        "org.freedesktop.GeoClue2.Manager",
    )
    .await?;
    let client_path: zbus::zvariant::OwnedObjectPath = manager.call("GetClient", &()).await?;
    let client = Proxy::new(&bus, GEOCLUE_NAME, client_path, GEOCLUE_CLIENT).await?;
    client.set_property("DesktopId", DESKTOP_ID).await?;
    client
        .set_property("RequestedAccuracyLevel", CITY_ACCURACY)
        .await?;

    let mut updates = client.receive_signal("LocationUpdated").await?;
    client.call_method("Start", &()).await?;
    let update = tokio::time::timeout(LOCATE_TIMEOUT, updates.next())
        .await
        .map_err(|_| zbus::Error::Failure("Timed out waiting for the location service.".into()))?
        .ok_or_else(|| zbus::Error::Failure("The location service stopped.".into()))?;
    let (_old, new_path): (zbus::zvariant::OwnedObjectPath, zbus::zvariant::OwnedObjectPath) =
        update.body().deserialize()?;

    let location = Proxy::new(&bus, GEOCLUE_NAME, new_path, GEOCLUE_LOCATION).await?;
    let result = json!({
        "latitude": location.get_property::<f64>("Latitude").await?,
        "longitude": location.get_property::<f64>("Longitude").await?,
        "description": location.get_property::<String>("Description").await?,
    });
    let _ = client.call_method("Stop", &()).await;
    Ok(result)
}

async fn run_command(command: &str, arguments: &[&str]) -> Option<String> {
    let output = tokio::time::timeout(
        COMMAND_TIMEOUT,
        tokio::process::Command::new(command)
            .args(arguments)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{Array, Dict, Signature, Str};

    #[test]
    fn plain_turns_nested_values_into_json() {
        let mut dict = Dict::new(&Signature::Str, &Signature::Variant);
        dict.add("xesam:title", DbusValue::new("Song")).unwrap();
        dict.add("mpris:length", DbusValue::new(DbusValue::I64(180_000_000)))
            .unwrap();
        assert_eq!(
            plain(&DbusValue::Dict(dict)),
            json!({ "xesam:title": "Song", "mpris:length": 180_000_000 })
        );
        let array = Array::from(vec![Str::from("a"), Str::from("b")]);
        assert_eq!(plain(&DbusValue::Array(array)), json!(["a", "b"]));
        assert_eq!(plain(&DbusValue::Bool(true)), json!(true));
        assert_eq!(plain(&DbusValue::F64(0.5)), json!(0.5));
    }

    #[test]
    fn a_player_without_reports_gets_the_defaults_the_window_expects() {
        assert_eq!(
            player_entry("org.mpris.MediaPlayer2.vlc", &Map::new()),
            json!({ "name": "org.mpris.MediaPlayer2.vlc", "status": "Stopped", "metadata": {}, "position": null, "volume": null })
        );
    }

    #[test]
    fn media_commands_map_onto_mpris_methods() {
        assert_eq!(media_method("play"), Some("Play"));
        assert_eq!(media_method("previous"), Some("Previous"));
        assert_eq!(media_method("eject"), None);
    }

    #[test]
    fn volume_steps_stay_between_silent_and_full() {
        assert!((stepped_volume(0.5, "volumeup") - 0.6).abs() < 1e-9);
        assert_eq!(stepped_volume(0.95, "volumeup"), 1.0);
        assert_eq!(stepped_volume(0.05, "volumedown"), 0.0);
    }

    #[tokio::test]
    async fn a_missing_command_counts_as_no_answer() {
        assert_eq!(run_command("neotime-no-such-command", &[]).await, None);
        assert_eq!(run_command("echo", &["true"]).await.as_deref(), Some("true"));
    }
}
