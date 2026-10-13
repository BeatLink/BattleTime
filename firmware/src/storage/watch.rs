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

use embedded_storage::nor_flash::ReadNorFlash;
use neotime_boot::{
    bootloader_identity, is_firmware_vector_table, UpdaterNote, BOOT_STATE, FIRMWARE, FIRMWARE_UPDATE,
    PREVIOUS_FIRMWARE, RECOVERY_FIRMWARE, VECTOR_TABLE_CHECK_SIZE,
};
use neotime_external_flash::{BusyWait, ExternalFlash};
use neotime_packages::commands::{refuse_command, Change};
use neotime_storage::{
    block_address, next_block_to_erase_ahead, prepare_file_system, ErasedBlocks, FileSystem, FileSystemFlash,
    Preparation,
};
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

/// Waits between busy polls of the flash chip by spinning the 64 MHz CPU.
const BUSY_WAIT: BusyWait = BusyWait {
    cpu_frequency_in_hertz: 64_000_000,
};

type FlashOnBus = ExternalFlash<SpiDevice<'static, NoopRawMutex, Spim<'static>, Output<'static>>, BusyWait>;
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
static ERASED_AHEAD: ErasedBlocks = ErasedBlocks::new();

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
    let external_flash = match ExternalFlash::new(SpiDevice::new(spi_bus, flash_chip_select), BUSY_WAIT) {
        Ok(flash) => EXTERNAL_FLASH.init(Mutex::new(RefCell::new(flash))),
        Err(_) => {
            screen.write_line(line::FILES, "No chip");
            return None;
        }
    };
    let internal_flash =
        INTERNAL_FLASH.init(Mutex::new(RefCell::new(Nvmc::new(peripherals.flash_controller))));

    let file_system_flash = FileSystemFlash::with_blocks_erased_ahead(
        BlockingPartition::new(
            external_flash,
            0,
            neotime_external_flash::CAPACITY_IN_BYTES as u32,
        ),
        &ERASED_AHEAD,
    );
    let prepared = prepare_file_system(file_system_flash);
    screen.write_line(
        line::FILES,
        match &prepared {
            Ok(prepared) if prepared.preparation == Preparation::AlreadyFormatted => "OK",
            Ok(_) => "Formatted",
            Err(_) => "Damaged",
        },
    );
    let files: &'static Files = FILES.init(Files {
        file_system: prepared.ok().map(|prepared| prepared.file_system),
        internal_flash,
        external_flash,
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
    let identity = bootloader_identity(&mut status::BootloaderPages).map_err(|_| ());
    screen.write_line(line::BOOTLOADER, &status::bootloader_words(identity, mode));
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
    /// For copying the shared library into its region of the internal flash.
    #[cfg_attr(not(with_shell), allow(dead_code))]
    internal_flash: &'static SharedInternalFlash,
    /// For erasing free blocks ahead of littlefs, outside it.
    #[cfg_attr(not(with_shell), allow(dead_code))]
    external_flash: &'static SharedExternalFlash,
}

impl Files {
    pub fn file_system(&self) -> Option<&WatchFileSystem> {
        self.file_system.as_ref()
    }

    /// Starts erasing the next free block littlefs will want, if one is not erased yet; gives the block.
    #[cfg_attr(not(with_shell), allow(dead_code))]
    fn start_erasing_ahead(&self) -> Option<u32> {
        let block = next_block_to_erase_ahead(self.file_system.as_ref()?, &ERASED_AHEAD)?;
        ERASED_AHEAD.begin_erasing(block);
        self.external_flash
            .lock(|flash| flash.borrow_mut().start_erasing_sector(block_address(block)))
            .ok()?;
        Some(block)
    }

    /// Whether the erase `start_erasing_ahead` began is over, counting its block erased once it is.
    #[cfg_attr(not(with_shell), allow(dead_code))]
    fn erase_ahead_has_finished(&self, block: u32) -> bool {
        let finished = self
            .external_flash
            .lock(|flash| flash.borrow_mut().erase_has_finished())
            .unwrap_or(true);
        if finished {
            ERASED_AHEAD.finish_erasing(block);
            crate::diagnostics::add_one(crate::diagnostics::Counter::BlocksErasedAhead);
            crate::diagnostics::set(
                crate::diagnostics::Counter::ErasesSkipped,
                ERASED_AHEAD.erases_skipped(),
            );
        }
        finished
    }

