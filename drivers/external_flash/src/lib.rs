//! Driver for the PineTime's 4 MB SPI NOR flash, an XT25F32B, as an `embedded-storage` `NorFlash`.
//!
//! The chip shares its SPI bus with the screen. Writes are split at the chip's 256-byte program
//! pages, and the chip is woken from deep power-down on start, since the firmware may have left it
//! asleep before a reset. `ExternalFlash` blocks, for the bootloader; `AsyncExternalFlash` waits
//! with an async delay while the chip is busy, so the firmware's other tasks keep running.

#![no_std]

use embedded_hal::{
    delay::DelayNs,
    spi::{Operation, SpiDevice},
};
use embedded_storage::nor_flash::{
    ErrorType, MultiwriteNorFlash, NorFlash, NorFlashError, NorFlashErrorKind, ReadNorFlash,
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
const BUSY_POLL_INTERVAL_IN_MICROSECONDS: u32 = 100;

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
// Commands, ranges and pages, shared by both drivers
// ============================================================================

fn command_with_address(command: u8, address: u32) -> [u8; 4] {
    let [_, high_byte, middle_byte, low_byte] = address.to_be_bytes();
    [command, high_byte, middle_byte, low_byte]
}

fn check_chip_id<SpiError>(chip_id: ChipId) -> Result<(), Error<SpiError>> {
    if chip_id.capacity == FOUR_MEGABYTE_CAPACITY_CODE {
        Ok(())
    } else {
        Err(Error::UnknownChip(chip_id))
    }
}

fn check_read_or_write<SpiError>(offset: u32, length: usize) -> Result<(), Error<SpiError>> {
    if offset as usize + length > CAPACITY_IN_BYTES {
        return Err(Error::BadRange(NorFlashErrorKind::OutOfBounds));
    }
    Ok(())
}

fn check_erase<SpiError>(from: u32, to: u32) -> Result<(), Error<SpiError>> {
    if from > to || to as usize > CAPACITY_IN_BYTES {
        return Err(Error::BadRange(NorFlashErrorKind::OutOfBounds));
    }
    if !(from as usize).is_multiple_of(SECTOR_SIZE) || !(to as usize).is_multiple_of(SECTOR_SIZE) {
        return Err(Error::BadRange(NorFlashErrorKind::NotAligned));
    }
    Ok(())
}

/// One piece of a write that stays inside a single 256-byte program page.
struct ProgramChunk {
    address: u32,
    first_byte: usize,
    end_byte: usize,
}

fn program_chunks(offset: u32, length: usize) -> impl Iterator<Item = ProgramChunk> {
    let mut bytes_planned = 0;
    core::iter::from_fn(move || {
        if bytes_planned == length {
            return None;
        }
        let address = offset as usize + bytes_planned;
        let room_left_in_page = PROGRAM_PAGE_SIZE - address % PROGRAM_PAGE_SIZE;
        let chunk_length = room_left_in_page.min(length - bytes_planned);

        let chunk = ProgramChunk {
            address: address as u32,
            first_byte: bytes_planned,
            end_byte: bytes_planned + chunk_length,
        };
        bytes_planned += chunk_length;
        Some(chunk)
    })
}

fn sector_starts(from: u32, to: u32) -> impl Iterator<Item = u32> {
    (from..to).step_by(SECTOR_SIZE)
}

// ============================================================================
// Blocking external flash, for the bootloader
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

        check_chip_id(chip_id)?;
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

impl<FlashSpi: SpiDevice> ExternalFlash<FlashSpi> {
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
        let header = command_with_address(PAGE_PROGRAM, address);

        self.enable_writing()?;
        self.spi
            .transaction(&mut [Operation::Write(&header), Operation::Write(data)])
            .map_err(Error::Spi)?;
        self.wait_until_ready()
    }

    fn erase_sector(&mut self, address: u32) -> Result<(), Error<FlashSpi::Error>> {
        let command = command_with_address(SECTOR_ERASE, address);

        self.enable_writing()?;
        self.spi.write(&command).map_err(Error::Spi)?;
        self.wait_until_ready()
    }
}

impl<FlashSpi: SpiDevice> ErrorType for ExternalFlash<FlashSpi> {
    type Error = Error<FlashSpi::Error>;
}

impl<FlashSpi: SpiDevice> ReadNorFlash for ExternalFlash<FlashSpi> {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        check_read_or_write(offset, bytes.len())?;

        let header = command_with_address(READ_DATA, offset);
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
        check_erase(from, to)?;

        for sector_start in sector_starts(from, to) {
            self.erase_sector(sector_start)?;
        }
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        check_read_or_write(offset, bytes.len())?;

        for chunk in program_chunks(offset, bytes.len()) {
            self.program_page(chunk.address, &bytes[chunk.first_byte..chunk.end_byte])?;
        }
        Ok(())
    }
}

/// NOR flash can be written again without an erase, as long as bits only go from 1 to 0.
impl<FlashSpi: SpiDevice> MultiwriteNorFlash for ExternalFlash<FlashSpi> {}

// ============================================================================
// Async external flash, for the firmware
// ============================================================================

pub struct AsyncExternalFlash<FlashSpi, Delay> {
    spi: FlashSpi,
    delay: Delay,
}

pub struct AsyncFlashParts<FlashSpi, Delay> {
    pub spi: FlashSpi,
    pub delay: Delay,
}

