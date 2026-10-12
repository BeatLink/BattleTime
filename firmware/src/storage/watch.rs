//! Storage on a watch running NeoTime's own bootloader: the file system, the update slots and the boot state.

use embassy_embedded_hal::{
    adapter::BlockingAsync, flash::partition::BlockingPartition, shared_bus::blocking::spi::SpiDevice,
};
use embassy_nrf::{
    gpio::{Level, Output, OutputDrive},
    nvmc::Nvmc,
    peripherals::{NVMC, P0_05},
    spim::Spim,
    wdt::HaltConfig,
    Peri,
};
use embassy_sync::blocking_mutex::{raw::NoopRawMutex, Mutex};
use embassy_time::Delay;
use embedded_storage::nor_flash::ReadNorFlash;
use neotime_boot::{
    is_firmware_vector_table, UpdaterNote, BOOT_STATE, FIRMWARE, FIRMWARE_UPDATE, PREVIOUS_FIRMWARE,
    RECOVERY_FIRMWARE, VECTOR_TABLE_CHECK_SIZE,
};
use neotime_external_flash::ExternalFlash;
use neotime_storage::{prepare_file_system, FileSystem, FileSystemFlash, Preparation};
use neotime_updates::{
    confirm_running_firmware, forget_rollback, running_firmware_is_on_trial,
    running_firmware_was_rolled_back, Receivers, RecoveryReceiver, UpdateReceiver,
};
use static_cell::StaticCell;

use core::cell::RefCell;

use crate::{
    line,
    status::{self, Mode},
    Screen, SpiBus,
};

// ============================================================================
// What this build is
// ============================================================================

pub const BUILD_NAME: &str = "";
pub const WATCHDOG_HALT: HaltConfig = HaltConfig::Pause;
/// The bootloader takes over at the next start, swapping in an update if one was marked.
pub async fn reset(_screen: &mut Screen) -> ! {
    embassy_time::Timer::after(embassy_time::Duration::from_millis(200)).await;
    cortex_m::peripheral::SCB::sys_reset()
}

pub const HEAP_SIZE: usize = 16 * 1024;

pub fn prepare_clocks() {}

// ============================================================================
// Breadcrumbs are only kept in the rehearsal
// ============================================================================

#[derive(Clone, Copy)]
pub struct Breadcrumbs {
    pub stage: u32,
    pub detail: u32,
    pub fault_status: u32,
}

pub fn previous_breadcrumbs() -> Option<Breadcrumbs> {
    None
}

pub fn leave_breadcrumb(_stage: u32) {}

pub fn record(_breadcrumbs: Breadcrumbs) {}

// ============================================================================
// Starting the storage
// ============================================================================

type FlashOnBus = ExternalFlash<SpiDevice<'static, NoopRawMutex, Spim<'static>, Output<'static>>>;
type SharedExternalFlash = Mutex<NoopRawMutex, RefCell<FlashOnBus>>;
type SharedInternalFlash = Mutex<NoopRawMutex, RefCell<Nvmc<'static>>>;
type ExternalSlot = BlockingAsync<BlockingPartition<'static, NoopRawMutex, FlashOnBus>>;
type BootStatePage = BlockingAsync<BlockingPartition<'static, NoopRawMutex, Nvmc<'static>>>;

pub type Targets = Receivers<'static, ExternalSlot, BootStatePage, ExternalSlot>;

static EXTERNAL_FLASH: StaticCell<SharedExternalFlash> = StaticCell::new();
static INTERNAL_FLASH: StaticCell<SharedInternalFlash> = StaticCell::new();
static STATE_ALIGNMENT: StaticCell<[u8; 4]> = StaticCell::new();
static CONFIRM_ALIGNMENT: StaticCell<[u8; 4]> = StaticCell::new();
static FILES: StaticCell<Files> = StaticCell::new();

pub struct StoragePeripherals {
    pub flash_chip_select: Peri<'static, P0_05>,
    pub flash_controller: Peri<'static, NVMC>,
}

