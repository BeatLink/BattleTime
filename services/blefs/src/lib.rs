//! Adafruit BLE file transfer: the file service InfiniTime calls `FSService`, which ITD,
//! Gadgetbridge, InfiniTime's resource loader and NeoTime's companion app use to read, write, list,
//! move and delete files on the watch.
//!
//! The phone writes one command to the transfer characteristic and the watch answers with one or
//! more notifications on it. `FileTransfer` turns a command into its answers against the littlefs
//! file system, with no Bluetooth code in it; the `trouble` feature adds the GATT service.
//! Numbers are little-endian, as in Adafruit's description of the protocol.
//!
//! NeoTime follows Adafruit's description where InfiniTime strays from it: a write truncates the file
//! to its total size, a write answer gives the offset of the next write, missing files report the
//! no-file status rather than a littlefs error code, and writes and new folders make their parents.

#![no_std]

extern crate alloc;

use alloc::{string::String, vec, vec::Vec};

use littlefs_rust::{Error, FileType, Filesystem, OpenFlags, SeekFrom, Storage};

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

/// How deep a delete follows folders inside folders before it gives up.
const DEEPEST_DELETE: usize = 8;

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

fn status_for(error: Error) -> u8 {
    match error {
        Error::NoEntry => status::NO_FILE,
        _ => status::ERROR,
    }
}

// ============================================================================
// The transfer in progress
// ============================================================================

/// A file being written, a piece at a time.
struct Upload {
    path: String,
    total_size: u32,
    next_offset: u32,
}

/// Answers the phone's file commands; it remembers the file being read or written between commands.
pub struct FileTransfer {
    capacity_in_bytes: u32,
    block_size: u32,
    reading: Option<String>,
    writing: Option<Upload>,
}

impl FileTransfer {
    pub fn new(block_size: u32, block_count: u32) -> FileTransfer {
        FileTransfer {
            capacity_in_bytes: block_size * block_count,
            block_size,
            reading: None,
            writing: None,
        }
    }

    /// Forgets a read or write the phone left unfinished, such as when it disconnects.
    pub fn abort(&mut self) {
        self.reading = None;
        self.writing = None;
    }

    pub fn is_writing(&self) -> bool {
        self.writing.is_some()
    }

    /// The answers to one command, each to be sent as its own notification of at most `notification_room` bytes.
    ///
    /// With no file system, every command is answered with the error status.
    pub fn answer<S: Storage>(
        &mut self,
        file_system: Option<&Filesystem<S>>,
        request: &[u8],
        notification_room: usize,
    ) -> Vec<Vec<u8>> {
        let Some(&command_byte) = request.first() else {
            return Vec::new();
        };
        let Some(file_system) = file_system else {
            return failure_for(command_byte, status::ERROR).into_iter().collect();
        };
        match command_byte {
            command::READ => vec![self.start_read(file_system, request, notification_room)],
            command::READ_PACING => vec![self.continue_read(file_system, request, notification_room)],
            command::WRITE => vec![self.start_write(file_system, request)],
            command::WRITE_DATA => vec![self.continue_write(file_system, request)],
            command::DELETE => vec![delete(file_system, request)],
            command::MAKE_DIRECTORY => vec![make_directory(file_system, request)],
            command::LIST_DIRECTORY => list_directory(file_system, request, notification_room),
            command::MOVE => vec![move_entry(file_system, request)],
            _ => Vec::new(),
        }
    }

    // ------------------------------------------------------------------------
    // Reading
    // ------------------------------------------------------------------------

    fn start_read<S: Storage>(
        &mut self,
        file_system: &Filesystem<S>,
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
        self.read_chunk(file_system, chunk_offset, chunk_size, room)
    }

    fn continue_read<S: Storage>(
        &mut self,
        file_system: &Filesystem<S>,
        request: &[u8],
        room: usize,
    ) -> Vec<u8> {
        if request.len() < READ_PACING_SIZE || self.reading.is_none() {
            return read_data(status::PROTOCOL_ERROR, 0, 0, &[]);
        }
        let chunk_offset = read_u32(request, 4);
        let chunk_size = read_u32(request, 8);
        self.read_chunk(file_system, chunk_offset, chunk_size, room)
    }

