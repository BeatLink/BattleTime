extern crate std;

use std::{vec, vec::Vec};

use super::*;

/// Splits advertising data into its pieces, each as its type and its bytes.
fn pieces(mut bytes: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut found = Vec::new();
    while let [length, rest @ ..] = bytes {
        let length = usize::from(*length);
        assert!(length >= 1 && length <= rest.len(), "a piece runs past the end");
        found.push((rest[0], rest[1..length].to_vec()));
        bytes = &rest[length..];
    }
    found
}

// ============================================================================
// The data
// ============================================================================

#[test]
fn the_advertisement_carries_what_infinitime_sends_and_fits() {
    assert!(ADVERTISEMENT.len() <= MOST_ADVERTISING_BYTES);
    assert_eq!(
        pieces(&ADVERTISEMENT),
        [
            (0x01, vec![0x06]),
            (0x03, vec![0x0D, 0x18]),
            (0x07, LEGACY_DFU_SERVICE.to_vec()),
            (0x0A, vec![0]),
        ]
    );
}

#[test]
fn the_dfu_service_is_written_little_endian() {
    let text = "00001530-1212-efde-1523-785feabcd123".replace('-', "");
    let mut big_endian: Vec<u8> = (0..16)
        .map(|place| u8::from_str_radix(&text[2 * place..2 * place + 2], 16).unwrap())
        .collect();
    big_endian.reverse();

    assert_eq!(big_endian, LEGACY_DFU_SERVICE);
}

#[test]
fn the_scan_response_holds_the_complete_name() {
    assert_eq!(
        pieces(&scan_response("BattleTime")),
        [(0x09, b"BattleTime".to_vec())]
    );
    assert_eq!(
        pieces(&scan_response(GADGETBRIDGE_NAME)),
        [(0x09, b"InfiniTime".to_vec())]
    );
}

#[test]
fn a_long_name_is_cut_to_fit_the_scan_response() {
    let response = scan_response("a name far longer than thirty-one bytes can hold");

    assert_eq!(response.len(), MOST_ADVERTISING_BYTES);
    assert_eq!(pieces(&response)[0].1.len(), MOST_NAME_BYTES);
}

// ============================================================================
// How often
// ============================================================================

#[test]
fn advertising_is_fast_for_thirty_seconds_then_about_once_a_second() {
    assert_eq!(advertising_interval(1_000, 1_000), FAST_INTERVAL);
    assert_eq!(advertising_interval(1_000, 30_999), FAST_INTERVAL);
    assert_eq!(advertising_interval(1_000, 31_000), SLOW_INTERVAL);
    assert_eq!(FAST_INTERVAL.shortest_in_microseconds, 20_000);
    assert!((1_000_000..1_040_000).contains(&SLOW_INTERVAL.shortest_in_microseconds));
}

#[test]
fn a_clock_behind_the_start_counts_as_fast() {
    assert_eq!(advertising_interval(5_000, 0), FAST_INTERVAL);
}

// The slow interval's range runs upwards, checked when the tests are built.
const _: () = assert!(SLOW_INTERVAL.shortest_in_microseconds < SLOW_INTERVAL.longest_in_microseconds);
