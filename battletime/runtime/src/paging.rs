//! Keeping an open app's code on the flash, so its image takes less of the heap.
//!
//! An app whose memory need is smaller paged, as `estimate_memory_need` works out, is opened from a
//! resident image: a copy of its image with every section but the code's data, which `resident_image`
//! builds. Its handler and function tables stay, and a paged section says where their data lies in
//! the image file. The interpreter then reads each handler or function into a `CodeCache` when it
//! runs, keeping those used most recently. A caller is found again by its number when a call
//! returns, so dropping any code but what is running is always safe.

use alloc::{string::String, vec::Vec};

use crate::image::{crc32, layout, AppImage, CodePlace, ImageError};
use crate::interpreter::{Failure, SystemValues};

// ============================================================================
// When an app is paged
// ============================================================================

/// The least a code cache holds, so a handler and the functions it calls in a loop stay in it.
pub const LEAST_CACHE_BYTES: usize = 1024;

const HEADER_BYTES: usize = layout::HEADER_BYTES;
const SECTION_ENTRY_BYTES: usize = layout::SECTION_ENTRY_BYTES;
const HANDLERS_SECTION: u16 = 4;
const FUNCTIONS_SECTION: u16 = 10;
/// The CRC field, and where the bytes it covers start.
const CRC_AT: usize = 12;
const CRC_COVERS_FROM: usize = 16;

/// Whether the app is opened paged: when keeping its code on the flash needs less of the heap, opening included.
pub fn pages_code(image: &AppImage) -> bool {
    crate::memory_need::estimate_memory_need(image).pages_code
}

/// The code cache a paged app holds: enough for its longest handler or function, and at least `LEAST_CACHE_BYTES`.
pub fn cache_bytes(image: &AppImage) -> usize {
    image.longest_code_bytes().max(LEAST_CACHE_BYTES)
}

/// How long the app's resident image is, at most: the whole image less its code, plus the paged section.
pub fn resident_bytes(image: &AppImage) -> usize {
    image.byte_count() - image.code_bytes() + SECTION_ENTRY_BYTES + 2 * layout::PAGED_ENTRY_BYTES
}

// ============================================================================
// The resident image
// ============================================================================

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// The resident image of a whole image: every section as it is, but the handlers' and functions'
/// tables without their data, and a paged section saying where that data lies in `whole`.
pub fn resident_image(whole: &[u8]) -> Result<Vec<u8>, ImageError> {
    let image = AppImage::parse(whole)?;
    if image.is_paged() {
        return Err(ImageError::Paged);
    }
    let section_count = read_u16(whole, layout::SECTION_COUNT_AT).ok_or(ImageError::TooShort)? as usize;

    // Each section's number, count and the bytes kept, and each paged section's place in `whole`.
    let mut kept: Vec<(u16, u16, &[u8])> = Vec::with_capacity(section_count + 1);
    let mut paged: Vec<(u16, CodePlace)> = Vec::new();
    for section in 0..section_count {
        let entry = HEADER_BYTES + section * SECTION_ENTRY_BYTES;
        let read = |at| read_u32(whole, entry + at).map(|value| value as usize);
        let number = read_u16(whole, entry).ok_or(ImageError::SectionOutOfBounds)?;
        let count = read_u16(whole, entry + 2).ok_or(ImageError::SectionOutOfBounds)?;
        let offset = read(4).ok_or(ImageError::SectionOutOfBounds)?;
        let length = read(8).ok_or(ImageError::SectionOutOfBounds)?;
        let bytes = whole
            .get(offset..offset + length)
            .ok_or(ImageError::SectionOutOfBounds)?;
        if number == HANDLERS_SECTION || number == FUNCTIONS_SECTION {
            let table_bytes = count as usize * layout::TABLE_ENTRY_BYTES;
            let place = CodePlace {
                at: (offset + table_bytes) as u32,
                length: (length - table_bytes) as u32,
            };
            kept.push((number, count, &bytes[..table_bytes]));
            paged.push((number, place));
        } else {
            kept.push((number, count, bytes));
        }
    }

    let mut paged_section = Vec::with_capacity(paged.len() * layout::PAGED_ENTRY_BYTES);
    for (number, place) in &paged {
        paged_section.extend_from_slice(&number.to_le_bytes());
        paged_section.extend_from_slice(&[0, 0]);
        paged_section.extend_from_slice(&place.at.to_le_bytes());
        paged_section.extend_from_slice(&place.length.to_le_bytes());
    }
    kept.push((layout::PAGED_SECTION, paged.len() as u16, &paged_section));

    let table_end = HEADER_BYTES + kept.len() * SECTION_ENTRY_BYTES;
    let length = table_end + kept.iter().map(|(_, _, bytes)| bytes.len()).sum::<usize>();
    let mut resident = Vec::new();
    resident
        .try_reserve_exact(length)
        .map_err(|_| ImageError::TooShort)?;
    resident.extend_from_slice(&whole[..HEADER_BYTES]);
    resident[layout::SECTION_COUNT_AT..layout::SECTION_COUNT_AT + 2]
        .copy_from_slice(&(kept.len() as u16).to_le_bytes());
    let mut offset = table_end;
    for (number, count, bytes) in &kept {
        resident.extend_from_slice(&number.to_le_bytes());
        resident.extend_from_slice(&count.to_le_bytes());
        resident.extend_from_slice(&(offset as u32).to_le_bytes());
        resident.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        offset += bytes.len();
    }
    for (_, _, bytes) in &kept {
        resident.extend_from_slice(bytes);
    }
    let crc = crc32(&resident[CRC_COVERS_FROM..]);
    resident[CRC_AT..CRC_AT + 4].copy_from_slice(&crc.to_le_bytes());
    Ok(resident)
}

