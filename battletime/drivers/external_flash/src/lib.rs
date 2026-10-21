//! Driver for the PineTime's 4 MB SPI NOR flash, an XT25F32B, as an `embedded-storage` `NorFlash`.
//!
//! The chip shares its SPI bus with the screen. Writes are split at the chip's 256-byte program
//! pages, and the chip is woken from deep power-down on start, since the firmware may have left it
//! asleep before a reset. `ExternalFlash` remembers putting the chip to sleep and wakes it before its
//! next command, and blocks and waits with a blocking delay while the chip is busy;
//! `AsyncExternalFlash` waits with an async delay, so the firmware's other tasks keep running.

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
const SUSPEND: u8 = 0x75;
const RESUME: u8 = 0x7A;
const READ_STATUS_2: u8 = 0x35;
const RELEASE_POWER_DOWN: u8 = 0xAB;

const WRITE_IN_PROGRESS_BIT: u8 = 0b0000_0001;
/// Status register 2's bit saying an erase or program is paused.
pub const SUSPENDED_BIT: u8 = 0b1000_0000;
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
// Reads a DMA engine moves itself
// ============================================================================

/// The most bytes one EasyDMA transfer moves on the nRF52832.
const LARGEST_DMA_PIECE: usize = 255;

/// The command that reads from `address`, for a driver that sends it itself.
pub fn read_command(address: u32) -> [u8; 4] {
    command_with_address(READ_DATA, address)
}

/// How many of `bytes_left` the next DMA piece of a read takes.
///
/// It never leaves one byte behind, since the nRF52832 clocks out an extra byte when a transfer
/// reads only one, and the chip would then skip a byte.
pub fn next_read_piece(bytes_left: usize) -> usize {
    match bytes_left {
        0..=LARGEST_DMA_PIECE => bytes_left,
        one_piece_and_a_byte if one_piece_and_a_byte == LARGEST_DMA_PIECE + 1 => LARGEST_DMA_PIECE - 1,
        _ => LARGEST_DMA_PIECE,
    }
}

// ============================================================================
// Busy wait
// ============================================================================

/// A blocking delay that only spins the CPU, so it reads no timer and needs no peripheral.
#[cfg(feature = "cortex-m")]
pub struct BusyWait {
    pub cpu_frequency_in_hertz: u32,
}

#[cfg(feature = "cortex-m")]
impl DelayNs for BusyWait {
    fn delay_ns(&mut self, nanoseconds: u32) {
        let cycles = u64::from(nanoseconds) * u64::from(self.cpu_frequency_in_hertz) / 1_000_000_000;
        cortex_m::asm::delay(cycles as u32);
    }
}

// ============================================================================
// Blocking external flash
// ============================================================================

pub struct ExternalFlash<FlashSpi, Delay> {
    spi: FlashSpi,
    delay: Delay,
    /// An erase or page program started without waiting may still be running in the chip.
    operation_may_be_running: bool,
    /// The chip is in deep power-down, where it ignores every command but the one that wakes it.
    asleep: bool,
}

pub struct FlashParts<FlashSpi, Delay> {
    pub spi: FlashSpi,
    pub delay: Delay,
}

impl<FlashSpi: SpiDevice, Delay: DelayNs> ExternalFlash<FlashSpi, Delay> {
    /// Wakes the chip and checks that it answers as a 4 MB chip, so a second-source part works too.
    pub fn new(spi: FlashSpi, delay: Delay) -> Result<Self, Error<FlashSpi::Error>> {
        let mut flash = ExternalFlash {
            spi,
            delay,
            operation_may_be_running: false,
            asleep: true,
        };

        flash.wake()?;
        let chip_id = flash.read_chip_id()?;

        check_chip_id(chip_id)?;
        Ok(flash)
    }

