//! The shared library in internal flash: icons now, fonts later, which every app can use.
//!
//! The image goes into the `SHARED_LIBRARY` region behind an 8-byte header, `NTLB` and its length,
//! because the region's erased tail would otherwise spoil the image's CRC. Nothing outside the
//! region is ever erased or written.

use embedded_storage::nor_flash::NorFlash;
use neotime_boot::{PAGE_SIZE, SHARED_LIBRARY};

use crate::{check_library_image, PackageError};

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

/// Checks a library image, then erases just the pages it needs in the region and writes it there.
/// `flash` addresses the whole internal flash, as the chip's flash controller does.
pub fn install_library<Flash: NorFlash>(flash: &mut Flash, image: &[u8]) -> Result<(), LibraryError> {
    check_library_image(image).map_err(LibraryError::Image)?;
    if image.len() as u32 > MOST_LIBRARY_BYTES {
        return Err(LibraryError::TooLarge);
    }
    let total = LIBRARY_HEADER_BYTES + image.len() as u32;
    let erase_end = SHARED_LIBRARY.start + total.div_ceil(PAGE_SIZE) * PAGE_SIZE;
    debug_assert!(erase_end <= SHARED_LIBRARY.end());
    flash
        .erase(SHARED_LIBRARY.start, erase_end)
        .map_err(|_| LibraryError::Flash)?;

    // The image first and the header last, so a cut in the middle leaves no header and no library.
    let image_start = SHARED_LIBRARY.start + LIBRARY_HEADER_BYTES;
    write_padded(flash, image_start, image)?;
    let mut header = [0; LIBRARY_HEADER_BYTES as usize];
    header[0..4].copy_from_slice(&LIBRARY_MAGIC);
    header[4..8].copy_from_slice(&(image.len() as u32).to_le_bytes());
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
