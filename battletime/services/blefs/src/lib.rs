//! Adafruit BLE file transfer: the file service InfiniTime calls `FSService`, which ITD,
//! Gadgetbridge, InfiniTime's resource loader and BattleTime's companion app use to read, write, list,
//! move and delete files on the watch.
//!
//! The phone writes one command to the transfer characteristic and the watch answers with one or
//! more notifications on it. `FileTransfer` turns a command into its answers against BattleTime's flash
//! store, with no Bluetooth code in it; the `trouble` feature adds the GATT service.
//! Numbers are little-endian, as in Adafruit's description of the protocol.
//!
//! BattleTime follows Adafruit's description where InfiniTime strays from it: a write replaces the file
//! with exactly its total size, a write answer gives the offset of the next write, and missing files
//! report the no-file status. The store has no folders of their own, so a folder exists through the
//! files in it, and a new upload replaces the old file only when its last piece arrives.
//!
//! Top-level folders can be locked, as the firmware locks the apps while updates are off: a command
//! that would change anything in a locked folder, the folder itself or the root above it is answered
//! with the error status.

#![no_std]

extern crate alloc;

use alloc::{string::String, vec, vec::Vec};

use battletime_flash_store::{FileAccess, FileError, SMALL_FILE_BYTES};

#[cfg(feature = "trouble")]
pub mod service;

#[cfg(test)]
mod tests;

// ============================================================================
// The protocol's numbers
// ============================================================================

/// Read by the phone to learn which version of the protocol the watch speaks; InfiniTime reports 4.
pub const PROTOCOL_VERSION: u32 = 4;

pub mod command {
    pub const READ: u8 = 0x10;
    pub const READ_DATA: u8 = 0x11;
    pub const READ_PACING: u8 = 0x12;
    pub const WRITE: u8 = 0x20;
    pub const WRITE_PACING: u8 = 0x21;
    pub const WRITE_DATA: u8 = 0x22;
    pub const DELETE: u8 = 0x30;
    pub const DELETE_STATUS: u8 = 0x31;
    pub const MAKE_DIRECTORY: u8 = 0x40;
    pub const MAKE_DIRECTORY_STATUS: u8 = 0x41;
    pub const LIST_DIRECTORY: u8 = 0x50;
    pub const LIST_DIRECTORY_ENTRY: u8 = 0x51;
    pub const MOVE: u8 = 0x60;
    pub const MOVE_STATUS: u8 = 0x61;
}

pub mod status {
    pub const OK: u8 = 0x01;
    pub const ERROR: u8 = 0x02;
    pub const NO_FILE: u8 = 0x03;
    pub const PROTOCOL_ERROR: u8 = 0x04;
}

/// The bytes before the file data in a read answer.
pub const READ_DATA_HEADER_SIZE: usize = 16;
/// The bytes before the name in a directory entry.
pub const LIST_DIRECTORY_ENTRY_HEADER_SIZE: usize = 28;

const READ_HEADER_SIZE: usize = 12;
const READ_PACING_SIZE: usize = 12;
const WRITE_HEADER_SIZE: usize = 20;
const WRITE_PACING_SIZE: usize = 20;
const WRITE_DATA_HEADER_SIZE: usize = 12;
const DELETE_HEADER_SIZE: usize = 4;
const MAKE_DIRECTORY_HEADER_SIZE: usize = 16;
const MAKE_DIRECTORY_STATUS_SIZE: usize = 16;
const LIST_DIRECTORY_HEADER_SIZE: usize = 4;
const MOVE_HEADER_SIZE: usize = 6;

/// Directory entry flag for a folder.
const IS_DIRECTORY: u32 = 1;

// ============================================================================
// Reading the numbers in a command
// ============================================================================

fn read_u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// The path that starts at `at` and runs for `length` bytes, if the command holds all of it as text.
fn read_path(bytes: &[u8], at: usize, length: usize) -> Option<&str> {
    core::str::from_utf8(bytes.get(at..at + length)?).ok()
}

