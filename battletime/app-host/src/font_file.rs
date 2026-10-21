//! The text font on the external flash, held in RAM only as far as the screen needs it.
//!
//! The watch's firmware carries no font. The text font lives in `TEXT_FONT_PATH`, and the font the
//! sizes draw with is a small copy of it in the heap, a font file of its own holding only the glyphs
//! drawn lately. A lookup that misses notes the character; once layout or a firmware page has
//! finished with the font, `load_missing_glyphs` reads the noted glyphs from the file and builds the
//! copy again, keeping the glyphs used since the last build and dropping others to stay within
//! `MOST_BYTES`, so the caller lays out or draws again. A character the file lacks is remembered as
//! absent, so it is not asked for again. Without a reader, as in tests and the simulator, which
//! carry the whole font, nothing here runs.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicPtr, AtomicU32, AtomicUsize, Ordering};

use crate::fonts;

/// Where the companion puts the text font, `fonts/geometric.ntfont`.
pub const TEXT_FONT_PATH: &str = "/system/font";

/// The most the copy in RAM takes, its header and table included; the memory need counts it with the system's share.
pub const MOST_BYTES: usize = battletime_runtime::memory_need::TEXT_FONT_BYTES as usize;

/// Reads bytes of the font file from `offset` into `out`, giving how many it read.
pub type FontFileReader = fn(offset: u32, out: &mut [u8]) -> Option<usize>;

// ============================================================================
// The font file's layout
// ============================================================================

const HEADER_BYTES: usize = 16;
const ENTRY_BYTES: usize = 6;
const GLYPH_COUNT_AT: usize = 6;
const MOST_SHAPES: usize = 8;
const LONGEST_SHAPE_BYTES: usize = 7;
const MAGIC: &[u8; 4] = b"NTGF";

/// One glyph's table entry and shapes, as a copy holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HeldGlyph {
    character: u16,
    width: u8,
    shape_count: u8,
    shapes: Vec<u8>,
}

fn read_u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

/// The bytes `shape_count` shapes take from the start of `shapes`, or none if they are cut short or unknown.
fn shapes_length(shapes: &[u8], shape_count: u8) -> Option<usize> {
    let mut at = 0;
    for _ in 0..(shape_count as usize).min(MOST_SHAPES) {
        let size = match *shapes.get(at)? {
            0 => 5,
            1 => 7,
            2 => 3,
            3 => 6,
            _ => return None,
        };
        at += size;
        if at > shapes.len() {
            return None;
        }
    }
    Some(at)
}

/// Every glyph a font file holds, in its order.
pub(crate) fn glyphs_of(font: &[u8]) -> Vec<HeldGlyph> {
    if font.len() < HEADER_BYTES {
        return Vec::new();
    }
    let count = read_u16(font, GLYPH_COUNT_AT) as usize;
    let shapes_at = HEADER_BYTES + count * ENTRY_BYTES;
    let mut glyphs = Vec::new();
    for index in 0..count {
        let entry = HEADER_BYTES + index * ENTRY_BYTES;
        let Some(fields) = font.get(entry..entry + ENTRY_BYTES) else {
            break;
        };
        let start = shapes_at + read_u16(fields, 4) as usize;
        let rest = font.get(start..).unwrap_or(&[]);
        let length = shapes_length(rest, fields[3]).unwrap_or(0);
        glyphs.push(HeldGlyph {
            character: read_u16(fields, 0),
            width: fields[2],
            shape_count: fields[3],
            shapes: rest[..length].to_vec(),
        });
    }
    glyphs
}

/// A font file of `header`'s heights holding `glyphs`, which must be sorted by character, as the lookups expect.
pub(crate) fn font_of(header: &[u8; HEADER_BYTES], glyphs: &[HeldGlyph]) -> Vec<u8> {
    let shapes_bytes: usize = glyphs.iter().map(|glyph| glyph.shapes.len()).sum();
    let mut font = Vec::with_capacity(HEADER_BYTES + glyphs.len() * ENTRY_BYTES + shapes_bytes);
    font.extend_from_slice(header);
    font[GLYPH_COUNT_AT..GLYPH_COUNT_AT + 2].copy_from_slice(&(glyphs.len() as u16).to_le_bytes());
    let mut offset = 0u16;
    for glyph in glyphs.iter() {
        font.extend_from_slice(&glyph.character.to_le_bytes());
        font.extend_from_slice(&[glyph.width, glyph.shape_count]);
        font.extend_from_slice(&offset.to_le_bytes());
        offset += glyph.shapes.len() as u16;
    }
    for glyph in glyphs.iter() {
        font.extend_from_slice(&glyph.shapes);
    }
    font
}

fn held_bytes(glyph: &HeldGlyph) -> usize {
    ENTRY_BYTES + glyph.shapes.len()
}

/// Reads the font file's header, checked to be a font.
fn read_header(read: &mut dyn FnMut(u32, &mut [u8]) -> Option<usize>) -> Option<[u8; HEADER_BYTES]> {
    let mut header = [0; HEADER_BYTES];
    (read(0, &mut header)? == HEADER_BYTES && &header[..4] == MAGIC).then_some(header)
}

