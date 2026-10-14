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

/// A firmware image as `tools/image_header.py` writes it: a sound header, then `code_size` bytes of code.
fn image(image_type: image_header::ImageType, code_size: usize) -> Vec<u8> {
    let code: Vec<u8> = (0..code_size).map(|index| (index % 251) as u8).collect();
    let header = image_header::ImageHeader {
        image_type,
        version: image_header::Version {
            major: 0,
            minor: 1,
            patch: 0,
        },
        load_address: FIRMWARE_VECTOR_TABLE,
        image_size_in_bytes: code_size as u32,
        image_crc32: crate::migration::crc32(crate::migration::CRC32_INITIAL, &code),
        build_time_in_seconds: 1_760_000_000,
        commit: *b"1b19aed*",
        library_offset: 0,
        library_size_in_bytes: 0,
    };
    [header.encode().to_vec(), code].concat()
}

fn firmware_image(code_size: usize) -> Vec<u8> {
    image(image_header::ImageType::Firmware, code_size)
}

// ============================================================================
// Layout
// ============================================================================

#[test]
fn internal_regions_follow_each_other_without_gaps() {
    assert_eq!(SLOT_TABLE.start, BOOTLOADER.end());
    assert_eq!(BOOT_STATE.start, SLOT_TABLE.end());
    assert_eq!(FIRMWARE.start, BOOT_STATE.end());
    assert_eq!(FIRMWARE.end(), INTERNAL_FLASH_SIZE);
}

#[test]
fn the_bootloader_block_fills_the_first_32_kb() {
    assert_eq!(BOOTLOADER.start, 0x0000);
    assert_eq!(SLOT_TABLE.start, 0x5000);
    assert_eq!(BOOT_STATE.start, 0x7000);
    assert_eq!(FIRMWARE.start, 0x8000);
    assert_eq!(FIRMWARE_VECTOR_TABLE, 0x8100);
    assert_eq!(FIRMWARE.size, 480 * 1024);
}

#[test]
fn the_default_recovery_slot_is_clear_of_the_388_kb_layouts_slots() {
    let old_update_end = 388 * 1024 + PAGE_SIZE;
    let old_recovery = Region {
        start: 0x6_5000,
        size: 388 * 1024,
    };

    assert!(DEFAULT_SLOTS.recovery.start >= old_update_end);
    assert!(DEFAULT_SLOTS.recovery.start >= old_recovery.end());
    assert_eq!(DEFAULT_SLOTS.recovery.end(), SLOT_AREA.end());
}

#[test]
fn the_replaced_firmware_fits_in_the_update_slot_one_page_up() {
    let previous = DEFAULT_SLOTS.previous_firmware();

    assert_eq!(previous.start, DEFAULT_SLOTS.update.start + PAGE_SIZE);
    assert_eq!(previous.end(), DEFAULT_SLOTS.update.end());
}

#[test]
fn the_default_slots_put_recovery_after_the_update_slot() {
    assert_eq!(DEFAULT_SLOTS.update.start, 0x00_0000);
    assert_eq!(DEFAULT_SLOTS.update.size, FIRMWARE.size + PAGE_SIZE);
    assert_eq!(DEFAULT_SLOTS.recovery.start, 0x0C_6000);
    assert!(DEFAULT_SLOTS.recovery.end() <= FILE_SYSTEM.start);
    assert_eq!(FILE_SYSTEM.start, 0x10_0000);
}

// ============================================================================
// Slot rules
// ============================================================================

mod slot_rules {
    use super::super::*;

    fn with_recovery(start: u32, size: u32) -> Slots {
        Slots {
            recovery: Region { start, size },
            ..DEFAULT_SLOTS
        }
    }

    #[test]
    fn the_default_slots_are_sound() {
        assert_eq!(DEFAULT_SLOTS.check(), Ok(()));
    }

    #[test]
    fn a_recovery_slot_sized_to_its_image_is_sound() {
        assert_eq!(with_recovery(0x7_9000, 172 * 1024).check(), Ok(()));
    }

    #[test]
    fn slots_must_be_whole_pages() {
        assert_eq!(
            with_recovery(0x7_9001, 172 * 1024).check(),
            Err(SlotProblem::NotWholePages)
        );
        assert_eq!(
            with_recovery(0x7_9000, 172 * 1024 + 1).check(),
            Err(SlotProblem::NotWholePages)
        );
    }

