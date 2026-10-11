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
    is_firmware_vector_table, BOOT_STATE, FIRMWARE, FIRMWARE_UPDATE, PREVIOUS_FIRMWARE, RECOVERY_FIRMWARE,
    VECTOR_TABLE_CHECK_SIZE,
};
use neotime_external_flash::ExternalFlash;
use neotime_storage::{prepare_file_system, FileSystemFlash, Preparation};
use neotime_updates::{
    confirm_running_firmware, running_firmware_is_on_trial, Receivers, RecoveryReceiver, UpdateReceiver,
};
use static_cell::StaticCell;

use core::cell::RefCell;

use crate::{Screen, SpiBus};

// ============================================================================
// What this build is
// ============================================================================

pub const BUILD_NAME: &str = "";
pub const WATCHDOG_HALT: HaltConfig = HaltConfig::Pause;
pub const RESET_AFTER_ACTIVATE: bool = true;

pub fn prepare_clocks() {}

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

pub struct StoragePeripherals {
    pub flash_chip_select: Peri<'static, P0_05>,
    pub flash_controller: Peri<'static, NVMC>,
}

pub async fn start(
    spi_bus: &'static SpiBus,
    peripherals: StoragePeripherals,
    screen: &mut Screen,
) -> Option<(Targets, Confirmation)> {
    let flash_chip_select = Output::new(peripherals.flash_chip_select, Level::High, OutputDrive::Standard);
    let external_flash = match ExternalFlash::new(SpiDevice::new(spi_bus, flash_chip_select), &mut Delay) {
        Ok(flash) => EXTERNAL_FLASH.init(Mutex::new(RefCell::new(flash))),
        Err(_) => {
            screen.write_line(2, "no flash chip");
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
    screen.write_line(
        2,
        match prepare_file_system(file_system_flash) {
            Ok(prepared) if prepared.preparation == Preparation::AlreadyFormatted => "files ok",
            Ok(_) => "files formatted",
            Err(_) => "files broken",
        },
    );

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
    if confirmation.is_on_trial().await && !rolling_back_would_help(external_flash) {
        screen.write_line(3, confirmation.confirm().await);
    }
    Some((targets, confirmation))
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
    async fn is_on_trial(&mut self) -> bool {
        running_firmware_is_on_trial(&mut self.boot_state, self.alignment).await == Ok(true)
    }

    pub async fn confirm(&mut self) -> &'static str {
        match confirm_running_firmware(&mut self.boot_state, self.alignment).await {
            Ok(()) => "confirmed",
            Err(_) => "confirm failed",
        }
    }
}