// ============================================================================
// The transfer in progress
// ============================================================================

/// Files up to this size are gathered in RAM and written whole, so the store keeps them as records.
const SMALL_UPLOAD_BYTES: u32 = SMALL_FILE_BYTES as u32;
/// How much of a file a copy for a move holds in RAM at once.
const COPY_PIECE_BYTES: usize = 256;

/// A file being written, a piece at a time.
struct Upload {
    path: String,
    total_size: u32,
    next_offset: u32,
    /// A small file's bytes, written whole when the last piece arrives; empty for a large one, which streams.
    gathered: Vec<u8>,
}

impl Upload {
    fn is_small(&self) -> bool {
        self.total_size <= SMALL_UPLOAD_BYTES
    }
}

/// Answers the phone's file commands; it remembers the file being read or written between commands.
pub struct FileTransfer {
    reading: Option<String>,
    writing: Option<Upload>,
    /// Top-level folders, such as `/apps`, whose files the phone may read but not change.
    locked_folders: &'static [&'static str],
}

impl Default for FileTransfer {
    fn default() -> Self {
        Self::new()
    }
}

impl FileTransfer {
    pub fn new() -> FileTransfer {
        FileTransfer {
            reading: None,
            writing: None,
            locked_folders: &[],
        }
    }

    /// Sets the top-level folders whose files the phone may read but not write, delete, make or move.
    pub fn lock_folders(&mut self, folders: &'static [&'static str]) {
        self.locked_folders = folders;
    }

    /// Forgets a read or write the phone left unfinished, such as when it disconnects.
    ///
    /// A large upload stays open in the store, so the phone can resume it where it stopped.
    pub fn abort(&mut self) {
        self.reading = None;
        self.writing = None;
    }

    pub fn is_writing(&self) -> bool {
        self.writing.is_some()
    }

    /// The error answer to one command, for a build that keeps no store.
    pub fn refuse(request: &[u8]) -> Vec<Vec<u8>> {
        request
            .first()
            .and_then(|command_byte| failure_for(*command_byte, status::ERROR))
            .into_iter()
            .collect()
    }

    /// The answers to one command, each to be sent as its own notification of at most `notification_room` bytes.
    ///
    /// With no store, every command is answered with the error status.
    pub async fn answer<Files: FileAccess>(
        &mut self,
        files: Option<&mut Files>,
        request: &[u8],
        notification_room: usize,
    ) -> Vec<Vec<u8>> {
        let Some(&command_byte) = request.first() else {
            return Vec::new();
        };
        let Some(files) = files else {
            return failure_for(command_byte, status::ERROR).into_iter().collect();
        };
        if self.changes_a_locked_folder(request) {
            return failure_for(command_byte, status::ERROR).into_iter().collect();
        }
        match command_byte {
            command::READ => vec![self.start_read(files, request, notification_room).await],
            command::READ_PACING => vec![self.continue_read(files, request, notification_room).await],
            command::WRITE => vec![self.start_write(files, request).await],
            command::WRITE_DATA => vec![self.continue_write(files, request).await],
            command::DELETE => vec![delete(files, request).await],
            command::MAKE_DIRECTORY => vec![make_directory(request)],
            command::LIST_DIRECTORY => list_directory(files, request, notification_room).await,
            command::MOVE => vec![move_entry(files, request).await],
            _ => Vec::new(),
        }
    }

    // ------------------------------------------------------------------------
    // Locked folders
    // ------------------------------------------------------------------------

    /// Whether a command would change a locked folder or anything in it.
    #[inline(never)]
    fn changes_a_locked_folder(&self, request: &[u8]) -> bool {
        let header_size = match request.first().copied() {
            Some(command::WRITE) => WRITE_HEADER_SIZE,
            Some(command::DELETE) => DELETE_HEADER_SIZE,
            Some(command::MAKE_DIRECTORY) => MAKE_DIRECTORY_HEADER_SIZE,
            Some(command::MOVE) => MOVE_HEADER_SIZE,
            Some(command::WRITE_DATA) => {
                return self
                    .writing
                    .as_ref()
                    .is_some_and(|upload| self.is_locked(upload.path.as_bytes()));
            }
            _ => return false,
        };
        if request.len() < header_size {
            return false;
        }
        let first_length = read_u16(request, 2) as usize;
        let first_path = request
            .get(header_size..header_size + first_length)
            .unwrap_or(&[]);
        let second_path = match request[0] {
            command::MOVE => {
                let second_start = header_size + first_length + 1;
                request
                    .get(second_start..second_start + read_u16(request, 4) as usize)
                    .unwrap_or(&[])
            }
            _ => &[],
        };
        self.is_locked(first_path) || (!second_path.is_empty() && self.is_locked(second_path))
    }

    /// Whether a path is a locked folder, inside one, or the root that holds them.
    fn is_locked(&self, path: &[u8]) -> bool {
        let top_folder = without_leading_slashes(path)
            .split(|&byte| byte == b'/')
            .next()
            .unwrap_or(&[]);
        self.locked_folders
            .iter()
            .any(|folder| top_folder.is_empty() || without_leading_slashes(folder.as_bytes()) == top_folder)
    }

    // ------------------------------------------------------------------------
    // Reading
    // ------------------------------------------------------------------------

    async fn start_read<Files: FileAccess>(
        &mut self,
        files: &mut Files,
        request: &[u8],
        room: usize,
    ) -> Vec<u8> {
        self.reading = None;
        if request.len() < READ_HEADER_SIZE {
            return read_data(status::PROTOCOL_ERROR, 0, 0, &[]);
        }
        let path_length = read_u16(request, 2) as usize;
        let chunk_offset = read_u32(request, 4);
        let chunk_size = read_u32(request, 8);
        let Some(path) = read_path(request, READ_HEADER_SIZE, path_length) else {
            return read_data(status::PROTOCOL_ERROR, chunk_offset, 0, &[]);
        };
        self.reading = Some(String::from(path));
        self.read_chunk(files, chunk_offset, chunk_size, room).await
    }

    async fn continue_read<Files: FileAccess>(
        &mut self,
        files: &mut Files,
        request: &[u8],
        room: usize,
    ) -> Vec<u8> {
        if request.len() < READ_PACING_SIZE || self.reading.is_none() {
            return read_data(status::PROTOCOL_ERROR, 0, 0, &[]);
        }
        let chunk_offset = read_u32(request, 4);
        let chunk_size = read_u32(request, 8);
        self.read_chunk(files, chunk_offset, chunk_size, room).await
    }

    /// Sends as much of the asked-for chunk as fits in one notification; the phone asks again for the rest.
    async fn read_chunk<Files: FileAccess>(
        &mut self,
        files: &mut Files,
        chunk_offset: u32,
        chunk_size: u32,
        room: usize,
    ) -> Vec<u8> {
        let Some(path) = self.reading.clone() else {
            return read_data(status::PROTOCOL_ERROR, chunk_offset, 0, &[]);
        };
        let total_length = match files.file_length(&path).await {
            Ok(Some(length)) => length as u32,
            Ok(None) => {
                self.reading = None;
                let missing = if is_directory(files, &path).await {
                    status::ERROR
                } else {
                    status::NO_FILE
                };
                return read_data(missing, chunk_offset, 0, &[]);
            }
            Err(_) => {
                self.reading = None;
                return read_data(status::ERROR, chunk_offset, 0, &[]);
            }
        };
        let remaining = total_length.saturating_sub(chunk_offset);
        let fits = room.saturating_sub(READ_DATA_HEADER_SIZE) as u32;
        let mut chunk = vec![0; chunk_size.min(remaining).min(fits) as usize];
        match files.read_file(&path, chunk_offset as usize, &mut chunk).await {
            Ok(Some(length)) => {
                chunk.truncate(length);
                if chunk_offset + length as u32 >= total_length {
                    self.reading = None;
                }
                read_data(status::OK, chunk_offset, total_length, &chunk)
            }
            Ok(None) => {
                self.reading = None;
                read_data(status::NO_FILE, chunk_offset, 0, &[])
            }
            Err(_) => {
                self.reading = None;
                read_data(status::ERROR, chunk_offset, total_length, &[])
            }
        }
    }

    // ------------------------------------------------------------------------
    // Writing
    // ------------------------------------------------------------------------

    /// Starts a file, or resumes one the store still holds open at `offset`, and says how much may follow.
    ///
    /// The old file stays until the last piece arrives, so an upload cut short never leaves half a file.
    async fn start_write<Files: FileAccess>(&mut self, files: &mut Files, request: &[u8]) -> Vec<u8> {
        self.writing = None;
        if request.len() < WRITE_HEADER_SIZE {
            return write_pacing(status::PROTOCOL_ERROR, 0, 0);
        }
        let path_length = read_u16(request, 2) as usize;
        let offset = read_u32(request, 4);
        let total_size = read_u32(request, 16);
        let Some(path) = read_path(request, WRITE_HEADER_SIZE, path_length) else {
            return write_pacing(status::PROTOCOL_ERROR, offset, 0);
        };
        if offset > total_size {
            return write_pacing(status::PROTOCOL_ERROR, offset, 0);
        }
        if is_directory(files, path).await {
            return write_pacing(status::ERROR, offset, 0);
        }
        let upload = Upload {
            path: String::from(path),
            total_size,
            next_offset: offset,
            gathered: Vec::new(),
        };
        let resuming = files.upload_progress() == Some((path, offset as usize));
        if !resuming {
            files.abandon_file();
            if offset != 0 {
                // Only the upload still open in the store can resume; anything else starts again from the top.
                return write_pacing(status::ERROR, offset, 0);
            }
            let started = if upload.is_small() {
                Ok(())
            } else {
                files.begin_file(path).await
            };
            if started.is_err() {
                return write_pacing(status::ERROR, offset, 0);
            }
        }
        if total_size == 0 {
            return match files.write_file(path, &[]).await {
                Ok(()) => write_pacing(status::OK, 0, 0),
                Err(_) => write_pacing(status::ERROR, 0, 0),
            };
        }
        self.writing = Some(upload);
        write_pacing(status::OK, offset, free_space(files).min(total_size - offset))
    }

    async fn continue_write<Files: FileAccess>(&mut self, files: &mut Files, request: &[u8]) -> Vec<u8> {
        if request.len() < WRITE_DATA_HEADER_SIZE {
            return write_pacing(status::PROTOCOL_ERROR, 0, 0);
        }
        let offset = read_u32(request, 4);
        let stated_size = read_u32(request, 8) as usize;
        let data = &request[WRITE_DATA_HEADER_SIZE..];
        let data = &data[..stated_size.min(data.len())];
        let Some(upload) = &mut self.writing else {
            return write_pacing(status::PROTOCOL_ERROR, offset, 0);
        };
        let end = offset as u64 + data.len() as u64;
        if offset > upload.next_offset || end > upload.total_size as u64 {
            return write_pacing(status::PROTOCOL_ERROR, offset, 0);
        }
        let written = if upload.is_small() {
            // A piece sent again overwrites what it covers; the rest extends the file.
            let start = offset as usize;
            upload.gathered.resize(upload.gathered.len().max(end as usize), 0);
            upload.gathered[start..end as usize].copy_from_slice(data);
            Ok(())
        } else {
            // The store appends only, so a piece sent again writes just what it adds.
            let already_written = (upload.next_offset - offset) as usize;
            files
                .write_file_part(&data[already_written.min(data.len())..])
                .await
        };
        if written.is_err() {
            files.abandon_file();
            self.writing = None;
            return write_pacing(status::ERROR, offset, 0);
        }
        upload.next_offset = upload.next_offset.max(end as u32);
        let next_offset = upload.next_offset;
        let total_size = upload.total_size;
        if next_offset >= total_size {
            let finished = if upload.is_small() {
                files.write_file(&upload.path, &upload.gathered).await
            } else {
                files.finish_file().await
            };
            self.writing = None;
            if finished.is_err() {
                files.abandon_file();
                return write_pacing(status::ERROR, offset, 0);
            }
        }
        write_pacing(
            status::OK,
            next_offset,
            free_space(files).min(total_size - next_offset),
        )
    }
}

