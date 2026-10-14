//! Why the watch last restarted: the chip's reset reasons in words, and a record of the last panic or fault.
//!
//! The firmware reads and clears the nRF52832's `RESETREAS` register at start. When it panics, runs
//! out of heap or takes a HardFault it writes a `RecordedCrash`, the crash and the app that was open,
//! into a 60-byte record in RAM that a reset keeps, and the next start reads it back. A magic word
//! and a CRC-32 tell a real record from whatever RAM held after the power came on. `screen` draws
//! the page the wearer sees after a crash, and when an app needs more memory than is free.

#![no_std]

use core::fmt;

pub mod screen;

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

/// Return addresses an allocation failure keeps, after the two sizes.
pub const OUT_OF_MEMORY_RETURN_ADDRESSES: usize = DETAIL_WORDS - 2;

/// Bytes of the open app's id the record keeps, padded with zeros.
pub const OPEN_APP_BYTES: usize = 16;
const OPEN_APP_WORDS: usize = OPEN_APP_BYTES / 4;

/// The record is little-endian words: the magic, the kind, the detail words, the open app's id, then a CRC-32 of all before it.
pub const RECORD_SIZE: usize = 4 * (3 + DETAIL_WORDS + OPEN_APP_WORDS);

/// "CRSH", so a record is easy to spot in a RAM dump.
const RECORD_MAGIC: u32 = 0x4853_5243;
const KIND_PANIC: u32 = 1;
const KIND_HARD_FAULT: u32 = 2;
const KIND_OUT_OF_MEMORY: u32 = 3;

const CHECKSUM_OFFSET: usize = RECORD_SIZE - 4;

/// The memory management bits of the fault status register, which only the stack guard raises.
const MEMORY_MANAGEMENT_FAULT_BITS: u32 = 0xFF;

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
    /// An allocation the heap could not give: its size, the heap's free bytes then, and code addresses on the stack.
    OutOfMemory {
        requested_bytes: u32,
        free_bytes: u32,
        return_addresses: [u32; OUT_OF_MEMORY_RETURN_ADDRESSES],
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
            Crash::OutOfMemory {
                requested_bytes,
                free_bytes,
                return_addresses,
            } => {
                let mut details = [0; DETAIL_WORDS];
                details[0] = requested_bytes;
                details[1] = free_bytes;
                details[2..].copy_from_slice(&return_addresses);
                (KIND_OUT_OF_MEMORY, details)
            }
        }
    }

    fn from_kind_and_details(kind: u32, details: [u32; DETAIL_WORDS]) -> Option<Crash> {
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
            KIND_OUT_OF_MEMORY => {
                let mut return_addresses = [0; OUT_OF_MEMORY_RETURN_ADDRESSES];
                return_addresses.copy_from_slice(&details[2..]);
                Some(Crash::OutOfMemory {
                    requested_bytes: details[0],
                    free_bytes: details[1],
                    return_addresses,
                })
            }
            _ => None,
        }
    }

    /// What went wrong in a few plain words, for the crash screen.
    pub fn cause(&self) -> &'static str {
        match self {
            Crash::Panic { .. } => "a software error",
            Crash::HardFault { fault_status, .. } if fault_status & MEMORY_MANAGEMENT_FAULT_BITS != 0 => {
                "out of stack space"
            }
            Crash::HardFault { .. } => "a processor fault",
            Crash::OutOfMemory { .. } => "out of memory",
        }
    }
}

/// One short line for the status page; the panic's addresses are too many for it and go over Bluetooth.
impl fmt::Display for Crash {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Crash::Panic { .. } => formatter.write_str("Last run panicked"),
            Crash::HardFault { program_counter, .. } => write!(formatter, "Fault at {program_counter:#010x}"),
            Crash::OutOfMemory { requested_bytes, .. } => {
                write!(formatter, "Out of memory: {requested_bytes} B")
            }
        }
    }
}

/// A crash and the id of the app that was open, which is all zeros when none was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordedCrash {
    pub crash: Crash,
    pub open_app: [u8; OPEN_APP_BYTES],
}