    pub fn read(&self, path: &str) -> Option<alloc::vec::Vec<u8>> {
        self.file_system.as_ref()?.read_to_vec(path).ok()
    }

    /// Reads part of a file from `offset`, as much as `out` holds; fewer only at the file's end.
    pub fn read_part(&self, path: &str, offset: u32, out: &mut [u8]) -> Option<usize> {
        neotime_storage::read_part(self.file_system.as_ref()?, path, offset, out)
    }

    /// Runs one line from the packages characteristic, returning the reply line and what changed.
    pub fn run_packages_command(&self, line: &[u8]) -> (alloc::string::String, Change) {
        let Some(file_system) = &self.file_system else {
            return refuse_command(line, "the file system is not mounted");
        };
        self.run_packages_command_on(file_system, line)
    }

    #[cfg(with_shell)]
    fn run_packages_command_on(
        &self,
        file_system: &WatchFileSystem,
        line: &[u8],
    ) -> (alloc::string::String, Change) {
        let mut library = shared_library::InternalFlashLibrary(self.internal_flash);
        neotime_packages::commands::handle_command(line, file_system, &mut library)
    }

    /// The recovery firmware has no apps, so it leaves the package manager out.
    #[cfg(not(with_shell))]
    fn run_packages_command_on(
        &self,
        _file_system: &WatchFileSystem,
        line: &[u8],
    ) -> (alloc::string::String, Change) {
        refuse_command(line, "the recovery firmware does not install apps")
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

// ============================================================================
// The shared library in internal flash
// ============================================================================

#[cfg(with_shell)]
mod shared_library {
    use super::SharedInternalFlash;
    use neotime_boot::SHARED_LIBRARY;
    use neotime_packages::{
        check_library_image,
        commands::LibraryInstaller,
        library::{install_library_from_file, LibraryError, LIBRARY_HEADER_BYTES},
    };

    /// Copies a library file into the `SHARED_LIBRARY` region and checks it where it lies, through the
    /// chip's flash controller. Each page erase stops the CPU for about 85 ms.
    pub struct InternalFlashLibrary(pub &'static SharedInternalFlash);

    impl LibraryInstaller for InternalFlashLibrary {
        fn install<S: littlefs_rust::Storage>(
            &mut self,
            files: &littlefs_rust::Filesystem<S>,
            path: &str,
        ) -> Result<(), LibraryError> {
            self.0.lock(|flash| {
                let mut flash = flash.borrow_mut();
                install_library_from_file(&mut *flash, files, path, |_, length| {
                    // The internal flash is mapped into the address space, so the image is read where it was written.
                    let start = (SHARED_LIBRARY.start + LIBRARY_HEADER_BYTES) as *const u8;
                    check_library_image(unsafe { core::slice::from_raw_parts(start, length as usize) })
                })
            })
        }

        fn installed_version(&self) -> Option<u16> {
            crate::apps::shared_library().and_then(neotime_packages::library::library_version)
        }
    }
}

// ============================================================================
// Erasing free blocks ahead of littlefs
// ============================================================================

/// How often the task looks for free blocks to erase ahead of littlefs.
#[cfg(with_shell)]
const ERASE_AHEAD_INTERVAL: embassy_time::Duration = embassy_time::Duration::from_secs(1);

/// How often the task asks the chip whether its erase is over.
#[cfg(with_shell)]
const ERASE_POLL_INTERVAL: embassy_time::Duration = embassy_time::Duration::from_millis(5);

/// Keeps the next few free blocks erased while the wearer is not watching the screen, so littlefs's
/// own erases return at once and the chip's tens of milliseconds pass while other tasks run.
#[cfg(with_shell)]
#[embassy_executor::task]
pub async fn keep_blocks_erased_ahead(files: &'static Files) {
    loop {
        embassy_time::Timer::after(ERASE_AHEAD_INTERVAL).await;
        loop {
            crate::foreground::wait_for_a_pause().await;
            // Choosing the block and starting its erase happen together, so littlefs cannot take it between.
            let Some(block) = files.start_erasing_ahead() else {
                break;
            };
            while !files.erase_ahead_has_finished(block) {
                embassy_time::Timer::after(ERASE_POLL_INTERVAL).await;
            }
        }
    }
}
