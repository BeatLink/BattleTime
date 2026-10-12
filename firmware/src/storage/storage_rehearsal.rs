//! The storage rehearsal: the rehearsal under wasp-os, plus NeoTime's flash driver and littlefs on the real chip.
//!
//! Before Bluetooth starts it erases, writes and reads back part of the update slot, then formats,
//! fills and remounts the file system; an upload then goes into the real update slot through
//! embassy-boot and is checked by reading it back from the chip. It erases wasp-os's files on the
//! external flash. Internal flash is never written: the boot state page lives in RAM here, so the
//! image holds no code that writes the chip's own flash.

/// Shared with the plain rehearsal; its RAM upload target is not used here.
#[allow(dead_code)]
#[path = "rehearsal.rs"]
mod wasp_os;

use core::cell::RefCell;

use embassy_embedded_hal::{flash::partition::BlockingPartition, shared_bus::blocking::spi::SpiDevice};
use embassy_nrf::{
    gpio::{Level, Output, OutputDrive},
    peripherals::{NVMC, P0_05},
    spim::Spim,
    Peri,
};
use embassy_sync::blocking_mutex::{raw::NoopRawMutex, Mutex};
use embassy_time::Delay;
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use embedded_storage_async::nor_flash::{
    ErrorType as AsyncErrorType, NorFlash as AsyncNorFlash, NorFlashErrorKind,
    ReadNorFlash as AsyncReadNorFlash,
};
use neotime_boot::{FIRMWARE_UPDATE, PAGE_SIZE};
use neotime_external_flash::ExternalFlash;
use neotime_storage::{prepare_file_system, FileSystemFlash, Preparation};
use neotime_updates::{ImageKind, UpdateError, UpdateReceiver, UpdateTargets};
use static_cell::StaticCell;

use crate::{line, status::Mode, Screen, SpiBus};

pub use wasp_os::{
    leave_breadcrumb, prepare_clocks, previous_breadcrumbs, record, reset, Breadcrumbs, Confirmation,
    WATCHDOG_HALT,
};

// ============================================================================
// What this build is
// ============================================================================

pub const BUILD_NAME: &str = "storage rehearsal";
/// littlefs keeps its buffers on the heap.
pub const HEAP_SIZE: usize = 16 * 1024;

const FLASH_TEST_SIZE: u32 = 32 * 1024;
const CHUNK_SIZE: usize = 1024;
const STAGE_FLASH_FOUND: u32 = 0x21;
const STAGE_FLASH_WRITTEN: u32 = 0x22;
const STAGE_FILES_CHECKED: u32 = 0x23;
const STAGE_FLASH_FAILED: u32 = 0x2E;
const STAGE_FILES_FAILED: u32 = 0x2F;

const WATCHDOG_RELOAD_REGISTER_0: *mut u32 = 0x4001_0600 as *mut u32;
const WATCHDOG_RELOAD: u32 = 0x6E52_4635;

/// The storage stage runs before the watchdog task gets a turn, and one sector erase can take hundreds of milliseconds.
fn feed_watchdog() {
    unsafe { WATCHDOG_RELOAD_REGISTER_0.write_volatile(WATCHDOG_RELOAD) };
}

// ============================================================================
// Starting the storage
// ============================================================================

type FlashOnBus = ExternalFlash<SpiDevice<'static, NoopRawMutex, Spim<'static>, Output<'static>>>;
type SharedExternalFlash = Mutex<NoopRawMutex, RefCell<FlashOnBus>>;
type UpdateSlot =
    embassy_embedded_hal::adapter::BlockingAsync<BlockingPartition<'static, NoopRawMutex, FlashOnBus>>;

static EXTERNAL_FLASH: StaticCell<SharedExternalFlash> = StaticCell::new();
static STATE_ALIGNMENT: StaticCell<[u8; 4]> = StaticCell::new();
static BOOT_STATE_IN_RAM: StaticCell<[u8; PAGE_SIZE as usize]> = StaticCell::new();

pub struct StoragePeripherals {
    pub flash_chip_select: Peri<'static, P0_05>,
    pub flash_controller: Peri<'static, NVMC>,
}