fn free_space<Files: FileAccess>(files: &Files) -> u32 {
    files.free_bytes().min(u32::MAX as usize) as u32
}

// ============================================================================
// Folders, deleting and moving
// ============================================================================

/// Every file's path under a folder, which exists only through the files inside it.
async fn paths_under<Files: FileAccess>(files: &mut Files, folder: &str) -> Vec<String> {
    let mut prefix = String::from(folder.trim_end_matches('/'));
    prefix.push('/');
    let mut paths = Vec::new();
    let _ = files
        .for_each_file(&prefix, &mut |path| paths.push(String::from(path)))
        .await;
    paths
}

async fn is_directory<Files: FileAccess>(files: &mut Files, path: &str) -> bool {
    !paths_under(files, path).await.is_empty()
}

/// Folders exist only through the files in them, so making one always succeeds.
fn make_directory(request: &[u8]) -> Vec<u8> {
    if request.len() < MAKE_DIRECTORY_HEADER_SIZE {
        return make_directory_status(status::PROTOCOL_ERROR);
    }
    let path_length = read_u16(request, 2) as usize;
    match read_path(request, MAKE_DIRECTORY_HEADER_SIZE, path_length) {
        Some(path) if !path.trim_end_matches('/').is_empty() => make_directory_status(status::OK),
        _ => make_directory_status(status::PROTOCOL_ERROR),
    }
}

