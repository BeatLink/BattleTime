//! A one-bit picture read whole from a file, such as a QR code the phone made.
//!
//! The file holds the picture's width and height in pixels, one byte each, then each row from the top,
//! packed eight pixels to a byte with the leftmost in the high bit; a set bit is drawn in the widget's colour.
//! A BitmapFile given a number reads the path with that number after it, so an app can pick one of several files.

use alloc::{string::String, vec, vec::Vec};

use battletime_runtime::widgets::BITMAP_FILE_MOST_SIDE;

use crate::text_file::ReadPart;

// ============================================================================
// Reading
// ============================================================================

/// One BitmapFile's picture as read, and the number it was read for.
pub struct ShownBitmap {
    pub widget_index: u16,
    pub number: Option<i32>,
    pub bitmap: Vec<u8>,
}

/// The path with the number written after it, the path alone without one, or nothing for a negative number.
pub fn numbered_path(path: &str, number: Option<i32>) -> Option<String> {
    let mut whole = String::from(path);
    let Some(number) = number else {
        return Some(whole);
    };
    let mut left = u32::try_from(number).ok()?;
    let digits_start = whole.len();
    loop {
        whole.insert(digits_start, char::from(b'0' + (left % 10) as u8));
        left /= 10;
        if left == 0 {
            return Some(whole);
        }
    }
}

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
    fn a_number_follows_the_path() {
        assert_eq!(numbered_path("/qr/", Some(0)).as_deref(), Some("/qr/0"));
        assert_eq!(numbered_path("/qr/", Some(407)).as_deref(), Some("/qr/407"));
        assert_eq!(numbered_path("/qr/code", None).as_deref(), Some("/qr/code"));
        assert_eq!(numbered_path("/qr/", Some(-1)), None);
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
        file.resize(battletime_runtime::widgets::BITMAP_FILE_MOST_BYTES, 0x55);
        assert_eq!(read_bitmap(&mut reader(&file, usize::MAX)).len(), file.len());
    }
}