impl RecordedCrash {
    pub fn encode(&self) -> [u8; RECORD_SIZE] {
        let (kind, details) = self.crash.kind_and_details();

        let mut record = [0; RECORD_SIZE];
        let words = [RECORD_MAGIC, kind].into_iter().chain(details);
        for (chunk, word) in record.as_chunks_mut::<4>().0.iter_mut().zip(words) {
            *chunk = word.to_le_bytes();
        }
        let open_app_offset = 4 * (2 + DETAIL_WORDS);
        record[open_app_offset..CHECKSUM_OFFSET].copy_from_slice(&self.open_app);
        let checksum = crc32(CRC32_INITIAL, &record[..CHECKSUM_OFFSET]);
        record[CHECKSUM_OFFSET..].copy_from_slice(&checksum.to_le_bytes());
        record
    }

    /// The crash a record holds, or `None` for a cleared record or one RAM came up with.
    pub fn decode(record: &[u8; RECORD_SIZE]) -> Option<RecordedCrash> {
        let mut words = [0; RECORD_SIZE / 4];
        for (word, chunk) in words.iter_mut().zip(record.as_chunks::<4>().0) {
            *word = u32::from_le_bytes(*chunk);
        }
        let magic = words[0];
        let checksum = words[RECORD_SIZE / 4 - 1];
        if magic != RECORD_MAGIC || checksum != crc32(CRC32_INITIAL, &record[..CHECKSUM_OFFSET]) {
            return None;
        }

        let mut details = [0; DETAIL_WORDS];
        details.copy_from_slice(&words[2..2 + DETAIL_WORDS]);
        let mut open_app = [0; OPEN_APP_BYTES];
        open_app.copy_from_slice(&record[4 * (2 + DETAIL_WORDS)..CHECKSUM_OFFSET]);
        Some(RecordedCrash {
            crash: Crash::from_kind_and_details(words[1], details)?,
            open_app,
        })
    }

    /// The open app's id as text, empty when none was open or it is not text.
    pub fn open_app_id(&self) -> &str {
        let length = self
            .open_app
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(OPEN_APP_BYTES);
        core::str::from_utf8(&self.open_app[..length]).unwrap_or("")
    }
}

/// The first bytes of an app's id, padded with zeros, as the record keeps it.
pub fn open_app_bytes(id: &str) -> [u8; OPEN_APP_BYTES] {
    let mut bytes = [0; OPEN_APP_BYTES];
    let length = id.len().min(OPEN_APP_BYTES);
    bytes[..length].copy_from_slice(&id.as_bytes()[..length]);
    bytes
}

// ============================================================================
// Why the watch restarted, in words
// ============================================================================

/// The crash when the run before had one, or else the reset reasons, as the system info app shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestartReason<'crash> {
    pub reasons: ResetReasons,
    pub crash: Option<&'crash Crash>,
}

impl fmt::Display for RestartReason<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self.crash {
            Some(crash) => write!(formatter, "{crash}"),
            None => write!(formatter, "{}", self.reasons),
        }
    }
}

// ============================================================================
// What the diagnostics service sends
// ============================================================================

pub const LAST_RESTART_WORDS: usize = 2 + DETAIL_WORDS + OPEN_APP_WORDS;

/// The reset reasons, the kind of crash (0 for none, 1 a panic, 2 a HardFault, 3 out of memory), its detail words, then the open app's id.
pub fn last_restart_words(
    reasons: ResetReasons,
    recorded: Option<&RecordedCrash>,
) -> [u32; LAST_RESTART_WORDS] {
    let (kind, details) = recorded.map_or((0, [0; DETAIL_WORDS]), |recorded| {
        recorded.crash.kind_and_details()
    });
    let open_app = recorded.map_or([0; OPEN_APP_BYTES], |recorded| recorded.open_app);

    let mut words = [0; LAST_RESTART_WORDS];
    words[0] = reasons.register;
    words[1] = kind;
    words[2..2 + DETAIL_WORDS].copy_from_slice(&details);
    for (word, chunk) in words[2 + DETAIL_WORDS..]
        .iter_mut()
        .zip(open_app.as_chunks::<4>().0)
    {
        *word = u32::from_le_bytes(*chunk);
    }
    words
}

#[cfg(test)]
mod tests;
