//! The shared library image: every icon in `ICON_NAMES`, in that order, as a mask of one size, and
//! the files the firmware reads, such as the accelerometer's configuration, in `LibraryData` order.
//!
//! The icons are drawn from free icon fonts and one picture kept in `third_party/`, whose README
//! says where each came from and under which licence. docs/compiler.md lists the glyph each name
//! uses.

use ab_glyph::{Font, FontVec, GlyphId, PxScale};
use battletime_runtime::resources::ImageFormat;
use battletime_runtime::writer::ImageWriter;
use battletime_runtime::AppKind;

use crate::compiler::SHARED_LIBRARY_VERSION;
use crate::images::{centre_mask, decode_png_alpha, Picture};
use crate::types::ICON_NAMES;

// ============================================================================
// Sizes and sources
// ============================================================================

/// The width and height of every icon in the library, in pixels.
pub const ICON_SIDE: u16 = 32;

/// The id and name the library image carries.
pub const LIBRARY_ID: &str = "battletime.shared";
pub const LIBRARY_NAME: &str = "BattleTime shared library";

const FONT_AWESOME: &[u8] =
    include_bytes!("../../third_party/font-awesome/FontAwesome5-Solid+Brands+Regular.woff");
const MATERIAL_ICONS: &[u8] = include_bytes!("../../third_party/material-icons/MaterialIcons-Regular.ttf");
const PINE_LOGO: &[u8] = include_bytes!("../../third_party/infinitime/pine_logo.png");

/// Where an icon's artwork comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconSource {
    /// A glyph of Font Awesome 5 Free, by code point.
    FontAwesome(u32),
    /// A glyph of Google's Material Icons, by code point.
    MaterialIcons(u32),
    /// InfiniTime's pine cone picture.
    PineLogo,
    /// Nothing drawn.
    Empty,
}

/// The artwork of each icon, by name; code points follow InfiniTime's Symbols.h where it has the icon.
pub fn icon_source(name: &str) -> Option<IconSource> {
    use IconSource::{FontAwesome, MaterialIcons};
    Some(match name {
        "alarm" => MaterialIcons(0xe855),
        "backspace" => FontAwesome(0xf55a),
        "battery" => FontAwesome(0xf242),
        "bell" => FontAwesome(0xf0f3),
        "bell_off" => FontAwesome(0xf1f6),
        "bluetooth" => FontAwesome(0xf294),
        "calculator" => FontAwesome(0xf1ec),
        "charging" => FontAwesome(0xf1e6),
        "chart" => FontAwesome(0xf080),
        "check" => FontAwesome(0xf560),
        "clock" => FontAwesome(0xf017),
        "close" => FontAwesome(0xf00d),
        "clouds" => FontAwesome(0xf0c2),
        "dice" => FontAwesome(0xf522),
        "few_clouds" => FontAwesome(0xf6c4),
        "flag" => FontAwesome(0xf024),
        "flashlight" => MaterialIcons(0xf00b),
        "game" => FontAwesome(0xf11b),
        "heart" => FontAwesome(0xf21e),
        "heavy_clouds" => FontAwesome(0xf73b),
        "level" => FontAwesome(0xf05b),
        "clear" => FontAwesome(0xf185),
        "metronome" => FontAwesome(0xf569),
        "minus" => FontAwesome(0xf068),
        "mist" => FontAwesome(0xf75f),
        "moon" => FontAwesome(0xf186),
        "music" => FontAwesome(0xf001),
        "navigation" => FontAwesome(0xf124),
        "next" => FontAwesome(0xf051),
        "notification" => FontAwesome(0xf0e0),
        "pause" => FontAwesome(0xf04c),
        "phone" => FontAwesome(0xf095),
        "play" => FontAwesome(0xf04b),
        "plus" => FontAwesome(0xf067),
        "previous" => FontAwesome(0xf048),
        "rain" => FontAwesome(0xf740),
        "settings" => MaterialIcons(0xe8b8),
        "showers" => FontAwesome(0xf743),
        "snow" => FontAwesome(0xf2dc),
        "steps" => MaterialIcons(0xe566),
        "stop" => FontAwesome(0xf04d),
        "stopwatch" => FontAwesome(0xf2f2),
        "sun" => FontAwesome(0xf185),
        "thunderstorm" => FontAwesome(0xf0e7),
        "timer" => FontAwesome(0xf252),
        "turn_arrive" => FontAwesome(0xf11e),
        "turn_left" => MaterialIcons(0xeba6),
        "turn_right" => MaterialIcons(0xebab),
        "turn_roundabout" => MaterialIcons(0xeba3),
        "turn_sharp_left" => MaterialIcons(0xeba7),
        "turn_sharp_right" => MaterialIcons(0xebaa),
        "turn_slight_left" => MaterialIcons(0xeba4),
        "turn_slight_right" => MaterialIcons(0xeb9a),
        "turn_straight" => MaterialIcons(0xeb95),
        "turn_u_turn" => MaterialIcons(0xeba1),
        "volume_down" => FontAwesome(0xf027),
        "volume_up" => FontAwesome(0xf028),
        "weather" => FontAwesome(0xf6c4),
        "none" => IconSource::Empty,
        "weather_night_clear" => FontAwesome(0xf186),
        "weather_night_cloudy" => FontAwesome(0xf6c3),
        "pine_logo" => IconSource::PineLogo,
        "info" => FontAwesome(0xf05a),
        "remote" => MaterialIcons(0xe8c7),
        "note" => MaterialIcons(0xf1fc),
        "qr_code" => FontAwesome(0xf029),
        "phone_missed" => FontAwesome(0xf3dd),
        "sms" => FontAwesome(0xf4ad),
        "chat" => FontAwesome(0xf086),
        "news" => FontAwesome(0xf1ea),
        "voicemail" => MaterialIcons(0xe0d9),
        "calendar" => FontAwesome(0xf073),
        _ => return None,
    })
}

