extern crate std;

use std::{vec, vec::Vec};

use super::*;

// ============================================================================
// A module file built by hand
// ============================================================================

const RAM_ADDRESS: u32 = 0x2000_1000;

/// An image of eight bytes of code and eight of data, whose data holds a code and a zeroed-data address.
fn module_file(relocations: &[u32]) -> Vec<u8> {
    let mut image = vec![0xAA; 8];
    image.extend_from_slice(&5u32.to_le_bytes());
    image.extend_from_slice(&16u32.to_le_bytes());
    file_holding(&image, relocations)
}

/// A module file of `image`, its first eight bytes code, with eight bytes of zeroed data.
fn file_holding(image: &[u8], relocations: &[u32]) -> Vec<u8> {
    let mut file = Vec::new();
    file.extend_from_slice(&MAGIC);
    file.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    file.extend_from_slice(&ABI_VERSION.to_le_bytes());
    let mut name = [0u8; NAME_BYTES];
    name[..4].copy_from_slice(b"test");
    file.extend_from_slice(&name);
    for word in [image.len() as u32, 8, 8, 1, relocations.len() as u32, 0, 0] {
        file.extend_from_slice(&word.to_le_bytes());
    }
    file.extend_from_slice(image);
    for relocation in relocations {
        file.extend_from_slice(&relocation.to_le_bytes());
    }
    let crc = file_crc(&file);
    file[CRC_AT..CRC_AT + 4].copy_from_slice(&crc.to_le_bytes());
    file
}

fn ram_holding(file: &[u8]) -> Vec<u8> {
    let mut ram = file.to_vec();
    ram.resize(file.len().max(64), 0x55);
    ram
}

fn word_at(ram: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(ram[at..at + 4].try_into().unwrap())
}

// ============================================================================
// Reading the header
// ============================================================================

#[test]
fn reads_the_header_a_packaged_module_carries() {
    let file = module_file(&[8, 12]);

    let header = ModuleHeader::parse(&file).unwrap();

    assert_eq!(header.name(), "test");
    assert_eq!(header.ram_bytes(), 24);
    assert_eq!(header.file_bytes(), file.len());
}

#[test]
fn its_stack_follows_its_ram_on_an_alignment_boundary() {
    let mut file = module_file(&[]);
    file[STACK_BYTES_AT..STACK_BYTES_AT + 4].copy_from_slice(&1024u32.to_le_bytes());

    let header = ModuleHeader::parse(&file).unwrap();

    assert_eq!(header.stack_at(), 24);
    assert_eq!(header.running_bytes(), 24 + 1024);
}

#[test]
fn refuses_a_stack_too_large_or_out_of_line() {
    for stack_bytes in [MOST_STACK_BYTES + 8, 1020] {
        let mut file = module_file(&[]);
        file[STACK_BYTES_AT..STACK_BYTES_AT + 4].copy_from_slice(&stack_bytes.to_le_bytes());

        assert_eq!(ModuleHeader::parse(&file), Err(ModuleError::BadStack));
    }
}

#[test]
fn refuses_a_module_built_for_another_abi() {
    let mut file = module_file(&[]);
    file[ABI_AT] ^= 0xFF;

    assert_eq!(ModuleHeader::parse(&file), Err(ModuleError::OtherAbi));
}

#[test]
fn refuses_an_entry_outside_the_code_or_without_the_thumb_bit() {
    let mut file = module_file(&[]);
    file[ENTRY_AT] = 9;
    assert_eq!(ModuleHeader::parse(&file), Err(ModuleError::BadEntry));

    file[ENTRY_AT] = 2;
    assert_eq!(ModuleHeader::parse(&file), Err(ModuleError::BadEntry));
}

// ============================================================================
// Placing it
// ============================================================================

#[test]
fn moves_code_addresses_by_the_code_alias_and_data_addresses_by_the_ram() {
    let file = module_file(&[8, 12]);
    let mut ram = ram_holding(&file);

    place(&mut ram, file.len(), RAM_ADDRESS).unwrap();

    assert_eq!(&ram[..8], &[0xAA; 8]);
    assert_eq!(word_at(&ram, 8), 0x0080_1000 + 5);
    assert_eq!(word_at(&ram, 12), RAM_ADDRESS + 16);
}

#[test]
fn clears_the_zeroed_data_after_the_image() {
    let file = module_file(&[8, 12]);
    let mut ram = ram_holding(&file);

    place(&mut ram, file.len(), RAM_ADDRESS).unwrap();

    assert_eq!(&ram[16..24], &[0; 8]);
}

#[test]
fn refuses_a_damaged_file() {
    let mut file = module_file(&[8]);
    file[HEADER_BYTES] ^= 1;
    let mut ram = ram_holding(&file);

    assert_eq!(
        place(&mut ram, file.len(), RAM_ADDRESS),
        Err(ModuleError::Damaged)
    );
}

#[test]
fn refuses_a_relocation_outside_the_image_or_out_of_line() {
    for relocation in [16, 6] {
        let file = module_file(&[relocation]);
        let mut ram = ram_holding(&file);

        assert_eq!(
            place(&mut ram, file.len(), RAM_ADDRESS),
            Err(ModuleError::BadRelocation)
        );
    }
}

// ============================================================================
// Addresses split over a MOVW and a MOVT
// ============================================================================

#[test]
fn reads_and_writes_the_immediate_of_a_movw() {
    // `movw r0, #0x1234`, as an assembler writes it.
    let mut image = vec![0x41, 0xF2, 0x34, 0x20];

    assert_eq!(move_immediate(&image, 0, MOVW), Some(0x1234));
    set_move_immediate(&mut image, 0, 0xBEEF);
    assert_eq!(move_immediate(&image, 0, MOVW), Some(0xBEEF));
    assert_eq!(move_immediate(&image, 0, MOVT), None);
}

#[test]
fn moves_an_address_split_over_a_movw_and_a_movt() {
    // `movw r0, #16` and `movt r0, #0`, loading the zeroed data's address.
    let mut image = vec![0x40, 0xF2, 0x10, 0x00, 0xC0, 0xF2, 0x00, 0x00];
    image.extend_from_slice(&[0; 8]);
    let file = file_holding(&image, &[MOVE_PAIR_BIT, 4]);
    let mut ram = ram_holding(&file);

    place(&mut ram, file.len(), RAM_ADDRESS).unwrap();

    let low = move_immediate(&ram, 0, MOVW).unwrap() as u32;
    let high = move_immediate(&ram, 4, MOVT).unwrap() as u32;
    assert_eq!(high << 16 | low, RAM_ADDRESS + 16);
}

#[test]
fn refuses_a_move_pair_that_is_not_a_movw_and_a_movt() {
    let file = module_file(&[MOVE_PAIR_BIT, 4]);
    let mut ram = ram_holding(&file);

    assert_eq!(
        place(&mut ram, file.len(), RAM_ADDRESS),
        Err(ModuleError::BadRelocation)
    );
}