    /// Sends as much of the asked-for chunk as fits in one notification; the phone asks again for the rest.
    fn read_chunk<S: Storage>(
        &mut self,
        file_system: &Filesystem<S>,
        chunk_offset: u32,
        chunk_size: u32,
        room: usize,
    ) -> Vec<u8> {
        let Some(path) = &self.reading else {
            return read_data(status::PROTOCOL_ERROR, chunk_offset, 0, &[]);
        };
        let file = match file_system.open(path, OpenFlags::READ) {
            Ok(file) => file,
            Err(error) => {
                self.reading = None;
                return read_data(status_for(error), chunk_offset, 0, &[]);
            }
        };
        let total_length = file.size();
        let remaining = total_length.saturating_sub(chunk_offset);
        let fits = room.saturating_sub(READ_DATA_HEADER_SIZE) as u32;
        let mut chunk = vec![0; chunk_size.min(remaining).min(fits) as usize];
        let read = file
            .seek(SeekFrom::Start(chunk_offset))
            .and_then(|_| file.read(&mut chunk));
        let _ = file.close();
        match read {
            Ok(length) => {
                chunk.truncate(length as usize);
                if chunk_offset + length >= total_length {
                    self.reading = None;
                }
                read_data(status::OK, chunk_offset, total_length, &chunk)
            }
            Err(error) => {
                self.reading = None;
                read_data(status_for(error), chunk_offset, total_length, &[])
            }
        }
    }

    // ------------------------------------------------------------------------
    // Writing
    // ------------------------------------------------------------------------

    /// Opens the file, cut back to `offset` so an interrupted upload can resume, and says how much may follow.
    fn start_write<S: Storage>(&mut self, file_system: &Filesystem<S>, request: &[u8]) -> Vec<u8> {
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
        make_parents(file_system, path);
        let prepared = file_system
            .open(path, OpenFlags::WRITE | OpenFlags::CREATE)
            .and_then(|file| {
                if file.size() < offset {
                    let _ = file.close();
                    return Err(Error::Invalid);
                }
                let truncated = file.truncate(offset);
                file.close().and(truncated)
            });
        if let Err(error) = prepared {
            return write_pacing(status_for(error), offset, 0);
        }
        if offset < total_size {
            self.writing = Some(Upload {
                path: String::from(path),
                total_size,
                next_offset: offset,
            });
        }
        write_pacing(
            status::OK,
            offset,
            self.free_space(file_system).min(total_size - offset),
        )
    }

    fn continue_write<S: Storage>(&mut self, file_system: &Filesystem<S>, request: &[u8]) -> Vec<u8> {
        if request.len() < WRITE_DATA_HEADER_SIZE {
            return write_pacing(status::PROTOCOL_ERROR, 0, 0);
        }
        let offset = read_u32(request, 4);
        let stated_size = read_u32(request, 8) as usize;
        let data = &request[WRITE_DATA_HEADER_SIZE..];
        let data = &data[..stated_size.min(data.len())];
        let Some(upload) = &self.writing else {
            return write_pacing(status::PROTOCOL_ERROR, offset, 0);
        };
        let end = offset as u64 + data.len() as u64;
        if offset > upload.next_offset || end > upload.total_size as u64 {
            return write_pacing(status::PROTOCOL_ERROR, offset, 0);
        }
        let written = file_system.open(&upload.path, OpenFlags::WRITE).and_then(|file| {
            let result = file
                .seek(SeekFrom::Start(offset))
                .and_then(|_| write_all(&file, data));
            file.close().and(result)
        });
        if let Err(error) = written {
            self.writing = None;
            return write_pacing(status_for(error), offset, 0);
        }
        let next_offset = end as u32;
        let total_size = upload.total_size;
        if next_offset >= total_size {
            self.writing = None;
        } else if let Some(upload) = &mut self.writing {
            upload.next_offset = upload.next_offset.max(next_offset);
        }
        write_pacing(
            status::OK,
            next_offset,
            self.free_space(file_system).min(total_size - next_offset),
        )
    }

    fn free_space<S: Storage>(&self, file_system: &Filesystem<S>) -> u32 {
        let used = file_system.fs_size().unwrap_or(0).saturating_mul(self.block_size);
        self.capacity_in_bytes.saturating_sub(used)
    }
}

fn write_all<S: Storage>(file: &littlefs_rust::File<'_, S>, data: &[u8]) -> Result<(), Error> {
    let mut sent = 0;
    while sent < data.len() {
        let length = file.write(&data[sent..])? as usize;
        if length == 0 {
            return Err(Error::NoSpace);
        }
        sent += length;
    }
    Ok(())
}

