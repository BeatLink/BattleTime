//! The rehearsal build, run as an app under wasp-os to prove Bluetooth before anything is replaced.
//!
//! It writes to neither flash chip. NeoTime's boot state page sits inside wasp-os's SoftDevice and the
//! external flash holds wasp-os's files, so an upload lands in RAM and is only checked, never kept.

use embassy_nrf::{
    gpio::{Level, Output, OutputDrive},
    peripherals::{NVMC, P0_05},
    wdt::HaltConfig,
    Peri,
};
use neotime_updates::{ImageKind, UpdateError, UpdateTargets};
use static_cell::StaticCell;

use crate::{line, status::Mode, Screen, SpiBus};

// ============================================================================
// What this build is
// ============================================================================

pub const BUILD_NAME: &str = "rehearsal";
/// wasp-os's bootloader keeps its watchdog running under a debugger, and the takeover must match it exactly.
pub const WATCHDOG_HALT: HaltConfig = HaltConfig::Run;
const POWER_GENERAL_PURPOSE_RETAINED: *mut u32 = 0x4000_051C as *mut u32;
/// The value wasp-os's bootloader takes as a request to start its Bluetooth update mode.
const WASP_OS_ENTER_UPDATE_MODE: u32 = 0xA8;

/// Shows the result, then restarts into wasp-os's update mode, so the next image can be sent without the button.
pub async fn reset(screen: &mut Screen) -> ! {
    screen.write_line(line::ACTION, "Done, back to wasp-os");
    embassy_time::Timer::after(embassy_time::Duration::from_millis(2000)).await;
    unsafe { POWER_GENERAL_PURPOSE_RETAINED.write_volatile(WASP_OS_ENTER_UPDATE_MODE) };
    cortex_m::peripheral::SCB::sys_reset()
}

/// The heap is unused here, since nothing mounts the file system.
pub const HEAP_SIZE: usize = 4 * 1024;

/// Nothing to prepare: wasp-os's watchdog keeps the low frequency clock running and a stop is ignored,
/// so waiting for one would hang until the watchdog fires; embassy's start-up copes with a running clock.
pub fn prepare_clocks() {}

// ============================================================================
// Breadcrumbs that survive a reset, for finding where a run stopped
// ============================================================================

/// Words in wasp-os's bootloader's retained RAM block that it leaves alone; the linker keeps the rehearsal clear of them.
const BREADCRUMBS: *mut u32 = 0x2000_39CC as *mut u32;
const BREADCRUMB_MAGIC: u32 = 0x4E54_4243;

#[derive(Clone, Copy)]
pub struct Breadcrumbs {
    pub stage: u32,
    pub detail: u32,
    pub fault_status: u32,
}

/// What the previous run left, if it left anything; read before this run overwrites it.
pub fn previous_breadcrumbs() -> Option<Breadcrumbs> {
    unsafe {
        if BREADCRUMBS.add(3).read_volatile() != BREADCRUMB_MAGIC {
            return None;
        }
        Some(Breadcrumbs {
            stage: BREADCRUMBS.read_volatile(),
            detail: BREADCRUMBS.add(1).read_volatile(),
            fault_status: BREADCRUMBS.add(2).read_volatile(),
        })
    }
}

pub fn leave_breadcrumb(stage: u32) {
    record(Breadcrumbs {
        stage,
        detail: 0,
        fault_status: 0,
    });
}

pub fn record(breadcrumbs: Breadcrumbs) {
    unsafe {
        BREADCRUMBS.write_volatile(breadcrumbs.stage);
        BREADCRUMBS.add(1).write_volatile(breadcrumbs.detail);
        BREADCRUMBS.add(2).write_volatile(breadcrumbs.fault_status);
        BREADCRUMBS.add(3).write_volatile(BREADCRUMB_MAGIC);
    }
}

// ============================================================================
// Taking an upload into RAM
// ============================================================================

pub const LARGEST_REHEARSAL_IMAGE: usize = 16 * 1024;

pub struct Targets {
    buffer: &'static mut [u8; LARGEST_REHEARSAL_IMAGE],
    image_size_in_bytes: usize,
    bytes_received: usize,
}

impl UpdateTargets for Targets {
    async fn begin(&mut self, _kind: ImageKind, image_size_in_bytes: u32) -> Result<(), UpdateError> {
        if image_size_in_bytes as usize > LARGEST_REHEARSAL_IMAGE {
            return Err(UpdateError::ImageTooLarge {
                image_size_in_bytes,
                largest_size_in_bytes: LARGEST_REHEARSAL_IMAGE as u32,
            });
        }
        self.image_size_in_bytes = image_size_in_bytes as usize;
        self.bytes_received = 0;
        Ok(())
    }