/// Deletes a file, or a folder with everything in it.
async fn delete<Files: FileAccess>(files: &mut Files, request: &[u8]) -> Vec<u8> {
    if request.len() < DELETE_HEADER_SIZE {
        return two_byte_status(command::DELETE_STATUS, status::PROTOCOL_ERROR);
    }
    let path_length = read_u16(request, 2) as usize;
    let Some(path) = read_path(request, DELETE_HEADER_SIZE, path_length) else {
        return two_byte_status(command::DELETE_STATUS, status::PROTOCOL_ERROR);
    };
    let path = path.trim_end_matches('/');
    if path.is_empty() {
        return two_byte_status(command::DELETE_STATUS, status::ERROR);
    }
    let mut targets = paths_under(files, path).await;
    if matches!(files.file_length(path).await, Ok(Some(_))) {
        targets.push(String::from(path));
    }
    if targets.is_empty() {
        return two_byte_status(command::DELETE_STATUS, status::NO_FILE);
    }
    for target in targets {
        if files.delete_file(&target).await.is_err() {
            return two_byte_status(command::DELETE_STATUS, status::ERROR);
        }
    }
    two_byte_status(command::DELETE_STATUS, status::OK)
}

/// The old and new paths sit one after the other, with one separator byte between them.
///
/// The store has no rename, so a move copies each file, then deletes the old one.
async fn move_entry<Files: FileAccess>(files: &mut Files, request: &[u8]) -> Vec<u8> {
    if request.len() < MOVE_HEADER_SIZE {
        return two_byte_status(command::MOVE_STATUS, status::PROTOCOL_ERROR);
    }
    let old_length = read_u16(request, 2) as usize;
    let new_length = read_u16(request, 4) as usize;
    let old_path = read_path(request, MOVE_HEADER_SIZE, old_length);
    let new_path = read_path(request, MOVE_HEADER_SIZE + old_length + 1, new_length);
    let (Some(old_path), Some(new_path)) = (old_path, new_path) else {
        return two_byte_status(command::MOVE_STATUS, status::PROTOCOL_ERROR);
    };
    let mut moves: Vec<(String, String)> = Vec::new();
    if matches!(files.file_length(old_path).await, Ok(Some(_))) {
        moves.push((String::from(old_path), String::from(new_path)));
    }
    for inner in paths_under(files, old_path).await {
        let mut moved = String::from(new_path.trim_end_matches('/'));
        moved.push_str(&inner[old_path.trim_end_matches('/').len()..]);
        moves.push((inner, moved));
    }
    if moves.is_empty() {
        return two_byte_status(command::MOVE_STATUS, status::NO_FILE);
    }
    for (from, to) in moves {
        if copy_file(files, &from, &to).await.is_err() || files.delete_file(&from).await.is_err() {
            return two_byte_status(command::MOVE_STATUS, status::ERROR);
        }
    }
    two_byte_status(command::MOVE_STATUS, status::OK)
}

