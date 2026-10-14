//! Tests for the shared library: icon artwork, drawing, unpacking WOFF fonts and the library image.

use neotime_runtime::resources::ImageFormat;
use neotime_runtime::{AppImage, AppKind};

use super::{
    build_library, draw_every_icon, draw_glyph, icon_source, woff_to_sfnt, IconFonts, IconSource, ICON_SIDE,
    LIBRARY_ID, LIBRARY_NAME,
};
use crate::types::ICON_NAMES;

// ============================================================================
// Helpers
// ============================================================================

const WOFF_FLAVOR: u32 = 0x0001_0000;

/// One table to pack into a test WOFF file.
struct WoffTable {
    tag: [u8; 4],
    checksum: u32,
    stored: Vec<u8>,
    original_length: u32,
}

/// Packs tables into a WOFF 1.0 file: a header, a table directory, then each table's stored bytes.
fn build_woff(tables: &[WoffTable]) -> Vec<u8> {
    let mut header = vec![0u8; super::WOFF_HEADER_BYTES];
    header[0..4].copy_from_slice(b"wOFF");
    header[4..8].copy_from_slice(&WOFF_FLAVOR.to_be_bytes());
    header[12..14].copy_from_slice(&(tables.len() as u16).to_be_bytes());
    let mut directory = Vec::new();
    let mut data = Vec::new();
    let mut offset = super::WOFF_HEADER_BYTES + tables.len() * super::WOFF_TABLE_ENTRY_BYTES;
    for table in tables {
        directory.extend(table.tag);
        directory.extend((offset as u32).to_be_bytes());
        directory.extend((table.stored.len() as u32).to_be_bytes());
        directory.extend(table.original_length.to_be_bytes());
        directory.extend(table.checksum.to_be_bytes());
        data.extend(&table.stored);
        offset += table.stored.len();
    }
    [header, directory, data].concat()
}

/// A table stored as it is.
fn plain_table(tag: &[u8; 4], contents: &[u8]) -> WoffTable {
    WoffTable {
        tag: *tag,
        checksum: 0x1122_3344,
        stored: contents.to_vec(),
        original_length: contents.len() as u32,
    }
}

/// Reads a big-endian u32 out of a byte slice.
fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

/// Reads a big-endian u16 out of a byte slice.
fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

// ============================================================================
// Icon artwork
// ============================================================================

#[test]
fn every_icon_name_has_artwork() {
    let missing: Vec<&str> = ICON_NAMES
        .iter()
        .copied()
        .filter(|name| icon_source(name).is_none())
        .collect();

    assert!(missing.is_empty(), "no artwork for {missing:?}");
}

#[test]
fn an_unknown_icon_name_has_no_artwork() {
    assert_eq!(icon_source("unicorn"), None);
}

#[test]
fn icons_name_the_font_and_glyph_they_come_from() {
    assert_eq!(icon_source("alarm"), Some(IconSource::MaterialIcons(0xe855)));
    assert_eq!(icon_source("heart"), Some(IconSource::FontAwesome(0xf21e)));
    assert_eq!(icon_source("pine_logo"), Some(IconSource::PineLogo));
    assert_eq!(icon_source("none"), Some(IconSource::Empty));
}

// ============================================================================
// Drawing icons
// ============================================================================

#[test]
fn every_icon_is_drawn_as_a_mask_of_icon_size() {
    let pictures = draw_every_icon().unwrap();

    assert_eq!(pictures.len(), ICON_NAMES.len());
    for (name, picture) in ICON_NAMES.iter().zip(&pictures) {
        assert_eq!(picture.format, ImageFormat::Mask, "{name}");
        assert_eq!((picture.width, picture.height), (ICON_SIDE, ICON_SIDE), "{name}");
        assert_eq!(
            picture.pixels.len(),
            ICON_SIDE as usize * ICON_SIDE as usize,
            "{name}"
        );
    }
}

#[test]
fn every_icon_but_none_shows_something() {
    let pictures = draw_every_icon().unwrap();

    for (name, picture) in ICON_NAMES.iter().zip(&pictures) {
        let shows_something = picture.pixels.iter().any(|&coverage| coverage > 0);
        assert_eq!(shows_something, *name != "none", "{name}");
    }
}

#[test]
fn the_empty_icon_is_entirely_clear() {
    let fonts = IconFonts::load().unwrap();

    let picture = fonts.draw("none").unwrap();

    assert!(picture.pixels.iter().all(|&coverage| coverage == 0));
}

