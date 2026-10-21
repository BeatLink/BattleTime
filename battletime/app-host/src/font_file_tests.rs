extern crate std;

use std::vec::Vec;

use super::*;

const WHOLE_FONT: &[u8] = include_bytes!("../fonts/geometric.ntfont");

/// Reads the whole font as the external flash would, counting the reads.
fn reader_over<'count>(
    file: &'static [u8],
    reads: &'count mut usize,
) -> impl FnMut(u32, &mut [u8]) -> Option<usize> + 'count {
    move |offset, out| {
        *reads += 1;
        let start = (offset as usize).min(file.len());
        let length = out.len().min(file.len() - start);
        out[..length].copy_from_slice(&file[start..start + length]);
        Some(length)
    }
}

fn characters(font: &[u8]) -> Vec<u16> {
    glyphs_of(font).iter().map(|glyph| glyph.character).collect()
}

fn whole_glyph(character: u16) -> HeldGlyph {
    glyphs_of(WHOLE_FONT)
        .into_iter()
        .find(|glyph| glyph.character == character)
        .unwrap()
}

// ============================================================================
// Building a copy
// ============================================================================

#[test]
fn a_copy_holds_the_wanted_glyphs_exactly_as_the_file_has_them() {
    let mut reads = 0;
    let wanted = [u16::from(b'7'), u16::from(b'A'), 0xE9];

    let built = build(
        &[],
        |_| false,
        &wanted,
        &mut reader_over(WHOLE_FONT, &mut reads),
        MOST_BYTES,
    )
    .unwrap();

    assert_eq!(characters(&built.font), [u16::from(b'7'), u16::from(b'A'), 0xE9]);
    for glyph in glyphs_of(&built.font) {
        assert_eq!(glyph, whole_glyph(glyph.character));
    }
    assert_eq!(&built.font[..6], &WHOLE_FONT[..6]);
    assert_eq!(&built.font[8..16], &WHOLE_FONT[8..16]);
}

#[test]
fn printable_ascii_is_found_with_one_table_read() {
    let mut reads = 0;

    build(
        &[],
        |_| false,
        &[u16::from(b'q')],
        &mut reader_over(WHOLE_FONT, &mut reads),
        MOST_BYTES,
    )
    .unwrap();

    // The header, the first entry, the guessed entry and the shapes.
    assert_eq!(reads, 4);
}

#[test]
fn a_character_the_file_lacks_is_reported_absent() {
    let mut reads = 0;

    let built = build(
        &[],
        |_| false,
        &[0x4E2D],
        &mut reader_over(WHOLE_FONT, &mut reads),
        MOST_BYTES,
    )
    .unwrap();

    assert_eq!(built.absent, [0x4E2D]);
    assert!(characters(&built.font).is_empty());
}

#[test]
fn used_glyphs_stay_and_unused_ones_make_room() {
    let mut reads = 0;
    let all: Vec<u16> = (0x21..0x7F).collect();
    let full = build(
        &[],
        |_| false,
        &all,
        &mut reader_over(WHOLE_FONT, &mut reads),
        MOST_BYTES,
    )
    .unwrap();
    let held = characters(&full.font);
    assert!(held.len() < all.len(), "the limit leaves some out");
    let used_index = held
        .iter()
        .position(|&character| character == u16::from(b'0'))
        .unwrap();
    let left_out = *all.iter().find(|character| !held.contains(character)).unwrap();

    let rebuilt = build(
        &full.font,
        |index| index == used_index,
        &[left_out],
        &mut reader_over(WHOLE_FONT, &mut reads),
        MOST_BYTES,
    )
    .unwrap();

    let now = characters(&rebuilt.font);
    assert!(now.contains(&u16::from(b'0')));
    assert!(now.contains(&left_out));
    assert!(rebuilt.font.len() <= MOST_BYTES);
}

#[test]
fn an_unreadable_file_builds_nothing() {
    let mut read_nothing = |_: u32, _: &mut [u8]| None;

    assert!(build(&[], |_| false, &[u16::from(b'a')], &mut read_nothing, MOST_BYTES).is_none());
}

#[test]
fn a_rebuilt_copy_draws_like_the_whole_font() {
    let glyphs = glyphs_of(WHOLE_FONT);
    let header: [u8; 16] = WHOLE_FONT[..16].try_into().unwrap();

    let rebuilt = font_of(&header, &glyphs);

    assert_eq!(rebuilt, WHOLE_FONT);
}