pub async fn start(
    spi_bus: &'static SpiBus,
    peripherals: StoragePeripherals,
    screen: &mut Screen,
) -> Option<(Targets, Confirmation, Mode, &'static Files)> {
    let flash_chip_select = Output::new(peripherals.flash_chip_select, Level::High, OutputDrive::Standard);
    let external_flash = match ExternalFlash::new(SpiDevice::new(spi_bus, flash_chip_select), &mut Delay) {
        Ok(flash) => EXTERNAL_FLASH.init(Mutex::new(RefCell::new(flash))),
        Err(_) => {
            screen.write_line(line::FILES, "No flash chip found");
            return None;
        }
    };
    let internal_flash =
        INTERNAL_FLASH.init(Mutex::new(RefCell::new(Nvmc::new(peripherals.flash_controller))));

    let file_system_flash = FileSystemFlash::new(BlockingPartition::new(
        external_flash,
        0,
        neotime_external_flash::CAPACITY_IN_BYTES as u32,
    ));
    let prepared = prepare_file_system(file_system_flash);
    screen.write_line(
        line::FILES,
        match &prepared {
            Ok(prepared) if prepared.preparation == Preparation::AlreadyFormatted => "Files OK",
            Ok(_) => "Files set up",
            Err(_) => "Files damaged",
        },
    );
    let files: &'static Files = FILES.init(Files {
        file_system: prepared.ok().map(|prepared| prepared.file_system),
    });

    let targets = Receivers {
        firmware: UpdateReceiver::new(
            BlockingAsync::new(BlockingPartition::new(
                external_flash,
                FIRMWARE_UPDATE.start,
                FIRMWARE_UPDATE.size,
            )),
            BlockingAsync::new(BlockingPartition::new(
                internal_flash,
                BOOT_STATE.start,
                BOOT_STATE.size,
            )),
            STATE_ALIGNMENT.init([0; 4]),
        ),
        recovery: RecoveryReceiver::new(BlockingAsync::new(BlockingPartition::new(
            external_flash,
            RECOVERY_FIRMWARE.start,
            RECOVERY_FIRMWARE.size,
        ))),
    };
    let mut confirmation = Confirmation {
        boot_state: BlockingAsync::new(BlockingPartition::new(
            internal_flash,
            BOOT_STATE.start,
            BOOT_STATE.size,
        )),
        alignment: CONFIRM_ALIGNMENT.init([0; 4]),
    };
    let mut mode = confirmation.mode().await;
    if mode == Mode::OnTrial && !rolling_back_would_help(external_flash) && confirmation.confirm().await {
        mode = Mode::Confirmed;
    }
    if mode == Mode::RolledBack {
        match status::take_updater_note() {
            Some(UpdaterNote::BootloaderWritten) if confirmation.forget_rollback().await => {
                mode = Mode::BootloaderUpdated;
            }
            Some(UpdaterNote::BootloaderAlreadyCurrent) if confirmation.forget_rollback().await => {
                mode = Mode::BootloaderAlreadyCurrent;
            }
            _ => {}
        }
    }
    if status::recovery_was_just_installed() || status::IS_RECOVERY_BUILD {
        mode = Mode::Recovery;
    }
    Some((targets, confirmation, mode, files))
}

/// A rollback restores the update slot, which after a migration holds the old system's leftovers and after
/// a recovery holds the broken firmware; in both cases this firmware must stay.
fn rolling_back_would_help(external_flash: &'static SharedExternalFlash) -> bool {
    let mut previous_firmware =
        BlockingPartition::new(external_flash, PREVIOUS_FIRMWARE.start, PREVIOUS_FIRMWARE.size);
    let mut vector_table_start = [0; VECTOR_TABLE_CHECK_SIZE];
    let previous_is_firmware = previous_firmware.read(0, &mut vector_table_start).is_ok()
        && is_firmware_vector_table(vector_table_start);
    if !previous_is_firmware {
        return false;
    }

    let mut recovery =
        BlockingPartition::new(external_flash, RECOVERY_FIRMWARE.start, RECOVERY_FIRMWARE.size);
    !running_firmware_is(&mut recovery)
}

/// Compares the running firmware with an image in the external flash, 256 bytes at a time.
fn running_firmware_is(image: &mut impl ReadNorFlash) -> bool {
    const CHUNK_SIZE: usize = 256;
    let running = unsafe { core::slice::from_raw_parts(FIRMWARE.start as *const u8, FIRMWARE.size as usize) };
    let mut chunk = [0; CHUNK_SIZE];
    running
        .chunks(CHUNK_SIZE)
        .enumerate()
        .all(|(index, running_chunk)| {
            image.read((index * CHUNK_SIZE) as u32, &mut chunk).is_ok() && chunk[..] == *running_chunk
        })
}

// ============================================================================
// Telling the bootloader this firmware works
// ============================================================================

pub struct Confirmation {
    boot_state: BootStatePage,
    alignment: &'static mut [u8; 4],
}

impl Confirmation {
    /// What the boot state says: on trial after a swap, rolled back after a revert, confirmed otherwise.
    async fn mode(&mut self) -> Mode {
        if running_firmware_is_on_trial(&mut self.boot_state, self.alignment).await == Ok(true) {
            Mode::OnTrial
        } else if running_firmware_was_rolled_back(&mut self.boot_state, self.alignment).await == Ok(true) {
            Mode::RolledBack
        } else {
            Mode::Confirmed
        }
    }

    /// True once the rollback's mark is gone, so the page no longer says the update failed.
    async fn forget_rollback(&mut self) -> bool {
        forget_rollback(&mut self.boot_state, self.alignment)
            .await
            .is_ok()
    }

    /// True once the bootloader will keep this firmware.
    pub async fn confirm(&mut self) -> bool {
        confirm_running_firmware(&mut self.boot_state, self.alignment)
            .await
            .is_ok()
    }
}

// ============================================================================
// Files for saved app state
// ============================================================================

/// The littlefs file system in the external flash.
pub type WatchFileSystem = FileSystem<BlockingPartition<'static, NoopRawMutex, FlashOnBus>>;

/// The mounted file system, or nothing when it would not mount; the shell and the Bluetooth task share it.
pub struct Files {
    file_system: Option<WatchFileSystem>,
}

impl Files {
    pub fn file_system(&self) -> Option<&WatchFileSystem> {
        self.file_system.as_ref()
    }

    pub fn read(&self, path: &str) -> Option<alloc::vec::Vec<u8>> {
        self.file_system.as_ref()?.read_to_vec(path).ok()
    }

    /// Writes the whole file, making its folder first; false when the file system is missing or full.
    pub fn write(&self, path: &str, bytes: &[u8]) -> bool {
        let Some(file_system) = &self.file_system else {
            return false;
        };
        if let Some((folder, _)) = path.rsplit_once('/') {
            if !folder.is_empty() && !file_system.exists(folder) {
                let _ = file_system.mkdir(folder);
            }
        }
        file_system.write_file(path, bytes).is_ok()
    }
}
