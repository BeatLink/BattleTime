extern crate std;

use std::{vec, vec::Vec};

use embedded_storage::nor_flash::{ErrorType, NorFlashErrorKind, ReadNorFlash};

use super::*;

// ============================================================================
// Fake flash
// ============================================================================

struct FakeFlash {
    memory: Vec<u8>,
    erase_count: usize,
}

impl FakeFlash {
    fn erased(size: u32) -> FakeFlash {
        FakeFlash {
            memory: vec![0xFF; size as usize],
            erase_count: 0,
        }
    }

    fn put(&mut self, address: u32, bytes: &[u8]) {
        let start = address as usize;
        self.memory[start..start + bytes.len()].copy_from_slice(bytes);
    }

    fn bytes_at(&self, address: u32, length: usize) -> &[u8] {
        &self.memory[address as usize..address as usize + length]
    }
}

impl ErrorType for FakeFlash {
    type Error = NorFlashErrorKind;
}

impl ReadNorFlash for FakeFlash {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        bytes.copy_from_slice(self.bytes_at(offset, bytes.len()));
        Ok(())
    }

    fn capacity(&self) -> usize {
        self.memory.len()
    }
}

impl NorFlash for FakeFlash {
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = PAGE_SIZE as usize;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        self.memory[from as usize..to as usize].fill(0xFF);
        self.erase_count += 1;
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        let start = offset as usize;
        for (memory_byte, new_byte) in self.memory[start..start + bytes.len()].iter_mut().zip(bytes) {
            *memory_byte &= new_byte;
        }
        Ok(())
    }
}

fn vector_table(stack_pointer: u32, reset_handler: u32) -> Vec<u8> {
    [stack_pointer.to_le_bytes(), reset_handler.to_le_bytes()].concat()
}

fn valid_vector_table() -> Vec<u8> {
    vector_table(RAM.end(), FIRMWARE.start + 0x101)
}

// ============================================================================
// Layout
// ============================================================================

#[test]
fn internal_regions_follow_each_other_without_gaps() {
    assert_eq!(BOOT_STATE.start, BOOTLOADER.end());
    assert_eq!(FIRMWARE.start, BOOT_STATE.end());
    assert_eq!(SHARED_LIBRARY.start, FIRMWARE.end());
}

#[test]
fn firmware_starts_where_the_reloader_expects_it() {
    assert_eq!(BOOTLOADER.start, 0x0000);
    assert_eq!(BOOT_STATE.start, 0x4000);
    assert_eq!(FIRMWARE.start, 0x5000);
}

#[test]
fn update_slot_has_one_spare_page_for_the_swap() {
    assert_eq!(FIRMWARE_UPDATE.size, FIRMWARE.size + PAGE_SIZE);
}

// ============================================================================
// Firmware image check
// ============================================================================

#[test]
fn accepts_an_image_with_a_valid_vector_table() {
    let mut flash = FakeFlash::erased(PAGE_SIZE);
    flash.put(0, &valid_vector_table());

    assert_eq!(looks_like_firmware(&mut flash, 0), Ok(true));
}

#[test]
fn rejects_erased_flash() {
    let mut flash = FakeFlash::erased(PAGE_SIZE);

    assert_eq!(looks_like_firmware(&mut flash, 0), Ok(false));
}

#[test]
fn rejects_an_image_linked_for_another_address() {
    let mut flash = FakeFlash::erased(PAGE_SIZE);
    flash.put(0, &vector_table(RAM.end(), SHARED_LIBRARY.start + 0x101));

    assert_eq!(looks_like_firmware(&mut flash, 0), Ok(false));
}

#[test]
fn rejects_a_reset_handler_without_the_thumb_bit() {
    let mut flash = FakeFlash::erased(PAGE_SIZE);
    flash.put(0, &vector_table(RAM.end(), FIRMWARE.start + 0x100));

    assert_eq!(looks_like_firmware(&mut flash, 0), Ok(false));
}

// ============================================================================
// Recovery install
// ============================================================================

#[test]
fn copies_the_recovery_image_into_the_update_slot() {
    let mut flash = FakeFlash::erased(EXTERNAL_FLASH_SIZE);
    let image: Vec<u8> = valid_vector_table()
        .into_iter()
        .chain((0..5000).map(|index| index as u8))
        .collect();
    flash.put(RECOVERY_FIRMWARE.start, &image);
    flash.put(FIRMWARE_UPDATE.start, &[0; 64]);

    let result = copy_recovery_into_update_slot(&mut flash);

    assert_eq!(result, Ok(RecoveryCopy::Copied));
    assert_eq!(flash.bytes_at(FIRMWARE_UPDATE.start, image.len()), &image[..]);
    assert!(flash
        .bytes_at(FIRMWARE_UPDATE.start + image.len() as u32, 64)
        .iter()
        .all(|byte| *byte == 0xFF));
}

