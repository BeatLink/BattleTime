//! BattleTime's native modules: rarely used native code kept on the external flash and run from RAM.
//!
//! A module is a `no_std` crate under `modules/`, linked at address 0 with its relocations kept.
//! `tools/package_module.py` packs its code and data into a module file, with the place of every
//! 32-bit address in it, and the firmware keeps it at `/system/modules/<name>`. To run one, the
//! firmware reads the file into the heap, `place`s it by adding where it landed to each address,
//! and calls its entry through the RAM's code alias, which runs faster than the RAM's own address.
//! The module talks to the firmware only through the `Host` table it is given, whose ABI number
//! both sides check. docs/external-flash-architecture.md explains the design.

#![no_std]

pub mod crashes;
#[cfg(feature = "module")]
pub mod module;

use battletime_boot::migration::{crc32, CRC32_INITIAL};

// ============================================================================
// The firmware's side of the call
// ============================================================================

/// The ABI a module is built for; a module built for another is refused.
pub const ABI_VERSION: u16 = 1;

/// The firmware calls a module may make, in an order that only grows, so `length` tells which it has.
#[repr(C)]
pub struct Host {
    /// The size of this table in bytes.
    pub length: u32,
    pub abi_version: u32,
    /// Takes `size` bytes aligned to `align` from the firmware's heap, or gives null.
    pub allocate: unsafe extern "C" fn(size: usize, align: usize) -> *mut u8,
    /// Gives back what `allocate` gave.
    pub free: unsafe extern "C" fn(pointer: *mut u8, size: usize, align: usize),
    /// Records a panic as a crash of this module and restarts the watch.
    pub panic: extern "C" fn() -> !,
    /// Microseconds since start, wrapping.
    pub microseconds: extern "C" fn() -> u32,
}

/// A module's entry: the call's number, its input and room for its output; a negative answer is a failure.
pub type Entry = unsafe extern "C" fn(
    host: &'static Host,
    call: u32,
    input: *const u8,
    input_length: usize,
    output: *mut u8,
    output_length: usize,
) -> i32;

// ============================================================================
// The module file
// ============================================================================

pub const MAGIC: [u8; 4] = *b"NTMD";
pub const FORMAT_VERSION: u16 = 1;
pub const HEADER_BYTES: usize = 48;
/// The longest module name, which the crash record also holds.
pub const NAME_BYTES: usize = 16;
/// The alignment the firmware gives a module's RAM, which no section of it may exceed.
pub const ALIGNMENT_BYTES: usize = 8;

const FORMAT_VERSION_AT: usize = 4;
const ABI_AT: usize = 6;
const NAME_AT: usize = 8;
const IMAGE_BYTES_AT: usize = 24;
const ZEROED_BYTES_AT: usize = 28;
const CODE_BYTES_AT: usize = 32;
const ENTRY_AT: usize = 36;
const RELOCATION_COUNT_AT: usize = 40;
const CRC_AT: usize = 44;

/// Why a module file was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleError {
    TooShort,
    NotAModule,
    OtherFormat,
    OtherAbi,
    Damaged,
    /// An address to place lies outside the image, or a placed address outside the module.
    BadRelocation,
    BadEntry,
    /// The file names another module than the one asked for.
    OtherName,
}

impl ModuleError {
    pub fn describe(self) -> &'static str {
        match self {
            ModuleError::TooShort => "the module file is cut short",
            ModuleError::NotAModule => "the file is not a module",
            ModuleError::OtherFormat => "the module file is of another format",
            ModuleError::OtherAbi => "the module was built for another firmware",
            ModuleError::Damaged => "the module file is damaged",
            ModuleError::BadRelocation => "the module has an address it cannot place",
            ModuleError::BadEntry => "the module's entry lies outside its code",
            ModuleError::OtherName => "the module file holds another module",
        }
    }
}

/// What a module file's header says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleHeader {
    pub name: [u8; NAME_BYTES],
    /// The code, read-only data and data, as loaded.
    pub image_bytes: u32,
    /// The zeroed data that follows the image in RAM.
    pub zeroed_bytes: u32,
    /// The image's first part, its code, which runs from the code alias.
    pub code_bytes: u32,
    /// Where the entry lies in the image, with the Thumb bit set.
    pub entry: u32,
    pub relocation_count: u32,
    pub crc: u32,
}

