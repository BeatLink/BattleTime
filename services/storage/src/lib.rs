//! NeoTime's file system: littlefs in the `FILE_SYSTEM` region of the external flash.
//!
//! littlefs survives a power cut at any point and spreads wear across the flash. It comes from
//! `littlefs-rust`, a pure Rust translation of the C littlefs with the same on-disk format. A
//! blank region is formatted on first start, but flash that cannot be read is never formatted, so a
//! bus fault does not wipe the watch. The file system keeps its buffers on the heap.

#![no_std]

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use embedded_storage::nor_flash::NorFlash;
use littlefs_rust::{Config, Error, Filesystem, OpenFlags, SeekFrom, Storage};
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
    erased_ahead: Option<&'static ErasedBlocks>,
}

impl<Flash: NorFlash> FileSystemFlash<Flash> {
    pub fn new(flash: Flash) -> FileSystemFlash<Flash> {
        FileSystemFlash {
            flash,
            erased_ahead: None,
        }
    }

    /// Skips littlefs's erase of any block `erased_ahead` says was erased ahead of it.
    pub fn with_blocks_erased_ahead(
        flash: Flash,
        erased_ahead: &'static ErasedBlocks,
    ) -> FileSystemFlash<Flash> {
        FileSystemFlash {
            flash,
            erased_ahead: Some(erased_ahead),
        }
    }

    pub fn release(self) -> Flash {
        self.flash
    }

    fn flash_address(block: u32, offset_in_block: u32) -> u32 {
        block_address(block) + offset_in_block
    }
}

/// Where a littlefs block starts in the external flash.
pub fn block_address(block: u32) -> u32 {
    FILE_SYSTEM.start + block * BLOCK_SIZE
}

impl<Flash: NorFlash> Storage for FileSystemFlash<Flash> {
    fn read(&mut self, block: u32, offset_in_block: u32, bytes: &mut [u8]) -> Result<(), Error> {
        self.flash
            .read(Self::flash_address(block, offset_in_block), bytes)
            .map_err(|_| Error::Io)
    }

    fn write(&mut self, block: u32, offset_in_block: u32, bytes: &[u8]) -> Result<(), Error> {
        if let Some(erased_ahead) = self.erased_ahead {
            erased_ahead.forget(block);
        }
        self.flash
            .write(Self::flash_address(block, offset_in_block), bytes)
            .map_err(|_| Error::Io)
    }

    fn erase(&mut self, block: u32) -> Result<(), Error> {
        if let Some(erased_ahead) = self.erased_ahead {
            if erased_ahead.take_erased(block) {
                return Ok(());
            }
        }
        let block_start = Self::flash_address(block, 0);
        self.flash
            .erase(block_start, block_start + BLOCK_SIZE)
            .map_err(|_| Error::Io)
    }
}

// ============================================================================
// Erasing ahead
// ============================================================================

const BLOCK_WORDS: usize = BLOCK_COUNT.div_ceil(u32::BITS) as usize;

/// Blocks erased ahead of littlefs, so its own erase of one returns at once instead of waiting on the chip.
///
/// A block is first marked as being erased, and counts as erased only if littlefs neither erased nor
/// wrote it while that erase ran. Any write or erase by littlefs forgets the block again.
pub struct ErasedBlocks {
    erased: [AtomicU32; BLOCK_WORDS],
    being_erased: [AtomicU32; BLOCK_WORDS],
    /// The block littlefs last erased itself, near which it allocates next.
    last_erased_by_littlefs: AtomicU32,
    /// littlefs makes itself consistent before its first erase, after which no orphan block remains.
    littlefs_has_erased: AtomicBool,
    erases_skipped: AtomicU32,
}

impl Default for ErasedBlocks {
    fn default() -> Self {
        Self::new()
    }
}

impl ErasedBlocks {
    pub const fn new() -> ErasedBlocks {
        ErasedBlocks {
            erased: [const { AtomicU32::new(0) }; BLOCK_WORDS],
            being_erased: [const { AtomicU32::new(0) }; BLOCK_WORDS],
            last_erased_by_littlefs: AtomicU32::new(0),
            littlefs_has_erased: AtomicBool::new(false),
            erases_skipped: AtomicU32::new(0),
        }
    }

    /// How many of littlefs's erases returned at once because their block was erased ahead.
    pub fn erases_skipped(&self) -> u32 {
        self.erases_skipped.load(Ordering::Relaxed)
    }

