// The packages characteristic: lists, installs, turns on and off and removes apps and faces.
// A file is sent first through the file transfer service; these commands then check it and act on it.
// The watch side is neotime/services/packages, described in docs/companion-protocol.md.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::app_image::{read_image_header, ImageHeader, ImageKind};
use super::link::{GattLink, WriteMode};
use super::neotime::{NEOTIME_PACKAGES, NEOTIME_SERVICE};

// ============================================================================
// Limits
// ============================================================================

// How long a command may take; checking a large app's code is the slowest.
const REPLY_TIMEOUT: Duration = Duration::from_secs(20);

// One write's payload at the smallest MTU.
const WRITE_BYTES: usize = 20;

// The commands this client sends, so it can ignore lines meant for anyone else on the characteristic.
const COMMANDS: [&str; 7] = ["apps", "install", "remove", "enable", "launcher", "run", "stop"];

// ============================================================================
// Answers
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WatchApp {
    pub id: String,
    pub name: String,
    pub kind: ImageKind,
    pub enabled: bool,
    pub bytes: u32,
    // The heap the app needs while open, when the firmware says.
    pub need: Option<u32>,
    // The launcher group it declares, empty when none.
    pub group: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WatchApps {
    pub apps: Vec<WatchApp>,
    // The most heap an app may need.
    pub room: Option<u32>,
}

// What the watch said about the heap after installing an app or turning it on or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct MemoryAnswer {
    pub need: Option<u32>,
    pub room: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackagesError {
    pub message: String,
    // The refusal's number from docs/companion-protocol.md, when the watch refused a change.
    pub why: Option<u32>,
    pub need: Option<u32>,
    pub room: Option<u32>,
}

impl PackagesError {
    fn plain(message: impl Into<String>) -> PackagesError {
        PackagesError {
            message: message.into(),
            why: None,
            need: None,
            room: None,
        }
    }
}

impl std::fmt::Display for PackagesError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.need, self.room) {
            (Some(need), Some(room)) => {
                write!(
                    formatter,
                    "{} (needs {} of {})",
                    self.message,
                    memory_text(need),
                    memory_text(room)
                )
            }
            _ => formatter.write_str(&self.message),
        }
    }
}

// Bytes as kilobytes with one decimal, as the apps screen and the command line show memory.
pub fn memory_text(bytes: u32) -> String {
    format!("{:.1} kB", bytes as f64 / 1024.0)
}

#[derive(Debug, Default, Deserialize)]
struct Reply {
    c: String,
    err: Option<String>,
    why: Option<u32>,
    need: Option<u32>,
    room: Option<u32>,
    #[serde(default)]
    a: Vec<Vec<Value>>,
}

pub fn app_image_path(id: &str) -> Result<String, PackagesError> {
    let fits = !id.is_empty()
        && id.len() <= 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    if fits {
        Ok(format!("/apps/{id}/app.ntapp"))
    } else {
        Err(PackagesError::plain(format!(
            "{id}: an app id is 1 to 32 letters, digits, \"-\" and \"_\"."
        )))
    }
}

fn app_from_row(row: &[Value]) -> Option<WatchApp> {
    let text = |at: usize| {
        row.get(at)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let number = |at: usize| row.get(at).and_then(Value::as_u64).map(|value| value as u32);
    Some(WatchApp {
        id: row.first()?.as_str()?.to_string(),
        name: text(1),
        kind: if text(2) == "face" {
            ImageKind::Face
        } else {
            ImageKind::App
        },
        enabled: number(3).unwrap_or(0) != 0,
        bytes: number(4).unwrap_or(0),
        need: number(5),
        group: text(6),
    })
}

// ============================================================================
// The client
// ============================================================================

pub struct PackagesClient<'link, Link: GattLink> {
    link: &'link Link,
    replies: tokio::sync::Mutex<mpsc::UnboundedReceiver<String>>,
}

impl<'link, Link: GattLink> PackagesClient<'link, Link> {
    pub async fn open(link: &'link Link) -> Result<Self, String> {
        let (sender, receiver) = mpsc::unbounded_channel();
        let pending = Arc::new(Mutex::new(String::new()));
        link.listen(
            NEOTIME_SERVICE,
            NEOTIME_PACKAGES,
            Arc::new(move |value: &[u8]| {
                let mut pending = pending.lock().unwrap();
                pending.push_str(&String::from_utf8_lossy(value));
                while let Some(newline) = pending.find('\n') {
                    let line: String = pending.drain(..=newline).collect();
                    let line = line.trim().to_string();
                    let ours = serde_json::from_str::<Value>(&line)
                        .ok()
                        .and_then(|reply| reply.get("c").and_then(Value::as_str).map(str::to_string))
                        .is_some_and(|command| COMMANDS.contains(&command.as_str()));
                    if ours {
                        let _ = sender.send(line);
                    }
                }
            }),
        )
        .await?;
        Ok(PackagesClient {
            link,
            replies: tokio::sync::Mutex::new(receiver),
        })
    }