// ============================================================================
// Folders, deleting and moving
// ============================================================================

/// Makes every folder above `path` that is missing; any failure shows up when the file itself is made.
fn make_parents<S: Storage>(file_system: &Filesystem<S>, path: &str) {
    for (index, character) in path.char_indices() {
        if character == '/' && index > 0 && !file_system.exists(&path[..index]) {
            let _ = file_system.mkdir(&path[..index]);
        }
    }
}

fn make_directory<S: Storage>(file_system: &Filesystem<S>, request: &[u8]) -> Vec<u8> {
    if request.len() < MAKE_DIRECTORY_HEADER_SIZE {
        return make_directory_status(status::PROTOCOL_ERROR);
    }
    let path_length = read_u16(request, 2) as usize;
    let Some(path) = read_path(request, MAKE_DIRECTORY_HEADER_SIZE, path_length) else {
        return make_directory_status(status::PROTOCOL_ERROR);
    };
    let path = path.trim_end_matches('/');
    make_parents(file_system, path);
    match file_system.mkdir(path) {
        Ok(()) => make_directory_status(status::OK),
        Err(Error::Exists) if is_directory(file_system, path) => make_directory_status(status::OK),
        Err(error) => make_directory_status(status_for(error)),
    }
}

fn is_directory<S: Storage>(file_system: &Filesystem<S>, path: &str) -> bool {
    matches!(file_system.stat(path), Ok(metadata) if metadata.file_type == FileType::Dir)
}

/// Deletes a file, or a folder with everything in it.
fn delete<S: Storage>(file_system: &Filesystem<S>, request: &[u8]) -> Vec<u8> {
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
    match remove_with_contents(file_system, path, DEEPEST_DELETE) {
        Ok(()) => two_byte_status(command::DELETE_STATUS, status::OK),
        Err(error) => two_byte_status(command::DELETE_STATUS, status_for(error)),
    }
}

fn remove_with_contents<S: Storage>(
    file_system: &Filesystem<S>,
    path: &str,
    depth_left: usize,
) -> Result<(), Error> {
    if file_system.stat(path)?.file_type == FileType::Dir {
        if depth_left == 0 {
            return Err(Error::NotEmpty);
        }
        for entry in file_system.list_dir(path)? {
            let mut inner_path = String::from(path);
            inner_path.push('/');
            inner_path.push_str(&entry.name);
            remove_with_contents(file_system, &inner_path, depth_left - 1)?;
        }
    }
    file_system.remove(path)
}

/// The old and new paths sit one after the other, with one separator byte between them.
fn move_entry<S: Storage>(file_system: &Filesystem<S>, request: &[u8]) -> Vec<u8> {
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
    match file_system.rename(old_path, new_path) {
        Ok(()) => two_byte_status(command::MOVE_STATUS, status::OK),
        Err(error) => two_byte_status(command::MOVE_STATUS, status_for(error)),
    }
}

// ============================================================================
// Listing a folder
// ============================================================================

/// One notification per entry, then a closing one whose entry number equals the total and whose name is empty.
///
/// A name longer than the notification has room for is cut short.
fn list_directory<S: Storage>(file_system: &Filesystem<S>, request: &[u8], room: usize) -> Vec<Vec<u8>> {
    if request.len() < LIST_DIRECTORY_HEADER_SIZE {
        return vec![directory_entry(status::PROTOCOL_ERROR, 0, 0, 0, 0, b"")];
    }
    let path_length = read_u16(request, 2) as usize;
    let Some(path) = read_path(request, LIST_DIRECTORY_HEADER_SIZE, path_length) else {
        return vec![directory_entry(status::PROTOCOL_ERROR, 0, 0, 0, 0, b"")];
    };
    let path = if path.is_empty() { "/" } else { path };
    let entries = match file_system.list_dir(path) {
        Ok(entries) => entries,
        Err(error) => return vec![directory_entry(status_for(error), 0, 0, 0, 0, b"")],
    };
    let total = entries.len() as u32;
    let name_room = room.saturating_sub(LIST_DIRECTORY_ENTRY_HEADER_SIZE);
    let mut answers: Vec<Vec<u8>> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let (flags, size) = match entry.file_type {
                FileType::Dir => (IS_DIRECTORY, 0),
                FileType::File => (0, entry.size),
            };
            let name = entry.name.as_bytes();
            let name = &name[..name.len().min(name_room)];
            directory_entry(status::OK, index as u32, total, flags, size, name)
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