    pub fn read_chip_id(&mut self) -> Result<ChipId, Error<FlashSpi::Error>> {
        self.wake_if_asleep()?;
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

    /// Puts the chip in deep power-down once any running erase or program ends; the next command wakes it.
    pub fn sleep(&mut self) -> Result<(), Error<FlashSpi::Error>> {
        if self.asleep {
            return Ok(());
        }
        self.finish_running_operation()?;
        self.spi.write(&[DEEP_POWER_DOWN]).map_err(Error::Spi)?;
        self.asleep = true;
        Ok(())
    }

    pub fn wake(&mut self) -> Result<(), Error<FlashSpi::Error>> {
        self.spi.write(&[RELEASE_POWER_DOWN]).map_err(Error::Spi)?;
        self.delay.delay_us(WAKE_UP_TIME_IN_MICROSECONDS);
        self.asleep = false;
        Ok(())
    }

    /// Wakes the chip if `sleep` put it down, for anything about to reach it, even past this driver.
    pub fn wake_if_asleep(&mut self) -> Result<(), Error<FlashSpi::Error>> {
        if self.asleep {
            self.wake()?;
        }
        Ok(())
    }

    pub fn is_asleep(&self) -> bool {
        self.asleep
    }

    pub fn release(self) -> FlashParts<FlashSpi, Delay> {
        FlashParts {
            spi: self.spi,
            delay: self.delay,
        }
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
            self.delay.delay_us(BUSY_POLL_INTERVAL_IN_MICROSECONDS);
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

    /// Starts erasing the sector at `address` and returns at once, while the chip works on.
    ///
    /// Until `erase_has_finished` says so, any other call first waits for the erase to end.
    pub fn start_erasing_sector(&mut self, address: u32) -> Result<(), Error<FlashSpi::Error>> {
        check_erase(address, address + SECTOR_SIZE as u32)?;
        self.finish_running_operation()?;

        self.enable_writing()?;
        self.spi
            .write(&command_with_address(SECTOR_ERASE, address))
            .map_err(Error::Spi)?;
        self.operation_may_be_running = true;
        Ok(())
    }

    /// Starts programming bytes within one 256-byte page and returns at once, while the chip works on.
    ///
    /// Until `operation_has_finished` says so, any other call first waits for the program to end.
    pub fn start_programming_page(
        &mut self,
        address: u32,
        data: &[u8],
    ) -> Result<(), Error<FlashSpi::Error>> {
        check_read_or_write(address, data.len())?;
        if address as usize % PROGRAM_PAGE_SIZE + data.len() > PROGRAM_PAGE_SIZE {
            return Err(Error::BadRange(NorFlashErrorKind::NotAligned));
        }
        self.finish_running_operation()?;
        if data.is_empty() {
            return Ok(());
        }

        self.enable_writing()?;
        let header = command_with_address(PAGE_PROGRAM, address);
        self.spi
            .transaction(&mut [Operation::Write(&header), Operation::Write(data)])
            .map_err(Error::Spi)?;
        self.operation_may_be_running = true;
        Ok(())
    }

    /// Asks the chip to pause the erase or program under way, so it reads again; `resume_operation` carries on.
    ///
    /// The XT25F32B is idle about 94 µs later, with `SUSPENDED_BIT` set in status register 2.
    pub fn suspend_operation(&mut self) -> Result<(), Error<FlashSpi::Error>> {
        self.spi.write(&[SUSPEND]).map_err(Error::Spi)
    }

    /// Carries on the erase or program `suspend_operation` paused.
    pub fn resume_operation(&mut self) -> Result<(), Error<FlashSpi::Error>> {
        self.spi.write(&[RESUME]).map_err(Error::Spi)?;
        // A read while paused saw the chip idle, so the driver must wait for the erase again.
        self.operation_may_be_running = true;
        Ok(())
    }

    /// Both status registers: the first holds the busy bit, the second the suspended bit, its top one.
    pub fn read_status_registers(&mut self) -> Result<[u8; 2], Error<FlashSpi::Error>> {
        let mut first = [0];
        let mut second = [0];
        self.spi
            .transaction(&mut [Operation::Write(&[READ_STATUS]), Operation::Read(&mut first)])
            .map_err(Error::Spi)?;
        self.spi
            .transaction(&mut [Operation::Write(&[READ_STATUS_2]), Operation::Read(&mut second)])
            .map_err(Error::Spi)?;
        Ok([first[0], second[0]])
    }

    /// Reads the chip's status once, and says whether the erase `start_erasing_sector` began is over.
    pub fn erase_has_finished(&mut self) -> Result<bool, Error<FlashSpi::Error>> {
        self.operation_has_finished()
    }

    /// Reads the chip's status once, and says whether the erase or program started without waiting is over.
    pub fn operation_has_finished(&mut self) -> Result<bool, Error<FlashSpi::Error>> {
        if self.operation_may_be_running && self.is_busy()? {
            return Ok(false);
        }
        self.operation_may_be_running = false;
        Ok(true)
    }

    /// Wakes the chip and waits for an erase or program started without waiting, since the chip ignores other commands meanwhile.
    fn finish_running_operation(&mut self) -> Result<(), Error<FlashSpi::Error>> {
        self.wake_if_asleep()?;
        if self.operation_may_be_running {
            self.wait_until_ready()?;
            self.operation_may_be_running = false;
        }
        Ok(())
    }

    /// Checks a read's range and waits for an erase or program still running, for a caller that sends the read itself.
    pub fn prepare_read(&mut self, offset: u32, length: usize) -> Result<(), Error<FlashSpi::Error>> {
        check_read_or_write(offset, length)?;
        self.finish_running_operation()
    }

    fn is_busy(&mut self) -> Result<bool, Error<FlashSpi::Error>> {
        let mut status = [0];
        self.spi
            .transaction(&mut [Operation::Write(&[READ_STATUS]), Operation::Read(&mut status)])
            .map_err(Error::Spi)?;
        Ok(status[0] & WRITE_IN_PROGRESS_BIT != 0)
    }
}

impl<FlashSpi: SpiDevice, Delay: DelayNs> ErrorType for ExternalFlash<FlashSpi, Delay> {
    type Error = Error<FlashSpi::Error>;
}

impl<FlashSpi: SpiDevice, Delay: DelayNs> ReadNorFlash for ExternalFlash<FlashSpi, Delay> {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        self.prepare_read(offset, bytes.len())?;

        let header = command_with_address(READ_DATA, offset);
        self.spi
            .transaction(&mut [Operation::Write(&header), Operation::Read(bytes)])
            .map_err(Error::Spi)
    }

    fn capacity(&self) -> usize {
        CAPACITY_IN_BYTES
    }
}

impl<FlashSpi: SpiDevice, Delay: DelayNs> NorFlash for ExternalFlash<FlashSpi, Delay> {
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = SECTOR_SIZE;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        check_erase(from, to)?;
        self.finish_running_operation()?;

        for sector_start in sector_starts(from, to) {
            self.erase_sector(sector_start)?;
        }
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        check_read_or_write(offset, bytes.len())?;
        self.finish_running_operation()?;

        for chunk in program_chunks(offset, bytes.len()) {
            self.program_page(chunk.address, &bytes[chunk.first_byte..chunk.end_byte])?;
        }
        Ok(())
    }
}

/// NOR flash can be written again without an erase, as long as bits only go from 1 to 0.
impl<FlashSpi: SpiDevice, Delay: DelayNs> MultiwriteNorFlash for ExternalFlash<FlashSpi, Delay> {}

// ============================================================================
// Async external flash, for the firmware
// ============================================================================

pub struct AsyncExternalFlash<FlashSpi, Delay> {
    spi: FlashSpi,
    delay: Delay,
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

    pub fn release(self) -> FlashParts<FlashSpi, Delay> {
        FlashParts {
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