/// Copies a file a piece at a time, so a large one never has to be in RAM.
async fn copy_file<Files: FileAccess>(files: &mut Files, from: &str, to: &str) -> Result<(), FileError> {
    let length = files.file_length(from).await?.unwrap_or(0);
    if length <= SMALL_FILE_BYTES {
        let mut data = vec![0; length];
        files.read_file(from, 0, &mut data).await?;
        return files.write_file(to, &data).await;
    }
    files.abandon_file();
    files.begin_file(to).await?;
    let mut piece = [0u8; COPY_PIECE_BYTES];
    let mut copied = 0;
    while copied < length {
        let size = COPY_PIECE_BYTES.min(length - copied);
        let read = files.read_file(from, copied, &mut piece[..size]).await;
        let written = match read {
            Ok(Some(_)) => files.write_file_part(&piece[..size]).await,
            Ok(None) => Err(FileError::Damaged),
            Err(error) => Err(error),
        };
        if let Err(error) = written {
            files.abandon_file();
            return Err(error);
        }
        copied += size;
    }
    files.finish_file().await
}

// ============================================================================
// Listing a folder
// ============================================================================

/// What a folder holds: its files with their sizes, and the folders inside it, each once.
struct FolderEntry {
    name: String,
    is_directory: bool,
    size: u32,
}