pub async fn start(
    spi_bus: &'static SpiBus,
    peripherals: StoragePeripherals,
    screen: &mut Screen,
) -> Option<(Targets, Confirmation, Mode, &'static Files)> {
    let _flash_controller_never_used = peripherals.flash_controller;
    let flash_chip_select = Output::new(peripherals.flash_chip_select, Level::High, OutputDrive::Standard);
    let mut flash = match ExternalFlash::new(SpiDevice::new(spi_bus, flash_chip_select), &mut Delay) {
        Ok(flash) => flash,
        Err(_) => {
            screen.write_line(line::FILES, "no flash chip");
            leave_breadcrumb(STAGE_FLASH_FAILED);
            return None;
        }
    };
    let chip_id = flash.read_chip_id().ok()?;
    let mut text = heapless::String::<24>::new();
    let _ = core::fmt::write(
        &mut text,
        format_args!(
            "flash {:02x}{:02x}{:02x}",
            chip_id.manufacturer, chip_id.memory_type, chip_id.capacity
        ),
    );
    screen.write_line(line::FILES, &text);
    leave_breadcrumb(STAGE_FLASH_FOUND);

    match exercise_the_flash(&mut flash) {
        Ok(()) => {
            screen.write_line(line::FILES, "flash write ok");
            leave_breadcrumb(STAGE_FLASH_WRITTEN);
        }
        Err(offset) => {
            let mut text = heapless::String::<24>::new();
            let _ = core::fmt::write(&mut text, format_args!("flash bad {:x}", offset));
            screen.write_line(line::FILES, &text);
            record(Breadcrumbs {
                stage: STAGE_FLASH_FAILED,
                detail: offset,
                fault_status: 0,
            });
        }
    }

    let external_flash = EXTERNAL_FLASH.init(Mutex::new(RefCell::new(flash)));
    match exercise_the_file_system(external_flash) {
        Ok(()) => {
            screen.write_line(line::FILES, "flash, files ok");
            leave_breadcrumb(STAGE_FILES_CHECKED);
        }
        Err(step) => {
            screen.write_line(line::FILES, step);
            leave_breadcrumb(STAGE_FILES_FAILED);
        }
    }

    let boot_state = RamBootState {
        page: BOOT_STATE_IN_RAM.init([0xFF; PAGE_SIZE as usize]),
    };
    let targets = Targets {
        firmware: UpdateReceiver::new(
            embassy_embedded_hal::adapter::BlockingAsync::new(BlockingPartition::new(
                external_flash,
                FIRMWARE_UPDATE.start,
                FIRMWARE_UPDATE.size,
            )),
            boot_state,
            STATE_ALIGNMENT.init([0; 4]),
        ),
    };
    Some((targets, Confirmation, Mode::Confirmed, &Files))
}

// ============================================================================
// The raw flash test
// ============================================================================

fn test_byte(offset: u32) -> u8 {
    (offset.wrapping_mul(31) ^ (offset >> 8) ^ 0x5A) as u8
}

/// Erases, writes and reads back the start of the update slot; returns the first offset that reads back wrong.
fn exercise_the_flash(flash: &mut FlashOnBus) -> Result<(), u32> {
    let start = FIRMWARE_UPDATE.start;
    let end = start + FLASH_TEST_SIZE;
    for sector in (start..end).step_by(PAGE_SIZE as usize) {
        feed_watchdog();
        flash.erase(sector, sector + PAGE_SIZE).map_err(|_| sector)?;
    }

    let mut chunk = [0u8; CHUNK_SIZE];
    for chunk_start in (start..end).step_by(CHUNK_SIZE) {
        for (index, byte) in chunk.iter_mut().enumerate() {
            *byte = test_byte(chunk_start + index as u32);
        }
        feed_watchdog();
        // A short write first, then the rest across a program page boundary, as real updates do.
        flash.write(chunk_start, &chunk[..37]).map_err(|_| chunk_start)?;
        flash
            .write(chunk_start + 37, &chunk[37..])
            .map_err(|_| chunk_start)?;
    }

    for chunk_start in (start..end).step_by(CHUNK_SIZE) {
        feed_watchdog();
        flash.read(chunk_start, &mut chunk).map_err(|_| chunk_start)?;
        if let Some(index) =
            (0..CHUNK_SIZE).find(|&index| chunk[index] != test_byte(chunk_start + index as u32))
        {
            return Err(chunk_start + index as u32);
        }
    }

    let mut erased = [0u8; CHUNK_SIZE];
    feed_watchdog();
    flash.erase(start, start + PAGE_SIZE).map_err(|_| start)?;
    flash.read(start, &mut erased).map_err(|_| start)?;
    if erased.iter().any(|byte| *byte != 0xFF) {
        return Err(start);
    }
    Ok(())
}