/// Finds `character` in the file's table and reads its shapes; none when the file lacks it or would not read.
fn read_glyph(
    read: &mut dyn FnMut(u32, &mut [u8]) -> Option<usize>,
    header: &[u8; HEADER_BYTES],
    character: u16,
) -> Result<Option<HeldGlyph>, ()> {
    let count = read_u16(header, GLYPH_COUNT_AT) as usize;
    let mut entry_at = |index: usize| -> Result<[u8; ENTRY_BYTES], ()> {
        let mut entry = [0; ENTRY_BYTES];
        match read((HEADER_BYTES + index * ENTRY_BYTES) as u32, &mut entry) {
            Some(ENTRY_BYTES) => Ok(entry),
            _ => Err(()),
        }
    };
    if count == 0 {
        return Ok(None);
    }
    // The file holds printable ASCII without gaps from its first glyph, so most are found at once.
    let first = read_u16(&entry_at(0)?, 0);
    let mut found = None;
    if let Some(index) = character
        .checked_sub(first)
        .map(usize::from)
        .filter(|&index| index < count)
    {
        let entry = entry_at(index)?;
        if read_u16(&entry, 0) == character {
            found = Some(entry);
        }
    }
    let (mut low, mut high) = (0, count);
    while found.is_none() && low < high {
        let middle = (low + high) / 2;
        let entry = entry_at(middle)?;
        match read_u16(&entry, 0).cmp(&character) {
            core::cmp::Ordering::Equal => found = Some(entry),
            core::cmp::Ordering::Less => low = middle + 1,
            core::cmp::Ordering::Greater => high = middle,
        }
    }
    let Some(entry) = found else {
        return Ok(None);
    };
    let mut shapes = [0; MOST_SHAPES * LONGEST_SHAPE_BYTES];
    let shapes_at = HEADER_BYTES + count * ENTRY_BYTES + read_u16(&entry, 4) as usize;
    let length = read(shapes_at as u32, &mut shapes).ok_or(())?;
    let used = shapes_length(&shapes[..length], entry[3]).ok_or(())?;
    Ok(Some(HeldGlyph {
        character,
        width: entry[2],
        shape_count: entry[3],
        shapes: shapes[..used].to_vec(),
    }))
}

/// What building a copy found.
pub(crate) struct Built {
    pub font: Vec<u8>,
    /// The characters the file lacks.
    pub absent: Vec<u16>,
}

/// A copy holding the glyphs of `wanted` read from the file, then those of `current` that `was_used`
/// marks, then the rest of `current`, as far as `most_bytes` allows; none if the file would not read.
pub(crate) fn build(
    current: &[u8],
    was_used: impl Fn(usize) -> bool,
    wanted: &[u16],
    read: &mut dyn FnMut(u32, &mut [u8]) -> Option<usize>,
    most_bytes: usize,
) -> Option<Built> {
    let header = read_header(read)?;
    let held = glyphs_of(current);
    let mut absent = Vec::new();
    let mut fresh = Vec::new();
    for &character in wanted {
        if held.iter().any(|glyph| glyph.character == character) {
            continue;
        }
        match read_glyph(read, &header, character).ok()? {
            Some(glyph) => fresh.push(glyph),
            None => absent.push(character),
        }
    }
    let mut room = most_bytes.saturating_sub(HEADER_BYTES);
    let mut kept: Vec<HeldGlyph> = Vec::new();
    let mut keep = |glyph: &HeldGlyph| {
        if held_bytes(glyph) <= room {
            room -= held_bytes(glyph);
            // A few dozen glyphs at most, so each goes straight into its place.
            let place = kept
                .iter()
                .position(|other| other.character > glyph.character)
                .unwrap_or(kept.len());
            kept.insert(place, glyph.clone());
        }
    };
    // The ones the screen just missed first, then those used lately, then the rest while room is left.
    fresh.iter().for_each(&mut keep);
    held.iter()
        .enumerate()
        .filter(|(index, _)| was_used(*index))
        .for_each(|(_, glyph)| keep(glyph));
    held.iter()
        .enumerate()
        .filter(|(index, _)| !was_used(*index))
        .for_each(|(_, glyph)| keep(glyph));
    Some(Built {
        font: font_of(&header, &kept),
        absent,
    })
}

// ============================================================================
// The copy the text font draws from
// ============================================================================

const MOST_MISSING: usize = 64;
/// The most times a layout or a page reads glyphs and goes again, for screens that miss more than one read takes.
pub const MOST_PASSES: usize = 4;
const MOST_ABSENT: usize = 16;
/// The copy's glyphs whose use is tracked; it never holds more than this many.
const TRACKED_GLYPHS: usize = 128;

static READER: AtomicUsize = AtomicUsize::new(0);
static COPY_START: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());
static COPY_LENGTH: AtomicUsize = AtomicUsize::new(0);
/// Characters a lookup missed since the last build, zero for an empty slot.
static MISSING: [AtomicU32; MOST_MISSING] = [const { AtomicU32::new(0) }; MOST_MISSING];
/// Characters the file lacks, so they are not read for again.
static ABSENT: [AtomicU32; MOST_ABSENT] = [const { AtomicU32::new(0) }; MOST_ABSENT];
static NEXT_ABSENT: AtomicUsize = AtomicUsize::new(0);
/// One bit per glyph of the copy, set when a lookup found it since the last build.
static USED: [AtomicU32; TRACKED_GLYPHS / 32] = [const { AtomicU32::new(0) }; TRACKED_GLYPHS / 32];

