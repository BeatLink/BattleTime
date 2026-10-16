//! Tests for the icons file.

use alloc::vec::Vec;

use super::{held_icon_bytes, icon_count, icons_file, read_icon, LIBRARY_ICON_MOST_BYTES};

/// A solid mask of 2 by 2, and one of 1 by 3, as the shared library holds them.
const SQUARE: [u8; 8] = [1, 0, 2, 0, 2, 0, 4, 255];
const BAR: [u8; 8] = [1, 0, 1, 0, 3, 0, 3, 128];

fn reader(file: &[u8]) -> impl FnMut(u32, &mut [u8]) -> Option<usize> + '_ {
    move |offset, out| {
        let rest = file.get(offset as usize..)?;
        let length = rest.len().min(out.len());
        out[..length].copy_from_slice(&rest[..length]);
        Some(length)
    }
}

#[test]
fn each_icon_reads_back_by_its_number() {
    let file = icons_file(&[&SQUARE, &BAR]);
    let mut read = reader(&file);

    assert_eq!(icon_count(&mut read), Some(2));
    assert_eq!(read_icon(&mut read, 2, 0).as_deref(), Some(&SQUARE[..]));
    assert_eq!(read_icon(&mut read, 2, 1).as_deref(), Some(&BAR[..]));
    assert_eq!(read_icon(&mut read, 2, 2), None);
}

#[test]
fn a_missing_or_cut_short_file_gives_no_icons() {
    let mut missing = |_: u32, _: &mut [u8]| None;
    assert_eq!(icon_count(&mut missing), None);

    let file = icons_file(&[&SQUARE, &BAR]);
    let cut = &file[..file.len() - 1];
    let mut read = reader(cut);
    assert_eq!(read_icon(&mut read, 2, 0).as_deref(), Some(&SQUARE[..]));
    assert_eq!(read_icon(&mut read, 2, 1), None);
}

#[test]
fn an_icon_that_is_not_a_whole_image_or_too_large_is_refused() {
    let broken = [1, 0, 2, 0, 2, 0, 3, 255];
    let file = icons_file(&[&broken]);
    assert_eq!(read_icon(&mut reader(&file), 1, 0), None);

    let mut huge = Vec::from([1, 0, 255, 0, 4, 0]);
    huge.resize(6 + 2 * 512, 1);
    assert!(huge.len() > LIBRARY_ICON_MOST_BYTES);
    let file = icons_file(&[&huge]);
    assert_eq!(read_icon(&mut reader(&file), 1, 0), None);
}

#[test]
fn a_place_pointing_into_the_table_is_refused() {
    let mut file = icons_file(&[&SQUARE]);
    file[4..8].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(read_icon(&mut reader(&file), 1, 0), None);
}

#[test]
fn a_held_icon_costs_its_rounded_bytes_and_its_place() {
    assert_eq!(held_icon_bytes(8), 8 + 16);
    assert_eq!(held_icon_bytes(9), 12 + 16);
}
