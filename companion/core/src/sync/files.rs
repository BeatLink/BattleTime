// Adafruit BLE file transfer, the file service the watch serves at 0xFEBB.
//
// The layout is in docs/companion-protocol.md under "File transfer"; `src/protocol/file-service.ts` is
// the window's copy of this client and the two must agree.

use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use tokio::sync::{mpsc, Mutex, OwnedMutexGuard};

// ============================================================================
// Names and numbers
// ============================================================================

pub const FILE_SERVICE: &str = "0000febb-0000-1000-8000-00805f9b34fb";
pub const FILE_TRANSFER: &str = "adaf0200-4669-6c65-5472-616e73666572";

const READ: u8 = 0x10;
const READ_DATA: u8 = 0x11;
const READ_PACING: u8 = 0x12;
const WRITE: u8 = 0x20;
const WRITE_PACING: u8 = 0x21;
const WRITE_DATA: u8 = 0x22;
const DELETE: u8 = 0x30;
const DELETE_STATUS: u8 = 0x31;
const LIST: u8 = 0x50;
const LIST_ENTRY: u8 = 0x51;

const OK: u8 = 0x01;
const NO_FILE: u8 = 0x03;
const PROTOCOL_ERROR: u8 = 0x04;

// The watch's 124-byte MTU less the 3-byte header.
const LONGEST_WRITE: usize = 121;
const WRITE_DATA_HEADER: usize = 12;
const READ_DATA_HEADER: usize = 16;
const LIST_ENTRY_HEADER: usize = 28;
const READ_CHUNK: u32 = 4096;

// How long one answer may take, which covers a sector erase before a write.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(15);

// ============================================================================
// What the client needs and gives
// ============================================================================

// Writes one command to the transfer characteristic; answers come back through `FileClient::on_answer`.
pub trait FileTransport: Send + Sync + 'static {
    fn send(&self, command: Vec<u8>) -> BoxFuture<'static, Result<(), String>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileError {
    Missing(String),
    Failed(String),
}

impl std::fmt::Display for FileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileError::Missing(path) => write!(formatter, "{path} does not exist."),
            FileError::Failed(why) => formatter.write_str(why),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchEntry {
    pub name: String,
    pub size: u32,
    pub folder: bool,
}

// ============================================================================
// The client
// ============================================================================

pub struct FileClient {
    transport: Arc<dyn FileTransport>,
    // One operation at a time, here or in the window, since answers carry no tag.
    service: Arc<Mutex<()>>,
    answers_in: mpsc::UnboundedSender<Vec<u8>>,
    answers: Mutex<mpsc::UnboundedReceiver<Vec<u8>>>,
}

type Turn<'client> = (
    OwnedMutexGuard<()>,
    tokio::sync::MutexGuard<'client, mpsc::UnboundedReceiver<Vec<u8>>>,
);

impl FileClient {
    pub fn new(transport: Arc<dyn FileTransport>) -> FileClient {
        let (answers_in, answers) = mpsc::unbounded_channel();
        FileClient {
            transport,
            service: Arc::new(Mutex::new(())),
            answers_in,
            answers: Mutex::new(answers),
        }
    }

    // Keeps the service for someone else, such as the window's own client, until the guard is dropped.
    pub async fn lease(&self) -> OwnedMutexGuard<()> {
        self.service.clone().lock_owned().await
    }

    // A notification from the transfer characteristic.
    pub fn on_answer(&self, bytes: Vec<u8>) {
        let _ = self.answers_in.send(bytes);
    }

    pub async fn list(&self, folder: &str) -> Result<Vec<WatchEntry>, FileError> {
        let (_turn, mut answers) = self.take_turn().await;
        let mut answer = self
            .command(&mut answers, path_command(LIST, folder)?, LIST_ENTRY, folder)
            .await?;
        let mut entries = Vec::new();
        loop {
            let name_length = u16_at(&answer, 2) as usize;
            if u32_at(&answer, 4) >= u32_at(&answer, 8) {
                break;
            }
            let name = answer
                .get(LIST_ENTRY_HEADER..LIST_ENTRY_HEADER + name_length)
                .ok_or_else(|| cut_short(folder))?;
            let name = String::from_utf8_lossy(name).into_owned();
            if name != "." && name != ".." {
                entries.push(WatchEntry {
                    name,
                    size: u32_at(&answer, 24),
                    folder: u32_at(&answer, 12) & 1 == 1,
                });
            }
            answer = self.answer(&mut answers, LIST_ENTRY, folder).await?;
        }
        Ok(entries)
    }