    #[test]
    fn slots_never_reach_into_the_file_system() {
        assert_eq!(
            with_recovery(FILE_SYSTEM.start - PAGE_SIZE, 172 * 1024).check(),
            Err(SlotProblem::OutsideSlotArea)
        );
        assert_eq!(
            with_recovery(0xFFFF_F000, 172 * 1024).check(),
            Err(SlotProblem::OutsideSlotArea)
        );
    }

    #[test]
    fn the_update_slot_must_be_one_page_more_than_the_firmware() {
        let slots = Slots {
            update: Region {
                start: 0,
                size: FIRMWARE.size,
            },
            ..DEFAULT_SLOTS
        };

        assert_eq!(
            slots.check(),
            Err(SlotProblem::UpdateSlotNotFirmwareSizePlusOnePage)
        );
    }

    #[test]
    fn the_recovery_slot_holds_at_least_a_small_firmware_and_at_most_a_full_one() {
        assert_eq!(
            with_recovery(0x7_9000, PAGE_SIZE).check(),
            Err(SlotProblem::RecoverySlotTooSmall)
        );
        assert_eq!(
            with_recovery(0x7_9000, FIRMWARE.size + PAGE_SIZE).check(),
            Err(SlotProblem::RecoverySlotTooLarge)
        );
    }

    #[test]
    fn the_slots_must_not_overlap() {
        assert_eq!(
            with_recovery(0x6_1000, 172 * 1024).check(),
            Err(SlotProblem::Overlapping)
        );
    }
}

// ============================================================================
// Image headers
// ============================================================================

mod image_headers {
    use super::super::image_header::*;
    use super::*;

    fn header_of(image: &[u8]) -> [u8; IMAGE_HEADER_SIZE as usize] {
        image[..IMAGE_HEADER_SIZE as usize].try_into().unwrap()
    }

    #[test]
    fn a_header_reads_back_as_itself() {
        let image = firmware_image(1000);

        let header = ImageHeader::decode(&header_of(&image)).unwrap();

        assert_eq!(header.image_type, ImageType::Firmware);
        assert_eq!(header.load_address, FIRMWARE_VECTOR_TABLE);
        assert_eq!(header.image_size_in_bytes, 1000);
        assert_eq!(header.build_time_in_seconds, 1_760_000_000);
        assert_eq!(&header.commit, b"1b19aed*");
        assert_eq!(header.encode(), header_of(&image));
    }

    /// The same bytes `tools/image_header.py --self-test` checks, so the tool and the watch agree.
    #[test]
    fn the_header_matches_the_one_the_packaging_tool_writes() {
        let header = ImageHeader {
            image_type: ImageType::Recovery,
            version: Version {
                major: 1,
                minor: 2,
                patch: 3,
            },
            load_address: 0x8100,
            image_size_in_bytes: 9,
            image_crc32: crate::migration::crc32(crate::migration::CRC32_INITIAL, b"123456789"),
            build_time_in_seconds: 1_760_000_000,
            commit: *b"abcdef0\0",
            library_offset: 260,
            library_size_in_bytes: 5,
        };

        let bytes = header.encode();

        assert_eq!(&bytes[..4], b"NTIM");
        assert_eq!(u32::from_le_bytes(bytes[20..24].try_into().unwrap()), 0xCBF4_3926);
        assert_eq!(
            u32::from_le_bytes(bytes[252..].try_into().unwrap()),
            PACKAGING_TOOL_HEADER_CRC
        );
    }

    #[test]
    fn erased_flash_has_no_header() {
        assert_eq!(ImageHeader::decode(&[0xFF; IMAGE_HEADER_SIZE as usize]), None);
    }

    #[test]
    fn a_changed_byte_spoils_the_header() {
        let mut bytes = header_of(&firmware_image(1000));
        bytes[16] ^= 1;

        assert_eq!(ImageHeader::decode(&bytes), None);
    }

    #[test]
    fn an_unknown_image_type_is_refused() {
        let mut header = ImageHeader::decode(&header_of(&firmware_image(10)))
            .unwrap()
            .encode();
        header[8] = 9;
        let crc = crate::migration::crc32(crate::migration::CRC32_INITIAL, &header[..252]);
        header[252..].copy_from_slice(&crc.to_le_bytes());

        assert_eq!(ImageHeader::decode(&header), None);
    }

    fn with_library(offset: u32, size: u32) -> [u8; IMAGE_HEADER_SIZE as usize] {
        let mut header = ImageHeader::decode(&header_of(&firmware_image(1000))).unwrap();
        header.library_offset = offset;
        header.library_size_in_bytes = size;
        header.encode()
    }