#[test]
fn leaves_the_update_slot_alone_without_a_recovery_image() {
    let mut flash = FakeFlash::erased(EXTERNAL_FLASH_SIZE);
    flash.put(FIRMWARE_UPDATE.start, &[0; 64]);

    let result = copy_recovery_into_update_slot(&mut flash);

    assert_eq!(result, Ok(RecoveryCopy::NoRecoveryImage));
    assert_eq!(flash.erase_count, 0);
    assert_eq!(flash.bytes_at(FIRMWARE_UPDATE.start, 64), &[0; 64]);
}

#[test]
fn external_slots_fit_before_the_file_system() {
    assert_eq!(FIRMWARE_UPDATE.start, 0x00_0000);
    assert_eq!(RECOVERY_FIRMWARE.start, 0x06_5000);
    assert_eq!(RESERVED_FOR_GROWTH.start, 0x0C_9000);
    assert_eq!(FILE_SYSTEM.start, 0x10_0000);
}

// ============================================================================
// Migration plan
// ============================================================================

mod migration {
    use super::super::migration::*;
    use super::super::*;

    fn sizes(recovery: Option<u32>) -> PayloadSizes {
        PayloadSizes {
            bootloader_in_bytes: 7900,
            firmware_in_bytes: 105_000,
            recovery_in_bytes: recovery,
        }
    }

    #[test]
    fn external_flash_is_written_before_the_window_and_the_swap_mark_after() {
        let (steps, count) = plan(sizes(Some(60_000)));
        let steps = &steps[..count];

        let window = steps.iter().position(|step| step.opens_the_window()).unwrap();
        assert!(steps[..window].iter().all(|step| step.flash() == Flash::External));
        assert_eq!(steps[window + 1..], [Step::MarkSwap { region: BOOT_STATE }]);
    }

    #[test]
    fn only_the_bootloader_step_opens_the_window() {
        let (steps, count) = plan(sizes(Some(60_000)));

        assert_eq!(
            steps[..count]
                .iter()
                .filter(|step| step.opens_the_window())
                .count(),
            1
        );
    }

    #[test]
    fn the_window_covers_only_the_bootloader_pages_in_use() {
        let (steps, count) = plan(sizes(None));

        let bootloader = steps[..count]
            .iter()
            .find(|step| step.opens_the_window())
            .unwrap()
            .region();
        assert_eq!(
            bootloader,
            Region {
                start: 0,
                size: 2 * PAGE_SIZE
            }
        );
    }

    #[test]
    fn no_step_writes_outside_its_slot() {
        let (steps, count) = plan(sizes(Some(RECOVERY_FIRMWARE.size)));

        for step in &steps[..count] {
            let slot = match step {
                Step::StageRecovery { .. } => RECOVERY_FIRMWARE,
                Step::StageFirmware { .. } => FIRMWARE_UPDATE,
                Step::WriteBootloader { .. } => BOOTLOADER,
                Step::MarkSwap { .. } => BOOT_STATE,
            };
            assert!(
                step.region().start >= slot.start && step.region().end() <= slot.end(),
                "{step:?}"
            );
        }
    }

    #[test]
    fn the_plan_is_shorter_without_a_recovery_firmware() {
        let (_, with) = plan(sizes(Some(100)));
        let (_, without) = plan(sizes(None));

        assert_eq!((with, without), (4, 3));
    }

    #[test]
    fn oversized_payloads_are_refused() {
        assert_eq!(sizes(None).check(), Ok(()));
        assert_eq!(
            PayloadSizes {
                bootloader_in_bytes: BOOTLOADER.size + 1,
                ..sizes(None)
            }
            .check(),
            Err(PayloadProblem::BootloaderTooLarge)
        );
        assert_eq!(
            PayloadSizes {
                firmware_in_bytes: FIRMWARE.size + 1,
                ..sizes(None)
            }
            .check(),
            Err(PayloadProblem::FirmwareTooLarge)
        );
        assert_eq!(
            sizes(Some(RECOVERY_FIRMWARE.size + 1)).check(),
            Err(PayloadProblem::RecoveryTooLarge)
        );
    }

    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(CRC32_INITIAL, b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(crc32(CRC32_INITIAL, b"1234"), b"56789"), 0xCBF4_3926);
    }
}
