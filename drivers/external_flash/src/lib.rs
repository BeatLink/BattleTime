//! Driver for the PineTime's 4 MB SPI NOR flash, an XT25F32B, as an `embedded-storage` `NorFlash`.
//!
//! The chip shares its SPI bus with the screen. Writes are split at the chip's 256-byte program
//! pages, and the chip is woken from deep power-down on start, since the firmware may have left it
//! asleep before a reset.

#![no_std]

use embedded_hal::{
    delay::DelayNs,
    spi::{Operation, SpiDevice},
};
use embedded_storage::nor_flash::{
    check_erase, check_read, check_write, ErrorType, NorFlash, NorFlashError, NorFlashErrorKind, ReadNorFlash,
};

// ============================================================================
// Chip
// ============================================================================

pub const CAPACITY_IN_BYTES: usize = 4 * 1024 * 1024;
pub const SECTOR_SIZE: usize = 4096;
pub const PROGRAM_PAGE_SIZE: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChipId {
    pub manufacturer: u8,
    pub memory_type: u8,
    pub capacity: u8,
}

pub const XT25F32B_CHIP_ID: ChipId = ChipId {
    manufacturer: 0x0B,
    memory_type: 0x40,
    capacity: 0x16,
};
const FOUR_MEGABYTE_CAPACITY_CODE: u8 = 0x16;

const READ_DATA: u8 = 0x03;
const PAGE_PROGRAM: u8 = 0x02;
const SECTOR_ERASE: u8 = 0x20;
const WRITE_ENABLE: u8 = 0x06;
const READ_STATUS: u8 = 0x05;
const READ_CHIP_ID: u8 = 0x9F;
const DEEP_POWER_DOWN: u8 = 0xB9;
const RELEASE_POWER_DOWN: u8 = 0xAB;

const WRITE_IN_PROGRESS_BIT: u8 = 0b0000_0001;
const WAKE_UP_TIME_IN_MICROSECONDS: u32 = 50;

// ============================================================================
// Errors
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error<SpiError> {
    Spi(SpiError),
    UnknownChip(ChipId),
    BadRange(NorFlashErrorKind),
}

impl<SpiError: core::fmt::Debug> NorFlashError for Error<SpiError> {
    fn kind(&self) -> NorFlashErrorKind {
        match self {
            Error::BadRange(kind) => *kind,
            Error::Spi(_) | Error::UnknownChip(_) => NorFlashErrorKind::Other,
        }
    }
}

// ============================================================================
// External flash
// ============================================================================

pub struct ExternalFlash<FlashSpi> {
    spi: FlashSpi,
}

impl<FlashSpi: SpiDevice> ExternalFlash<FlashSpi> {
    /// Wakes the chip and checks that it answers as a 4 MB chip, so a second-source part works too.
    pub fn new(spi: FlashSpi, delay: &mut impl DelayNs) -> Result<Self, Error<FlashSpi::Error>> {
        let mut flash = ExternalFlash { spi };

        flash.wake(delay)?;
        let chip_id = flash.read_chip_id()?;

        if chip_id.capacity != FOUR_MEGABYTE_CAPACITY_CODE {
            return Err(Error::UnknownChip(chip_id));
        }
        Ok(flash)
    }

    pub fn read_chip_id(&mut self) -> Result<ChipId, Error<FlashSpi::Error>> {
        let mut id_bytes = [0; 3];
        self.spi
            .transaction(&mut [Operation::Write(&[READ_CHIP_ID]), Operation::Read(&mut id_bytes)])
            .map_err(Error::Spi)?;

        let [manufacturer, memory_type, capacity] = id_bytes;
        Ok(ChipId {
            manufacturer,
            memory_type,
            capacity,
        })
    }

    pub fn sleep(&mut self) -> Result<(), Error<FlashSpi::Error>> {
        self.spi.write(&[DEEP_POWER_DOWN]).map_err(Error::Spi)
    }

    pub fn wake(&mut self, delay: &mut impl DelayNs) -> Result<(), Error<FlashSpi::Error>> {
        self.spi.write(&[RELEASE_POWER_DOWN]).map_err(Error::Spi)?;
        delay.delay_us(WAKE_UP_TIME_IN_MICROSECONDS);
        Ok(())
    }

    pub fn release(self) -> FlashSpi {
        self.spi
    }
}

// ============================================================================
// Commands
// ============================================================================

impl<FlashSpi: SpiDevice> ExternalFlash<FlashSpi> {
    fn command_with_address(command: u8, address: u32) -> [u8; 4] {
        let [_, high_byte, middle_byte, low_byte] = address.to_be_bytes();
        [command, high_byte, middle_byte, low_byte]
    }

    fn enable_writing(&mut self) -> Result<(), Error<FlashSpi::Error>> {
        self.spi.write(&[WRITE_ENABLE]).map_err(Error::Spi)
    }

    fn wait_until_ready(&mut self) -> Result<(), Error<FlashSpi::Error>> {
        loop {
            let mut status = [0];
            self.spi
                .transaction(&mut [Operation::Write(&[READ_STATUS]), Operation::Read(&mut status)])
                .map_err(Error::Spi)?;

            if status[0] & WRITE_IN_PROGRESS_BIT == 0 {
                return Ok(());
            }
        }
    }

    fn program_page(&mut self, address: u32, data: &[u8]) -> Result<(), Error<FlashSpi::Error>> {
        let header = Self::command_with_address(PAGE_PROGRAM, address);

        self.enable_writing()?;
        self.spi
            .transaction(&mut [Operation::Write(&header), Operation::Write(data)])
            .map_err(Error::Spi)?;
        self.wait_until_ready()
    }

    fn erase_sector(&mut self, address: u32) -> Result<(), Error<FlashSpi::Error>> {
        let command = Self::command_with_address(SECTOR_ERASE, address);

        self.enable_writing()?;
        self.spi.write(&command).map_err(Error::Spi)?;
        self.wait_until_ready()
    }
}

// ============================================================================
// embedded-storage
// ============================================================================

impl<FlashSpi: SpiDevice> ErrorType for ExternalFlash<FlashSpi> {
    type Error = Error<FlashSpi::Error>;
}

impl<FlashSpi: SpiDevice> ReadNorFlash for ExternalFlash<FlashSpi> {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        check_read(self, offset, bytes.len()).map_err(Error::BadRange)?;

        let header = Self::command_with_address(READ_DATA, offset);
        self.spi
            .transaction(&mut [Operation::Write(&header), Operation::Read(bytes)])
            .map_err(Error::Spi)
    }

    fn capacity(&self) -> usize {
        CAPACITY_IN_BYTES
    }
}

impl<FlashSpi: SpiDevice> NorFlash for ExternalFlash<FlashSpi> {
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = SECTOR_SIZE;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        check_erase(self, from, to).map_err(Error::BadRange)?;

        for sector_start in (from..to).step_by(SECTOR_SIZE) {
            self.erase_sector(sector_start)?;
        }
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        check_write(self, offset, bytes.len()).map_err(Error::BadRange)?;

        let mut bytes_written = 0;
        while bytes_written < bytes.len() {
            let address = offset as usize + bytes_written;
            let room_left_in_page = PROGRAM_PAGE_SIZE - address % PROGRAM_PAGE_SIZE;
            let chunk_length = room_left_in_page.min(bytes.len() - bytes_written);

            self.program_page(
                address as u32,
                &bytes[bytes_written..bytes_written + chunk_length],
            )?;
            bytes_written += chunk_length;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
