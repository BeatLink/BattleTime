//! CRC-32 (IEEE), fed a piece at a time so data never has to be in RAM all at once.

#[derive(Clone, Copy, Debug)]
pub(crate) struct Checksum {
    state: u32,
}

const POLYNOMIAL: u32 = 0xedb8_8320;

/// The remainder after four shifted rounds for each nibble, so a byte costs two lookups instead of eight rounds.
const NIBBLE_TABLE: [u32; 16] = nibble_table();

const fn nibble_table() -> [u32; 16] {
    let mut table = [0u32; 16];
    let mut nibble = 0;
    while nibble < 16 {
        let mut remainder = nibble as u32;
        let mut round = 0;
        while round < 4 {
            remainder = (remainder >> 1) ^ (POLYNOMIAL & (remainder & 1).wrapping_neg());
            round += 1;
        }
        table[nibble] = remainder;
        nibble += 1;
    }
    table
}

impl Checksum {
    pub(crate) fn new() -> Checksum {
        Checksum { state: u32::MAX }
    }

    pub(crate) fn add(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= *byte as u32;
            self.state = (self.state >> 4) ^ NIBBLE_TABLE[(self.state & 0xf) as usize];
            self.state = (self.state >> 4) ^ NIBBLE_TABLE[(self.state & 0xf) as usize];
        }
    }

    pub(crate) fn value(self) -> u32 {
        !self.state
    }

    pub(crate) fn of(bytes: &[u8]) -> u32 {
        let mut checksum = Checksum::new();
        checksum.add(bytes);
        checksum.value()
    }
}

/// FNV-1a, for finding a path without keeping it in RAM; the full path stored beside the data settles any clash.
pub(crate) fn path_hash(path: &str) -> u32 {
    const OFFSET_BASIS: u32 = 0x811c_9dc5;
    const PRIME: u32 = 0x0100_0193;
    path.bytes().fold(OFFSET_BASIS, |hash, byte| {
        (hash ^ byte as u32).wrapping_mul(PRIME)
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::Checksum;

    const POLYNOMIAL: u32 = 0xedb8_8320;

    fn bit_serial(bytes: &[u8]) -> u32 {
        let mut state = u32::MAX;
        for byte in bytes {
            state ^= *byte as u32;
            for _ in 0..8 {
                state = (state >> 1) ^ (POLYNOMIAL & (state & 1).wrapping_neg());
            }
        }
        !state
    }

    #[test]
    fn matches_the_standard_check_value() {
        assert_eq!(Checksum::of(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn matches_the_bit_serial_form_on_every_byte_value() {
        let every_byte: [u8; 256] = core::array::from_fn(|index| index as u8);
        assert_eq!(Checksum::of(&every_byte), bit_serial(&every_byte));
        let mut piecewise = Checksum::new();
        piecewise.add(&every_byte[..100]);
        piecewise.add(&every_byte[100..]);
        assert_eq!(piecewise.value(), bit_serial(&every_byte));
    }
}