// ============================================================================
// Drawing icons
// ============================================================================

/// The two icon fonts, loaded once for a run of drawing.
pub struct IconFonts {
    font_awesome: FontVec,
    material_icons: FontVec,
}

impl IconFonts {
    pub fn load() -> Result<IconFonts, String> {
        let font_awesome = woff_to_sfnt(FONT_AWESOME)?;
        Ok(IconFonts {
            font_awesome: FontVec::try_from_vec(font_awesome)
                .map_err(|_| "the Font Awesome font cannot be read".to_string())?,
            material_icons: FontVec::try_from_vec(MATERIAL_ICONS.to_vec())
                .map_err(|_| "the Material Icons font cannot be read".to_string())?,
        })
    }

    /// Draws one icon by name as a mask of `ICON_SIDE` pixels on each side.
    pub fn draw(&self, name: &str) -> Result<Picture, String> {
        let source =
            icon_source(name).ok_or_else(|| format!("no artwork is chosen for the icon `{name}`"))?;
        match source {
            IconSource::FontAwesome(code_point) => draw_glyph(&self.font_awesome, code_point, name),
            IconSource::MaterialIcons(code_point) => draw_glyph(&self.material_icons, code_point, name),
            IconSource::PineLogo => {
                let mask =
                    decode_png_alpha(PINE_LOGO).map_err(|problem| format!("the pine logo {problem}"))?;
                Ok(centre_mask(&mask, ICON_SIDE))
            }
            IconSource::Empty => Ok(Picture {
                format: ImageFormat::Mask,
                width: ICON_SIDE,
                height: ICON_SIDE,
                pixels: vec![0; ICON_SIDE as usize * ICON_SIDE as usize],
            }),
        }
    }
}

/// Draws a glyph with its em square as tall as an icon, shrunk if it would not fit, then centres it.
fn draw_glyph(font: &FontVec, code_point: u32, name: &str) -> Result<Picture, String> {
    let character = char::from_u32(code_point).ok_or_else(|| format!("`{name}` has no valid code point"))?;
    let glyph_id = font.glyph_id(character);
    if glyph_id == GlyphId(0) {
        return Err(format!(
            "the font has no glyph {code_point:#x} for the icon `{name}`"
        ));
    }
    let side = ICON_SIDE as f32;
    let units_per_em = font.units_per_em().unwrap_or(1000.0);
    let em_scale = side * font.height_unscaled() / units_per_em;
    let mut scale = em_scale;
    for _ in 0..2 {
        let glyph = glyph_id.with_scale(PxScale::from(scale));
        let outlined = font
            .outline_glyph(glyph)
            .ok_or_else(|| format!("the glyph for `{name}` has no outline"))?;
        let bounds = outlined.px_bounds();
        let too_large = (side / bounds.width()).min(side / bounds.height());
        if too_large < 1.0 {
            scale *= too_large * 0.999;
            continue;
        }
        let width = bounds.width() as u16;
        let height = bounds.height() as u16;
        let mut pixels = vec![0u16; width as usize * height as usize];
        outlined.draw(|x, y, coverage| {
            if let Some(pixel) = pixels.get_mut(y as usize * width as usize + x as usize) {
                *pixel = (coverage.clamp(0.0, 1.0) * 255.0).round() as u16;
            }
        });
        let glyph_picture = Picture {
            format: ImageFormat::Mask,
            width,
            height,
            pixels,
        };
        return Ok(centre_mask(&glyph_picture, ICON_SIDE));
    }
    Err(format!(
        "the glyph for `{name}` does not fit in {ICON_SIDE} pixels"
    ))
}

// ============================================================================
// WOFF
// ============================================================================

const WOFF_HEADER_BYTES: usize = 44;
const WOFF_TABLE_ENTRY_BYTES: usize = 20;
const SFNT_TABLE_RECORD_BYTES: usize = 16;

