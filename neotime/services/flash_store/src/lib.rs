//! NeoTime's store in the external flash. It knows how data is written, not what it is, so new kinds of data need
//! no new format.
//!
//! - **Files** are named by path and written whole, at once or a part at a time: each is a list of up to 64 sectors
//!   in any order, committed by a header written last, so a cut-off write never replaces the old file. Files of
//!   1 KB or less are kept as records instead, so they cost no sector of their own.
//! - **Records** are small values rewritten often, such as each app's saved values and the settings. Each key
//!   carries its owner's app number in its top 12 bits. They are appended to a ring with a table in RAM of each
//!   key's newest copy; when the ring wraps, newest copies are carried forward.
//! - **Logs** are appended over time, batched in RAM, and drop their oldest sector when their ring wraps.
//!
//! Everything survives a power cut at any point. Its tables are fixed in size by the type's parameters; only a
//! file written a part at a time holds a little heap while it is written. See docs/flash-store.md.

#![no_std]

extern crate alloc;

mod access;
mod checksum;
mod files;
mod flash;
#[cfg(any(test, feature = "ram-flash"))]
pub mod ram_flash;
mod ring;

pub use access::{FileAccess, FileError};

#[cfg(test)]
mod power_cut_tests;
#[cfg(test)]
mod tests;

use alloc::boxed::Box;
use embedded_storage_async::nor_flash::NorFlash;
use heapless::Vec;

use files::{FileArea, FileHeader, Upload, FILE_HEADER_BYTES};
use ring::{find_newest, Newest, Ring, APPENDS, TOMBSTONE};

// ============================================================================
// Sizes
// ============================================================================

/// The external flash's erase sector, which every part of the store works in.
pub const SECTOR_BYTES: u32 = 4096;
/// Changes whenever the layout on the flash changes; a store of another version is refused, never overwritten.
pub const FORMAT_VERSION: u8 = 1;
/// The largest record: an app's 2 KB of saved values with room for its group's own keys.
pub const LARGEST_RECORD_BYTES: usize = 2112;
/// Files up to this size, written whole, are kept as records.
pub const SMALL_FILE_BYTES: usize = 1024;
pub const LONGEST_PATH_BYTES: usize = 64;
pub const MOST_SECTORS_PER_FILE: usize = 64;
pub const LARGEST_FILE_BYTES: usize = MOST_SECTORS_PER_FILE * SECTOR_BYTES as usize - FILE_HEADER_BYTES;
/// The record flash each app may use; the watch's own owners, from `FIRST_SYSTEM_OWNER` up, have no limit.
pub const OWNER_RECORD_BYTES: usize = 4096;
pub const FIRST_SYSTEM_OWNER: u32 = 0xf00;
/// Small files are records under this owner, keyed by 20 bits of their path's hash.
const SMALL_FILE_OWNER: u32 = 0xffe;
/// How many log streams are batched in RAM at once, and how much each batch holds before it is written.
pub const LOG_STREAMS: usize = 4;
pub const LOG_BATCH_BYTES: usize = 64;

/// A record or log key: the owner's app number in the top 12 bits, and 20 bits the owner chooses.
pub type RecordKey = u32;

pub const fn record_key(owner: u32, item: u32) -> RecordKey {
    (owner & 0xfff) << 20 | (item & 0x000f_ffff)
}

pub const fn owner_of(key: RecordKey) -> u32 {
    key >> 20
}

// ============================================================================
// Errors and layout
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreError<FlashError> {
    Flash(FlashError),
    /// The records would take more than half their ring; delete some first.
    Full,
    KeyTableFull,
    FileTableFull,
    FileAreaFull,
    TooLarge,
    PathTooLong,
    /// The owner's records would take more than `OWNER_RECORD_BYTES`.
    OverOwnerLimit,
    UploadInProgress,
    NoUpload,
    /// The flash holds a store of another format version; migrate it rather than mount it.
    OtherFormatVersion,
    /// The layout has more sectors than the type's tables were built for.
    LayoutDoesNotFit,
    /// A whole file read back does not match its checksum.
    Corrupted,
    /// The heap had no room for an upload's list of sectors.
    OutOfMemory,
}

/// A run of bytes that lie one after another in the flash.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Extent {
    pub address: u32,
    pub length: u32,
}

