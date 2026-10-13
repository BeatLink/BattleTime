//! Flash access that turns the flash's errors into the store's, and a writer that programs each page once.

use embedded_storage_async::nor_flash::NorFlash;

use crate::{StoreError, SECTOR_BYTES};

/// The most RAM any operation borrows to move data, and the flash's program page.
pub(crate) const PIECE_BYTES: usize = 256;

pub(crate) async fn read<Flash: NorFlash>(
    flash: &mut Flash,
    address: u32,
    bytes: &mut [u8],
) -> Result<(), StoreError<Flash::Error>> {
    flash.read(address, bytes).await.map_err(StoreError::Flash)
}

pub(crate) async fn write<Flash: NorFlash>(
    flash: &mut Flash,
    address: u32,
    bytes: &[u8],
) -> Result<(), StoreError<Flash::Error>> {
    flash.write(address, bytes).await.map_err(StoreError::Flash)
}

pub(crate) async fn erase<Flash: NorFlash>(
    flash: &mut Flash,
    sector_address: u32,
) -> Result<(), StoreError<Flash::Error>> {
    flash
        .erase(sector_address, sector_address + SECTOR_BYTES)
        .await
        .map_err(StoreError::Flash)
}

pub(crate) fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

pub(crate) fn half_word(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

pub(crate) fn rounded_to_word(length: usize) -> usize {
    length.next_multiple_of(4)
}

// ============================================================================
// Writing a page at a time
// ============================================================================

/// Gathers bytes and writes them at each 256-byte page boundary, so each page is programmed once.
///
/// Pages go out in address order, so what is pushed first is on the flash before what is pushed last.
pub(crate) struct PageWriter {
    start: u32,
    buffer: [u8; PIECE_BYTES],
    filled: usize,
}

impl PageWriter {
    pub(crate) fn new(address: u32) -> PageWriter {
        PageWriter {
            start: address,
            buffer: [0xff; PIECE_BYTES],
            filled: 0,
        }
    }

    pub(crate) async fn push<Flash: NorFlash>(
        &mut self,
        flash: &mut Flash,
        mut bytes: &[u8],
    ) -> Result<(), StoreError<Flash::Error>> {
        while !bytes.is_empty() {
            let room_in_page = PIECE_BYTES - (self.start as usize + self.filled) % PIECE_BYTES;
            let size = room_in_page.min(bytes.len());
            self.buffer[self.filled..self.filled + size].copy_from_slice(&bytes[..size]);
            self.filled += size;
            bytes = &bytes[size..];
            if (self.start as usize + self.filled).is_multiple_of(PIECE_BYTES) {
                self.finish(flash).await?;
            }
        }
        Ok(())
    }

    pub(crate) async fn finish<Flash: NorFlash>(
        &mut self,
        flash: &mut Flash,
    ) -> Result<(), StoreError<Flash::Error>> {
        if self.filled > 0 {
            write(flash, self.start, &self.buffer[..self.filled]).await?;
            self.start += self.filled as u32;
            self.filled = 0;
        }
        Ok(())
    }
}