    #[test]
    fn a_library_inside_the_image_reads_back_with_its_place() {
        let header = ImageHeader::decode(&with_library(IMAGE_HEADER_SIZE + 600, 400)).unwrap();

        assert_eq!(
            header.library_region(FIRMWARE.start),
            Some(Region {
                start: FIRMWARE.start + IMAGE_HEADER_SIZE + 600,
                size: 400,
            })
        );
    }

    #[test]
    fn an_image_without_a_library_has_no_library_region() {
        let header = ImageHeader::decode(&header_of(&firmware_image(1000))).unwrap();

        assert_eq!(header.library_region(FIRMWARE.start), None);
    }

    #[test]
    fn a_library_reaching_past_the_image_spoils_the_header() {
        assert_eq!(
            ImageHeader::decode(&with_library(IMAGE_HEADER_SIZE + 600, 401)),
            None
        );
    }

    #[test]
    fn a_library_inside_the_header_spoils_the_header() {
        assert_eq!(ImageHeader::decode(&with_library(IMAGE_HEADER_SIZE - 4, 4)), None);
    }

    #[test]
    fn an_offset_without_a_library_spoils_the_header() {
        assert_eq!(ImageHeader::decode(&with_library(IMAGE_HEADER_SIZE, 0)), None);
    }

    #[test]
    fn a_header_of_the_388_kb_layout_is_refused() {
        let mut header = header_of(&firmware_image(10));
        header[4..6].copy_from_slice(&1u16.to_le_bytes());
        let crc = crate::migration::crc32(crate::migration::CRC32_INITIAL, &header[..252]);
        header[252..].copy_from_slice(&crc.to_le_bytes());

        assert_eq!(ImageHeader::decode(&header), None);
    }

    #[test]
    fn an_image_linked_for_the_first_layout_does_not_run_as_firmware() {
        let mut header = ImageHeader::decode(&header_of(&firmware_image(10))).unwrap();
        header.load_address = 0x5000;

        assert!(!header.runs_as_firmware());
    }

    #[test]
    fn an_image_larger_than_the_firmware_slot_does_not_run_as_firmware() {
        let mut header = ImageHeader::decode(&header_of(&firmware_image(10))).unwrap();
        header.image_size_in_bytes = FIRMWARE.size - IMAGE_HEADER_SIZE + 1;

        assert!(!header.runs_as_firmware());
    }

    #[test]
    fn a_whole_image_is_installable() {
        let mut flash = FakeFlash::erased(PAGE_SIZE * 4);
        flash.put(0, &firmware_image(5000));

        let header = installable_image(&mut flash, 0, PAGE_SIZE * 4).unwrap();

        assert_eq!(header.map(|header| header.image_size_in_bytes), Some(5000));
    }

    #[test]
    fn a_damaged_image_is_not_installable() {
        let mut flash = FakeFlash::erased(PAGE_SIZE * 4);
        let mut image = firmware_image(5000);
        image[3000] ^= 0x10;
        flash.put(0, &image);

        assert_eq!(installable_image(&mut flash, 0, PAGE_SIZE * 4), Ok(None));
    }

    #[test]
    fn an_image_longer_than_its_slot_is_not_installable() {
        let mut flash = FakeFlash::erased(PAGE_SIZE * 4);
        flash.put(0, &firmware_image(5000));

        assert_eq!(installable_image(&mut flash, 0, PAGE_SIZE), Ok(None));
    }

    #[test]
    fn the_firmware_slot_is_runnable_only_with_a_sound_header() {
        let mut flash = FakeFlash::erased(INTERNAL_FLASH_SIZE);
        assert_eq!(firmware_is_runnable(&mut flash), Ok(false));

        flash.put(FIRMWARE.start, &firmware_image(100));
        assert_eq!(firmware_is_runnable(&mut flash), Ok(true));
    }
}

/// The header CRC `tools/image_header.py --self-test` expects for the same fields.
const PACKAGING_TOOL_HEADER_CRC: u32 = 0x98DE_B2F0;

// ============================================================================
// Slot table
// ============================================================================

mod slot_table_log {
    use super::super::slot_table::*;
    use super::*;

    fn table() -> FakeFlash {
        FakeFlash::erased(SLOT_TABLE.size)
    }

    fn recovery_at(start: u32) -> Slots {
        Slots {
            recovery: Region {
                start,
                size: 172 * 1024,
            },
            ..DEFAULT_SLOTS
        }
    }