/// How much of `file_extents`'s slice it filled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileExtents {
    pub count: usize,
    pub bytes: usize,
}

/// Where the store lives in the flash and how its sectors are divided.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub start_address: u32,
    pub file_sectors: usize,
    pub record_sectors: usize,
    pub log_sectors: usize,
}

impl Layout {
    const fn record_start_address(&self) -> u32 {
        self.start_address + self.file_sectors as u32 * SECTOR_BYTES
    }

    const fn log_start_address(&self) -> u32 {
        self.record_start_address() + self.record_sectors as u32 * SECTOR_BYTES
    }

    pub fn total_sectors(&self) -> usize {
        self.file_sectors + self.record_sectors + self.log_sectors
    }
}

// ============================================================================
// The store
// ============================================================================

#[derive(Clone, Copy)]
struct LogBatch {
    key: RecordKey,
    length: usize,
    bytes: [u8; LOG_BATCH_BYTES],
}

struct SmallFile {
    key: RecordKey,
    /// Bytes before the data: the path's length and the path.
    prefix: usize,
    sequence: u32,
}

/// The store, with tables for `MOST_FILES` files, `MOST_RECORD_KEYS` record keys and rings of up to `RING_SECTORS`.
pub struct FlashStore<
    Flash,
    const MOST_FILES: usize,
    const MOST_RECORD_KEYS: usize,
    const RING_SECTORS: usize,
> {
    flash: Flash,
    files: FileArea<MOST_FILES>,
    records: Ring<RING_SECTORS>,
    logs: Ring<RING_SECTORS>,
    /// Each record key's newest copy; the rings share one build of their code, so the table lives here.
    record_keys: Vec<Newest, MOST_RECORD_KEYS>,
    no_keys: Vec<Newest, 0>,
    log_batches: Vec<LogBatch, LOG_STREAMS>,
    /// A file being written a part at a time, on the heap only while it is written.
    upload: Option<Box<Upload>>,
}