/// One notification per entry, then a closing one whose entry number equals the total and whose name is empty.
///
/// A name longer than the notification has room for is cut short.
async fn list_directory<Files: FileAccess>(files: &mut Files, request: &[u8], room: usize) -> Vec<Vec<u8>> {
    if request.len() < LIST_DIRECTORY_HEADER_SIZE {
        return vec![directory_entry(status::PROTOCOL_ERROR, 0, 0, 0, 0, b"")];
    }
    let path_length = read_u16(request, 2) as usize;
    let Some(path) = read_path(request, LIST_DIRECTORY_HEADER_SIZE, path_length) else {
        return vec![directory_entry(status::PROTOCOL_ERROR, 0, 0, 0, 0, b"")];
    };
    let folder = path.trim_end_matches('/');
    let inside = paths_under(files, folder).await;
    if inside.is_empty() && !folder.is_empty() {
        let missing = if matches!(files.file_length(folder).await, Ok(Some(_))) {
            status::ERROR
        } else {
            status::NO_FILE
        };
        return vec![directory_entry(missing, 0, 0, 0, 0, b"")];
    }
    let mut entries: Vec<FolderEntry> = Vec::new();
    for inner in inside {
        let rest = &inner[folder.len() + 1..];
        let (name, is_directory) = match rest.split_once('/') {
            Some((folder_name, _)) => (folder_name, true),
            None => (rest, false),
        };
        if entries.iter().any(|entry| entry.name == name) {
            continue;
        }
        let size = if is_directory {
            0
        } else {
            files.file_length(&inner).await.ok().flatten().unwrap_or(0) as u32
        };
        entries.push(FolderEntry {
            name: String::from(name),
            is_directory,
            size,
        });
    }
    let total = entries.len() as u32;
    let name_room = room.saturating_sub(LIST_DIRECTORY_ENTRY_HEADER_SIZE);
    let mut answers: Vec<Vec<u8>> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let flags = if entry.is_directory { IS_DIRECTORY } else { 0 };
            let name = entry.name.as_bytes();
            let name = &name[..name.len().min(name_room)];
            directory_entry(status::OK, index as u32, total, flags, entry.size, name)
        })
        .collect();
    answers.push(directory_entry(status::OK, total, total, 0, 0, b""));
    answers
}

