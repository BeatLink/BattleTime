//! CRC-32 (IEEE), fed a piece at a time so data never has to be in RAM all at once.

#[derive(Clone, Copy, Debug)]
pub(crate) struct Checksum {
    state: u32,
}

const POLYNOMIAL: u32 = 0xedb8_8320;

impl Checksum {
    pub(crate) fn new() -> Checksum {
        Checksum { state: u32::MAX }
    }

    pub(crate) fn add(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= *byte as u32;
            for _ in 0..8 {
                self.state = (self.state >> 1) ^ (POLYNOMIAL & (self.state & 1).wrapping_neg());
            }
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
