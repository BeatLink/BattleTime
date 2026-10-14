//! The benchmark's view of NeoTime's flash store (`services/flash_store`): apps as files, steps as a log, the rest
//! as records.

use embedded_storage_async::nor_flash::{ErrorType, NorFlash, ReadNorFlash};
use neotime_flash_store::{FlashStore, Layout};

use crate::{
    flash::{FlashError, SimulatedFlash, SECTOR_BYTES},
    stores::Store,
    workload::{kind_of, Key, Kind},
};

// ============================================================================
// Where each part lives
// ============================================================================

#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub file_sectors: usize,
    pub record_sectors: usize,
    pub log_sectors: usize,
}

impl Geometry {
    /// The 3 MB file system region: 2 MB for files, 256 KB for records and 768 KB for logs.
    pub const WATCH: Geometry = Geometry {
        file_sectors: 512,
        record_sectors: 64,
        log_sectors: 192,
    };

    pub fn sector_count(&self) -> usize {
        self.file_sectors + self.record_sectors + self.log_sectors
    }

    pub fn file_range(&self) -> std::ops::Range<usize> {
        0..self.file_sectors
    }

    pub fn record_range(&self) -> std::ops::Range<usize> {
        self.file_sectors..self.file_sectors + self.record_sectors
    }

    pub fn log_range(&self) -> std::ops::Range<usize> {
        self.file_sectors + self.record_sectors..self.sector_count()
    }

    fn layout(&self) -> Layout {
        Layout {
            start_address: 0,
            file_sectors: self.file_sectors,
            record_sectors: self.record_sectors,
            log_sectors: self.log_sectors,
        }
    }
}

// ============================================================================
// The simulated flash as the store needs it: single bytes, like the watch's driver
// ============================================================================

pub struct ByteFlash(SimulatedFlash);

impl ErrorType for ByteFlash {
    type Error = FlashError;
}

impl ReadNorFlash for ByteFlash {
    const READ_SIZE: usize = 1;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), FlashError> {
        ReadNorFlash::read(&mut self.0, offset, bytes).await
    }

    fn capacity(&self) -> usize {
        ReadNorFlash::capacity(&self.0)
    }
}

impl NorFlash for ByteFlash {
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = SECTOR_BYTES;

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), FlashError> {
        NorFlash::erase(&mut self.0, from, to).await
    }

    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), FlashError> {
        NorFlash::write(&mut self.0, offset, bytes).await
    }
}

/// Sized as the watch would build it: 128 files, 128 record keys, rings of up to 192 sectors.
pub type WatchStore = FlashStore<ByteFlash, 128, 128, 192>;

// ============================================================================
// The store
// ============================================================================

pub struct CustomStore {
    store: WatchStore,
    /// The step day last appended to and its length, so each write appends only what the day gained.
    current_day: Option<(Key, usize)>,
}

impl CustomStore {
    pub async fn mount(flash: SimulatedFlash, geometry: Geometry) -> CustomStore {
        let store = WatchStore::mount(ByteFlash(flash), geometry.layout()).await.expect("mount");
        CustomStore { store, current_day: None }
    }

    pub fn flash(&self) -> &SimulatedFlash {
        &self.store.flash().0
    }

    pub fn ram_bytes(&self) -> usize {
        WatchStore::RAM_BYTES
    }

    pub async fn write_file(&mut self, path: &str, data: &[u8]) {
        self.store.write_file(path, data).await.expect("write file");
    }

    pub async fn read_file(&mut self, path: &str, offset: usize, buffer: &mut [u8]) -> Option<usize> {
        self.store.read_file(path, offset, buffer).await.expect("read file")
    }

    pub async fn delete_file(&mut self, path: &str) {
        self.store.delete_file(path).await.expect("delete file");
    }

    pub async fn list_files(&mut self, prefix: &str) -> Vec<String> {
        let mut paths = Vec::new();
        self.store.for_each_file(prefix, |path| paths.push(path.to_string())).await.expect("list files");
        paths.sort();
        paths
    }
}

pub fn app_path(key: Key) -> String {
    format!("/apps/{:03x}.ntapp", key >> 20)
}

impl Store for CustomStore {
    async fn write_all(&mut self, values: &[(Key, Vec<u8>)]) {
        for (key, value) in values {
            let kind = kind_of(*key);
            if kind == Kind::WholeImage as u32 {
                self.write_file(&app_path(*key), value).await;
            } else if kind == Kind::Steps as u32 {
                // Days only move forward, so a newer day starts empty and the log need not be scanned.
                let known = match self.current_day {
                    Some((day, length)) if day == *key => length,
                    Some((day, _)) if day < *key => 0,
                    None => 0,
                    _ => self.store.read_log(*key, &mut []).await.expect("log length").unwrap_or(0),
                };
                if value.len() > known {
                    self.store.append_log(*key, &value[known..]).await.expect("append log");
                    self.store.flush_logs().await.expect("flush logs");
                }
                self.current_day = Some((*key, value.len()));
            } else {
                self.store.write_record(*key, value).await.expect("write record");
            }
        }
    }

    async fn read(&mut self, key: Key, buffer: &mut [u8]) -> Option<usize> {
        let kind = kind_of(key);
        if kind == Kind::WholeImage as u32 {
            self.read_file(&app_path(key), 0, buffer).await
        } else if kind == Kind::Steps as u32 {
            self.store.read_log(key, buffer).await.expect("read log")
        } else {
            self.store.read_record(key, 0, buffer).await.expect("read record")
        }
    }
}
