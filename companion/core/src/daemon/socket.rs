// The daemon's local socket, through which the command line tool shares the watch with the window.
//
// Each line is one JSON object. A request `{id, command, args}` is answered `{id, ok, value}` or
// `{id, ok: false, error}`, and every event the window hears arrives as `{event, payload}`. The
// commands are the window's own from tauri/src/bridge.js; docs/companion/command-line.md describes it.

use std::collections::HashMap;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use futures::future::BoxFuture;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, mpsc};

use super::{Daemon, HostEvent};

pub const SOCKET_NAME: &str = "battletime-companion.sock";

// How long to wait after a failed accept, so a full file table does not spin the loop.
const ACCEPT_RETRY: Duration = Duration::from_millis(100);

// ============================================================================
// Where the socket lives
// ============================================================================

// The socket goes in the session's runtime folder, or in the app's data folder when there is none.
pub fn socket_path(runtime_dir: Option<&Path>, data_dir: &Path) -> PathBuf {
    match runtime_dir {
        Some(folder) if !folder.as_os_str().is_empty() => folder.join(SOCKET_NAME),
        _ => data_dir.join(SOCKET_NAME),
    }
}

pub fn default_socket_path(data_dir: &Path) -> PathBuf {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    socket_path(runtime_dir.as_deref(), data_dir)
}

// ============================================================================
// The protocol
// ============================================================================

// Whatever answers the socket's commands; `client` names the connection that asked.
pub trait SocketCommands: Send + Sync + 'static {
    fn run<'a>(&'a self, client: u64, command: &'a str, args: Value) -> BoxFuture<'a, Result<Value, String>>;
    // Gives back whatever the connection still held when it closed.
    fn client_closed(&self, client: u64);
}

// The answer to one request line, or None for a blank line.
pub async fn answer_line(commands: &dyn SocketCommands, client: u64, line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let request: Value = match serde_json::from_str(line) {
        Ok(request) => request,
        Err(error) => {
            return Some(reply_line(
                &Value::Null,
                Err(format!("The request is not JSON: {error}")),
            ))
        }
    };
    let id = request["id"].clone();
    let Some(command) = request["command"].as_str() else {
        return Some(reply_line(&id, Err("The request names no command.".into())));
    };
    let args = match &request["args"] {
        Value::Null => json!({}),
        args => args.clone(),
    };
    Some(reply_line(&id, commands.run(client, command, args).await))
}

pub fn reply_line(id: &Value, result: Result<Value, String>) -> String {
    match result {
        Ok(value) => json!({ "id": id, "ok": true, "value": value }),
        Err(error) => json!({ "id": id, "ok": false, "error": error }),
    }
    .to_string()
}

pub fn event_line(event: &str, payload: &Value) -> String {
    json!({ "event": event, "payload": payload }).to_string()
}

// ============================================================================
// The socket
// ============================================================================

// Binds the socket for this user alone, replacing one a companion that has gone left behind.
pub async fn bind(path: &Path) -> io::Result<UnixListener> {
    if path.exists() {
        if UnixStream::connect(path).await.is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                format!("another companion already answers at {}", path.display()),
            ));
        }
        std::fs::remove_file(path)?;
    }
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

// Answers every connection for as long as the listener lives, each hearing the events from `subscribe`.
pub async fn serve(
    listener: UnixListener,
    commands: Arc<dyn SocketCommands>,
    subscribe: Arc<dyn Fn() -> broadcast::Receiver<HostEvent> + Send + Sync>,
) {
    let mut next_client = 0;
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            tokio::time::sleep(ACCEPT_RETRY).await;
            continue;
        };
        next_client += 1;
        tokio::spawn(serve_client(stream, next_client, commands.clone(), subscribe()));
    }
}