fn read_u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

impl ModuleHeader {
    /// The header at the start of a module file, checked to be one this firmware runs.
    pub fn parse(bytes: &[u8]) -> Result<ModuleHeader, ModuleError> {
        if bytes.len() < HEADER_BYTES {
            return Err(ModuleError::TooShort);
        }
        if bytes[..4] != MAGIC {
            return Err(ModuleError::NotAModule);
        }
        if read_u16(bytes, FORMAT_VERSION_AT) != FORMAT_VERSION {
            return Err(ModuleError::OtherFormat);
        }
        if read_u16(bytes, ABI_AT) != ABI_VERSION {
            return Err(ModuleError::OtherAbi);
        }
        let mut name = [0; NAME_BYTES];
        name.copy_from_slice(&bytes[NAME_AT..NAME_AT + NAME_BYTES]);
        let header = ModuleHeader {
            name,
            image_bytes: read_u32(bytes, IMAGE_BYTES_AT),
            zeroed_bytes: read_u32(bytes, ZEROED_BYTES_AT),
            code_bytes: read_u32(bytes, CODE_BYTES_AT),
            entry: read_u32(bytes, ENTRY_AT),
            relocation_count: read_u32(bytes, RELOCATION_COUNT_AT),
            crc: read_u32(bytes, CRC_AT),
        };
        let entry_offset = header.entry & !1;
        if header.entry & 1 == 0
            || entry_offset >= header.code_bytes
            || header.code_bytes > header.image_bytes
        {
            return Err(ModuleError::BadEntry);
        }
        Ok(header)
    }

    /// The module's name, without its zero padding.
    pub fn name(&self) -> &str {
        let length = self.name.iter().position(|&byte| byte == 0).unwrap_or(NAME_BYTES);
        core::str::from_utf8(&self.name[..length]).unwrap_or("")
    }

    /// The whole file's length: the header, the image and a word per relocation.
    pub fn file_bytes(&self) -> usize {
        HEADER_BYTES + self.image_bytes as usize + 4 * self.relocation_count as usize
    }

    /// The RAM the placed module takes: its image and zeroed data.
    pub fn ram_bytes(&self) -> usize {
        self.image_bytes as usize + self.zeroed_bytes as usize
    }
}

/// The CRC-32 of everything in a module file after its header.
pub fn file_crc(file: &[u8]) -> u32 {
    crc32(CRC32_INITIAL, &file[HEADER_BYTES..])
}

// ============================================================================
// Placing a module in RAM
// ============================================================================

/// The code alias of a RAM address: the same RAM, fetched through the instruction bus.
pub const fn code_alias(ram_address: u32) -> u32 {
    ram_address - 0x2000_0000 + 0x0080_0000
}

/// A relocation word with this bit set is a MOVW's offset, and the next word its MOVT's.
pub const MOVE_PAIR_BIT: u32 = 1 << 31;