/// Has the text font read from the file through `reader`, starting with its heights and no glyphs.
pub fn read_text_font_with(reader: FontFileReader) {
    READER.store(reader as usize, Ordering::Release);
    load_missing_glyphs();
}

fn reader() -> Option<FontFileReader> {
    match READER.load(Ordering::Acquire) {
        0 => None,
        // Only `read_text_font_with` stores it, from a `FontFileReader`.
        address => Some(unsafe { core::mem::transmute::<usize, FontFileReader>(address) }),
    }
}

/// Whether the font has been read, at least its heights; the firmware's pages draw without text until it has.
pub fn text_font_readable() -> bool {
    COPY_LENGTH.load(Ordering::Acquire) != 0
}

fn is_absent(character: u32) -> bool {
    ABSENT
        .iter()
        .any(|slot| slot.load(Ordering::Relaxed) == character)
}

/// Notes a character the copy lacks, for the next build.
pub(crate) fn note_missing(character: char) {
    let character = character as u32;
    if character == 0 || character > u16::MAX as u32 || is_absent(character) {
        return;
    }
    for slot in &MISSING {
        let held = slot.load(Ordering::Relaxed);
        if held == character {
            return;
        }
        if held == 0
            && slot
                .compare_exchange(0, character, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            return;
        }
    }
}

/// Notes that the copy's glyph at `index` was looked up, so the next build keeps it.
pub(crate) fn note_used(index: usize) {
    if index < TRACKED_GLYPHS {
        USED[index / 32].fetch_or(1 << (index % 32), Ordering::Relaxed);
    }
}

/// Whether any lookup missed since the last build.
pub fn has_missing_glyphs() -> bool {
    MISSING.iter().any(|slot| slot.load(Ordering::Relaxed) != 0)
}

/// Reads the glyphs lookups missed and builds the copy again; gives whether it gained any, so the caller lays out or draws again.
///
/// Call it only where no text is being measured or drawn, since the old copy goes away.
pub fn load_missing_glyphs() -> bool {
    let Some(reader) = reader() else {
        return false;
    };
    let installed = COPY_LENGTH.load(Ordering::Acquire) != 0;
    if installed && !has_missing_glyphs() {
        return false;
    }
    let wanted: Vec<u16> = MISSING
        .iter()
        .map(|slot| slot.swap(0, Ordering::Relaxed))
        .filter(|&character| character != 0)
        .map(|character| character as u16)
        .collect();
    let used: [u32; TRACKED_GLYPHS / 32] = core::array::from_fn(|word| USED[word].swap(0, Ordering::Relaxed));
    let current = copy();
    let mut read = |offset: u32, out: &mut [u8]| reader(offset, out);
    let was_used = |index: usize| index < TRACKED_GLYPHS && used[index / 32] & (1 << (index % 32)) != 0;
    let Some(built) = build(current, was_used, &wanted, &mut read, MOST_BYTES) else {
        // The store may only have been busy, so the same glyphs are read for next time.
        wanted
            .iter()
            .for_each(|&character| note_missing(char::from_u32(character as u32).unwrap_or('\0')));
        return false;
    };
    for character in built.absent {
        let place = NEXT_ABSENT.fetch_add(1, Ordering::Relaxed) % MOST_ABSENT;
        ABSENT[place].store(character as u32, Ordering::Relaxed);
    }
    // When no missed glyph found room, nothing would draw differently, so the caller need not go again.
    let gained = glyphs_of(&built.font)
        .iter()
        .any(|glyph| wanted.contains(&glyph.character))
        || current.is_empty();
    install(built.font);
    gained
}

/// The copy the text font draws from now.
fn copy() -> &'static [u8] {
    let start = COPY_START.load(Ordering::Acquire);
    let length = COPY_LENGTH.load(Ordering::Acquire);
    if start.is_null() {
        return &[];
    }
    // Only `install` sets these, from a leaked buffer it alone frees.
    unsafe { core::slice::from_raw_parts(start, length) }
}

/// Makes `font` the copy the text font draws from, then frees the one before.
fn install(font: Vec<u8>) {
    let old = copy();
    let new: &'static [u8] = alloc::boxed::Box::leak(font.into_boxed_slice());
    fonts::install_text_font(new);
    COPY_LENGTH.store(0, Ordering::Release);
    COPY_START.store(new.as_ptr() as *mut u8, Ordering::Release);
    COPY_LENGTH.store(new.len(), Ordering::Release);
    if !old.is_empty() {
        // Leaked from a boxed slice of exactly this length above.
        unsafe { drop(alloc::boxed::Box::from_raw(old as *const [u8] as *mut [u8])) };
    }
}

#[cfg(test)]
#[path = "font_file_tests.rs"]
mod font_file_tests;
