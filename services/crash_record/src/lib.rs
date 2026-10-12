//! Why the watch last restarted: the chip's reset reasons in words, and a record of the last panic or fault.
//!
//! The firmware reads and clears the nRF52832's `RESETREAS` register at start. When it panics or
//! takes a HardFault it writes a `Crash` into a 44-byte record in RAM that a reset keeps, and the
//! next start reads it back and clears it. A magic word and a CRC-32 tell a real record from
//! whatever RAM held after the power came on.

#![no_std]

use core::fmt;

use neotime_boot::migration::{crc32, CRC32_INITIAL};

// ============================================================================
// Reset reasons
// ============================================================================

/// Each bit of `RESETREAS` with a short name, most telling first.
const RESET_REASON_NAMES: [(u32, &str); 8] = [
    (1 << 1, "watchdog"),
    (1 << 3, "lockup"),
    (1 << 2, "software"),
    (1 << 0, "reset pin"),
    (1 << 16, "woke by pin"),
    (1 << 17, "woke by comparator"),
    (1 << 18, "debugger"),
    (1 << 19, "woke by NFC"),
];

const KNOWN_RESET_REASON_BITS: u32 = 0b1111 | 0b1111 << 16;

/// What `RESETREAS` held at start; the chip sets a bit for each kind of reset until it is cleared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResetReasons {
    pub register: u32,
}

impl ResetReasons {
    pub const fn from_register(register: u32) -> ResetReasons {
        ResetReasons { register }
    }

    /// No bit set means the power came on, or dropped low enough to reset the chip.
    pub const fn is_power_on(self) -> bool {
        self.register == 0
    }

    /// The name of each bit that is set, with "unknown" for bits the nRF52832 does not define.
    pub fn names(self) -> impl Iterator<Item = &'static str> {
        let unknown_bits = self.register & !KNOWN_RESET_REASON_BITS;
        RESET_REASON_NAMES
            .into_iter()
            .filter(move |(bit, _)| self.register & bit != 0)
            .map(|(_, name)| name)
            .chain((unknown_bits != 0).then_some("unknown"))
    }
}

impl fmt::Display for ResetReasons {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        if self.is_power_on() {
            return formatter.write_str("power on");
        }
        for (index, name) in self.names().enumerate() {
            if index > 0 {
                formatter.write_str(", ")?;
            }
            formatter.write_str(name)?;
        }
        Ok(())
    }
}

// ============================================================================
// Return addresses on the stack
// ============================================================================

/// How many code addresses a panic keeps from its stack, nearest first.
pub const PANIC_RETURN_ADDRESSES: usize = 8;

/// Picks out the words that look like Thumb return addresses into the code, nearest first; a stale word can pass too.
pub fn find_return_addresses(
    stack_words: impl IntoIterator<Item = u32>,
    code: core::ops::Range<u32>,
) -> [u32; PANIC_RETURN_ADDRESSES] {
    let mut found = [0; PANIC_RETURN_ADDRESSES];
    let candidates = stack_words
        .into_iter()
        .filter(|word| word & 1 == 1 && code.contains(word));
    for (slot, address) in found.iter_mut().zip(candidates) {
        *slot = address;
    }
    found
}

// ============================================================================
// The crash record
// ============================================================================

/// Each kind of crash fills up to this many words; a panic fills them all with return addresses.
pub const DETAIL_WORDS: usize = PANIC_RETURN_ADDRESSES;

/// The record is little-endian words: the magic, the kind, the detail words, then a CRC-32 of all before it.
pub const RECORD_SIZE: usize = 4 * (3 + DETAIL_WORDS);

/// "CRSH", so a record is easy to spot in a RAM dump.
const RECORD_MAGIC: u32 = 0x4853_5243;
const KIND_PANIC: u32 = 1;
const KIND_HARD_FAULT: u32 = 2;

const CHECKSUM_OFFSET: usize = RECORD_SIZE - 4;

/// How the previous run ended, when it ended badly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Crash {
    /// Code addresses found on the stack, nearest first, with 0 for none.
    Panic {
        return_addresses: [u32; PANIC_RETURN_ADDRESSES],
    },
    /// The instruction it stopped at and the return address, then the fault status register (CFSR) and the bus fault address register (BFAR).
    HardFault {
        program_counter: u32,
        link_register: u32,
        fault_status: u32,
        fault_address: u32,
    },
}

impl Crash {
    /// The number that names the kind of crash, and its fields as detail words.
    fn kind_and_details(&self) -> (u32, [u32; DETAIL_WORDS]) {
        match *self {
            Crash::Panic { return_addresses } => (KIND_PANIC, return_addresses),
            Crash::HardFault {
                program_counter,
                link_register,
                fault_status,
                fault_address,
            } => (
                KIND_HARD_FAULT,
                [
                    program_counter,
                    link_register,
                    fault_status,
                    fault_address,
                    0,
                    0,
                    0,
                    0,
                ],
            ),
        }
    }

    pub fn encode(&self) -> [u8; RECORD_SIZE] {
        let (kind, details) = self.kind_and_details();

        let mut record = [0; RECORD_SIZE];
        let words = [RECORD_MAGIC, kind].into_iter().chain(details);
        for (chunk, word) in record.as_chunks_mut::<4>().0.iter_mut().zip(words) {
            *chunk = word.to_le_bytes();
        }
        let checksum = crc32(CRC32_INITIAL, &record[..CHECKSUM_OFFSET]);
        record[CHECKSUM_OFFSET..].copy_from_slice(&checksum.to_le_bytes());
        record
    }

    /// The crash a record holds, or `None` for a cleared record or one RAM came up with.
    pub fn decode(record: &[u8; RECORD_SIZE]) -> Option<Crash> {
        let mut words = [0; RECORD_SIZE / 4];
        for (word, chunk) in words.iter_mut().zip(record.as_chunks::<4>().0) {
            *word = u32::from_le_bytes(*chunk);
        }
        let [magic, kind, details @ .., checksum] = words;
        if magic != RECORD_MAGIC || checksum != crc32(CRC32_INITIAL, &record[..CHECKSUM_OFFSET]) {
            return None;
        }

        match kind {
            KIND_PANIC => Some(Crash::Panic {
                return_addresses: details,
            }),
            KIND_HARD_FAULT => Some(Crash::HardFault {
                program_counter: details[0],
                link_register: details[1],
                fault_status: details[2],
                fault_address: details[3],
            }),
            _ => None,
        }
    }
}

/// One short line for the status page; the panic's addresses are too many for it and go over Bluetooth.
impl fmt::Display for Crash {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Crash::Panic { .. } => formatter.write_str("Last run panicked"),
            Crash::HardFault { program_counter, .. } => write!(formatter, "Fault at {program_counter:#010x}"),
        }
    }
}

// ============================================================================
// What the diagnostics service sends
// ============================================================================

pub const LAST_RESTART_WORDS: usize = 2 + DETAIL_WORDS;

/// The reset reasons, the kind of crash (0 for none, 1 a panic, 2 a HardFault), then its detail words.
pub fn last_restart_words(reasons: ResetReasons, crash: Option<&Crash>) -> [u32; LAST_RESTART_WORDS] {
    let (kind, details) = crash.map_or((0, [0; DETAIL_WORDS]), Crash::kind_and_details);

    let mut words = [0; LAST_RESTART_WORDS];
    words[0] = reasons.register;
    words[1] = kind;
    words[2..].copy_from_slice(&details);
    words
}

#[cfg(test)]
mod tests;
