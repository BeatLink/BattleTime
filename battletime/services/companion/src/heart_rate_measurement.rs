//! The standard Heart Rate service's measurement, in the two-byte form InfiniTime sends.

/// The flags byte with every bit clear: the rate is one byte, with no contact, energy or RR fields.
const BEATS_PER_MINUTE_AS_ONE_BYTE: u8 = 0;

/// The Heart Rate Measurement (0x2A37): the flags, then the rate, held at 255 since it is one byte.
pub fn heart_rate_measurement(beats_per_minute: u32) -> [u8; 2] {
    let rate = beats_per_minute.min(u32::from(u8::MAX)) as u8;
    [BEATS_PER_MINUTE_AS_ONE_BYTE, rate]
}