    #[test]
    fn an_empty_table_reads_as_the_default_slots() {
        assert_eq!(read_slots(&mut table()), Ok(DEFAULT_SLOTS));
    }

    #[test]
    fn a_written_change_reads_back() {
        let mut table = table();

        write_slots(&mut table, recovery_at(0x7_9000)).unwrap();

        assert_eq!(read_slots(&mut table), Ok(recovery_at(0x7_9000)));
    }

    #[test]
    fn the_newest_record_wins() {
        let mut table = table();

        write_slots(&mut table, recovery_at(0x7_9000)).unwrap();
        write_slots(&mut table, recovery_at(0x8_0000)).unwrap();

        assert_eq!(read_slots(&mut table), Ok(recovery_at(0x8_0000)));
        assert_eq!(table.erase_count, 1);
    }

    #[test]
    fn a_full_page_moves_the_log_to_the_other_page() {
        let mut table = table();
        let records_per_page = PAGE_SIZE / RECORD_SIZE;

        for index in 0..records_per_page + 1 {
            write_slots(&mut table, recovery_at(0x7_9000 + (index % 2) * 0x1_D000)).unwrap();
        }

        assert_eq!(
            read_slots(&mut table),
            Ok(recovery_at(0x7_9000 + (records_per_page % 2) * 0x1_D000))
        );
        assert_ne!(table.bytes_at(PAGE_SIZE, 4), &[0xFF; 4]);
    }

    #[test]
    fn filling_the_second_page_reuses_the_first() {
        let mut table = table();
        let records_per_page = PAGE_SIZE / RECORD_SIZE;

        for index in 0..2 * records_per_page + 1 {
            write_slots(&mut table, recovery_at(0x7_9000 + (index % 3) * 0x1000)).unwrap();
        }

        let last = 2 * records_per_page;
        assert_eq!(
            read_slots(&mut table),
            Ok(recovery_at(0x7_9000 + (last % 3) * 0x1000))
        );
        assert_eq!(table.erase_count, 3);
    }

    #[test]
    fn a_record_cut_short_by_a_power_cut_leaves_the_one_before() {
        let mut table = table();
        write_slots(&mut table, recovery_at(0x7_9000)).unwrap();
        let mut cut_short = table.bytes_at(0, RECORD_SIZE as usize).to_vec();
        cut_short[28..].fill(0xFF);
        cut_short[8..12].copy_from_slice(&0x8_0000u32.to_le_bytes());
        table.put(RECORD_SIZE, &cut_short);

        assert_eq!(read_slots(&mut table), Ok(recovery_at(0x7_9000)));

        write_slots(&mut table, recovery_at(0x9_0000)).unwrap();
        assert_eq!(read_slots(&mut table), Ok(recovery_at(0x9_0000)));
    }

    #[test]
    fn code_left_in_the_pages_reads_as_the_default_slots_and_is_written_over() {
        let mut table = table();
        let code: Vec<u8> = (0..SLOT_TABLE.size)
            .map(|index| (index * 7 % 256) as u8)
            .collect();
        table.put(0, &code);

        assert_eq!(read_slots(&mut table), Ok(DEFAULT_SLOTS));

        write_slots(&mut table, recovery_at(0x7_9000)).unwrap();
        assert_eq!(read_slots(&mut table), Ok(recovery_at(0x7_9000)));
    }

    #[test]
    fn unsound_slots_are_never_written() {
        let mut table = table();
        let overlapping = recovery_at(0x1000);

        assert_eq!(
            write_slots(&mut table, overlapping),
            Err(SlotTableError::Unsound(SlotProblem::Overlapping))
        );
        assert_eq!(table.erase_count, 0);
        assert_eq!(read_slots(&mut table), Ok(DEFAULT_SLOTS));
    }
}

// ============================================================================
// Installing the recovery or the previous firmware
// ============================================================================

#[test]
fn copies_the_recovery_image_into_the_update_slot() {
    let mut flash = FakeFlash::erased(EXTERNAL_FLASH_SIZE);
    let recovery = image(image_header::ImageType::Recovery, 5000);
    flash.put(DEFAULT_SLOTS.recovery.start, &recovery);
    flash.put(DEFAULT_SLOTS.update.start + 8192, &[0; 64]);

    let result = copy_image_into_update_slot(&mut flash, DEFAULT_SLOTS, DEFAULT_SLOTS.recovery);

    assert_eq!(result, Ok(ImageCopy::Copied));
    assert_eq!(
        flash.bytes_at(DEFAULT_SLOTS.update.start, recovery.len()),
        &recovery[..]
    );
    assert!(flash
        .bytes_at(DEFAULT_SLOTS.update.start + recovery.len() as u32, 64)
        .iter()
        .all(|byte| *byte == 0xFF));
}

