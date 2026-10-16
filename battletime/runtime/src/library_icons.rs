//! The shared library's icons, kept in one file on the external flash rather than in the library, so
//! the firmware region keeps their room. The companion app puts the file on the watch.
//!
//! The file starts with the icon count (u16, little-endian) and two reserved bytes, then each icon's
//! start and the file's end (u32 each), then each icon's bytes as `ImageResource::parse` reads them.
//! The app host reads only the icons a screen shows, after layout and before painting.

use alloc::vec::Vec;

use crate::resources::ImageResource;

// ============================================================================
// Layout
// ============================================================================

/// Where the icons file lives on the watch.
pub const LIBRARY_ICONS_PATH: &str = "/system/icons";

/// Every library icon is a square of this side, so layout can size one before it is read.
pub const LIBRARY_ICON_SIDE: u16 = 32;

/// The most bytes one icon may take; the compiler refuses artwork that needs more.
pub const LIBRARY_ICON_MOST_BYTES: usize = 1024;

/// The most the app host holds of library icons at once; a screen that shows more draws the rest as placeholders.
pub const HELD_ICONS_MOST_BYTES: u32 = 8192;

/// What one held icon costs beside its bytes: its number and its place in the host's list.
pub const HELD_ICON_OVERHEAD_BYTES: u32 = 16;

const HEADER_BYTES: u32 = 4;
const PLACE_BYTES: u32 = 4;

// ============================================================================
// Writing
// ============================================================================

/// The icons file for these icons, each as the shared library would have held it.
#[cfg(any(test, feature = "writer"))]
pub fn icons_file(icons: &[&[u8]]) -> Vec<u8> {
    let table_end = HEADER_BYTES + PLACE_BYTES * (icons.len() as u32 + 1);
    let mut file = Vec::new();
    file.extend_from_slice(&(icons.len() as u16).to_le_bytes());
    file.extend_from_slice(&[0, 0]);
    let mut start = table_end;
    for icon in icons {
        file.extend_from_slice(&start.to_le_bytes());
        start += icon.len() as u32;
    }
    file.extend_from_slice(&start.to_le_bytes());
    for icon in icons {
        file.extend_from_slice(icon);
    }
    file
}

// ============================================================================
// Reading
// ============================================================================

/// Reads part of the icons file at an offset, giving how many bytes it read, or nothing if it cannot.
pub type ReadPart<'read> = dyn FnMut(u32, &mut [u8]) -> Option<usize> + 'read;

/// How many icons the file holds, or nothing when it is missing or cut short.
pub fn icon_count(read: &mut ReadPart) -> Option<u16> {
    let mut header = [0u8; HEADER_BYTES as usize];
    (read(0, &mut header)? == header.len()).then(|| u16::from_le_bytes([header[0], header[1]]))
}

/// Reads icon `number` of a file holding `count`, or nothing when it is missing, too large or not a whole image.
pub fn read_icon(read: &mut ReadPart, count: u16, number: u16) -> Option<Vec<u8>> {
    if number >= count {
        return None;
    }
    let mut place = [0u8; 2 * PLACE_BYTES as usize];
    if read(HEADER_BYTES + PLACE_BYTES * number as u32, &mut place)? != place.len() {
        return None;
    }
    let start = u32::from_le_bytes([place[0], place[1], place[2], place[3]]);
    let end = u32::from_le_bytes([place[4], place[5], place[6], place[7]]);
    let table_end = HEADER_BYTES + PLACE_BYTES * (count as u32 + 1);
    let length = end.checked_sub(start)? as usize;
    if start < table_end || length > LIBRARY_ICON_MOST_BYTES {
        return None;
    }
    let mut icon = Vec::new();
    icon.try_reserve_exact(length).ok()?;
    icon.resize(length, 0);
    if read(start, &mut icon)? != length {
        return None;
    }
    ImageResource::parse(&icon)
        .is_some_and(|image| image.is_well_formed())
        .then_some(icon)
}

/// What holding this icon costs the heap.
pub fn held_icon_bytes(icon_bytes: usize) -> u32 {
    (icon_bytes as u32).next_multiple_of(4) + HELD_ICON_OVERHEAD_BYTES
}

#[cfg(test)]
#[path = "library_icons_tests.rs"]
mod library_icons_tests;