// ============================================================================
// The file system test
// ============================================================================

const TEST_FILE: &str = "/settings/storage-rehearsal";
const TEST_FILE_SIZE: usize = 3000;

fn exercise_the_file_system(external_flash: &'static SharedExternalFlash) -> Result<(), &'static str> {
    let whole_chip = || {
        FileSystemFlash::new(BlockingPartition::new(
            external_flash,
            0,
            neotime_external_flash::CAPACITY_IN_BYTES as u32,
        ))
    };
    let mut contents = [0u8; TEST_FILE_SIZE];
    for (index, byte) in contents.iter_mut().enumerate() {
        *byte = test_byte(index as u32 * 7);
    }

    feed_watchdog();
    let first = prepare_file_system(whole_chip()).map_err(|_| "files: no mount")?;
    feed_watchdog();
    first
        .file_system
        .write_file(TEST_FILE, &contents)
        .map_err(|_| "files: no write")?;
    first.file_system.unmount().map_err(|_| "files: no unmount")?;

    feed_watchdog();
    let second = prepare_file_system(whole_chip()).map_err(|_| "files: no remount")?;
    if second.preparation != Preparation::AlreadyFormatted {
        return Err("files: lost format");
    }
    let read_back = second
        .file_system
        .read_to_vec(TEST_FILE)
        .map_err(|_| "files: no read")?;
    if read_back[..] != contents[..] {
        return Err("files: bad read");
    }
    second.file_system.unmount().map_err(|_| "files: no unmount")?;
    Ok(())
}

// ============================================================================
// An upload into the real update slot, with the boot state kept in RAM
// ============================================================================

/// A stand-in for the boot state page, so marking a swap changes nothing on the chip.
pub struct RamBootState {
    page: &'static mut [u8; PAGE_SIZE as usize],
}

impl AsyncErrorType for RamBootState {
    type Error = NorFlashErrorKind;
}

impl AsyncReadNorFlash for RamBootState {
    const READ_SIZE: usize = 1;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        let start = offset as usize;
        bytes.copy_from_slice(&self.page[start..start + bytes.len()]);
        Ok(())
    }

    fn capacity(&self) -> usize {
        self.page.len()
    }
}

impl AsyncNorFlash for RamBootState {
    const WRITE_SIZE: usize = 4;
    const ERASE_SIZE: usize = PAGE_SIZE as usize;

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        self.page[from as usize..to as usize].fill(0xFF);
        Ok(())
    }

    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        let start = offset as usize;
        self.page[start..start + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }
}

pub struct Targets {
    firmware: UpdateReceiver<'static, UpdateSlot, RamBootState>,
}

impl UpdateTargets for Targets {
    async fn begin(&mut self, kind: ImageKind, image_size_in_bytes: u32) -> Result<(), UpdateError> {
        match kind {
            ImageKind::Firmware => self.firmware.begin(image_size_in_bytes),
            ImageKind::Recovery => Err(UpdateError::NotStarted),
        }
    }

    async fn receive(&mut self, _kind: ImageKind, chunk: &[u8]) -> Result<(), UpdateError> {
        self.firmware.receive(chunk).await
    }

    async fn read_back(
        &mut self,
        _kind: ImageKind,
        offset: u32,
        bytes: &mut [u8],
    ) -> Result<(), UpdateError> {
        self.firmware.read_back(offset, bytes).await
    }

    /// Checks the image and marks a swap, in the RAM boot state, so nothing is installed.
    async fn finish(&mut self, _kind: ImageKind) -> Result<(), UpdateError> {
        self.firmware.finish().await
    }

    fn abort(&mut self, _kind: ImageKind) {
        self.firmware.abort();
    }
}

// ============================================================================
// Files for saved app state, which the rehearsal does not keep
// ============================================================================

/// The rehearsal has no shell, so it keeps no files for it.
pub struct Files;

/// Stands in for the watch's file system type; the rehearsal never has one.
pub type WatchFileSystem = littlefs_rust::Filesystem<littlefs_rust::RamStorage>;

impl Files {
    /// Always nothing, so the file service answers every command with an error.
    pub fn file_system(&self) -> Option<&WatchFileSystem> {
        None
    }

    pub fn read(&self, _path: &str) -> Option<alloc::vec::Vec<u8>> {
        None
    }

    pub fn write(&self, _path: &str, _bytes: &[u8]) -> bool {
        false
    }
}