#[test]
fn the_pine_logo_sits_in_the_middle_of_its_square() {
    let fonts = IconFonts::load().unwrap();

    let picture = fonts.draw("pine_logo").unwrap();

    let side = ICON_SIDE as usize;
    let centre = picture.pixels[(side / 2) * side + side / 2];
    assert!(centre > 0, "the centre of the pine logo is clear");
}

#[test]
fn icons_that_share_a_glyph_are_drawn_the_same() {
    let fonts = IconFonts::load().unwrap();

    let sun = fonts.draw("sun").unwrap();
    let clear = fonts.draw("clear").unwrap();

    assert_eq!(sun, clear);
}

#[test]
fn drawing_an_unknown_icon_is_refused() {
    let fonts = IconFonts::load().unwrap();

    let problem = fonts.draw("unicorn").unwrap_err();

    assert_eq!(problem, "no artwork is chosen for the icon `unicorn`");
}

#[test]
fn a_glyph_the_font_lacks_is_refused() {
    let fonts = IconFonts::load().unwrap();

    let problem = draw_glyph(&fonts.font_awesome, 0x10_FFFD, "missing").unwrap_err();

    assert_eq!(problem, "the font has no glyph 0x10fffd for the icon `missing`");
}

#[test]
fn a_code_point_that_is_not_a_character_is_refused() {
    let fonts = IconFonts::load().unwrap();

    let problem = draw_glyph(&fonts.material_icons, 0xD800, "surrogate").unwrap_err();

    assert_eq!(problem, "`surrogate` has no valid code point");
}

// ============================================================================
// WOFF
// ============================================================================

#[test]
fn a_font_that_is_not_woff_passes_through_unchanged() {
    let font = b"\x00\x01\x00\x00plain truetype".to_vec();

    let unpacked = woff_to_sfnt(&font).unwrap();

    assert_eq!(unpacked, font);
}

#[test]
fn a_woff_font_is_unpacked_into_padded_sfnt_tables() {
    let compressible = [7u8; 40];
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&compressible, 6);
    assert!(compressed.len() < compressible.len());
    let woff = build_woff(&[
        plain_table(b"head", &[1, 2, 3]),
        WoffTable {
            tag: *b"name",
            checksum: 0x5566_7788,
            stored: compressed,
            original_length: compressible.len() as u32,
        },
    ]);

    let sfnt = woff_to_sfnt(&woff).unwrap();

    assert_eq!(read_u32(&sfnt, 0), WOFF_FLAVOR);
    assert_eq!(read_u16(&sfnt, 4), 2);
    assert_eq!(read_u16(&sfnt, 6), 32);
    assert_eq!(read_u16(&sfnt, 8), 1);
    assert_eq!(read_u16(&sfnt, 10), 0);
    assert_eq!(&sfnt[12..16], b"head");
    assert_eq!(read_u32(&sfnt, 16), 0x1122_3344);
    assert_eq!(read_u32(&sfnt, 20), 44);
    assert_eq!(read_u32(&sfnt, 24), 3);
    assert_eq!(&sfnt[28..32], b"name");
    assert_eq!(read_u32(&sfnt, 32), 0x5566_7788);
    assert_eq!(read_u32(&sfnt, 36), 48);
    assert_eq!(read_u32(&sfnt, 40), 40);
    assert_eq!(&sfnt[44..48], &[1, 2, 3, 0]);
    assert_eq!(&sfnt[48..88], &compressible);
    assert_eq!(sfnt.len(), 88);
}

#[test]
fn the_sfnt_search_fields_follow_the_table_count() {
    let tables: Vec<WoffTable> = (0..5u8)
        .map(|index| plain_table(&[b't', b'a', b'b', index], &[index]))
        .collect();
    let woff = build_woff(&tables);

    let sfnt = woff_to_sfnt(&woff).unwrap();

    assert_eq!(read_u16(&sfnt, 4), 5);
    assert_eq!(read_u16(&sfnt, 6), 64);
    assert_eq!(read_u16(&sfnt, 8), 2);
    assert_eq!(read_u16(&sfnt, 10), 16);
}

#[test]
fn a_woff_header_cut_short_is_refused() {
    let problem = woff_to_sfnt(b"wOFF\x00\x01").unwrap_err();

    assert_eq!(problem, "the WOFF font is cut short");
}