// ============================================================================
// Building the answers
// ============================================================================

fn read_data(status: u8, chunk_offset: u32, total_length: u32, chunk: &[u8]) -> Vec<u8> {
    let mut answer = Vec::with_capacity(READ_DATA_HEADER_SIZE + chunk.len());
    answer.extend_from_slice(&[command::READ_DATA, status, 0, 0]);
    answer.extend_from_slice(&chunk_offset.to_le_bytes());
    answer.extend_from_slice(&total_length.to_le_bytes());
    answer.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
    answer.extend_from_slice(chunk);
    answer
}

/// The modification time is always zero, as the watch keeps none.
fn write_pacing(status: u8, offset: u32, free_space: u32) -> Vec<u8> {
    let mut answer = Vec::with_capacity(WRITE_PACING_SIZE);
    answer.extend_from_slice(&[command::WRITE_PACING, status, 0, 0]);
    answer.extend_from_slice(&offset.to_le_bytes());
    answer.extend_from_slice(&0u64.to_le_bytes());
    answer.extend_from_slice(&free_space.to_le_bytes());
    answer
}

fn make_directory_status(status: u8) -> Vec<u8> {
    let mut answer = vec![0; MAKE_DIRECTORY_STATUS_SIZE];
    answer[0] = command::MAKE_DIRECTORY_STATUS;
    answer[1] = status;
    answer
}

fn two_byte_status(command_byte: u8, status: u8) -> Vec<u8> {
    vec![command_byte, status]
}

fn directory_entry(status: u8, entry: u32, total: u32, flags: u32, size: u32, name: &[u8]) -> Vec<u8> {
    let mut answer = Vec::with_capacity(LIST_DIRECTORY_ENTRY_HEADER_SIZE + name.len());
    answer.extend_from_slice(&[command::LIST_DIRECTORY_ENTRY, status]);
    answer.extend_from_slice(&(name.len() as u16).to_le_bytes());
    answer.extend_from_slice(&entry.to_le_bytes());
    answer.extend_from_slice(&total.to_le_bytes());
    answer.extend_from_slice(&flags.to_le_bytes());
    answer.extend_from_slice(&0u64.to_le_bytes());
    answer.extend_from_slice(&size.to_le_bytes());
    answer.extend_from_slice(name);
    answer
}

/// The failure answer each command expects, or nothing for a command the watch does not know.
/// A path with the slashes at its start taken off.
fn without_leading_slashes(mut bytes: &[u8]) -> &[u8] {
    while let [b'/', rest @ ..] = bytes {
        bytes = rest;
    }
    bytes
}

fn failure_for(command_byte: u8, status: u8) -> Option<Vec<u8>> {
    match command_byte {
        command::READ | command::READ_PACING => Some(read_data(status, 0, 0, &[])),
        command::WRITE | command::WRITE_DATA => Some(write_pacing(status, 0, 0)),
        command::DELETE => Some(two_byte_status(command::DELETE_STATUS, status)),
        command::MAKE_DIRECTORY => Some(make_directory_status(status)),
        command::LIST_DIRECTORY => Some(directory_entry(status, 0, 0, 0, 0, b"")),
        command::MOVE => Some(two_byte_status(command::MOVE_STATUS, status)),
        _ => None,
    }
}