    async fn receive(&mut self, _kind: ImageKind, chunk: &[u8]) -> Result<(), UpdateError> {
        let end = self.bytes_received + chunk.len();
        if end > self.image_size_in_bytes {
            return Err(UpdateError::MoreThanAnnounced {
                image_size_in_bytes: self.image_size_in_bytes as u32,
            });
        }
        self.buffer[self.bytes_received..end].copy_from_slice(chunk);
        self.bytes_received = end;
        Ok(())
    }

    async fn read_back(
        &mut self,
        _kind: ImageKind,
        offset: u32,
        bytes: &mut [u8],
    ) -> Result<(), UpdateError> {
        let start = offset as usize;
        let end = start + bytes.len();
        if end > self.bytes_received {
            return Err(UpdateError::Flash);
        }
        bytes.copy_from_slice(&self.buffer[start..end]);
        Ok(())
    }

    /// Nothing is installed in a rehearsal; reaching here means the image arrived whole and its CRC matched.
    async fn finish(&mut self, _kind: ImageKind) -> Result<(), UpdateError> {
        Ok(())
    }

    fn abort(&mut self, _kind: ImageKind) {
        self.bytes_received = 0;
    }
}

static REHEARSAL_BUFFER: StaticCell<[u8; LARGEST_REHEARSAL_IMAGE]> = StaticCell::new();

pub struct StoragePeripherals {
    pub flash_chip_select: Peri<'static, P0_05>,
    pub flash_controller: Peri<'static, NVMC>,
}

/// Holds the external flash off the shared bus and leaves the flash controller untouched.
pub async fn start(
    _spi_bus: &'static SpiBus,
    peripherals: StoragePeripherals,
    screen: &mut Screen,
) -> Option<(Targets, Confirmation, Mode, &'static Files)> {
    core::mem::forget(Output::new(
        peripherals.flash_chip_select,
        Level::High,
        OutputDrive::Standard,
    ));
    let _flash_controller_never_used = peripherals.flash_controller;
    screen.write_line(line::FILES, "Not used");
    screen.write_line(line::BOOTLOADER, "wasp-os");

    let targets = Targets {
        buffer: REHEARSAL_BUFFER.init([0; LARGEST_REHEARSAL_IMAGE]),
        image_size_in_bytes: 0,
        bytes_received: 0,
    };
    Some((targets, Confirmation, Mode::Confirmed, &Files))
}

// ============================================================================
// Confirming does nothing under wasp-os
// ============================================================================

pub struct Confirmation;

impl Confirmation {
    pub async fn confirm(&mut self) -> bool {
        true
    }
}

// ============================================================================
// Files for saved app state, which the rehearsal does not keep
// ============================================================================

/// The rehearsal has no shell, so it keeps no files for it.
pub struct Files;

/// Every call finds nothing, so the file service answers each command with an error.
#[allow(dead_code)]
impl Files {
    pub async fn wait_until_free(&self) {}

    pub fn read_now(&self, _path: &str) -> Option<alloc::vec::Vec<u8>> {
        None
    }

    pub fn read_part_now(&self, _path: &str, _offset: u32, _out: &mut [u8]) -> Option<usize> {
        None
    }

    pub fn write_now(&self, _path: &str, _bytes: &[u8]) -> bool {
        false
    }

    pub fn free_kilobytes_now(&self) -> Option<u32> {
        None
    }

    pub fn installed_apps_now(&self) -> alloc::vec::Vec<neotime_packages::InstalledApp> {
        alloc::vec::Vec::new()
    }

    pub fn set_app_enabled_now(&self, _id: &str, _enabled: bool) -> bool {
        false
    }

    pub fn read_app_image_now(&self, _id: &str) -> Option<alloc::vec::Vec<u8>> {
        None
    }
}

/// Answers every request as a build with no store must: file commands with an error, steps with nothing.
#[embassy_executor::task]
pub async fn run_storage(_files: &'static Files) {
    use crate::storage_requests::{self, StorageRequest};
    use embassy_futures::select::{select, Either};

    loop {
        match select(
            storage_requests::REQUESTS.receive(),
            crate::packages::COMMANDS.receive(),
        )
        .await
        {
            Either::First(StorageRequest::FileTransfer { request, .. }) => {
                storage_requests::FILE_ANSWERS
                    .send(neotime_blefs::FileTransfer::refuse(&request))
                    .await;
            }
            Either::First(StorageRequest::LoadSteps) => storage_requests::STEPS_LOADED.signal(()),
            Either::First(_) => {}
            Either::Second(line) => {
                let (reply, _) =
                    neotime_packages::commands::refuse_command(&line, "this build keeps no apps");
                crate::packages::REPLIES.send(reply).await;
            }
        }
    }
}