#[test]
fn a_woff_directory_cut_short_is_refused() {
    let mut woff = build_woff(&[plain_table(b"head", &[1, 2, 3, 4])]);
    woff.truncate(super::WOFF_HEADER_BYTES + 2);

    let problem = woff_to_sfnt(&woff).unwrap_err();

    assert_eq!(problem, "the WOFF font is cut short");
}

#[test]
fn a_woff_table_outside_the_file_is_refused() {
    let mut woff = build_woff(&[plain_table(b"head", &[1, 2, 3, 4])]);
    woff.truncate(woff.len() - 1);

    let problem = woff_to_sfnt(&woff).unwrap_err();

    assert_eq!(problem, "a WOFF table lies outside the file");
}

#[test]
fn a_woff_table_that_does_not_decompress_is_refused() {
    let woff = build_woff(&[WoffTable {
        tag: *b"glyf",
        checksum: 0,
        stored: vec![0xFF, 0xFF, 0xFF, 0xFF],
        original_length: 100,
    }]);

    let problem = woff_to_sfnt(&woff).unwrap_err();

    assert_eq!(problem, "a WOFF table does not decompress");
}

#[test]
fn a_woff_table_of_the_wrong_length_is_refused() {
    let woff = build_woff(&[WoffTable {
        tag: *b"glyf",
        checksum: 0,
        stored: vec![1, 2, 3, 4, 5, 6],
        original_length: 4,
    }]);

    let problem = woff_to_sfnt(&woff).unwrap_err();

    assert_eq!(problem, "a WOFF table has the wrong length");
}

#[test]
fn a_woff_table_that_decompresses_to_the_wrong_length_is_refused() {
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&[0u8; 30], 6);
    let woff = build_woff(&[WoffTable {
        tag: *b"glyf",
        checksum: 0,
        stored: compressed,
        original_length: 50,
    }]);

    let problem = woff_to_sfnt(&woff).unwrap_err();

    assert_eq!(problem, "a WOFF table has the wrong length");
}

// ============================================================================
// The library image
// ============================================================================

#[test]
fn the_library_image_holds_every_icon_and_passes_the_install_checks() {
    let bytes = build_library().unwrap();

    let image = AppImage::parse(&bytes).unwrap();
    assert_eq!(image.kind, AppKind::Library);
    assert_eq!(image.string(image.id_string), Some(LIBRARY_ID));
    assert_eq!(image.string(image.name_string), Some(LIBRARY_NAME));
    assert_eq!(image.resource_count() as usize, ICON_NAMES.len());
    let first = image.resource(0).unwrap();
    assert_eq!(
        (first.format, first.width, first.height),
        (ImageFormat::Mask, ICON_SIDE, ICON_SIDE)
    );
}

#[test]
fn the_library_carries_the_text_font() {
    let bytes = build_library().unwrap();
    let image = neotime_runtime::AppImage::parse(&bytes).unwrap();
    assert_eq!(image.font_count(), 1);
    for (index, expected) in super::LIBRARY_FONTS.iter().enumerate() {
        let font = image.font(index as u16).unwrap();
        assert_eq!(&font[..4], b"NTGF");
        assert_eq!(font, *expected);
    }
}

#[test]
fn the_icons_native_widgets_draw_have_the_numbers_the_runtime_expects() {
    use neotime_runtime::resources::library_icon;
    let number_of = |name: &str| {
        crate::types::ICON_NAMES
            .iter()
            .position(|icon| *icon == name)
            .unwrap() as u16
    };

    assert_eq!(number_of("alarm"), library_icon::ALARM);
    assert_eq!(number_of("bluetooth"), library_icon::BLUETOOTH);
    assert_eq!(number_of("charging"), library_icon::CHARGING);
    assert_eq!(number_of("notification"), library_icon::NOTIFICATION);
}

// ============================================================================
// Files for the firmware
// ============================================================================

#[test]
fn the_library_carries_each_accelerometers_configuration_in_library_data_order() {
    use neotime_runtime::LibraryData;

    let bytes = build_library().unwrap();
    let image = AppImage::parse(&bytes).unwrap();

    assert_eq!(
        image.data(LibraryData::Bma421Configuration as u16),
        Some(neotime_accelerometer::BMA421_CONFIGURATION_FILE)
    );
    assert_eq!(
        image.data(LibraryData::Bma425Configuration as u16),
        Some(neotime_accelerometer::BMA425_CONFIGURATION_FILE)
    );
}