#[test]
fn copies_from_wherever_the_slot_table_puts_the_recovery_slot() {
    let mut flash = FakeFlash::erased(EXTERNAL_FLASH_SIZE);
    let slots = Slots {
        recovery: Region {
            start: 0x8_0000,
            size: 172 * 1024,
        },
        ..DEFAULT_SLOTS
    };
    let recovery = image(image_header::ImageType::Recovery, 5000);
    flash.put(slots.recovery.start, &recovery);

    assert_eq!(
        copy_image_into_update_slot(&mut flash, slots, slots.recovery),
        Ok(ImageCopy::Copied)
    );
    assert_eq!(flash.bytes_at(slots.update.start, recovery.len()), &recovery[..]);
}

#[test]
fn a_damaged_recovery_image_leaves_the_update_slot_alone() {
    let mut flash = FakeFlash::erased(EXTERNAL_FLASH_SIZE);
    let mut recovery = image(image_header::ImageType::Recovery, 5000);
    recovery[4000] ^= 1;
    flash.put(DEFAULT_SLOTS.recovery.start, &recovery);
    flash.put(DEFAULT_SLOTS.update.start, &[0; 64]);

    let result = copy_image_into_update_slot(&mut flash, DEFAULT_SLOTS, DEFAULT_SLOTS.recovery);

    assert_eq!(result, Ok(ImageCopy::NoImage));
    assert_eq!(flash.erase_count, 0);
    assert_eq!(flash.bytes_at(DEFAULT_SLOTS.update.start, 64), &[0; 64]);
}

#[test]
fn a_first_layout_recovery_image_is_not_installed() {
    let mut flash = FakeFlash::erased(EXTERNAL_FLASH_SIZE);
    // What the first layout's recovery slot held: a bare vector table for 0x5000, with no header.
    flash.put(
        DEFAULT_SLOTS.recovery.start,
        &[0x00, 0x00, 0x01, 0x20, 0x01, 0x51, 0x00, 0x00],
    );

    let result = copy_image_into_update_slot(&mut flash, DEFAULT_SLOTS, DEFAULT_SLOTS.recovery);

    assert_eq!(result, Ok(ImageCopy::NoImage));
}

#[test]
fn moves_the_previous_firmware_down_a_page_to_the_start_of_the_update_slot() {
    let mut flash = FakeFlash::erased(EXTERNAL_FLASH_SIZE);
    // Filling the whole slot, with an erased stretch, as a real previous firmware might.
    let mut previous = firmware_image(FIRMWARE.size as usize - image_header::IMAGE_HEADER_SIZE as usize);
    previous[6000..9000].fill(0xFF);
    let code_crc = crate::migration::crc32(crate::migration::CRC32_INITIAL, &previous[256..]);
    let mut header = image_header::ImageHeader::decode(&previous[..256].try_into().unwrap()).unwrap();
    header.image_crc32 = code_crc;
    previous[..256].copy_from_slice(&header.encode());
    flash.put(DEFAULT_SLOTS.previous_firmware().start, &previous);

    let result = copy_previous_into_update_slot(&mut flash, DEFAULT_SLOTS);

    assert_eq!(result, Ok(ImageCopy::Copied));
    assert_eq!(
        flash.bytes_at(DEFAULT_SLOTS.update.start, previous.len()),
        &previous[..]
    );
    assert!(flash
        .bytes_at(DEFAULT_SLOTS.update.end() - PAGE_SIZE, PAGE_SIZE as usize)
        .iter()
        .all(|byte| *byte == 0xFF));
}

#[test]
fn a_rollback_leaves_the_recovery_firmware_alone() {
    let mut flash = FakeFlash::erased(EXTERNAL_FLASH_SIZE);
    flash.put(DEFAULT_SLOTS.previous_firmware().start, &firmware_image(100));
    flash.put(DEFAULT_SLOTS.recovery.start, &[0x42; 64]);

    copy_previous_into_update_slot(&mut flash, DEFAULT_SLOTS).unwrap();

    assert_eq!(flash.bytes_at(DEFAULT_SLOTS.recovery.start, 64), &[0x42; 64]);
}