// Answers one connection's requests in the order they came, while its events flow beside them.
pub async fn serve_client(
    stream: UnixStream,
    client: u64,
    commands: Arc<dyn SocketCommands>,
    mut events: broadcast::Receiver<HostEvent>,
) {
    let (reader, mut writer) = stream.into_split();
    let (outgoing, mut to_write) = mpsc::unbounded_channel::<String>();
    let writing = tokio::spawn(async move {
        while let Some(mut line) = to_write.recv().await {
            line.push('\n');
            if writer.write_all(line.as_bytes()).await.is_err() {
                return;
            }
        }
    });
    let event_outgoing = outgoing.clone();
    let forwarding = tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok((event, payload)) => {
                    if event_outgoing.send(event_line(&event, &payload)).is_err() {
                        return;
                    }
                }
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return,
            }
        }
    });

    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(answer) = answer_line(&*commands, client, &line).await {
            if outgoing.send(answer).is_err() {
                break;
            }
        }
    }

    forwarding.abort();
    let _ = forwarding.await;
    drop(outgoing);
    let _ = writing.await;
    commands.client_closed(client);
}

// Serves the daemon at `path` until the process ends.
pub async fn serve_daemon(daemon: &Arc<Daemon>, path: &Path) -> io::Result<()> {
    let listener = bind(path).await?;
    let commands = Arc::new(DaemonCommands::new(daemon));
    let events = Arc::downgrade(daemon);
    let subscribe = Arc::new(move || match events.upgrade() {
        Some(daemon) => daemon.events(),
        None => broadcast::channel(1).1,
    });
    serve(listener, commands, subscribe).await;
    Ok(())
}

// ============================================================================
// The daemon's answers
// ============================================================================

// What one connection holds, given back when it closes so a crashed tool never keeps the watch.
#[derive(Default)]
struct ClientTurns {
    file_leases: Vec<u64>,
    holding: bool,
    scanning: bool,
}

pub struct DaemonCommands {
    daemon: Weak<Daemon>,
    clients: Mutex<HashMap<u64, ClientTurns>>,
}

impl DaemonCommands {
    pub fn new(daemon: &Arc<Daemon>) -> Self {
        DaemonCommands {
            daemon: Arc::downgrade(daemon),
            clients: Mutex::new(HashMap::new()),
        }
    }

    fn turns(&self, client: u64, change: impl FnOnce(&mut ClientTurns)) {
        change(self.clients.lock().unwrap().entry(client).or_default());
    }

    async fn answer(&self, client: u64, command: &str, args: &Value) -> Result<Value, String> {
        let daemon = self
            .daemon
            .upgrade()
            .ok_or_else(|| "The companion is shutting down.".to_string())?;
        let text = |name: &str| {
            args[name]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("{command} needs {name}."))
        };
        let bytes = |name: &str| {
            serde_json::from_value::<Vec<u8>>(args[name].clone())
                .map_err(|_| format!("{command} needs {name} as bytes."))
        };

        match command {
            "daemon_state" => return Ok(daemon.state()),
            "daemon_connect" => {
                daemon
                    .connect(&text("id")?, args["name"].as_str().unwrap_or_default())
                    .await?;
                return Ok(Value::Null);
            }
            "daemon_disconnect" => {
                daemon.disconnect().await?;
                return Ok(Value::Null);
            }
            "daemon_hold" => {
                let held = args["held"].as_bool().unwrap_or(false);
                daemon.hold(held);
                self.turns(client, |turns| turns.holding = held);
                return Ok(Value::Null);
            }
            "files_begin" => {
                let number = daemon.begin_files_turn().await;
                self.turns(client, |turns| turns.file_leases.push(number));
                return Ok(json!(number));
            }
            "files_end" => {
                let number = args["lease"].as_u64().ok_or("files_end needs lease.")?;
                daemon.lease().release(Some(number));
                self.turns(client, |turns| turns.file_leases.retain(|held| *held != number));
                return Ok(Value::Null);
            }
            // The window or the daemon's sync may share the subscription, so the client only stops listening.
            "ble_gatt_unsubscribe" => return Ok(Value::Null),
            _ => {}
        }