    pub fn is_erased(&self, block: u32) -> bool {
        has_bit(&self.erased, block)
    }

    /// Marks `block` as about to be erased ahead of littlefs.
    pub fn begin_erasing(&self, block: u32) {
        set_bit(&self.being_erased, block, true);
    }

    /// Counts `block` as erased, unless littlefs used it since `begin_erasing`.
    pub fn finish_erasing(&self, block: u32) {
        if has_bit(&self.being_erased, block) {
            set_bit(&self.being_erased, block, false);
            set_bit(&self.erased, block, true);
        }
    }

    fn forget(&self, block: u32) {
        set_bit(&self.being_erased, block, false);
        set_bit(&self.erased, block, false);
    }

    /// Says whether `block` is already erased, and notes littlefs is about to write it either way.
    fn take_erased(&self, block: u32) -> bool {
        let was_erased = self.is_erased(block);
        self.forget(block);
        self.last_erased_by_littlefs.store(block, Ordering::Relaxed);
        self.littlefs_has_erased.store(true, Ordering::Relaxed);
        if was_erased {
            self.erases_skipped.fetch_add(1, Ordering::Relaxed);
        }
        was_erased
    }
}

fn has_bit(words: &[AtomicU32; BLOCK_WORDS], block: u32) -> bool {
    let Some(word) = words.get((block / u32::BITS) as usize) else {
        return false;
    };
    word.load(Ordering::Relaxed) & (1 << (block % u32::BITS)) != 0
}

fn set_bit(words: &[AtomicU32; BLOCK_WORDS], block: u32, value: bool) {
    let Some(word) = words.get((block / u32::BITS) as usize) else {
        return;
    };
    let bit = 1 << (block % u32::BITS);
    match value {
        true => word.fetch_or(bit, Ordering::Relaxed),
        false => word.fetch_and(!bit, Ordering::Relaxed),
    };
}

/// How many free blocks past littlefs's last erase are kept erased ahead of it.
pub const BLOCKS_KEPT_ERASED: usize = 4;

/// The next free block past where littlefs last erased that is not erased yet, or none when the
/// blocks kept erased are all done, or littlefs has not erased anything since it mounted.
pub fn next_block_to_erase_ahead<AnyStorage: Storage>(
    file_system: &Filesystem<AnyStorage>,
    erased_ahead: &ErasedBlocks,
) -> Option<u32> {
    if !erased_ahead.littlefs_has_erased.load(Ordering::Relaxed) {
        return None;
    }
    let mut used = [0u32; BLOCK_WORDS];
    file_system
        .for_each_used_block(&mut |block| {
            if let Some(word) = used.get_mut((block / u32::BITS) as usize) {
                *word |= 1 << (block % u32::BITS);
            }
        })
        .ok()?;
    let is_used = |block: u32| used[(block / u32::BITS) as usize] & (1 << (block % u32::BITS)) != 0;

    let start = erased_ahead.last_erased_by_littlefs.load(Ordering::Relaxed);
    (1..BLOCK_COUNT)
        .map(|step| (start + step) % BLOCK_COUNT)
        .filter(|&block| !is_used(block))
        .take(BLOCKS_KEPT_ERASED)
        .find(|&block| !erased_ahead.is_erased(block))
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

// ============================================================================
// Free space
// ============================================================================

const BYTES_IN_A_KILOBYTE: u32 = 1024;

/// The space no file or folder uses, in whole kilobytes, or nothing when littlefs cannot count it.
pub fn free_kilobytes<AnyStorage: Storage>(file_system: &Filesystem<AnyStorage>) -> Option<u32> {
    let used_blocks = file_system.fs_size().ok()?;
    Some(BLOCK_COUNT.saturating_sub(used_blocks) * (BLOCK_SIZE / BYTES_IN_A_KILOBYTE))
}

// ============================================================================
// Reading part of a file
// ============================================================================

/// Reads from `offset` bytes into a file, as much as `out` holds; fewer only at the file's end.
///
/// Gives none when the file cannot be opened or read, such as when it is missing.
pub fn read_part<AnyStorage: Storage>(
    file_system: &Filesystem<AnyStorage>,
    path: &str,
    offset: u32,
    out: &mut [u8],
) -> Option<usize> {
    let file = file_system.open(path, OpenFlags::READ).ok()?;
    if offset >= file.size() {
        return Some(0);
    }
    file.seek(SeekFrom::Start(offset)).ok()?;
    file.read(out).ok().map(|length| length as usize)
}

#[cfg(test)]
mod tests;