#[test]
fn a_rollback_without_a_previous_firmware_changes_nothing() {
    let mut flash = FakeFlash::erased(EXTERNAL_FLASH_SIZE);
    // An update waiting at the start of the slot, which a page up has no header.
    flash.put(DEFAULT_SLOTS.update.start, &firmware_image(100));

    let result = copy_previous_into_update_slot(&mut flash, DEFAULT_SLOTS);

    assert_eq!(result, Ok(ImageCopy::NoImage));
    assert_eq!(flash.erase_count, 0);
}

// ============================================================================
// Moving the recovery slot
// ============================================================================

mod recovery_moves {
    use super::*;

    fn flash_with_recovery(code_size: usize) -> (FakeFlash, Vec<u8>) {
        let mut flash = FakeFlash::erased(EXTERNAL_FLASH_SIZE);
        let recovery = image(image_header::ImageType::Recovery, code_size);
        flash.put(DEFAULT_SLOTS.recovery.start, &recovery);
        (flash, recovery)
    }

    fn region(start: u32, size: u32) -> Region {
        Region { start, size }
    }

    #[test]
    fn shrinking_in_place_keeps_the_image_where_it_is() {
        let (mut flash, _) = flash_with_recovery(5000);

        let result = move_recovery(
            &mut flash,
            DEFAULT_SLOTS.recovery,
            region(0xC_6000, 16 * PAGE_SIZE),
        );

        assert_eq!(result, Ok(RecoveryMove::KeptInPlace));
        assert_eq!(flash.erase_count, 0);
    }

    #[test]
    fn a_move_to_free_space_copies_the_image_and_leaves_the_old_one() {
        let (mut flash, recovery) = flash_with_recovery(9000);

        let result = move_recovery(
            &mut flash,
            DEFAULT_SLOTS.recovery,
            region(0x7_9000, 16 * PAGE_SIZE),
        );

        assert_eq!(result, Ok(RecoveryMove::Copied));
        assert_eq!(flash.bytes_at(0x7_9000, recovery.len()), &recovery[..]);
        assert_eq!(
            flash.bytes_at(DEFAULT_SLOTS.recovery.start, recovery.len()),
            &recovery[..]
        );
    }

    #[test]
    fn an_image_larger_than_the_new_slot_stays_put() {
        let (mut flash, _) = flash_with_recovery(70_000);

        let result = move_recovery(
            &mut flash,
            DEFAULT_SLOTS.recovery,
            region(0x7_9000, 16 * PAGE_SIZE),
        );

        assert_eq!(result, Ok(RecoveryMove::ImageTooLarge));
        assert!(!result.unwrap().may_switch());
        assert_eq!(flash.erase_count, 0);
    }

    #[test]
    fn a_move_onto_part_of_the_old_slot_is_refused() {
        let (mut flash, _) = flash_with_recovery(5000);

        let result = move_recovery(
            &mut flash,
            DEFAULT_SLOTS.recovery,
            region(0xC_7000, 16 * PAGE_SIZE),
        );

        assert_eq!(result, Ok(RecoveryMove::Overlapping));
        assert_eq!(flash.erase_count, 0);
    }

    #[test]
    fn an_empty_slot_moves_freely() {
        let mut flash = FakeFlash::erased(EXTERNAL_FLASH_SIZE);

        let result = move_recovery(
            &mut flash,
            DEFAULT_SLOTS.recovery,
            region(0xC_7000, 16 * PAGE_SIZE),
        );

        assert_eq!(result, Ok(RecoveryMove::NoImage));
        assert!(result.unwrap().may_switch());
    }
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
        let (steps, count) = plan(sizes(Some(DEFAULT_SLOTS.recovery.size)));

        for step in &steps[..count] {
            let slot = match step {
                Step::StageRecovery { .. } => DEFAULT_SLOTS.recovery,
                Step::StageFirmware { .. } => DEFAULT_SLOTS.update,
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
            sizes(Some(DEFAULT_SLOTS.recovery.size + 1)).check(),
            Err(PayloadProblem::RecoveryTooLarge)
        );
    }

    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(CRC32_INITIAL, b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(crc32(CRC32_INITIAL, b"1234"), b"56789"), 0xCBF4_3926);
    }
}

mod boot_note {
    use crate::BootNote;

    #[test]
    fn a_note_reads_back_as_itself() {
        for note in [BootNote::Normal, BootNote::RecoveryInstalled] {
            assert_eq!(BootNote::from_word(note.to_word()), Some(note));
        }
    }

    #[test]
    fn power_on_noise_is_not_a_note() {
        for word in [0, u32::MAX, 0xB007_0000, 0xB007_0003, 0x0000_0002] {
            assert_eq!(BootNote::from_word(word), None);
        }
    }