// ============================================================================
// The code cache
// ============================================================================

/// A piece of an app's code, by its number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeUnit {
    Handler(u16),
    /// A function's header and code.
    Function(u8),
}

/// One piece of code the cache holds, where it starts in the buffer, and when it was last used.
#[derive(Clone, Copy)]
struct Held {
    unit: CodeUnit,
    start: u16,
    length: u16,
    last_used: u32,
}

/// The paged app's code read so far, packed from the buffer's start in the order it was read, the least
/// recently used dropped first.
pub struct CodeCache {
    /// The image file the code is read from.
    path: String,
    /// The whole image's CRC, which the file must still carry, so code is never read from an image put in its place.
    image_crc: u32,
    buffer: Vec<u8>,
    held: Vec<Held>,
    clock: u32,
    /// How many times code was read from the file, for tests and diagnostics.
    pub reads: u32,
}

impl CodeCache {
    /// A cache of `capacity` bytes for `whole`, the image at `path`, or none if the heap has no room for it.
    pub fn new(path: &str, whole: &[u8], capacity: usize) -> Option<CodeCache> {
        let image_crc = read_u32(whole, CRC_AT)?;
        let mut buffer = Vec::new();
        buffer.try_reserve_exact(capacity).ok()?;
        buffer.resize(capacity, 0);
        Some(CodeCache {
            path: String::from(path),
            image_crc,
            buffer,
            held: Vec::new(),
            clock: 0,
            reads: 0,
        })
    }

    /// The bytes of a piece of the paged image's code, read from its file unless the cache holds it.
    pub fn code(
        &mut self,
        image: &AppImage,
        unit: CodeUnit,
        system: &impl SystemValues,
    ) -> Result<&[u8], Failure> {
        self.clock = self.clock.wrapping_add(1);
        if let Some(found) = self.held.iter_mut().find(|held| held.unit == unit) {
            found.last_used = self.clock;
            let (start, length) = (found.start as usize, found.length as usize);
            return Ok(&self.buffer[start..start + length]);
        }
        let place = match unit {
            CodeUnit::Handler(index) => image.handler_place(index),
            CodeUnit::Function(index) => image.function_place(index as u16),
        }
        .ok_or(Failure::NoSuchCode)?;
        let length = place.length as usize;
        if length > self.buffer.len() {
            return Err(Failure::CodeUnreadable);
        }
        let start = self.make_room(length);
        let read = system.read_file(&self.path, place.at, &mut self.buffer[start..start + length]);
        self.reads += 1;
        let mut crc = [0u8; 4];
        let still_the_image = system.read_file(&self.path, CRC_AT as u32, &mut crc) == Some(crc.len())
            && u32::from_le_bytes(crc) == self.image_crc;
        if read != Some(length) || !still_the_image {
            return Err(Failure::CodeUnreadable);
        }
        self.held.push(Held {
            unit,
            start: start as u16,
            length: length as u16,
            last_used: self.clock,
        });
        Ok(&self.buffer[start..start + length])
    }

    /// Drops the least recently used pieces until `length` bytes fit after the rest, packed down; gives where they go.
    fn make_room(&mut self, length: usize) -> usize {
        loop {
            let used: usize = self.held.iter().map(|held| held.length as usize).sum();
            if used + length <= self.buffer.len() {
                break;
            }
            let oldest = self
                .held
                .iter()
                .enumerate()
                .min_by_key(|(_, held)| held.last_used)
                .map(|(place, _)| place);
            match oldest {
                Some(place) => {
                    self.held.remove(place);
                }
                None => break,
            }
        }
        // Each piece goes after the rest and removing keeps the order, so `held` is in the buffer's order.
        let mut end = 0;
        for held in self.held.iter_mut() {
            let (start, length) = (held.start as usize, held.length as usize);
            self.buffer.copy_within(start..start + length, end);
            held.start = end as u16;
            end += length;
        }
        end
    }

    /// The bytes the cache takes on the heap.
    pub fn capacity(&self) -> usize {
        self.buffer.len()
    }
}

#[cfg(test)]
#[path = "paging_tests.rs"]
mod paging_tests;
