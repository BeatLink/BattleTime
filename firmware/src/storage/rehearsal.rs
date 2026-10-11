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

use crate::{Screen, SpiBus};

// ============================================================================
// What this build is
// ============================================================================

pub const BUILD_NAME: &str = "rehearsal";
/// wasp-os's bootloader keeps its watchdog running under a debugger, and the takeover must match it exactly.
pub const WATCHDOG_HALT: HaltConfig = HaltConfig::Run;
/// Staying up after an upload leaves the result on the screen; a reset would only restart the rehearsal.
pub const RESET_AFTER_ACTIVATE: bool = false;

const CLOCK_TASKS_LOW_FREQUENCY_STOP: *mut u32 = 0x4000_000C as *mut u32;
const CLOCK_LOW_FREQUENCY_STATUS: *const u32 = 0x4000_0418 as *const u32;
const LOW_FREQUENCY_CLOCK_RUNNING: u32 = 1 << 16;

/// wasp-os's bootloader hands over with the low frequency clock running, and it must stop before it is reconfigured.
pub fn prepare_clocks() {
    unsafe {
        CLOCK_TASKS_LOW_FREQUENCY_STOP.write_volatile(1);
        while CLOCK_LOW_FREQUENCY_STATUS.read_volatile() & LOW_FREQUENCY_CLOCK_RUNNING != 0 {}
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
) -> Option<(Targets, Confirmation)> {
    core::mem::forget(Output::new(
        peripherals.flash_chip_select,
        Level::High,
        OutputDrive::Standard,
    ));
    let _flash_controller_never_used = peripherals.flash_controller;
    screen.write_line(2, "no flash writes");

    let targets = Targets {
        buffer: REHEARSAL_BUFFER.init([0; LARGEST_REHEARSAL_IMAGE]),
        image_size_in_bytes: 0,
        bytes_received: 0,
    };
    Some((targets, Confirmation))
}

// ============================================================================
// Confirming does nothing under wasp-os
// ============================================================================

pub struct Confirmation;

impl Confirmation {
    pub async fn confirm(&mut self) -> &'static str {
        "phone exchanged data"
    }
}