    #[test]
    fn the_note_sits_in_the_words_both_linkers_keep_free() {
        assert!(crate::FIRMWARE_ALIVE_MARKER.contains(crate::BOOT_NOTE_ADDRESS));
        assert!(crate::FIRMWARE_ALIVE_MARKER.contains(crate::BOOT_NOTE_ADDRESS + 3));
    }
}

mod updater_note {
    use crate::{BootNote, UpdaterNote};

    #[test]
    fn a_note_reads_back_as_itself() {
        for note in [
            UpdaterNote::BootloaderWritten,
            UpdaterNote::BootloaderAlreadyCurrent,
        ] {
            assert_eq!(UpdaterNote::from_word(note.to_word()), Some(note));
        }
    }

    #[test]
    fn the_bootloader_note_is_not_an_updater_note() {
        assert_eq!(UpdaterNote::from_word(BootNote::Normal.to_word()), None);
        assert_eq!(UpdaterNote::from_word(0), None);
        assert_eq!(UpdaterNote::from_word(u32::MAX), None);
    }

    #[test]
    fn it_sits_in_the_last_word_the_linkers_keep_free_and_apart_from_the_boot_note() {
        assert!(crate::FIRMWARE_ALIVE_MARKER.contains(crate::UPDATER_NOTE_ADDRESS + 3));
        assert_ne!(crate::UPDATER_NOTE_ADDRESS, crate::BOOT_NOTE_ADDRESS);
    }
}

mod bootloader_image_check {
    use super::{vec, FakeFlash};
    use crate::{
        bootloader_identity, is_bootloader_vector_table,
        migration::{crc32, CRC32_INITIAL},
        BOOTLOADER, FIRMWARE_VECTOR_TABLE, PAGE_SIZE,
    };

    fn vector_table(stack_pointer: u32, reset_handler: u32) -> [u8; 8] {
        let mut start = [0; 8];
        start[..4].copy_from_slice(&stack_pointer.to_le_bytes());
        start[4..].copy_from_slice(&reset_handler.to_le_bytes());
        start
    }

    #[test]
    fn a_bootloader_starts_inside_the_bootloader_region() {
        assert!(is_bootloader_vector_table(vector_table(0x2001_0000, 0x0000_0101)));
    }

    #[test]
    fn a_firmware_is_not_a_bootloader() {
        assert!(!is_bootloader_vector_table(vector_table(
            0x2000_FFF0,
            FIRMWARE_VECTOR_TABLE + 0xDD
        )));
    }

    #[test]
    fn erased_flash_is_not_a_bootloader() {
        assert!(!is_bootloader_vector_table([0xFF; 8]));
    }

    #[test]
    fn the_identity_is_the_crc_of_the_image_without_its_erased_tail() {
        let mut image = vector_table(0x2001_0000, 0x0000_0101).to_vec();
        // Longer than one read and ending past a chunk boundary, with an erased byte inside it.
        image.extend((0..700).map(|index| if index == 300 { 0xFF } else { index as u8 | 1 }));
        let mut flash = FakeFlash::erased(BOOTLOADER.size + PAGE_SIZE);
        flash.put(BOOTLOADER.start, &image);

        assert_eq!(
            bootloader_identity(&mut flash),
            Ok(Some(crc32(CRC32_INITIAL, &image)))
        );
    }

    #[test]
    fn a_bootloader_filling_its_whole_region_is_read_to_the_end() {
        let mut image = vec![0x5A; BOOTLOADER.size as usize];
        image[..8].copy_from_slice(&vector_table(0x2001_0000, 0x0000_0101));
        let mut flash = FakeFlash::erased(BOOTLOADER.size);
        flash.put(BOOTLOADER.start, &image);

        assert_eq!(
            bootloader_identity(&mut flash),
            Ok(Some(crc32(CRC32_INITIAL, &image)))
        );
    }

    #[test]
    fn a_region_without_a_bootloader_has_no_identity() {
        assert_eq!(
            bootloader_identity(&mut FakeFlash::erased(BOOTLOADER.size)),
            Ok(None)
        );
    }
}

// ============================================================================
// Version text
// ============================================================================

mod version {
    use super::super::version::*;

    use super::std::string::String;

    fn version_of(package_version: &str, commit: Option<BuildCommit<'_>>) -> String {
        let mut text = String::new();
        write_version(&mut text, package_version, commit).unwrap();
        text
    }