        let ble = daemon
            .ble()
            .ok_or_else(|| "This companion has no Bluetooth.".to_string())?;
        let answer = match command {
            "ble_start_scan" => {
                daemon.set_scanning(true);
                self.turns(client, |turns| turns.scanning = true);
                ble.start_scan().await.map(|_| Value::Null)
            }
            "ble_stop_scan" => {
                daemon.set_scanning(false);
                self.turns(client, |turns| turns.scanning = false);
                ble.stop_scan().await.map(|_| Value::Null)
            }
            "ble_write" => ble.write(&text("text")?).await.map(|_| Value::Null),
            "ble_write_bytes" => ble.write_bytes(&bytes("data")?).await.map(|_| Value::Null),
            "ble_gatt_has" => Ok(json!(ble.gatt_has(&text("service")?, &text("characteristic")?))),
            "ble_gatt_read" => ble
                .gatt_read(&text("service")?, &text("characteristic")?)
                .await
                .map(|value| json!(value)),
            "ble_gatt_write" => ble
                .gatt_write(
                    &text("service")?,
                    &text("characteristic")?,
                    &bytes("data")?,
                    &text("mode")?,
                )
                .await
                .map(|_| Value::Null),
            "ble_gatt_subscribe" => ble
                .gatt_subscribe(&text("service")?, &text("characteristic")?)
                .await
                .map(|_| Value::Null),
            "ble_reconnect" => ble.reconnect(&text("id")?).await.map(|_| Value::Null),
            other => return Err(format!("Unknown command: {other}")),
        };
        answer.map_err(|error| error.0)
    }
}

impl SocketCommands for DaemonCommands {
    fn run<'a>(&'a self, client: u64, command: &'a str, args: Value) -> BoxFuture<'a, Result<Value, String>> {
        Box::pin(async move { self.answer(client, command, &args).await })
    }

