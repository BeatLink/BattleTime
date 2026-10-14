//! The standard Device Information service's text, the same set InfiniTime serves.

/// The Manufacturer Name String (0x2A29).
pub const MANUFACTURER_NAME: &str = "PINE64";

/// The Model Number String (0x2A24).
pub const MODEL_NUMBER: &str = "PineTime";

/// The Hardware Revision String (0x2A27), which InfiniTime also reports for every PineTime.
pub const HARDWARE_REVISION: &str = "1.0.0";

/// The Software Revision String (0x2A28): the firmware's name, as InfiniTime gives its own.
pub const SOFTWARE_REVISION: &str = "NeoTime";

/// The Serial Number String (0x2A25): the chip's 64-bit factory id as 16 upper-case hex digits.
pub fn serial_number(device_id: u64) -> [u8; 16] {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    let mut text = [0; 16];
    for (place, digit) in text.iter_mut().enumerate() {
        let shift = 60 - 4 * place;
        *digit = DIGITS[((device_id >> shift) & 0xF) as usize];
    }
    text
}
