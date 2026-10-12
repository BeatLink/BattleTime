extern crate std;

use std::{string::ToString, vec::Vec};

use super::*;

// ============================================================================
// Reset reasons
// ============================================================================

#[test]
fn no_reset_reason_means_the_power_came_on() {
    let reasons = ResetReasons::from_register(0);

    assert!(reasons.is_power_on());
    assert_eq!(reasons.to_string(), "power on");
    assert_eq!(reasons.names().count(), 0);
}

#[test]
fn each_reset_reason_bit_has_its_name() {
    let expected = [
        (1 << 0, "reset pin"),
        (1 << 1, "watchdog"),
        (1 << 2, "software"),
        (1 << 3, "lockup"),
        (1 << 16, "woke by pin"),
        (1 << 17, "woke by comparator"),
        (1 << 18, "debugger"),
        (1 << 19, "woke by NFC"),
    ];

    for (bit, name) in expected {
        let reasons = ResetReasons::from_register(bit);
        assert!(!reasons.is_power_on());
        assert_eq!(reasons.to_string(), name);
    }
}

#[test]
fn several_reset_reasons_are_listed_most_telling_first() {
    let reasons = ResetReasons::from_register(1 << 0 | 1 << 1 | 1 << 2);

    assert_eq!(reasons.to_string(), "watchdog, software, reset pin");
}

#[test]
fn bits_the_chip_does_not_define_read_as_unknown() {
    let reasons = ResetReasons::from_register(1 << 1 | 1 << 31);

    assert_eq!(reasons.names().collect::<Vec<_>>(), ["watchdog", "unknown"]);
    assert_eq!(ResetReasons::from_register(1 << 8).to_string(), "unknown");
}

// ============================================================================
// Return addresses on the stack
// ============================================================================

const CODE: core::ops::Range<u32> = 0x5000..0x6_0000;

#[test]
fn only_odd_words_inside_the_code_count_as_return_addresses() {
    let stack = [
        0x2000_1234,
        0x0000_5101,
        0x0000_5100,
        0x0007_0001,
        3,
        0x0001_A2B5,
        0x0000_4FFF,
    ];

    let found = find_return_addresses(stack, CODE);

    assert_eq!(found, [0x5101, 0x1_A2B5, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn only_the_nearest_return_addresses_are_kept() {
    let stack = (0..20).map(|index| 0x5001 + index * 2);

    let found = find_return_addresses(stack, CODE);

    assert_eq!(
        found,
        [0x5001, 0x5003, 0x5005, 0x5007, 0x5009, 0x500B, 0x500D, 0x500F]
    );
}

// ============================================================================
// The crash record
// ============================================================================

fn panic() -> Crash {
    Crash::Panic {
        return_addresses: [0x5101, 0x1_A2B5, 0x2_0001, 0, 0, 0, 0, 0],
    }
}

fn hard_fault() -> Crash {
    Crash::HardFault {
        program_counter: 0x0001_A2B4,
        link_register: 0x0001_0F03,
        fault_status: 0x0001_0000,
        fault_address: 0xE000_0000,
    }
}

#[test]
fn a_panic_survives_encoding_and_decoding() {
    let record = panic().encode();

    assert_eq!(Crash::decode(&record), Some(panic()));
    assert_eq!(record.len(), 44);
    assert_eq!(&record[..4], b"CRSH");
    assert_eq!(record[4..8], [1, 0, 0, 0]);
    assert_eq!(record[8..12], [0x01, 0x51, 0, 0]);
}

#[test]
fn a_hard_fault_survives_encoding_and_decoding() {
    let record = hard_fault().encode();

    assert_eq!(Crash::decode(&record), Some(hard_fault()));
    assert_eq!(record[4..8], [2, 0, 0, 0]);
    assert_eq!(record[8..12], [0xB4, 0xA2, 0x01, 0x00]);
}

#[test]
fn the_checksum_is_the_crc32_of_everything_before_it() {
    let record = hard_fault().encode();

    let checksum = crc32(CRC32_INITIAL, &record[..RECORD_SIZE - 4]);

    assert_eq!(record[RECORD_SIZE - 4..], checksum.to_le_bytes());
}

#[test]
fn a_cleared_record_holds_no_crash() {
    assert_eq!(Crash::decode(&[0; RECORD_SIZE]), None);
    assert_eq!(Crash::decode(&[0xFF; RECORD_SIZE]), None);
}

#[test]
fn any_changed_byte_makes_the_record_invalid() {
    let record = panic().encode();

    for index in 0..RECORD_SIZE {
        let mut damaged = record;
        damaged[index] ^= 0x10;
        assert_eq!(Crash::decode(&damaged), None, "byte {index} was changed");
    }
}

#[test]
fn an_unknown_kind_is_not_a_crash() {
    let mut record = [0; RECORD_SIZE];
    record[..4].copy_from_slice(b"CRSH");
    record[4] = 7;
    let checksum = crc32(CRC32_INITIAL, &record[..RECORD_SIZE - 4]);
    record[RECORD_SIZE - 4..].copy_from_slice(&checksum.to_le_bytes());

    assert_eq!(Crash::decode(&record), None);
}

#[test]
fn crashes_read_as_one_short_line() {
    assert_eq!(panic().to_string(), "Last run panicked");
    assert_eq!(hard_fault().to_string(), "Fault at 0x0001a2b4");
}

// ============================================================================
// What the diagnostics service sends
// ============================================================================

#[test]
fn the_last_restart_is_the_reasons_the_kind_then_the_details() {
    let reasons = ResetReasons::from_register(1 << 1);

    let words = last_restart_words(reasons, Some(&hard_fault()));

    assert_eq!(
        words,
        [
            2,
            2,
            0x0001_A2B4,
            0x0001_0F03,
            0x0001_0000,
            0xE000_0000,
            0,
            0,
            0,
            0
        ]
    );
}

#[test]
fn a_panic_sends_all_its_return_addresses() {
    let words = last_restart_words(ResetReasons::from_register(1 << 1), Some(&panic()));

    assert_eq!(words, [2, 1, 0x5101, 0x1_A2B5, 0x2_0001, 0, 0, 0, 0, 0]);
}

#[test]
fn without_a_crash_only_the_reasons_are_set() {
    let words = last_restart_words(ResetReasons::from_register(1 << 2), None);

    assert_eq!(words, [4, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
}