    fn client_closed(&self, client: u64) {
        let Some(turns) = self.clients.lock().unwrap().remove(&client) else {
            return;
        };
        let Some(daemon) = self.daemon.upgrade() else {
            return;
        };
        for number in turns.file_leases {
            daemon.lease().release(Some(number));
        }
        if turns.holding {
            daemon.hold(false);
        }
        if turns.scanning {
            daemon.set_scanning(false);
            if let Some(ble) = daemon.ble().cloned() {
                daemon.runtime().spawn(async move {
                    let _ = ble.stop_scan().await;
                });
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::tests::rig_with;

    // Answers `echo` with its arguments and fails anything else, remembering who closed.
    #[derive(Default)]
    struct Echo {
        closed: Mutex<Vec<u64>>,
    }

    impl SocketCommands for Echo {
        fn run<'a>(
            &'a self,
            client: u64,
            command: &'a str,
            args: Value,
        ) -> BoxFuture<'a, Result<Value, String>> {
            Box::pin(async move {
                match command {
                    "echo" => Ok(json!({ "client": client, "args": args })),
                    other => Err(format!("Unknown command: {other}")),
                }
            })
        }

        fn client_closed(&self, client: u64) {
            self.closed.lock().unwrap().push(client);
        }
    }

    fn parsed(line: Option<String>) -> Value {
        serde_json::from_str(&line.unwrap()).unwrap()
    }

    #[tokio::test]
    async fn requests_are_answered_with_their_id() {
        let echo = Echo::default();
        let answer = parsed(answer_line(&echo, 3, r#"{"id":7,"command":"echo","args":{"a":1}}"#).await);
        assert_eq!(
            answer,
            json!({ "id": 7, "ok": true, "value": { "client": 3, "args": { "a": 1 } } })
        );

        let answer = parsed(answer_line(&echo, 3, r#"{"id":"x","command":"echo"}"#).await);
        assert_eq!(answer["value"]["args"], json!({}));

        let answer = parsed(answer_line(&echo, 3, r#"{"id":8,"command":"nope"}"#).await);
        assert_eq!(
            answer,
            json!({ "id": 8, "ok": false, "error": "Unknown command: nope" })
        );
    }

    #[tokio::test]
    async fn a_bad_line_is_refused_and_a_blank_one_ignored() {
        let echo = Echo::default();
        assert_eq!(answer_line(&echo, 1, "   ").await, None);
        let answer = parsed(answer_line(&echo, 1, "not json").await);
        assert_eq!(answer["ok"], false);
        assert_eq!(answer["id"], Value::Null);
        let answer = parsed(answer_line(&echo, 1, r#"{"id":2}"#).await);
        assert_eq!(
            answer,
            json!({ "id": 2, "ok": false, "error": "The request names no command." })
        );
    }

    #[test]
    fn the_socket_lives_in_the_runtime_folder_when_there_is_one() {
        let data = Path::new("/data");
        assert_eq!(
            socket_path(Some(Path::new("/run/user/1000")), data),
            Path::new("/run/user/1000/battletime-companion.sock")
        );
        assert_eq!(
            socket_path(Some(Path::new("")), data),
            Path::new("/data/battletime-companion.sock")
        );
        assert_eq!(
            socket_path(None, data),
            Path::new("/data/battletime-companion.sock")
        );
    }

    #[tokio::test]
    async fn a_client_on_the_socket_gets_answers_and_events_and_its_close_is_seen() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join(SOCKET_NAME);
        let listener = bind(&path).await.unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);

        let echo = Arc::new(Echo::default());
        let (events, _) = broadcast::channel::<HostEvent>(16);
        let sender = events.clone();
        tokio::spawn(serve(
            listener,
            echo.clone(),
            Arc::new(move || sender.subscribe()),
        ));

        let stream = UnixStream::connect(&path).await.unwrap();
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();
        writer
            .write_all(b"{\"id\":1,\"command\":\"echo\",\"args\":[5]}\n")
            .await
            .unwrap();
        let answer: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(
            answer,
            json!({ "id": 1, "ok": true, "value": { "client": 1, "args": [5] } })
        );

        events
            .send(("daemon:state".into(), json!({ "connection": "connected" })))
            .unwrap();
        let event: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(
            event,
            json!({ "event": "daemon:state", "payload": { "connection": "connected" } })
        );

        drop(writer);
        drop(lines);
        for _ in 0..100 {
            if !echo.closed.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(*echo.closed.lock().unwrap(), vec![1]);
    }

    #[tokio::test]
    async fn a_live_socket_is_not_taken_over_but_a_stale_one_is() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join(SOCKET_NAME);
        let first = bind(&path).await.unwrap();
        assert_eq!(bind(&path).await.unwrap_err().kind(), io::ErrorKind::AddrInUse);
        drop(first);
        bind(&path).await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn a_closed_client_gives_back_its_file_turn_and_its_hold() {
        let rig = rig_with(Value::Null, |_| {});
        let commands = DaemonCommands::new(&rig.daemon);

        let state = commands.run(4, "daemon_state", json!({})).await.unwrap();
        assert_eq!(state["connection"], "disconnected");

        let lease = commands.run(4, "files_begin", json!({})).await.unwrap();
        assert!(lease.is_u64());
        assert!(rig.daemon.lease().held());
        commands
            .run(4, "daemon_hold", json!({ "held": true }))
            .await
            .unwrap();
        assert!(rig.daemon.holds.lock().unwrap().held);

        commands.client_closed(4);
        assert!(!rig.daemon.lease().held());
        assert!(!rig.daemon.holds.lock().unwrap().held);
    }

    #[tokio::test(start_paused = true)]
    async fn a_daemon_without_bluetooth_refuses_its_commands() {
        let rig = rig_with(Value::Null, |_| {});
        let commands = DaemonCommands::new(&rig.daemon);
        let refused = commands
            .run(1, "ble_write", json!({ "text": "hi" }))
            .await
            .unwrap_err();
        assert_eq!(refused, "This companion has no Bluetooth.");
        let refused = commands.run(1, "files_end", json!({})).await.unwrap_err();
        assert_eq!(refused, "files_end needs lease.");
    }

    #[tokio::test(start_paused = true)]
    async fn the_daemon_hands_its_events_to_every_listener() {
        let rig = rig_with(Value::Null, |_| {});
        let mut events = rig.daemon.events();
        rig.daemon.emit("daemon:sent", json!("hello"));
        assert_eq!(
            events.recv().await.unwrap(),
            ("daemon:sent".to_string(), json!("hello"))
        );
        assert!(rig
            .events
            .lock()
            .unwrap()
            .iter()
            .any(|(name, _)| name == "daemon:sent"));
    }
}