    // Every installed app and face, with each one's heap need and the most an app may have.
    pub async fn listing(&self) -> Result<WatchApps, PackagesError> {
        let reply = self.command(json!({ "c": "apps" })).await?;
        Ok(WatchApps {
            apps: reply.a.iter().filter_map(|row| app_from_row(row)).collect(),
            room: reply.room,
        })
    }

    pub async fn install(&self, id: &str) -> Result<MemoryAnswer, PackagesError> {
        app_image_path(id)?;
        Ok(memory_of(
            &self.command(json!({ "c": "install", "id": id })).await?,
        ))
    }

    pub async fn remove(&self, id: &str) -> Result<(), PackagesError> {
        app_image_path(id)?;
        self.command(json!({ "c": "remove", "id": id })).await.map(|_| ())
    }

    // Turns an app on or off; the watch first checks it fits, and refuses with the reason when not.
    pub async fn enable(&self, id: &str, on: bool) -> Result<MemoryAnswer, PackagesError> {
        app_image_path(id)?;
        Ok(memory_of(
            &self.command(json!({ "c": "enable", "id": id, "on": on })).await?,
        ))
    }

    // Has the launcher read /settings/launcher again, after it was written.
    pub async fn reload_launcher(&self) -> Result<(), PackagesError> {
        self.command(json!({ "c": "launcher" })).await.map(|_| ())
    }

    // Sends one command and waits for its reply, one command at a time.
    async fn command(&self, message: Value) -> Result<Reply, PackagesError> {
        let command = message["c"].as_str().unwrap_or_default().to_string();
        let mut replies = self.replies.lock().await;
        while replies.try_recv().is_ok() {}
        let line = format!("{message}\n");
        for piece in line.as_bytes().chunks(WRITE_BYTES) {
            self.link
                .write(NEOTIME_SERVICE, NEOTIME_PACKAGES, piece, WriteMode::Request)
                .await
                .map_err(PackagesError::plain)?;
        }
        let deadline = tokio::time::Instant::now() + REPLY_TIMEOUT;
        loop {
            let line = tokio::time::timeout_at(deadline, replies.recv())
                .await
                .map_err(|_| PackagesError::plain(format!("The watch did not answer \"{command}\".")))?
                .ok_or_else(|| PackagesError::plain("The watch went away."))?;
            let Ok(reply) = serde_json::from_str::<Reply>(&line) else {
                continue;
            };
            if reply.c != command {
                continue;
            }
            if let Some(error) = reply.err {
                return Err(PackagesError {
                    message: error,
                    why: reply.why,
                    need: reply.need,
                    room: reply.room,
                });
            }
            return Ok(reply);
        }
    }
}

fn memory_of(reply: &Reply) -> MemoryAnswer {
    MemoryAnswer {
        need: reply.need,
        room: reply.room,
    }
}

// ============================================================================
// Installing an image
// ============================================================================

// What installing needs from the file transfer service, which src/sync/files.rs provides.
pub trait FileWriter: Send + Sync {
    fn write(&self, path: &str, contents: &[u8]) -> impl Future<Output = Result<(), String>> + Send;
}