    pub async fn read(&self, path: &str) -> Result<Vec<u8>, FileError> {
        let (_turn, mut answers) = self.take_turn().await;
        let encoded = checked(path)?;
        let mut command = vec![READ, 0];
        command.extend((encoded.len() as u16).to_le_bytes());
        command.extend(0u32.to_le_bytes());
        command.extend(READ_CHUNK.to_le_bytes());
        command.extend(encoded);
        let mut answer = self.command(&mut answers, command, READ_DATA, path).await?;
        let total = u32_at(&answer, 8) as usize;
        let mut out = Vec::with_capacity(total);
        loop {
            let offset = u32_at(&answer, 4) as usize;
            let length = u32_at(&answer, 12) as usize;
            let piece = answer
                .get(READ_DATA_HEADER..READ_DATA_HEADER + length)
                .filter(|_| offset == out.len() && offset + length <= total)
                .ok_or_else(|| FileError::Failed(format!("{path}: the watch sent a piece out of place.")))?;
            out.extend_from_slice(piece);
            if out.len() >= total {
                return Ok(out);
            }
            if length == 0 {
                return Err(FileError::Failed(format!("{path}: the watch stopped sending.")));
            }
            let mut pacing = vec![READ_PACING, OK, 0, 0];
            pacing.extend((out.len() as u32).to_le_bytes());
            pacing.extend(READ_CHUNK.to_le_bytes());
            answer = self.command(&mut answers, pacing, READ_DATA, path).await?;
        }
    }

    // The watch makes any missing folders and keeps the old file until the last piece arrives.
    pub async fn write(&self, path: &str, contents: &[u8]) -> Result<(), FileError> {
        let (_turn, mut answers) = self.take_turn().await;
        let encoded = checked(path)?;
        let mut command = vec![WRITE, 0];
        command.extend((encoded.len() as u16).to_le_bytes());
        command.extend(0u32.to_le_bytes());
        command.extend(0u64.to_le_bytes());
        command.extend((contents.len() as u32).to_le_bytes());
        command.extend(encoded);
        let mut answer = self.command(&mut answers, command, WRITE_PACING, path).await?;
        let mut offset = 0;
        while offset < contents.len() {
            let allowed = u32_at(&answer, 16) as usize;
            if allowed == 0 {
                return Err(FileError::Failed(format!("{path}: the watch has no room left.")));
            }
            let length = allowed
                .min(LONGEST_WRITE - WRITE_DATA_HEADER)
                .min(contents.len() - offset);
            let mut data = vec![WRITE_DATA, OK, 0, 0];
            data.extend((offset as u32).to_le_bytes());
            data.extend((length as u32).to_le_bytes());
            data.extend(&contents[offset..offset + length]);
            answer = self.command(&mut answers, data, WRITE_PACING, path).await?;
            offset += length;
        }
        Ok(())
    }

    pub async fn remove(&self, path: &str) -> Result<(), FileError> {
        let (_turn, mut answers) = self.take_turn().await;
        self.command(&mut answers, path_command(DELETE, path)?, DELETE_STATUS, path)
            .await
            .map(|_| ())
    }

    // ========================================================================
    // Commands and answers
    // ========================================================================

