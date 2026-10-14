//! A one-bit picture read whole from a file, such as a QR code the phone made.
//!
//! The file holds the picture's width and height in pixels, one byte each, then each row from the top,
//! packed eight pixels to a byte with the leftmost in the high bit; a set bit is drawn in the widget's colour.

use alloc::{vec, vec::Vec};

use neotime_runtime::widgets::BITMAP_FILE_MOST_SIDE;

use crate::text_file::ReadPart;

// ============================================================================
// Reading
// ============================================================================

/// The whole picture, header included, or nothing when the file is missing, too large or cut short.
pub fn read_bitmap(read: &mut ReadPart) -> Vec<u8> {
    let mut header = [0u8; 2];
    if read(0, &mut header) != Some(2) {
        return Vec::new();
    }
    let [width, height] = header;
    let fits = |side: u8| (1..=BITMAP_FILE_MOST_SIDE).contains(&side);
    if !fits(width) || !fits(height) {
        return Vec::new();
    }
    let length = 2 + usize::from(height) * usize::from(width).div_ceil(8);
    let mut bitmap = vec![0u8; length];
    // A read stops short only at the file's end, so one read gives the whole picture or shows the file is cut short.
    if read(0, &mut bitmap) != Some(length) {
        return Vec::new();
    }
    bitmap
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads from the bytes given, at most `chunk` at a time.
    fn reader(bytes: &[u8], chunk: usize) -> impl FnMut(u32, &mut [u8]) -> Option<usize> + '_ {
        move |offset, out| {
            let rest = bytes.get(offset as usize..)?;
            let count = rest.len().min(out.len()).min(chunk);
            out[..count].copy_from_slice(&rest[..count]);
            Some(count)
        }
    }

    #[test]
    fn reads_a_whole_picture_and_ignores_what_follows() {
        let file = [9, 2, 0xff, 0x80, 0x00, 0x00, 0xaa];
        assert_eq!(read_bitmap(&mut reader(&file, 64)), file[..6].to_vec());
    }

    #[test]
    fn a_missing_file_gives_nothing() {
        assert!(read_bitmap(&mut |_, _| None).is_empty());
    }

    #[test]
    fn a_file_cut_short_gives_nothing() {
        assert!(read_bitmap(&mut reader(&[8, 2, 0xff], 64)).is_empty());
    }

    #[test]
    fn an_empty_or_oversized_picture_gives_nothing() {
        assert!(read_bitmap(&mut reader(&[0, 4, 0], 64)).is_empty());
        let side = BITMAP_FILE_MOST_SIDE + 1;
        let mut file = vec![side, side];
        file.resize(2 + usize::from(side) * usize::from(side).div_ceil(8), 0);
        assert!(read_bitmap(&mut reader(&file, 64)).is_empty());
    }

    #[test]
    fn the_largest_picture_fits_its_budget() {
        let side = BITMAP_FILE_MOST_SIDE;
        let mut file = vec![side, side];
        file.resize(neotime_runtime::widgets::BITMAP_FILE_MOST_BYTES, 0x55);
        assert_eq!(read_bitmap(&mut reader(&file, usize::MAX)).len(), file.len());
    }
}