impl<FlashSpi, Delay> AsyncExternalFlash<FlashSpi, Delay>
where
    FlashSpi: embedded_hal_async::spi::SpiDevice,
    Delay: embedded_hal_async::delay::DelayNs,
{
    /// Wakes the chip and checks that it answers as a 4 MB chip, so a second-source part works too.
    pub async fn new(spi: FlashSpi, delay: Delay) -> Result<Self, Error<FlashSpi::Error>> {
        let mut flash = AsyncExternalFlash { spi, delay };

        flash.wake().await?;
        let chip_id = flash.read_chip_id().await?;

        check_chip_id(chip_id)?;
        Ok(flash)
    }

    pub async fn read_chip_id(&mut self) -> Result<ChipId, Error<FlashSpi::Error>> {
        let mut id_bytes = [0; 3];
        self.spi
            .transaction(&mut [Operation::Write(&[READ_CHIP_ID]), Operation::Read(&mut id_bytes)])
            .await
            .map_err(Error::Spi)?;

        let [manufacturer, memory_type, capacity] = id_bytes;
        Ok(ChipId {
            manufacturer,
            memory_type,
            capacity,
        })
    }

    pub async fn sleep(&mut self) -> Result<(), Error<FlashSpi::Error>> {
        self.spi.write(&[DEEP_POWER_DOWN]).await.map_err(Error::Spi)
    }

    pub async fn wake(&mut self) -> Result<(), Error<FlashSpi::Error>> {
        self.spi.write(&[RELEASE_POWER_DOWN]).await.map_err(Error::Spi)?;
        self.delay.delay_us(WAKE_UP_TIME_IN_MICROSECONDS).await;
        Ok(())
    }

    pub fn release(self) -> AsyncFlashParts<FlashSpi, Delay> {
        AsyncFlashParts {
            spi: self.spi,
            delay: self.delay,
        }
    }

    async fn enable_writing(&mut self) -> Result<(), Error<FlashSpi::Error>> {
        self.spi.write(&[WRITE_ENABLE]).await.map_err(Error::Spi)
    }

    async fn wait_until_ready(&mut self) -> Result<(), Error<FlashSpi::Error>> {
        loop {
            let mut status = [0];
            self.spi
                .transaction(&mut [Operation::Write(&[READ_STATUS]), Operation::Read(&mut status)])
                .await
                .map_err(Error::Spi)?;

            if status[0] & WRITE_IN_PROGRESS_BIT == 0 {
                return Ok(());
            }
            self.delay.delay_us(BUSY_POLL_INTERVAL_IN_MICROSECONDS).await;
        }
    }

    async fn program_page(&mut self, address: u32, data: &[u8]) -> Result<(), Error<FlashSpi::Error>> {
        let header = command_with_address(PAGE_PROGRAM, address);

        self.enable_writing().await?;
        self.spi
            .transaction(&mut [Operation::Write(&header), Operation::Write(data)])
            .await
            .map_err(Error::Spi)?;
        self.wait_until_ready().await
    }

    async fn erase_sector(&mut self, address: u32) -> Result<(), Error<FlashSpi::Error>> {
        let command = command_with_address(SECTOR_ERASE, address);

        self.enable_writing().await?;
        self.spi.write(&command).await.map_err(Error::Spi)?;
        self.wait_until_ready().await
    }
}

impl<FlashSpi, Delay> ErrorType for AsyncExternalFlash<FlashSpi, Delay>
where
    FlashSpi: embedded_hal_async::spi::SpiDevice,
{
    type Error = Error<FlashSpi::Error>;
}

impl<FlashSpi, Delay> embedded_storage_async::nor_flash::ReadNorFlash for AsyncExternalFlash<FlashSpi, Delay>
where
    FlashSpi: embedded_hal_async::spi::SpiDevice,
    Delay: embedded_hal_async::delay::DelayNs,
{
    const READ_SIZE: usize = 1;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        check_read_or_write(offset, bytes.len())?;

        let header = command_with_address(READ_DATA, offset);
        self.spi
            .transaction(&mut [Operation::Write(&header), Operation::Read(bytes)])
            .await
            .map_err(Error::Spi)
    }

    fn capacity(&self) -> usize {
        CAPACITY_IN_BYTES
    }
}

impl<FlashSpi, Delay> embedded_storage_async::nor_flash::NorFlash for AsyncExternalFlash<FlashSpi, Delay>
where
    FlashSpi: embedded_hal_async::spi::SpiDevice,
    Delay: embedded_hal_async::delay::DelayNs,
{
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = SECTOR_SIZE;

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        check_erase(from, to)?;

        for sector_start in sector_starts(from, to) {
            self.erase_sector(sector_start).await?;
        }
        Ok(())
    }

    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        check_read_or_write(offset, bytes.len())?;

        for chunk in program_chunks(offset, bytes.len()) {
            self.program_page(chunk.address, &bytes[chunk.first_byte..chunk.end_byte])
                .await?;
        }
        Ok(())
    }
}

impl<FlashSpi, Delay> embedded_storage_async::nor_flash::MultiwriteNorFlash
    for AsyncExternalFlash<FlashSpi, Delay>
where
    FlashSpi: embedded_hal_async::spi::SpiDevice,
    Delay: embedded_hal_async::delay::DelayNs,
{
}

#[cfg(test)]
mod tests;