    // Answers left from an operation that gave up belong to nobody, so they are dropped first.
    async fn take_turn(&self) -> Turn<'_> {
        let turn = self.lease().await;
        let mut answers = self.answers.lock().await;
        while answers.try_recv().is_ok() {}
        (turn, answers)
    }

    async fn command(
        &self,
        answers: &mut mpsc::UnboundedReceiver<Vec<u8>>,
        command: Vec<u8>,
        expected: u8,
        path: &str,
    ) -> Result<Vec<u8>, FileError> {
        if command.len() > LONGEST_WRITE {
            return Err(FileError::Failed(format!(
                "{path}: the path is too long to send."
            )));
        }
        self.transport.send(command).await.map_err(FileError::Failed)?;
        self.answer(answers, expected, path).await
    }

    async fn answer(
        &self,
        answers: &mut mpsc::UnboundedReceiver<Vec<u8>>,
        expected: u8,
        path: &str,
    ) -> Result<Vec<u8>, FileError> {
        let answer = tokio::time::timeout(ANSWER_TIMEOUT, answers.recv())
            .await
            .ok()
            .flatten()
            .ok_or_else(|| FileError::Failed(format!("{path}: the watch did not answer.")))?;
        if answer.first() != Some(&expected) || answer.len() < 2 {
            return Err(FileError::Failed(format!(
                "{path}: the watch answered {:?} to a command expecting {expected}.",
                answer.first()
            )));
        }
        match answer[1] {
            OK => Ok(answer),
            NO_FILE => Err(FileError::Missing(path.to_string())),
            PROTOCOL_ERROR => Err(FileError::Failed(format!(
                "{path}: the watch did not understand the request."
            ))),
            _ => Err(FileError::Failed(format!("{path}: the watch could not do that."))),
        }
    }
}

// Installing an app sends its image through this client first.
impl crate::protocol::packages::FileWriter for FileClient {
    async fn write(&self, path: &str, contents: &[u8]) -> Result<(), String> {
        FileClient::write(self, path, contents)
            .await
            .map_err(|error| error.to_string())
    }
}

// ============================================================================
// Bytes
// ============================================================================

fn checked(path: &str) -> Result<&[u8], FileError> {
    if !path.starts_with('/') || path.split('/').any(|part| part == "..") {
        return Err(FileError::Failed(format!(
            "{path}: give an absolute path with no \"..\"."
        )));
    }
    Ok(path.as_bytes())
}