    #[test]
    fn a_full_hash_is_cut_to_seven_digits() {
        let commit = read_commit("1b19aed0c4f2a8e3d5b6c7a8f9e0d1c2b3a4f5e6\n").unwrap();

        assert_eq!(commit.short_hash, "1b19aed");
        assert!(!commit.has_uncommitted_changes);
    }

    #[test]
    fn nix_dirty_short_rev_marks_uncommitted_changes() {
        let commit = read_commit("1b19aed-dirty").unwrap();

        assert_eq!(commit.short_hash, "1b19aed");
        assert!(commit.has_uncommitted_changes);
    }

    #[test]
    fn text_that_is_not_a_hash_gives_no_commit() {
        assert_eq!(read_commit(""), None);
        assert_eq!(read_commit("unknown"), None);
        assert_eq!(read_commit("1b19ae"), None);
        assert_eq!(read_commit("-dirty"), None);
        assert_eq!(read_commit("1b19aez"), None);
    }

    #[test]
    fn the_version_joins_the_package_version_and_the_commit() {
        let commit = read_commit("1b19aed").unwrap();

        assert_eq!(version_of("0.1.0", Some(commit)), "0.1.0+1b19aed");
    }

    #[test]
    fn uncommitted_changes_add_a_star() {
        let commit = read_commit("1b19aed-dirty").unwrap();

        assert_eq!(version_of("0.1.0", Some(commit)), "0.1.0+1b19aed*");
    }

    #[test]
    fn an_unknown_commit_says_so() {
        assert_eq!(version_of("0.1.0", None), "0.1.0+unknown");
    }

    fn date_of(unix_milliseconds: u64) -> String {
        let mut text = String::new();
        write_date(&mut text, unix_milliseconds).unwrap();
        text
    }

    #[test]
    fn a_date_is_written_as_year_month_and_day() {
        const DAY: u64 = 24 * 60 * 60 * 1000;
        assert_eq!(date_of(0), "1970-01-01");
        assert_eq!(date_of(1_791_946_182_482), "2026-10-14");
        assert_eq!(date_of(11_016 * DAY), "2000-02-29");
        assert_eq!(date_of(11_017 * DAY - 1), "2000-02-29");
        assert_eq!(date_of(20_819 * DAY), "2027-01-01");
        assert_eq!(date_of(20_819 * DAY - 1), "2026-12-31");
    }

    #[test]
    fn a_version_record_is_found_anywhere_in_the_region_and_read_back() {
        let record = VersionRecord::new("0.1.0+1b19aed*", "2026-10-13");
        for place in [0, 4, 212, 216, 300, 0x5000 - 44] {
            let mut flash = super::FakeFlash::erased(0x5000);
            flash.put(place, &record.bytes());

            let found = VersionRecord::find(&mut flash, crate::BOOTLOADER)
                .unwrap()
                .unwrap();

            assert_eq!(found.version(), "0.1.0+1b19aed*", "at {place}");
            assert_eq!(found.date(), "2026-10-13");
        }
    }

    #[test]
    fn a_bootloader_without_a_whole_aligned_record_has_none() {
        let mut flash = super::FakeFlash::erased(0x5000);
        flash.put(100, &VERSION_RECORD_MAGIC[..7]);
        flash.put(201, &VersionRecord::new("0.1.0", "2026-10-13").bytes());

        assert_eq!(VersionRecord::find(&mut flash, crate::BOOTLOADER), Ok(None));
    }

    #[test]
    fn a_record_cuts_text_too_long_and_reads_nothing_from_bad_bytes() {
        let record = VersionRecord::new("0.1.0+1b19aed and a great deal more", "2026-10-13T12:00");

        assert_eq!(record.version().len(), RECORD_VERSION_BYTES);
        assert_eq!(record.date(), "2026-10-13");

        let mut bytes = record.bytes();
        bytes[VERSION_RECORD_MAGIC.len()] = 0xFF;
        let mut flash = super::FakeFlash::erased(0x5000);
        flash.put(0, &bytes);
        let found = VersionRecord::find(&mut flash, crate::BOOTLOADER)
            .unwrap()
            .unwrap();
        assert_eq!(found.version(), "");
    }

    #[test]
    fn the_longest_version_fits_the_status_page_title() {
        const STATUS_LINE_CHARACTERS: usize = 22;
        let commit = read_commit("1b19aed-dirty").unwrap();

        let title_length = "NeoTime ".len() + version_of("0.1.0", Some(commit)).len();

        assert!(title_length <= STATUS_LINE_CHARACTERS);
    }
}
