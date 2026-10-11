//! NeoTime's file system: littlefs in the `FILE_SYSTEM` region of the external flash.
//!
//! littlefs survives a power cut at any point and spreads wear across the flash. It comes from
//! `littlefs-rust`, a pure Rust translation of the C littlefs with the same on-disk format. A
//! blank region is formatted on first start, but flash that cannot be read is never formatted, so a
//! bus fault does not wipe the watch. The file system keeps its buffers on the heap.

#![no_std]

use embedded_storage::nor_flash::NorFlash;
use littlefs_rust::{Config, Error, Filesystem, Storage};
use neotime_boot::FILE_SYSTEM;

// ============================================================================
// Directories
// ============================================================================

/// Each installed app or watch face keeps its code and data in its own folder here.
pub const APPS_DIRECTORY: &str = "/apps";
pub const SETTINGS_DIRECTORY: &str = "/settings";
/// Fonts and pictures shared by the system and the apps.
pub const RESOURCES_DIRECTORY: &str = "/resources";

pub const TOP_LEVEL_DIRECTORIES: [&str; 3] = [APPS_DIRECTORY, SETTINGS_DIRECTORY, RESOURCES_DIRECTORY];

// ============================================================================
// Settings
// ============================================================================

pub const BLOCK_SIZE: u32 = 4096;
pub const BLOCK_COUNT: u32 = FILE_SYSTEM.size / BLOCK_SIZE;
const SMALLEST_READ_OR_WRITE: u32 = 16;
const CACHE_SIZE: u32 = 256;
const LOOKAHEAD_SIZE_IN_BYTES: u32 = 32;
const ERASES_BEFORE_MOVING_METADATA: i32 = 500;

const _: () = assert!(FILE_SYSTEM.size.is_multiple_of(BLOCK_SIZE));

pub fn file_system_config() -> Config {
    let mut config = Config::new(BLOCK_SIZE, BLOCK_COUNT);
    config.read_size = SMALLEST_READ_OR_WRITE;
    config.prog_size = SMALLEST_READ_OR_WRITE;
    config.cache_size = CACHE_SIZE;
    config.lookahead_size = LOOKAHEAD_SIZE_IN_BYTES;
    config.block_cycles = ERASES_BEFORE_MOVING_METADATA;
    config
}

// ============================================================================
// The flash region littlefs sees
// ============================================================================

pub struct FileSystemFlash<Flash> {
    flash: Flash,
}

impl<Flash: NorFlash> FileSystemFlash<Flash> {
    pub fn new(flash: Flash) -> FileSystemFlash<Flash> {
        FileSystemFlash { flash }
    }

    pub fn release(self) -> Flash {
        self.flash
    }

    fn flash_address(block: u32, offset_in_block: u32) -> u32 {
        FILE_SYSTEM.start + block * BLOCK_SIZE + offset_in_block
    }
}

impl<Flash: NorFlash> Storage for FileSystemFlash<Flash> {
    fn read(&mut self, block: u32, offset_in_block: u32, bytes: &mut [u8]) -> Result<(), Error> {
        self.flash
            .read(Self::flash_address(block, offset_in_block), bytes)
            .map_err(|_| Error::Io)
    }

    fn write(&mut self, block: u32, offset_in_block: u32, bytes: &[u8]) -> Result<(), Error> {
        self.flash
            .write(Self::flash_address(block, offset_in_block), bytes)
            .map_err(|_| Error::Io)
    }

    fn erase(&mut self, block: u32) -> Result<(), Error> {
        let block_start = Self::flash_address(block, 0);
        self.flash
            .erase(block_start, block_start + BLOCK_SIZE)
            .map_err(|_| Error::Io)
    }
}

// ============================================================================
// Preparing the file system at start
// ============================================================================

pub type FileSystem<Flash> = Filesystem<FileSystemFlash<Flash>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preparation {
    AlreadyFormatted,
    FormattedBlankFlash,
}

pub struct PreparedFileSystem<Flash: NorFlash> {
    pub file_system: FileSystem<Flash>,
    pub preparation: Preparation,
}

/// The flash is handed back so the caller can try again.
pub struct PreparationFailed<Flash> {
    pub error: Error,
    pub storage: Option<FileSystemFlash<Flash>>,
}

/// Mounts the file system, formatting it only if littlefs finds none, and makes the top-level folders.
pub fn prepare_file_system<Flash: NorFlash>(
    storage: FileSystemFlash<Flash>,
) -> Result<PreparedFileSystem<Flash>, PreparationFailed<Flash>> {
    let (file_system, preparation) = match Filesystem::mount(storage, file_system_config()) {
        Ok(file_system) => (file_system, Preparation::AlreadyFormatted),
        Err((Error::Corrupt, mut storage)) => {
            if let Err(error) = Filesystem::format(&mut storage, &file_system_config()) {
                return Err(PreparationFailed {
                    error,
                    storage: Some(storage),
                });
            }
            match Filesystem::mount(storage, file_system_config()) {
                Ok(file_system) => (file_system, Preparation::FormattedBlankFlash),
                Err((error, storage)) => {
                    return Err(PreparationFailed {
                        error,
                        storage: Some(storage),
                    })
                }
            }
        }
        Err((error, storage)) => {
            return Err(PreparationFailed {
                error,
                storage: Some(storage),
            })
        }
    };

    for directory in TOP_LEVEL_DIRECTORIES {
        if !file_system.exists(directory) {
            if let Err(error) = file_system.mkdir(directory) {
                return Err(PreparationFailed {
                    error,
                    storage: file_system.unmount().ok(),
                });
            }
        }
    }

    Ok(PreparedFileSystem {
        file_system,
        preparation,
    })
}

#[cfg(test)]
mod tests;
