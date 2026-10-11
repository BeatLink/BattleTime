//! The steps that move a watch from InfiniTime or wasp-os onto NeoTime, in the order that keeps
//! the time when a power cut would leave the watch unable to start as short as it can be.
//!
//! The reloader runs these steps as a normal app under the old system. Everything that goes into
//! the external flash is written first, since the old system still starts while that happens.
//! Then the NeoTime bootloader is written over the start of internal flash: from the first erase
//! until its last word is in place, nothing can start. After that the watch is always recoverable,
//! because the bootloader's button hold can install the staged recovery firmware. The boot state
//! is written last, so the first start installs the staged firmware on its own.

use crate::{Region, BOOTLOADER, BOOT_STATE, FIRMWARE, FIRMWARE_UPDATE, PAGE_SIZE, RECOVERY_FIRMWARE};

// ============================================================================
// What the reloader carries
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PayloadSizes {
    pub bootloader_in_bytes: u32,
    pub firmware_in_bytes: u32,
    pub recovery_in_bytes: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadProblem {
    BootloaderTooLarge,
    FirmwareTooLarge,
    RecoveryTooLarge,
}

impl PayloadSizes {
    pub const fn check(self) -> Result<(), PayloadProblem> {
        if self.bootloader_in_bytes > BOOTLOADER.size {
            return Err(PayloadProblem::BootloaderTooLarge);
        }
        if self.firmware_in_bytes > FIRMWARE.size {
            return Err(PayloadProblem::FirmwareTooLarge);
        }
        if let Some(recovery_in_bytes) = self.recovery_in_bytes {
            if recovery_in_bytes > RECOVERY_FIRMWARE.size {
                return Err(PayloadProblem::RecoveryTooLarge);
            }
        }
        Ok(())
    }
}

// ============================================================================
// The steps
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flash {
    Internal,
    External,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Copy the recovery firmware into its slot in the external flash.
    StageRecovery { region: Region },
    /// Copy the new firmware into the update slot in the external flash.
    StageFirmware { region: Region },
    /// Erase and write the bootloader's pages; a power cut in here leaves nothing that starts.
    WriteBootloader { region: Region },
    /// Mark the staged firmware for the bootloader to swap in at the first start.
    MarkSwap { region: Region },
}

impl Step {
    pub const fn flash(self) -> Flash {
        match self {
            Step::StageRecovery { .. } | Step::StageFirmware { .. } => Flash::External,
            Step::WriteBootloader { .. } | Step::MarkSwap { .. } => Flash::Internal,
        }
    }

    pub const fn region(self) -> Region {
        match self {
            Step::StageRecovery { region }
            | Step::StageFirmware { region }
            | Step::WriteBootloader { region }
            | Step::MarkSwap { region } => region,
        }
    }

    /// True while a power cut would leave the watch unable to start.
    pub const fn opens_the_window(self) -> bool {
        matches!(self, Step::WriteBootloader { .. })
    }
}

pub const MOST_STEPS: usize = 4;

/// The reloader's plan, in order; the slice is short when there is no recovery firmware to stage.
pub fn plan(sizes: PayloadSizes) -> ([Step; MOST_STEPS], usize) {
    let mut steps = [Step::MarkSwap { region: BOOT_STATE }; MOST_STEPS];
    let mut count = 0;

    if let Some(recovery_in_bytes) = sizes.recovery_in_bytes {
        steps[count] = Step::StageRecovery {
            region: Region {
                start: RECOVERY_FIRMWARE.start,
                size: whole_pages(recovery_in_bytes),
            },
        };
        count += 1;
    }
    steps[count] = Step::StageFirmware {
        region: Region {
            start: FIRMWARE_UPDATE.start,
            size: whole_pages(sizes.firmware_in_bytes),
        },
    };
    count += 1;
    steps[count] = Step::WriteBootloader {
        region: Region {
            start: BOOTLOADER.start,
            size: whole_pages(sizes.bootloader_in_bytes),
        },
    };
    count += 1;
    steps[count] = Step::MarkSwap { region: BOOT_STATE };
    count += 1;

    (steps, count)
}

const fn whole_pages(length_in_bytes: u32) -> u32 {
    length_in_bytes.div_ceil(PAGE_SIZE) * PAGE_SIZE
}

// ============================================================================
// CRC-32, which guards every payload and every copy
// ============================================================================

/// CRC-32 as zlib and most tools compute it; slow, bit by bit, but needs no table.
pub fn crc32(initial: u32, bytes: &[u8]) -> u32 {
    let mut crc = !initial;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

pub const CRC32_INITIAL: u32 = 0;