/// Turns a module file read whole into `ram`, at its start, into the placed module, and gives its header.
///
/// `ram` must hold at least the file and `ram_bytes`, and `ram_address` is where it lies. Each
/// address the module holds, as a word or split over a MOVW and MOVT, is moved by where it landed:
/// one into its code by the code alias, so calls through it run there, and any other by the RAM's
/// own address. The image then moves down over the header, and the zeroed data after it, where
/// the relocations were, is cleared.
pub fn place(ram: &mut [u8], file_length: usize, ram_address: u32) -> Result<ModuleHeader, ModuleError> {
    let header = ModuleHeader::parse(&ram[..file_length.min(ram.len())])?;
    if file_length != header.file_bytes() || ram.len() < file_length.max(header.ram_bytes()) {
        return Err(ModuleError::TooShort);
    }
    if file_crc(&ram[..file_length]) != header.crc {
        return Err(ModuleError::Damaged);
    }
    let image_bytes = header.image_bytes as usize;
    let relocations_at = HEADER_BYTES + image_bytes;
    let relocation_count = header.relocation_count as usize;
    let mut relocation = 0;
    while relocation < relocation_count {
        let word = read_u32(ram, relocations_at + 4 * relocation);
        let (image, _) = ram[HEADER_BYTES..].split_at_mut(image_bytes);
        if word & MOVE_PAIR_BIT == 0 {
            place_word(image, word as usize, &header, ram_address)?;
            relocation += 1;
        } else {
            if relocation + 1 == relocation_count {
                return Err(ModuleError::BadRelocation);
            }
            let high_at = read_u32(&ram[relocations_at..], 4 * (relocation + 1)) as usize;
            let (image, _) = ram[HEADER_BYTES..].split_at_mut(image_bytes);
            place_move_pair(
                image,
                (word & !MOVE_PAIR_BIT) as usize,
                high_at,
                &header,
                ram_address,
            )?;
            relocation += 2;
        }
    }
    ram.copy_within(HEADER_BYTES..relocations_at, 0);
    ram[image_bytes..header.ram_bytes()].fill(0);
    Ok(header)
}

/// Where a module address lands: one into its code at the code alias, any other in the RAM.
fn placed_address(address: u32, header: &ModuleHeader, ram_address: u32) -> Result<u32, ModuleError> {
    if address > header.ram_bytes() as u32 {
        return Err(ModuleError::BadRelocation);
    }
    Ok(match address & !1 < header.code_bytes {
        true => code_alias(ram_address) + address,
        false => ram_address + address,
    })
}

fn place_word(
    image: &mut [u8],
    at: usize,
    header: &ModuleHeader,
    ram_address: u32,
) -> Result<(), ModuleError> {
    if !at.is_multiple_of(4) || at + 4 > image.len() {
        return Err(ModuleError::BadRelocation);
    }
    let placed = placed_address(read_u32(image, at), header, ram_address)?;
    image[at..at + 4].copy_from_slice(&placed.to_le_bytes());
    Ok(())
}

// ============================================================================
// Addresses split over a MOVW and a MOVT
// ============================================================================

const MOVE_OPCODE_MASK: u16 = 0xFBF0;
const MOVW: u16 = 0xF240;
const MOVT: u16 = 0xF2C0;

/// The 16 bits a Thumb-2 MOVW or MOVT at `at` loads, or none if another instruction is there.
fn move_immediate(image: &[u8], at: usize, opcode: u16) -> Option<u16> {
    if !at.is_multiple_of(2) || at + 4 > image.len() {
        return None;
    }
    let first = read_u16(image, at);
    let second = read_u16(image, at + 2);
    if first & MOVE_OPCODE_MASK != opcode || second & 0x8000 != 0 {
        return None;
    }
    let immediate = (first & 0xF) << 12 | (first >> 10 & 1) << 11 | (second >> 12 & 7) << 8 | second & 0xFF;
    Some(immediate)
}

fn set_move_immediate(image: &mut [u8], at: usize, immediate: u16) {
    let first = read_u16(image, at) & MOVE_OPCODE_MASK | immediate >> 12 | (immediate >> 11 & 1) << 10;
    let second = read_u16(image, at + 2) & 0x8F00 | (immediate >> 8 & 7) << 12 | immediate & 0xFF;
    image[at..at + 2].copy_from_slice(&first.to_le_bytes());
    image[at + 2..at + 4].copy_from_slice(&second.to_le_bytes());
}

fn place_move_pair(
    image: &mut [u8],
    low_at: usize,
    high_at: usize,
    header: &ModuleHeader,
    ram_address: u32,
) -> Result<(), ModuleError> {
    let low = move_immediate(image, low_at, MOVW).ok_or(ModuleError::BadRelocation)?;
    let high = move_immediate(image, high_at, MOVT).ok_or(ModuleError::BadRelocation)?;
    let placed = placed_address((high as u32) << 16 | low as u32, header, ram_address)?;
    set_move_immediate(image, low_at, placed as u16);
    set_move_immediate(image, high_at, (placed >> 16) as u16);
    Ok(())
}

#[cfg(test)]
mod tests;
