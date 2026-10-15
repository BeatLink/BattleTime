//! A flash in memory that programs as NOR does, for other crates' tests.

extern crate alloc;

use alloc::{vec, vec::Vec};

use embedded_storage_async::nor_flash::{ErrorType, NorFlash, NorFlashErrorKind, ReadNorFlash};

use crate::SECTOR_BYTES;

pub struct RamFlash {
    pub bytes: Vec<u8>,
    /// How many sectors have been erased, for tests that count wear.
    pub erases: u32,
}

impl RamFlash {
    pub fn blank(sectors: usize) -> RamFlash {
        RamFlash {
            bytes: vec![0xff; sectors * SECTOR_BYTES as usize],
            erases: 0,
        }
    }
}

impl ErrorType for RamFlash {
    type Error = NorFlashErrorKind;
}

impl ReadNorFlash for RamFlash {
    const READ_SIZE: usize = 1;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), NorFlashErrorKind> {
        let start = offset as usize;
        let source = self
            .bytes
            .get(start..start + bytes.len())
            .ok_or(NorFlashErrorKind::OutOfBounds)?;
        bytes.copy_from_slice(source);
        Ok(())
    }

    fn capacity(&self) -> usize {
        self.bytes.len()
    }
}

impl NorFlash for RamFlash {
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = SECTOR_BYTES as usize;

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), NorFlashErrorKind> {
        let range = self
            .bytes
            .get_mut(from as usize..to as usize)
            .ok_or(NorFlashErrorKind::OutOfBounds)?;
        range.fill(0xff);
        self.erases += (to - from) / SECTOR_BYTES;
        Ok(())
    }

    async fn write(&mut self, offset: u32, data: &[u8]) -> Result<(), NorFlashErrorKind> {
        let start = offset as usize;
        let target = self
            .bytes
            .get_mut(start..start + data.len())
            .ok_or(NorFlashErrorKind::OutOfBounds)?;
        for (stored, new) in target.iter_mut().zip(data) {
            *stored &= new;
        }
        Ok(())
    }
}
