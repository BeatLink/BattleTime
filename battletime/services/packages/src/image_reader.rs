//! Reads the few parts of an installed image that a list of apps, the launcher's icons and the
//! shell's search for apps to run while closed need, a few bytes at a time, so none ever holds a
//! whole image.
//!
//! An image is laid out as `battletime_runtime::image::layout` describes: a header, a table of
//! sections, and inside each section a table of entries, so any string, the facts or the icon is
//! a few small reads away. These reads trust the image's structure without checking its CRC,
//! which install and opening the app both check.

use alloc::{string::String, vec, vec::Vec};

use battletime_flash_store::FileAccess;
use battletime_runtime::image::{layout, Facts, FORMAT_VERSION, MAGIC};
use battletime_runtime::subscriptions::{Subscription, SUBSCRIPTION_BYTES};
use battletime_runtime::AppKind;

// ============================================================================
// The header and the section table
// ============================================================================

/// The most sections an image has; anything more is not an image this firmware wrote.
const MOST_SECTIONS: usize = 16;

#[derive(Clone, Copy)]
struct Section {
    number: u16,
    count: u16,
    offset: u32,
}

/// What the header and the section table say about an image.
pub struct Outline {
    pub kind: AppKind,
    pub has_icon: bool,
    id_string: u16,
    name_string: u16,
    sections: Vec<Section>,
}

/// Reads exactly `bytes.len()` bytes from `offset`, or gives none.
async fn read_exactly<Files: FileAccess>(
    files: &mut Files,
    path: &str,
    offset: u32,
    bytes: &mut [u8],
) -> Option<()> {
    let read = files.read_file(path, offset as usize, bytes).await.ok()??;
    (read == bytes.len()).then_some(())
}

fn word_16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn word_32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Reads the header and the section table: two reads.
pub async fn outline<Files: FileAccess>(files: &mut Files, path: &str) -> Option<Outline> {
    let mut header = [0; layout::HEADER_BYTES];
    read_exactly(files, path, 0, &mut header).await?;
    if header[0..4] != MAGIC || word_16(&header, 4) != FORMAT_VERSION {
        return None;
    }
    let kind = match header[layout::KIND_AT] {
        0 => AppKind::App,
        1 => AppKind::Face,
        2 => AppKind::Library,
        _ => return None,
    };
    let section_count = word_16(&header, layout::SECTION_COUNT_AT) as usize;
    if section_count > MOST_SECTIONS {
        return None;
    }
    let mut table = vec![0; section_count * layout::SECTION_ENTRY_BYTES];
    read_exactly(files, path, layout::HEADER_BYTES as u32, &mut table).await?;
    let (entries, _) = table.as_chunks::<{ layout::SECTION_ENTRY_BYTES }>();
    let sections = entries
        .iter()
        .map(|entry| Section {
            number: word_16(entry, 0),
            count: word_16(entry, 2),
            offset: word_32(entry, 4),
        })
        .collect();
    Some(Outline {
        kind,
        has_icon: header[layout::FLAGS_AT] & 1 != 0,
        id_string: word_16(&header, layout::ID_STRING_AT),
        name_string: word_16(&header, layout::NAME_STRING_AT),
        sections,
    })
}

impl Outline {
    fn section(&self, number: u16) -> Option<Section> {
        self.sections
            .iter()
            .copied()
            .find(|section| section.number == number)
    }
}

// ============================================================================
// The parts a listing and the launcher read
// ============================================================================

/// Where entry `index` of a section's table lies in the file, and its length: one read.
async fn entry<Files: FileAccess>(
    files: &mut Files,
    path: &str,
    section: Section,
    index: u16,
) -> Option<(u32, u32)> {
    if index >= section.count {
        return None;
    }
    let mut pair = [0; layout::TABLE_ENTRY_BYTES];
    let at = section.offset + index as u32 * layout::TABLE_ENTRY_BYTES as u32;
    read_exactly(files, path, at, &mut pair).await?;
    let data_start = section.offset + section.count as u32 * layout::TABLE_ENTRY_BYTES as u32;
    Some((data_start + word_16(&pair, 0) as u32, word_16(&pair, 2) as u32))
}

/// The bytes of entry `index` of the section `number`: two reads.
async fn entry_bytes<Files: FileAccess>(
    files: &mut Files,
    path: &str,
    outline: &Outline,
    number: u16,
    index: u16,
) -> Option<Vec<u8>> {
    let (at, length) = entry(files, path, outline.section(number)?, index).await?;
    let mut bytes = vec![0; length as usize];
    read_exactly(files, path, at, &mut bytes).await?;
    Some(bytes)
}

/// A string of the image, by its number: two reads.
pub async fn string<Files: FileAccess>(
    files: &mut Files,
    path: &str,
    outline: &Outline,
    index: u16,
) -> Option<String> {
    let bytes = entry_bytes(files, path, outline, layout::STRINGS_SECTION, index).await?;
    String::from_utf8(bytes).ok()
}

pub async fn id<Files: FileAccess>(files: &mut Files, path: &str, outline: &Outline) -> Option<String> {
    string(files, path, outline, outline.id_string).await
}

pub async fn name<Files: FileAccess>(files: &mut Files, path: &str, outline: &Outline) -> Option<String> {
    string(files, path, outline, outline.name_string).await
}

/// The launcher group the image declares, or empty when it declares none: up to three reads.
pub async fn group<Files: FileAccess>(files: &mut Files, path: &str, outline: &Outline) -> Option<String> {
    let Some(section) = outline.section(layout::LAUNCHER_SECTION) else {
        return Some(String::new());
    };
    let mut index = [0; 2];
    read_exactly(files, path, section.offset, &mut index).await?;
    string(files, path, outline, word_16(&index, 0)).await
}

/// The facts the compiler stored: one read.
pub async fn facts<Files: FileAccess>(files: &mut Files, path: &str, outline: &Outline) -> Option<Facts> {
    let section = outline.section(layout::FACTS_SECTION)?;
    let mut bytes = [0; layout::FACTS_BYTES];
    read_exactly(files, path, section.offset, &mut bytes).await?;
    Facts::from_bytes(&bytes)
}

/// The app's icon, its first image, or none when it has none: two reads.
pub async fn icon<Files: FileAccess>(files: &mut Files, path: &str) -> Option<Vec<u8>> {
    let outline = outline(files, path).await?;
    if !outline.has_icon {
        return None;
    }
    entry_bytes(files, path, &outline, layout::RESOURCES_SECTION, 0).await
}

/// Whether any of the app's subscriptions runs while it is closed: one read of its subscriptions.
pub async fn runs_while_closed<Files: FileAccess>(files: &mut Files, path: &str) -> Option<bool> {
    let outline = outline(files, path).await?;
    let Some(section) = outline.section(layout::SUBSCRIPTIONS_SECTION) else {
        return Some(false);
    };
    let mut bytes = vec![0; section.count as usize * SUBSCRIPTION_BYTES];
    read_exactly(files, path, section.offset, &mut bytes).await?;
    let (records, _) = bytes.as_chunks::<SUBSCRIPTION_BYTES>();
    Some(records.iter().any(|record| {
        Subscription::parse(record).is_some_and(|subscription| subscription.runs_while_closed())
    }))
}
