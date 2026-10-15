//! What the watch advertises and how often, as InfiniTime does, so phones find the two the same way.
//!
//! The advertisement carries the flags, the Heart Rate service, the legacy DFU service and the
//! transmit power, and the scan response carries the name. The watch advertises fast for 30 seconds
//! after it starts or a phone leaves, then about once a second.

// ============================================================================
// The name
// ============================================================================

/// The name Gadgetbridge recognises as InfiniTime, which the `gadgetbridge_name` setting advertises.
pub const GADGETBRIDGE_NAME: &str = "InfiniTime";

/// The longest name a scan response holds, after its length and type bytes.
pub const MOST_NAME_BYTES: usize = MOST_ADVERTISING_BYTES - 2;

// ============================================================================
// The data
// ============================================================================

/// The most bytes legacy advertising carries in an advertisement or a scan response.
pub const MOST_ADVERTISING_BYTES: usize = 31;

/// Each piece starts with its length and this type byte.
const FLAGS: u8 = 0x01;
const COMPLETE_SERVICE_UUIDS_16: u8 = 0x03;
const COMPLETE_SERVICE_UUIDS_128: u8 = 0x07;
const COMPLETE_LOCAL_NAME: u8 = 0x09;
const TRANSMIT_POWER_LEVEL: u8 = 0x0A;

/// General discoverable, and no classic Bluetooth.
const DISCOVERABLE_LOW_ENERGY_ONLY: u8 = 0x06;

/// The Heart Rate service, 0x180D, little-endian.
pub const HEART_RATE_SERVICE: [u8; 2] = [0x0D, 0x18];

/// The legacy DFU service, `00001530-1212-efde-1523-785feabcd123`, little-endian.
pub const LEGACY_DFU_SERVICE: [u8; 16] = [
    0x23, 0xD1, 0xBC, 0xEA, 0x5F, 0x78, 0x23, 0x15, 0xDE, 0xEF, 0x12, 0x12, 0x30, 0x15, 0x00, 0x00,
];

/// The radio sends at 0 dBm.
pub const TRANSMIT_POWER_IN_DBM: i8 = 0;

/// The advertisement, the same pieces InfiniTime sends: flags, the heart rate and DFU services, and the transmit power.
pub const ADVERTISEMENT: [u8; 28] = {
    let mut bytes = [0; 28];
    bytes[0] = 2;
    bytes[1] = FLAGS;
    bytes[2] = DISCOVERABLE_LOW_ENERGY_ONLY;
    bytes[3] = 3;
    bytes[4] = COMPLETE_SERVICE_UUIDS_16;
    bytes[5] = HEART_RATE_SERVICE[0];
    bytes[6] = HEART_RATE_SERVICE[1];
    bytes[7] = 17;
    bytes[8] = COMPLETE_SERVICE_UUIDS_128;
    let mut place = 0;
    while place < 16 {
        bytes[9 + place] = LEGACY_DFU_SERVICE[place];
        place += 1;
    }
    bytes[25] = 2;
    bytes[26] = TRANSMIT_POWER_LEVEL;
    bytes[27] = TRANSMIT_POWER_IN_DBM as u8;
    bytes
};

/// The scan response, which holds only the complete name, cut to fit.
pub fn scan_response(name: &str) -> heapless::Vec<u8, MOST_ADVERTISING_BYTES> {
    let name = &name.as_bytes()[..name.len().min(MOST_NAME_BYTES)];
    let mut bytes = heapless::Vec::new();
    let _ = bytes.extend_from_slice(&[name.len() as u8 + 1, COMPLETE_LOCAL_NAME]);
    let _ = bytes.extend_from_slice(name);
    bytes
}

// ============================================================================
// How often
// ============================================================================

/// How long the watch advertises fast after it starts, a phone leaves or Bluetooth comes back on.
pub const FAST_ADVERTISING_MILLISECONDS: u64 = 30_000;

/// The shortest and longest time between advertisements, in microseconds, as InfiniTime asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdvertisingInterval {
    pub shortest_in_microseconds: u64,
    pub longest_in_microseconds: u64,
}

/// 32 to 47 units of 625 µs, about 20 to 30 ms.
pub const FAST_INTERVAL: AdvertisingInterval = AdvertisingInterval {
    shortest_in_microseconds: 32 * 625,
    longest_in_microseconds: 47 * 625,
};

/// 1636 to 1651 units of 625 µs, about once a second.
pub const SLOW_INTERVAL: AdvertisingInterval = AdvertisingInterval {
    shortest_in_microseconds: 1636 * 625,
    longest_in_microseconds: 1651 * 625,
};

/// The interval for advertising that started fast at `fast_from`, `now` milliseconds in.
pub fn advertising_interval(fast_from_in_milliseconds: u64, now_in_milliseconds: u64) -> AdvertisingInterval {
    match now_in_milliseconds.saturating_sub(fast_from_in_milliseconds) < FAST_ADVERTISING_MILLISECONDS {
        true => FAST_INTERVAL,
        false => SLOW_INTERVAL,
    }
}

#[cfg(test)]
#[path = "advertising_tests.rs"]
mod advertising_tests;