// Sends an app or face image and installs it, returning what it was. The shared library ships inside the firmware.
pub async fn install_image<Link: GattLink>(
    files: &impl FileWriter,
    packages: &PackagesClient<'_, Link>,
    image: &[u8],
) -> Result<ImageHeader, PackagesError> {
    let header = read_image_header(image).map_err(PackagesError::plain)?;
    if header.kind == ImageKind::Library {
        return Err(PackagesError::plain(
            "The shared library ships inside the firmware; update the firmware instead.",
        ));
    }
    let path = app_image_path(&header.id)?;
    files.write(&path, image).await.map_err(PackagesError::plain)?;
    packages.install(&header.id).await?;
    Ok(header)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::app_image::tests::image;
    use crate::protocol::link::tests::FakeWatch;

    // A watch that answers each whole command line with `answer(command)`, split into small notifications.
    fn watch_answering(answer: impl Fn(&Value) -> Value + Send + 'static) -> FakeWatch {
        let watch = FakeWatch::serving(&[(NEOTIME_PACKAGES, vec![])]);
        let mut pending = String::new();
        *watch.answer.lock().unwrap() = Some(Box::new(move |_, data| {
            pending.push_str(&String::from_utf8_lossy(data));
            let Some(newline) = pending.find('\n') else {
                return Vec::new();
            };
            let line: String = pending.drain(..=newline).collect();
            let command: Value = serde_json::from_str(line.trim()).unwrap();
            let reply = format!("{{\"c\":\"other\"}}\n{}\n", answer(&command));
            reply.as_bytes().chunks(7).map(<[u8]>::to_vec).collect()
        }));
        watch
    }

    #[tokio::test]
    async fn the_listing_reads_every_row_and_skips_other_lines() {
        let watch = watch_answering(|_| {
            json!({ "c": "apps", "room": 16384, "a": [
                ["digital", "Digital", "face", 1, 4000, 9000],
                ["buzz-back", "Buzz back", "app", 0, 484, 2000, "tools"]
            ]})
        });
        let packages = PackagesClient::open(&watch).await.unwrap();
        let listing = packages.listing().await.unwrap();
        assert_eq!(listing.room, Some(16384));
        assert_eq!(listing.apps.len(), 2);
        assert_eq!(listing.apps[0].kind, ImageKind::Face);
        assert_eq!(listing.apps[1].group, "tools");
        assert!(!listing.apps[1].enabled);
        let written: Vec<u8> = watch
            .writes
            .lock()
            .unwrap()
            .iter()
            .flat_map(|(_, data, _)| data.clone())
            .collect();
        assert_eq!(written, b"{\"c\":\"apps\"}\n");
    }

    #[tokio::test]
    async fn a_refusal_carries_the_reason_and_the_memory() {
        let watch = watch_answering(
            |command| json!({ "c": command["c"], "err": "it needs more memory than an app can have", "why": 1, "need": 20480, "room": 16384 }),
        );
        let packages = PackagesClient::open(&watch).await.unwrap();
        let error = packages.enable("big", true).await.unwrap_err();
        assert_eq!(error.why, Some(1));
        assert_eq!(
            error.to_string(),
            "it needs more memory than an app can have (needs 20.0 kB of 16.0 kB)"
        );
    }

    #[tokio::test]
    async fn a_bad_id_never_reaches_the_watch() {
        let watch = watch_answering(|command| json!({ "c": command["c"], "ok": 1 }));
        let packages = PackagesClient::open(&watch).await.unwrap();
        assert!(packages
            .remove("../x")
            .await
            .unwrap_err()
            .message
            .contains("an app id"));
        assert!(watch.writes.lock().unwrap().is_empty());
    }

    struct Files(Mutex<Vec<(String, usize)>>);

    impl FileWriter for Files {
        async fn write(&self, path: &str, contents: &[u8]) -> Result<(), String> {
            self.0.lock().unwrap().push((path.to_string(), contents.len()));
            Ok(())
        }
    }

    #[tokio::test]
    async fn installing_sends_the_image_then_installs_it_by_its_own_id() {
        let watch =
            watch_answering(|command| json!({ "c": command["c"], "ok": 1, "need": 2000, "room": 16384 }));
        let packages = PackagesClient::open(&watch).await.unwrap();
        let files = Files(Mutex::new(Vec::new()));
        let app = image(0, &["buzz-back", "Buzz back"], None);
        let header = install_image(&files, &packages, &app).await.unwrap();
        assert_eq!(header.id, "buzz-back");
        assert_eq!(
            files.0.lock().unwrap().as_slice(),
            [("/apps/buzz-back/app.ntapp".to_string(), app.len())]
        );
        let written: Vec<u8> = watch
            .writes
            .lock()
            .unwrap()
            .iter()
            .flat_map(|(_, data, _)| data.clone())
            .collect();
        assert_eq!(written, b"{\"c\":\"install\",\"id\":\"buzz-back\"}\n");
    }

    #[tokio::test]
    async fn the_shared_library_is_refused() {
        let watch = watch_answering(|command| json!({ "c": command["c"], "ok": 1 }));
        let packages = PackagesClient::open(&watch).await.unwrap();
        let files = Files(Mutex::new(Vec::new()));
        let library = image(2, &["shared", "Shared"], None);
        assert!(install_image(&files, &packages, &library)
            .await
            .unwrap_err()
            .message
            .contains("firmware"));
        assert!(files.0.lock().unwrap().is_empty());
    }
}
