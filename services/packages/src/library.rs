//! The shared library in internal flash: icons now, fonts later, which every app can use.
//!
//! The image goes into the `SHARED_LIBRARY` region behind an 8-byte header, `NTLB` and its length,
//! because the region's erased tail would otherwise spoil the image's CRC. Nothing outside the
//! region is ever erased or written.

use embedded_storage::nor_flash::NorFlash;
use littlefs_rust::{Filesystem, OpenFlags, Storage};
use neotime_boot::{PAGE_SIZE, SHARED_LIBRARY};

use crate::PackageError;

/// Marks the start of an installed library.
pub const LIBRARY_MAGIC: [u8; 4] = *b"NTLB";
pub const LIBRARY_HEADER_BYTES: u32 = 8;
/// The largest library the region holds.
pub const MOST_LIBRARY_BYTES: u32 = SHARED_LIBRARY.size - LIBRARY_HEADER_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryError {
    Image(PackageError),
    TooLarge,
    Flash,
}

impl LibraryError {
    pub fn describe(self) -> &'static str {
        match self {
            LibraryError::Image(error) => error.describe(),
            LibraryError::TooLarge => "the library is larger than its flash region",
            LibraryError::Flash => "writing the internal flash failed",
        }
    }
}

/// The installed library's image, from the bytes of the whole region, or None when none is installed.
pub fn installed_library(region: &[u8]) -> Option<&[u8]> {
    if region.len() < LIBRARY_HEADER_BYTES as usize || region[0..4] != LIBRARY_MAGIC {
        return None;
    }
    let length = u32::from_le_bytes([region[4], region[5], region[6], region[7]]) as usize;
    region.get(LIBRARY_HEADER_BYTES as usize..LIBRARY_HEADER_BYTES as usize + length)
}

/// The version an installed library image records, read from its header without checking the image.
pub fn library_version(image: &[u8]) -> Option<u16> {
    image
        .get(8..10)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
}

/// How much of the file is copied at a time, so the library never has to fit in RAM.
const COPY_BYTES: usize = 256;

/// Copies a shared library from a file into the region in pieces, checks it where it now lies, and
/// writes the header only if it passes. Only the pages it needs inside the region are erased.
/// `flash` addresses the whole internal flash. `check_in_place` is given the image's length and must
/// check the bytes now at `SHARED_LIBRARY.start + LIBRARY_HEADER_BYTES`; on the watch it reads them
/// straight from the memory-mapped flash.
pub fn install_library_from_file<S: Storage, Flash: NorFlash>(
    flash: &mut Flash,
    files: &Filesystem<S>,
    path: &str,
    check_in_place: impl FnOnce(&mut Flash, u32) -> Result<(), PackageError>,
) -> Result<(), LibraryError> {
    let missing = LibraryError::Image(PackageError::NotInstalled);
    let file = files.open(path, OpenFlags::READ).map_err(|_| missing)?;
    let length = file.size();
    if length > MOST_LIBRARY_BYTES {
        return Err(LibraryError::TooLarge);
    }
    let erase_end = SHARED_LIBRARY.start + (LIBRARY_HEADER_BYTES + length).div_ceil(PAGE_SIZE) * PAGE_SIZE;
    debug_assert!(erase_end <= SHARED_LIBRARY.end());
    flash
        .erase(SHARED_LIBRARY.start, erase_end)
        .map_err(|_| LibraryError::Flash)?;

    let mut address = SHARED_LIBRARY.start + LIBRARY_HEADER_BYTES;
    let mut piece = [0xFF; COPY_BYTES];
    loop {
        let read = file.read(&mut piece).map_err(|_| LibraryError::Flash)? as usize;
        if read == 0 {
            break;
        }
        write_padded(flash, address, &piece[..read])?;
        address += read as u32;
    }
    drop(file);
    if address - SHARED_LIBRARY.start - LIBRARY_HEADER_BYTES != length {
        return Err(LibraryError::Flash);
    }

    // The header goes last, so a library that fails its checks or is cut short is never found.
    check_in_place(flash, length).map_err(LibraryError::Image)?;
    let mut header = [0; LIBRARY_HEADER_BYTES as usize];
    header[0..4].copy_from_slice(&LIBRARY_MAGIC);
    header[4..8].copy_from_slice(&length.to_le_bytes());
    flash
        .write(SHARED_LIBRARY.start, &header)
        .map_err(|_| LibraryError::Flash)
}

/// Writes whole words, filling the last one with the erased value.
fn write_padded<Flash: NorFlash>(flash: &mut Flash, address: u32, bytes: &[u8]) -> Result<(), LibraryError> {
    let word = Flash::WRITE_SIZE.max(1);
    let whole = bytes.len() - bytes.len() % word;
    if whole > 0 {
        flash
            .write(address, &bytes[..whole])
            .map_err(|_| LibraryError::Flash)?;
    }
    if whole < bytes.len() {
        let mut last = [0xFF; 16];
        let tail = &bytes[whole..];
        last[..tail.len()].copy_from_slice(tail);
        flash
            .write(address + whole as u32, &last[..word])
            .map_err(|_| LibraryError::Flash)?;
    }
    Ok(())
}