/// Unpacks a WOFF 1.0 font into the plain TrueType or OpenType file it wraps.
pub fn woff_to_sfnt(woff: &[u8]) -> Result<Vec<u8>, String> {
    let read_u16 = |offset: usize| -> Result<u16, String> {
        woff.get(offset..offset + 2)
            .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
            .ok_or_else(|| "the WOFF font is cut short".to_string())
    };
    let read_u32 = |offset: usize| -> Result<u32, String> {
        woff.get(offset..offset + 4)
            .map(|bytes| u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            .ok_or_else(|| "the WOFF font is cut short".to_string())
    };
    // InfiniTime's copy is named .woff but holds a plain TrueType font, which needs no unpacking.
    if woff.get(0..4) != Some(b"wOFF") {
        return Ok(woff.to_vec());
    }
    let flavor = read_u32(4)?;
    let table_count = read_u16(12)? as usize;

    let mut tables: Vec<([u8; 4], u32, Vec<u8>)> = Vec::with_capacity(table_count);
    for index in 0..table_count {
        let entry = WOFF_HEADER_BYTES + index * WOFF_TABLE_ENTRY_BYTES;
        let tag: [u8; 4] = woff
            .get(entry..entry + 4)
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| "the WOFF font is cut short".to_string())?;
        let offset = read_u32(entry + 4)? as usize;
        let compressed_length = read_u32(entry + 8)? as usize;
        let original_length = read_u32(entry + 12)? as usize;
        let checksum = read_u32(entry + 16)?;
        let stored = woff
            .get(offset..offset + compressed_length)
            .ok_or_else(|| "a WOFF table lies outside the file".to_string())?;
        let data = if compressed_length < original_length {
            miniz_oxide::inflate::decompress_to_vec_zlib(stored)
                .map_err(|_| "a WOFF table does not decompress".to_string())?
        } else {
            stored.to_vec()
        };
        if data.len() != original_length {
            return Err("a WOFF table has the wrong length".to_string());
        }
        tables.push((tag, checksum, data));
    }

    let mut entry_selector = 0u16;
    while (2usize << entry_selector) <= table_count {
        entry_selector += 1;
    }
    let search_range = (1u16 << entry_selector) * 16;
    let range_shift = (table_count as u16) * 16 - search_range;

    let mut sfnt: Vec<u8> = Vec::new();
    sfnt.extend(flavor.to_be_bytes());
    sfnt.extend((table_count as u16).to_be_bytes());
    sfnt.extend(search_range.to_be_bytes());
    sfnt.extend(entry_selector.to_be_bytes());
    sfnt.extend(range_shift.to_be_bytes());
    let mut offset = 12 + table_count * SFNT_TABLE_RECORD_BYTES;
    for (tag, checksum, data) in &tables {
        sfnt.extend(tag);
        sfnt.extend(checksum.to_be_bytes());
        sfnt.extend((offset as u32).to_be_bytes());
        sfnt.extend((data.len() as u32).to_be_bytes());
        offset += data.len().next_multiple_of(4);
    }
    for (_, _, data) in &tables {
        sfnt.extend(data);
        sfnt.resize(sfnt.len().next_multiple_of(4), 0);
    }
    Ok(sfnt)
}

// ============================================================================
// The library image
// ============================================================================

/// Draws every icon in `ICON_NAMES`, in order.
pub fn draw_every_icon() -> Result<Vec<Picture>, String> {
    let fonts = IconFonts::load()?;
    ICON_NAMES.iter().map(|name| fonts.draw(name)).collect()
}

/// The text font `tools/generate_fonts.py` made, drawn from lines and arcs, which every text size scales.
pub const LIBRARY_FONTS: [&[u8]; 1] = [include_bytes!("../../app-host/fonts/geometric.ntfont")];

/// Writes the shared library image, which the runtime's install checks accept.
pub fn build_library() -> Result<Vec<u8>, String> {
    let writer = ImageWriter::new(LIBRARY_ID, LIBRARY_NAME, AppKind::Library, SHARED_LIBRARY_VERSION);
    let (mut writer, _) = crate::shared_functions::compile_library_functions(writer)?;
    for picture in draw_every_icon()? {
        writer.image_resource(picture.format, picture.width, picture.height, &picture.pixels);
    }
    for font in LIBRARY_FONTS {
        writer.font(font);
    }
    // In `LibraryData` order, so the firmware finds each chip's file by its number.
    writer.data(battletime_accelerometer::BMA421_CONFIGURATION_FILE);
    writer.data(battletime_accelerometer::BMA425_CONFIGURATION_FILE);
    let image = writer.build();
    crate::check_image(&image)?;
    Ok(image)
}

#[cfg(test)]
#[path = "library_tests.rs"]
mod library_tests;