impl<Flash: NorFlash, const MOST_FILES: usize, const MOST_RECORD_KEYS: usize, const RING_SECTORS: usize>
    FlashStore<Flash, MOST_FILES, MOST_RECORD_KEYS, RING_SECTORS>
{
    /// The RAM the store keeps, fixed when the firmware is built, less the flash driver it owns.
    pub const RAM_BYTES: usize = core::mem::size_of::<Self>() - core::mem::size_of::<Flash>();

    /// Reads what is on the flash and finishes anything a power cut interrupted; blank flash mounts empty.
    pub async fn mount(flash: Flash, layout: Layout) -> Result<Self, StoreError<Flash::Error>> {
        const {
            assert!(
                Flash::READ_SIZE == 1 && Flash::WRITE_SIZE == 1,
                "the store reads and writes single bytes"
            );
            assert!(
                Flash::ERASE_SIZE == SECTOR_BYTES as usize,
                "the store erases 4 KB sectors"
            );
        }
        let mut store = Self::unmounted(flash, layout);
        store.mount_in_place().await?;
        Ok(store)
    }

    /// A store with nothing read yet, which `mount_in_place` fills where it lies, such as in a static, so the
    /// mount's future never holds a copy of the store.
    pub const fn unmounted(flash: Flash, layout: Layout) -> Self {
        FlashStore {
            flash,
            files: FileArea::unmounted(layout.start_address, layout.file_sectors),
            records: Ring::unmounted(layout.record_start_address(), layout.record_sectors, true),
            logs: Ring::unmounted(layout.log_start_address(), layout.log_sectors, false),
            record_keys: Vec::new(),
            no_keys: Vec::new(),
            log_batches: Vec::new(),
            upload: None,
        }
    }

    /// Reads what is on the flash and finishes anything a power cut interrupted; blank flash mounts empty.
    pub async fn mount_in_place(&mut self) -> Result<(), StoreError<Flash::Error>> {
        self.upload = None;
        self.log_batches.clear();
        self.files.mount_in_place(&mut self.flash).await?;
        self.records
            .mount_in_place(&mut self.flash, self.record_keys.as_mut_view())
            .await?;
        self.logs
            .mount_in_place(&mut self.flash, self.no_keys.as_mut_view())
            .await?;
        self.share_sequence();
        Ok(())
    }

    pub fn flash(&self) -> &Flash {
        &self.flash
    }

    /// The flash driver, such as to switch how it waits; writing through it behind the store's back corrupts the store.
    pub fn flash_mut(&mut self) -> &mut Flash {
        &mut self.flash
    }

    pub fn into_flash(self) -> Flash {
        self.flash
    }

    /// Files and records draw on one sequence, so when a path is briefly both, the newer copy is known.
    fn share_sequence(&mut self) {
        let next = self.files.next_sequence.max(self.records.next_record_sequence);
        self.files.next_sequence = next;
        self.records.next_record_sequence = next;
    }

    pub fn record_bytes_in_use(&self) -> usize {
        self.records.bytes_in_use()
    }

    pub fn free_file_sectors(&self) -> usize {
        self.files.free_sectors()
    }

    pub fn file_sectors(&self) -> usize {
        self.files.sector_count()
    }

    // ------------------------------------------------------------------------
    // Records
    // ------------------------------------------------------------------------

    fn owner_bytes(&self, owner: u32) -> usize {
        self.record_keys
            .iter()
            .filter(|entry| owner_of(entry.key) == owner && !entry.is_tombstone())
            .map(|entry| ring::record_bytes(entry.length()))
            .sum()
    }

    pub async fn write_record(
        &mut self,
        key: RecordKey,
        value: &[u8],
    ) -> Result<(), StoreError<Flash::Error>> {
        let owner = owner_of(key);
        if owner < FIRST_SYSTEM_OWNER {
            let replaced =
                find_newest(&self.record_keys, key).map_or(0, |entry| ring::record_bytes(entry.length()));
            if self.owner_bytes(owner) - replaced + ring::record_bytes(value.len()) > OWNER_RECORD_BYTES {
                return Err(StoreError::OverOwnerLimit);
            }
        }
        self.records
            .append(&mut self.flash, self.record_keys.as_mut_view(), key, 0, value)
            .await?;
        self.share_sequence();
        Ok(())
    }

    /// Reads from `offset` into a record, as much as the buffer holds; nothing when the key is missing.
    pub async fn read_record(
        &mut self,
        key: RecordKey,
        offset: usize,
        buffer: &mut [u8],
    ) -> Result<Option<usize>, StoreError<Flash::Error>> {
        self.records
            .read_newest(&mut self.flash, &self.record_keys, key, offset, buffer)
            .await
    }

    pub fn record_length(&self, key: RecordKey) -> Option<usize> {
        find_newest(&self.record_keys, key).map(|entry| entry.length())
    }

    pub async fn delete_record(&mut self, key: RecordKey) -> Result<(), StoreError<Flash::Error>> {
        if find_newest(&self.record_keys, key).is_some() {
            self.records
                .append(
                    &mut self.flash,
                    self.record_keys.as_mut_view(),
                    key,
                    TOMBSTONE,
                    &[],
                )
                .await?;
            self.share_sequence();
        }
        Ok(())
    }

    // ------------------------------------------------------------------------
    // Files
    // ------------------------------------------------------------------------

    fn small_file_key(path: &str) -> RecordKey {
        record_key(SMALL_FILE_OWNER, checksum::path_hash(path))
    }

    /// A small file's record holds its path's length, its path, then its data.
    async fn find_small_file(&mut self, path: &str) -> Result<Option<SmallFile>, StoreError<Flash::Error>> {
        let key = Self::small_file_key(path);
        let Some(entry) = find_newest(&self.record_keys, key) else {
            return Ok(None);
        };
        let mut stored = [0u8; 1 + LONGEST_PATH_BYTES];
        let length = entry.length().min(stored.len());
        self.records
            .read_newest(&mut self.flash, &self.record_keys, key, 0, &mut stored[..length])
            .await?;
        let path_length = stored[0] as usize;
        if path_length != path.len()
            || length < 1 + path_length
            || &stored[1..1 + path_length] != path.as_bytes()
        {
            return Ok(None);
        }
        Ok(Some(SmallFile {
            key,
            prefix: 1 + path_length,
            sequence: self.records.record_sequence(&mut self.flash, entry).await?,
        }))
    }

    /// Where a path is kept, as a record and in sectors; only while it moves between them is it both.
    async fn locate(
        &mut self,
        path: &str,
    ) -> Result<(Option<SmallFile>, Option<FileHeader>), StoreError<Flash::Error>> {
        let small = self.find_small_file(path).await?;
        let large = self
            .files
            .find(&mut self.flash, path)
            .await?
            .map(|(_, header)| header);
        Ok((small, large))
    }

    /// Writes a whole file: as a record when small and its key is free for it, otherwise into sectors.
    ///
    /// The new copy is written first and the old form removed after, so a power cut between leaves both, and the
    /// newer is read.
    pub async fn write_file(&mut self, path: &str, data: &[u8]) -> Result<(), StoreError<Flash::Error>> {
        if path.len() > LONGEST_PATH_BYTES {
            return Err(StoreError::PathTooLong);
        }
        if data.len() > LARGEST_FILE_BYTES {
            return Err(StoreError::TooLarge);
        }
        let key = Self::small_file_key(path);
        let key_taken_by_another =
            find_newest(&self.record_keys, key).is_some() && self.find_small_file(path).await?.is_none();
        if data.len() <= SMALL_FILE_BYTES && !key_taken_by_another {
            let path_length = [path.len() as u8];
            let pieces: [&[u8]; 3] = [&path_length, path.as_bytes(), data];
            self.records
                .append_pieces(&mut self.flash, self.record_keys.as_mut_view(), key, &pieces)
                .await?;
            self.share_sequence();
            self.files.delete(&mut self.flash, path).await
        } else {
            self.begin_file(path).await?;
            if let Err(error) = self.write_file_part(data).await {
                self.abandon_file();
                return Err(error);
            }
            self.finish_file().await
        }
    }

    /// Starts writing a file a part at a time, as a Bluetooth upload arrives; it always goes into sectors.
    pub async fn begin_file(&mut self, path: &str) -> Result<(), StoreError<Flash::Error>> {
        if self.upload.is_some() {
            return Err(StoreError::UploadInProgress);
        }
        if path.len() > LONGEST_PATH_BYTES {
            return Err(StoreError::PathTooLong);
        }
        let upload = self.files.begin(&mut self.flash, path).await?;
        match try_box(upload) {
            Ok(upload) => {
                self.upload = Some(upload);
                Ok(())
            }
            Err(upload) => {
                self.files.abandon(&upload);
                Err(StoreError::OutOfMemory)
            }
        }
    }

    pub async fn write_file_part(&mut self, data: &[u8]) -> Result<(), StoreError<Flash::Error>> {
        let mut upload = self.upload.take().ok_or(StoreError::NoUpload)?;
        let result = self.files.write_part(&mut self.flash, &mut upload, data).await;
        self.upload = Some(upload);
        result
    }

    /// Makes the upload the file, then removes any copy of it kept as a record.
    pub async fn finish_file(&mut self) -> Result<(), StoreError<Flash::Error>> {
        let upload = self.upload.take().ok_or(StoreError::NoUpload)?;
        let path = upload_path(&upload);
        self.files.finish(&mut self.flash, &upload).await?;
        self.share_sequence();
        if self.find_small_file(&path).await?.is_some() {
            self.records
                .append(
                    &mut self.flash,
                    self.record_keys.as_mut_view(),
                    Self::small_file_key(&path),
                    TOMBSTONE,
                    &[],
                )
                .await?;
            self.share_sequence();
        }
        Ok(())
    }

    /// The path of the upload in progress and how many bytes it holds so far, so an interrupted one can resume.
    pub fn upload_progress(&self) -> Option<(&str, usize)> {
        self.upload
            .as_ref()
            .map(|upload| (upload.path().as_str(), upload.length()))
    }

    /// Drops an unfinished upload; the file stays as it was.
    pub fn abandon_file(&mut self) {
        if let Some(upload) = self.upload.take() {
            self.files.abandon(&upload);
        }
    }

    pub async fn read_file(
        &mut self,
        path: &str,
        offset: usize,
        buffer: &mut [u8],
    ) -> Result<Option<usize>, StoreError<Flash::Error>> {
        match self.locate(path).await? {
            (Some(small), Some(large)) if large.sequence > small.sequence => {
                self.files.read(&mut self.flash, &large, offset, buffer).await
            }
            (Some(small), _) => {
                self.records
                    .read_newest(
                        &mut self.flash,
                        &self.record_keys,
                        small.key,
                        small.prefix + offset,
                        buffer,
                    )
                    .await
            }
            (None, Some(large)) => self.files.read(&mut self.flash, &large, offset, buffer).await,
            (None, None) => Ok(None),
        }
    }

    /// Where part of a file lies in the flash, for a caller that reads it itself, such as by DMA.
    ///
    /// Gives none when the file is missing, is kept as a record rather than in sectors, or needs more extents
    /// than the slice holds; the caller then reads it with `read_file`. Bytes read this way skip the
    /// checksum a whole-file `read_file` checks.
    pub async fn file_extents(
        &mut self,
        path: &str,
        offset: usize,
        length: usize,
        extents: &mut [Extent],
    ) -> Result<Option<FileExtents>, StoreError<Flash::Error>> {
        let large = match self.locate(path).await? {
            (Some(small), Some(large)) if large.sequence > small.sequence => large,
            (None, Some(large)) => large,
            _ => return Ok(None),
        };
        Ok(self.files.extents(&large, offset, length, extents))
    }

    pub async fn file_length(&mut self, path: &str) -> Result<Option<usize>, StoreError<Flash::Error>> {
        Ok(match self.locate(path).await? {
            (Some(small), Some(large)) if large.sequence > small.sequence => Some(large.length),
            (Some(small), _) => {
                find_newest(&self.record_keys, small.key).map(|entry| entry.length() - small.prefix)
            }
            (None, Some(large)) => Some(large.length),
            (None, None) => None,
        })
    }

    /// Removes the older copy first when a path is both, so a power cut between leaves the file as it was.
    pub async fn delete_file(&mut self, path: &str) -> Result<(), StoreError<Flash::Error>> {
        let (small, large) = self.locate(path).await?;
        let large_is_newer =
            matches!((&small, &large), (Some(small), Some(large)) if large.sequence > small.sequence);
        if large_is_newer {
            self.records
                .append(
                    &mut self.flash,
                    self.record_keys.as_mut_view(),
                    Self::small_file_key(path),
                    TOMBSTONE,
                    &[],
                )
                .await?;
            self.files.delete(&mut self.flash, path).await?;
        } else {
            self.files.delete(&mut self.flash, path).await?;
            if small.is_some() {
                self.records
                    .append(
                        &mut self.flash,
                        self.record_keys.as_mut_view(),
                        Self::small_file_key(path),
                        TOMBSTONE,
                        &[],
                    )
                    .await?;
            }
        }
        self.share_sequence();
        Ok(())
    }

    /// Gives `visit` the path of every file that starts with `prefix`, each once.
    pub async fn for_each_file(
        &mut self,
        prefix: &str,
        mut visit: impl FnMut(&str),
    ) -> Result<(), StoreError<Flash::Error>> {
        self.for_each_file_with_length(prefix, &mut |path, _| visit(path))
            .await
    }

    /// Gives `visit` the path and length in bytes of every file that starts with `prefix`, each once.
    ///
    /// It takes the visitor by reference, so every caller shares one copy of the walk in the firmware.
    pub async fn for_each_file_with_length(
        &mut self,
        prefix: &str,
        visit: &mut dyn FnMut(&str, usize),
    ) -> Result<(), StoreError<Flash::Error>> {
        self.files
            .for_each(&mut self.flash, |header| {
                if header.path.starts_with(prefix) {
                    visit(&header.path, header.length);
                }
            })
            .await?;
        let small_keys: Vec<Newest, MOST_RECORD_KEYS> = self
            .record_keys
            .iter()
            .filter(|entry| owner_of(entry.key) == SMALL_FILE_OWNER && !entry.is_tombstone())
            .copied()
            .collect();
        for entry in small_keys {
            let mut stored = [0u8; 1 + LONGEST_PATH_BYTES];
            let length = entry.length().min(stored.len());
            self.records
                .read_newest(
                    &mut self.flash,
                    &self.record_keys,
                    entry.key,
                    0,
                    &mut stored[..length],
                )
                .await?;
            let path_length = (stored[0] as usize).min(length.saturating_sub(1));
            let Ok(path) = core::str::from_utf8(&stored[1..1 + path_length]) else {
                continue;
            };
            let also_large = self.files.find(&mut self.flash, path).await?.is_some();
            if path.starts_with(prefix) && !also_large {
                visit(path, entry.length().saturating_sub(1 + path_length));
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------------
    // Logs
    // ------------------------------------------------------------------------

    /// Adds bytes to a log entry, batched in RAM; a power cut loses at most what has not been flushed.
    pub async fn append_log(&mut self, key: RecordKey, bytes: &[u8]) -> Result<(), StoreError<Flash::Error>> {
        if bytes.len() > LOG_BATCH_BYTES {
            self.flush_log(key).await?;
            for piece in bytes.chunks(LARGEST_RECORD_BYTES) {
                self.logs
                    .append(&mut self.flash, self.no_keys.as_mut_view(), key, APPENDS, piece)
                    .await?;
            }
            return Ok(());
        }
        if let Some(batch) = self.log_batches.iter().find(|batch| batch.key == key) {
            if batch.length + bytes.len() > LOG_BATCH_BYTES {
                self.flush_log(key).await?;
            }
        }
        let position = match self.log_batches.iter().position(|batch| batch.key == key) {
            Some(position) => position,
            None => {
                if self.log_batches.is_full() {
                    let oldest = self.log_batches[0].key;
                    self.flush_log(oldest).await?;
                }
                let empty = LogBatch {
                    key,
                    length: 0,
                    bytes: [0; LOG_BATCH_BYTES],
                };
                let _ = self.log_batches.push(empty);
                self.log_batches.len() - 1
            }
        };
        let batch = &mut self.log_batches[position];
        batch.bytes[batch.length..batch.length + bytes.len()].copy_from_slice(bytes);
        batch.length += bytes.len();
        Ok(())
    }

    async fn flush_log(&mut self, key: RecordKey) -> Result<(), StoreError<Flash::Error>> {
        let Some(position) = self.log_batches.iter().position(|batch| batch.key == key) else {
            return Ok(());
        };
        let batch = self.log_batches.remove(position);
        if batch.length > 0 {
            self.logs
                .append(
                    &mut self.flash,
                    self.no_keys.as_mut_view(),
                    key,
                    APPENDS,
                    &batch.bytes[..batch.length],
                )
                .await?;
        }
        Ok(())
    }

    /// Writes every batch, such as before sleeping or on a timer.
    pub async fn flush_logs(&mut self) -> Result<(), StoreError<Flash::Error>> {
        while let Some(batch) = self.log_batches.first() {
            let key = batch.key;
            self.flush_log(key).await?;
        }
        Ok(())
    }

    /// Reads a log entry with what is still batched, as much as the buffer holds, and gives its whole length.
    pub async fn read_log(
        &mut self,
        key: RecordKey,
        buffer: &mut [u8],
    ) -> Result<Option<usize>, StoreError<Flash::Error>> {
        let stored = self.logs.read_entry(&mut self.flash, key, buffer).await?;
        let Some(batch) = self.log_batches.iter().find(|batch| batch.key == key) else {
            return Ok(stored);
        };
        let start = stored.unwrap_or(0);
        if start < buffer.len() {
            let copied = (start + batch.length).min(buffer.len()) - start;
            buffer[start..start + copied].copy_from_slice(&batch.bytes[..copied]);
        }
        Ok(Some(start + batch.length))
    }
}

/// Moves a value onto the heap when it has room, and gives the value back when it has not.
fn try_box<Value>(value: Value) -> Result<Box<Value>, Value> {
    let mut slot = alloc::vec::Vec::new();
    if slot.try_reserve_exact(1).is_err() {
        return Err(value);
    }
    slot.push(value);
    let one: Box<[Value]> = slot.into_boxed_slice();
    // SAFETY: a slice of exactly one value has that value's layout, so the allocation can be owned as a box of it.
    Ok(unsafe { Box::from_raw(Box::into_raw(one).cast::<Value>()) })
}

fn upload_path(upload: &Upload) -> heapless::String<LONGEST_PATH_BYTES> {
    upload.path().clone()
}