fn path_command(command: u8, path: &str) -> Result<Vec<u8>, FileError> {
    let encoded = checked(path)?;
    let mut out = vec![command, 0];
    out.extend((encoded.len() as u16).to_le_bytes());
    out.extend(encoded);
    Ok(out)
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    bytes
        .get(at..at + 2)
        .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    bytes
        .get(at..at + 4)
        .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn cut_short(path: &str) -> FileError {
    FileError::Failed(format!("{path}: the watch's answer was cut short."))
}

// ============================================================================
// A watch in memory, for tests here and in the sync engine
// ============================================================================

#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::{Mutex as StdMutex, OnceLock, Weak};

    // Files by absolute path; folders exist through the files in them, as on the watch.
    #[derive(Default)]
    pub(crate) struct FakeWatch {
        pub files: StdMutex<BTreeMap<String, Vec<u8>>>,
        pub commands: StdMutex<Vec<u8>>,
        client: OnceLock<Weak<FileClient>>,
        upload: StdMutex<Option<(String, Vec<u8>, usize)>>,
    }

    impl FakeWatch {
        pub(crate) fn with(files: &[(&str, &[u8])]) -> Arc<FakeWatch> {
            let watch = Arc::new(FakeWatch::default());
            for (path, contents) in files {
                watch
                    .files
                    .lock()
                    .unwrap()
                    .insert(path.to_string(), contents.to_vec());
            }
            watch
        }

        pub(crate) fn client(self: &Arc<FakeWatch>) -> Arc<FileClient> {
            let client = Arc::new(FileClient::new(self.clone()));
            let _ = self.client.set(Arc::downgrade(&client));
            client
        }

        // Records one command and gives the watch's answers, for a test that delivers them itself.
        pub(crate) fn answers(&self, command: &[u8]) -> Vec<Vec<u8>> {
            self.commands.lock().unwrap().push(command[0]);
            self.answer(command)
        }

        fn reply(&self, answers: Vec<Vec<u8>>) {
            if let Some(client) = self.client.get().and_then(Weak::upgrade) {
                for answer in answers {
                    client.on_answer(answer);
                }
            }
        }

        fn answer(&self, command: &[u8]) -> Vec<Vec<u8>> {
            let path = |at: usize| {
                let length = u16_at(command, 2) as usize;
                String::from_utf8_lossy(&command[at..at + length]).into_owned()
            };
            let files = self.files.lock().unwrap();
            match command[0] {
                LIST => {
                    let folder = path(4);
                    let prefix = if folder == "/" {
                        "/".to_string()
                    } else {
                        format!("{folder}/")
                    };
                    let mut names: BTreeMap<String, (u32, bool)> = BTreeMap::new();
                    for (file, contents) in files.iter() {
                        if let Some(rest) = file.strip_prefix(&prefix) {
                            match rest.split_once('/') {
                                Some((name, _)) => names.insert(name.into(), (0, true)),
                                None => names.insert(rest.into(), (contents.len() as u32, false)),
                            };
                        }
                    }
                    let total = names.len() as u32;
                    let entry = |index: u32, name: &str, size: u32, folder: bool| {
                        let mut out = vec![LIST_ENTRY, OK];
                        out.extend((name.len() as u16).to_le_bytes());
                        out.extend(index.to_le_bytes());
                        out.extend(total.to_le_bytes());
                        out.extend((folder as u32).to_le_bytes());
                        out.extend(0u64.to_le_bytes());
                        out.extend(size.to_le_bytes());
                        out.extend(name.as_bytes());
                        out
                    };
                    let mut out: Vec<Vec<u8>> = names
                        .iter()
                        .enumerate()
                        .map(|(index, (name, &(size, folder)))| entry(index as u32, name, size, folder))
                        .collect();
                    out.push(entry(total, "", 0, false));
                    out
                }
                READ | READ_PACING => {
                    let offset = u32_at(command, 4) as usize;
                    let reading = if command[0] == READ {
                        Some(path(12))
                    } else {
                        self.upload
                            .lock()
                            .unwrap()
                            .as_ref()
                            .map(|(path, _, _)| path.clone())
                    };
                    let Some(contents) = reading.as_ref().and_then(|path| files.get(path)) else {
                        return vec![vec![READ_DATA, NO_FILE, 0, 0]];
                    };
                    *self.upload.lock().unwrap() = Some((reading.unwrap(), Vec::new(), 0));
                    // Twenty bytes a notification, as an unextended link carries.
                    let piece = &contents[offset.min(contents.len())..(offset + 20).min(contents.len())];
                    let mut out = vec![READ_DATA, OK, 0, 0];
                    out.extend((offset as u32).to_le_bytes());
                    out.extend((contents.len() as u32).to_le_bytes());
                    out.extend((piece.len() as u32).to_le_bytes());
                    out.extend(piece);
                    vec![out]
                }
                WRITE => {
                    let total = u32_at(command, 16) as usize;
                    *self.upload.lock().unwrap() = Some((path(20), Vec::new(), total));
                    vec![write_answer(0, total)]
                }
                WRITE_DATA => {
                    let mut upload = self.upload.lock().unwrap();
                    let (path, data, total) = upload.as_mut().unwrap();
                    data.extend(&command[WRITE_DATA_HEADER..]);
                    let (done, left) = (data.len(), *total - data.len());
                    if left == 0 {
                        drop(files);
                        self.files.lock().unwrap().insert(path.clone(), data.clone());
                    }
                    vec![write_answer(done, left)]
                }
                DELETE => {
                    drop(files);
                    let path = path(4);
                    let mut files = self.files.lock().unwrap();
                    let before = files.len();
                    files.retain(|file, _| file != &path && !file.starts_with(&format!("{path}/")));
                    let status = if files.len() == before { NO_FILE } else { OK };
                    vec![vec![DELETE_STATUS, status]]
                }
                _ => vec![vec![0, PROTOCOL_ERROR, 0, 0]],
            }
        }
    }

    fn write_answer(offset: usize, left: usize) -> Vec<u8> {
        let mut out = vec![WRITE_PACING, OK, 0, 0];
        out.extend((offset as u32).to_le_bytes());
        out.extend(0u64.to_le_bytes());
        out.extend((left.min(200) as u32).to_le_bytes());
        out
    }

    impl FileTransport for FakeWatch {
        fn send(&self, command: Vec<u8>) -> BoxFuture<'static, Result<(), String>> {
            self.reply(self.answers(&command));
            Box::pin(async { Ok(()) })
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::fake::FakeWatch;
    use super::*;

    #[tokio::test]
    async fn a_folder_lists_its_files_and_folders() {
        let watch = FakeWatch::with(&[
            ("/apps/alarm/state", b"1234"),
            ("/apps/alarm/app.ntapp", b"x"),
            ("/apps/timer/state", b"12"),
        ]);
        let client = watch.client();
        let apps = client.list("/apps").await.unwrap();
        assert_eq!(
            apps.iter()
                .map(|entry| (&entry.name[..], entry.folder))
                .collect::<Vec<_>>(),
            vec![("alarm", true), ("timer", true)]
        );
        let alarm = client.list("/apps/alarm").await.unwrap();
        assert_eq!(
            alarm[1],
            WatchEntry {
                name: "state".into(),
                size: 4,
                folder: false
            }
        );
    }

    #[tokio::test]
    async fn a_long_file_is_read_in_pieces() {
        let contents: Vec<u8> = (0..=255).collect();
        let watch = FakeWatch::with(&[("/steps/2026/10-14", &contents)]);
        let client = watch.client();
        assert_eq!(client.read("/steps/2026/10-14").await.unwrap(), contents);
        assert!(
            watch
                .commands
                .lock()
                .unwrap()
                .iter()
                .filter(|&&c| c == READ_PACING)
                .count()
                > 10
        );
    }

    #[tokio::test]
    async fn a_missing_file_says_so() {
        let client = FakeWatch::with(&[]).client();
        assert_eq!(
            client.read("/apps/alarm/state").await.unwrap_err(),
            FileError::Missing("/apps/alarm/state".into())
        );
        assert!(client.remove("/nothing").await.is_err());
    }

    #[tokio::test]
    async fn a_write_lands_whole_and_a_delete_removes_it() {
        let watch = FakeWatch::with(&[]);
        let client = watch.client();
        let contents = vec![7u8; 300];
        client.write("/apps/alarm/state", &contents).await.unwrap();
        assert_eq!(watch.files.lock().unwrap()["/apps/alarm/state"], contents);
        client.remove("/apps/alarm").await.unwrap();
        assert!(watch.files.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn it_writes_app_images_for_the_package_installer() {
        use crate::protocol::packages::FileWriter;
        let watch = FakeWatch::with(&[]);
        let client = watch.client();
        FileWriter::write(&*client, "/apps/x/app.ntapp", b"NTAP")
            .await
            .unwrap();
        assert_eq!(watch.files.lock().unwrap()["/apps/x/app.ntapp"], b"NTAP");
    }

    #[tokio::test]
    async fn stray_answers_are_dropped_before_the_next_operation() {
        let watch = FakeWatch::with(&[("/a", b"ok")]);
        let client = watch.client();
        client.on_answer(vec![LIST_ENTRY, OK, 0, 0]);
        assert_eq!(client.read("/a").await.unwrap(), b"ok");
    }

    #[tokio::test]
    async fn a_lease_holds_every_operation_until_it_is_dropped() {
        let watch = FakeWatch::with(&[("/a", b"ok")]);
        let client = watch.client();
        let lease = client.lease().await;
        let waiting = tokio::spawn({
            let client = client.clone();
            async move { client.read("/a").await }
        });
        tokio::task::yield_now().await;
        assert!(watch.commands.lock().unwrap().is_empty());
        drop(lease);
        assert_eq!(waiting.await.unwrap().unwrap(), b"ok");
    }

    #[tokio::test]
    async fn bad_paths_never_reach_the_watch() {
        let watch = FakeWatch::with(&[]);
        let client = watch.client();
        assert!(client.read("relative").await.is_err());
        assert!(client.read("/apps/../settings").await.is_err());
        assert!(client.read(&format!("/{}", "x".repeat(200))).await.is_err());
        assert!(watch.commands.lock().unwrap().is_empty());
    }
}
