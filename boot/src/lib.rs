//! Where everything lives in the PineTime's two flash chips, and how the recovery firmware is installed.
//!
//! The bootloader and the firmware both build on these numbers, so they must change together. The
//! running firmware sits in internal flash; updates and the recovery firmware wait in external flash
//! until the bootloader swaps them in with embassy-boot.

#![no_std]

use embedded_storage::nor_flash::NorFlash;

// ============================================================================
// Regions
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub start: u32,
    pub size: u32,
}

impl Region {
    pub const fn end(self) -> u32 {
        self.start + self.size
    }

    pub const fn contains(self, address: u32) -> bool {
        address >= self.start && address < self.end()
    }
}

pub const PAGE_SIZE: u32 = 4096;

// ============================================================================
// Internal flash, 512 KB
// ============================================================================

pub const INTERNAL_FLASH_SIZE: u32 = 512 * 1024;

/// There is no Nordic master boot record; the bootloader is the first code to run.
pub const BOOTLOADER: Region = Region {
    start: 0x0000_0000,
    size: 16 * 1024,
};
pub const BOOT_STATE: Region = Region {
    start: BOOTLOADER.end(),
    size: PAGE_SIZE,
};
pub const FIRMWARE: Region = Region {
    start: BOOT_STATE.end(),
    size: 400 * 1024,
};
/// Images, fonts, components and common code that apps share; apps themselves live in /apps on the external flash.
pub const SHARED_LIBRARY: Region = Region {
    start: FIRMWARE.end(),
    size: INTERNAL_FLASH_SIZE - FIRMWARE.end(),
};

// ============================================================================
// External flash, 4 MB
// ============================================================================

pub const EXTERNAL_FLASH_SIZE: u32 = 4 * 1024 * 1024;

pub const FIRMWARE_UPDATE: Region = Region {
    start: 0x0000_0000,
    size: FIRMWARE.size + PAGE_SIZE,
};
pub const RECOVERY_FIRMWARE: Region = Region {
    start: FIRMWARE_UPDATE.end(),
    size: FIRMWARE.size,
};
pub const RESERVED_FOR_GROWTH: Region = Region {
    start: RECOVERY_FIRMWARE.end(),
    size: FILE_SYSTEM.start - RECOVERY_FIRMWARE.end(),
};
pub const FILE_SYSTEM: Region = Region {
    start: 1024 * 1024,
    size: EXTERNAL_FLASH_SIZE - 1024 * 1024,
};

// ============================================================================
// RAM and the watchdog
// ============================================================================

pub const RAM: Region = Region {
    start: 0x2000_0000,
    size: 64 * 1024,
};

/// The last words of RAM, where a running firmware writes `FIRMWARE_ALIVE_MAGIC` so an emulator can see it started.
pub const FIRMWARE_ALIVE_MARKER: Region = Region {
    start: RAM.end() - 16,
    size: 16,
};
pub const FIRMWARE_ALIVE_MAGIC: u32 = 0x4E45_4F54;

/// The firmware must start the watchdog with this same timeout, or it cannot take the running one over.
pub const WATCHDOG_TIMEOUT_IN_TICKS: u32 = 5 * WATCHDOG_TICKS_PER_SECOND;
pub const WATCHDOG_TICKS_PER_SECOND: u32 = 32768;

pub const RECOVERY_BUTTON_HOLD_IN_MILLISECONDS: u32 = 5000;

// ============================================================================
// Layout checks
// ============================================================================

const BOOT_STATE_WRITE_SIZE: u32 = 4;
const FIRMWARE_PAGES: u32 = FIRMWARE.size / PAGE_SIZE;

const _: () = {
    let regions = [
        BOOTLOADER,
        BOOT_STATE,
        FIRMWARE,
        SHARED_LIBRARY,
        FIRMWARE_UPDATE,
        RECOVERY_FIRMWARE,
        FILE_SYSTEM,
    ];
    let mut index = 0;
    while index < regions.len() {
        assert!(
            regions[index].start.is_multiple_of(PAGE_SIZE) && regions[index].size.is_multiple_of(PAGE_SIZE)
        );
        index += 1;
    }

    assert!(SHARED_LIBRARY.end() == INTERNAL_FLASH_SIZE);
    assert!(FILE_SYSTEM.end() == EXTERNAL_FLASH_SIZE);
    assert!(RECOVERY_FIRMWARE.end() <= FILE_SYSTEM.start);

    // These are the rules embassy-boot checks at boot; breaking them here fails the build instead.
    assert!(FIRMWARE_UPDATE.size >= FIRMWARE.size + PAGE_SIZE);
    assert!(2 + 4 * FIRMWARE_PAGES <= BOOT_STATE.size / BOOT_STATE_WRITE_SIZE);
};

// ============================================================================
// Firmware image check
// ============================================================================

pub const VECTOR_TABLE_CHECK_SIZE: usize = 8;

/// Checks the vector table at the start of an image: a stack pointer in RAM and a reset handler in `FIRMWARE`.
pub fn is_firmware_vector_table(vector_table_start: [u8; VECTOR_TABLE_CHECK_SIZE]) -> bool {
    let [stack_pointer, reset_handler] = [&vector_table_start[..4], &vector_table_start[4..]]
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()));
    let reset_handler_address = reset_handler & !1;

    let stack_pointer_is_in_ram = stack_pointer > RAM.start && stack_pointer <= RAM.end();
    let reset_handler_is_thumb_code = reset_handler & 1 == 1;
    let reset_handler_is_in_firmware = FIRMWARE.contains(reset_handler_address);

    stack_pointer_is_in_ram && reset_handler_is_thumb_code && reset_handler_is_in_firmware
}

pub fn looks_like_firmware<Flash: NorFlash>(
    flash: &mut Flash,
    image_start: u32,
) -> Result<bool, Flash::Error> {
    let mut vector_table_start = [0; VECTOR_TABLE_CHECK_SIZE];
    flash.read(image_start, &mut vector_table_start)?;

    Ok(is_firmware_vector_table(vector_table_start))
}

// ============================================================================
// Recovery install
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryCopy {
    Copied,
    NoRecoveryImage,
}

const COPY_CHUNK_SIZE: usize = 1024;

/// Copies the recovery firmware into the update slot, so the next swap installs it like any update.
pub fn copy_recovery_into_update_slot<Flash: NorFlash>(
    external_flash: &mut Flash,
) -> Result<RecoveryCopy, Flash::Error> {
    if !looks_like_firmware(external_flash, RECOVERY_FIRMWARE.start)? {
        return Ok(RecoveryCopy::NoRecoveryImage);
    }

    external_flash.erase(FIRMWARE_UPDATE.start, FIRMWARE_UPDATE.end())?;

    let mut chunk = [0; COPY_CHUNK_SIZE];
    for offset in (0..RECOVERY_FIRMWARE.size).step_by(COPY_CHUNK_SIZE) {
        external_flash.read(RECOVERY_FIRMWARE.start + offset, &mut chunk)?;

        let chunk_is_erased = chunk.iter().all(|byte| *byte == 0xFF);
        if !chunk_is_erased {
            external_flash.write(FIRMWARE_UPDATE.start + offset, &chunk)?;
        }
    }

    Ok(RecoveryCopy::Copied)
}


pub mod migration;

#[cfg(test)]
mod tests;
