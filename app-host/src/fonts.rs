//! The fonts an app picks with its `font` property, drawn through rlvgl's anti-aliased glyph path.
//!
//! Until the shared library carries real fonts, every size is an `embedded-graphics` mono font,
//! scaled up by whole pixels for the larger sizes.

use embedded_graphics::{
    image::GetPixel,
    mono_font::{
        ascii::{FONT_10X20, FONT_9X15},
        MonoFont,
    },
    pixelcolor::BinaryColor,
    prelude::*,
};
use rlvgl_core::font::{FontLineMetrics, FontMetrics, GlyphInfo};

// ============================================================================
// Scaled mono fonts
// ============================================================================

const FULL_COVERAGE: u8 = 255;
const NO_COVERAGE: u8 = 0;
const SUBPIXELS_PER_PIXEL: u16 = 16;

/// An `embedded-graphics` mono font drawn `scale` times its size.
pub struct ScaledMonoFont {
    font: &'static MonoFont<'static>,
    scale: u8,
}

impl ScaledMonoFont {
    pub const fn new(font: &'static MonoFont<'static>, scale: u8) -> Self {
        ScaledMonoFont { font, scale }
    }

    fn scale(&self) -> u32 {
        self.scale.max(1) as u32
    }

    /// The top-left corner of a character's cell in the font's image.
    fn cell_origin(&self, character: char) -> Option<Point> {
        let cell_width = self.font.character_size.width;
        let cell_height = self.font.character_size.height;
        let cells_per_row = self.font.image.size().width.checked_div(cell_width)?;
        let glyph = self.font.glyph_mapping.index(character) as u32;
        let row = glyph.checked_div(cells_per_row)?;
        let column = glyph - row * cells_per_row;
        Some(Point::new(
            (column * cell_width) as i32,
            (row * cell_height) as i32,
        ))
    }
}

impl FontMetrics for ScaledMonoFont {
    fn glyph_metrics(&self, _character: char) -> Option<GlyphInfo> {
        let scale = self.scale();
        let size = self.font.character_size;
        let advance = (size.width + self.font.character_spacing) * scale;
        Some(GlyphInfo {
            advance_fp16: (advance * SUBPIXELS_PER_PIXEL as u32) as u16,
            bearing_x: 0,
            bearing_y: (self.font.baseline * scale) as i16,
            width: (size.width * scale) as u16,
            height: (size.height * scale) as u16,
        })
    }

    fn line_metrics(&self) -> FontLineMetrics {
        let scale = self.scale();
        let height = self.font.character_size.height;
        FontLineMetrics {
            line_height: (height * scale) as u16,
            ascent: (self.font.baseline * scale) as i16,
            descent: ((height - self.font.baseline) * scale) as i16,
        }
    }

    fn glyph_coverage_row(&self, character: char, row: u16, x_offset: u16, coverage: &mut [u8]) -> bool {
        let Some(cell) = self.cell_origin(character) else {
            return false;
        };
        let scale = self.scale();
        let size = self.font.character_size;
        let source_row = row as u32 / scale;

        for (offset, pixel_coverage) in coverage.iter_mut().enumerate() {
            let source_column = (x_offset as u32 + offset as u32) / scale;
            let is_inside = source_row < size.height && source_column < size.width;
            let point = cell + Point::new(source_column as i32, source_row as i32);
            let is_lit = is_inside && self.font.image.pixel(point) == Some(BinaryColor::On);
            *pixel_coverage = if is_lit { FULL_COVERAGE } else { NO_COVERAGE };
        }
        true
    }
}

// ============================================================================
// Font numbers
// ============================================================================

static SMALL: ScaledMonoFont = ScaledMonoFont::new(&FONT_9X15, 1);
static NORMAL: ScaledMonoFont = ScaledMonoFont::new(&FONT_10X20, 1);
static MEDIUM: ScaledMonoFont = ScaledMonoFont::new(&FONT_9X15, 2);
static LARGE: ScaledMonoFont = ScaledMonoFont::new(&FONT_10X20, 2);
static HUGE: ScaledMonoFont = ScaledMonoFont::new(&FONT_10X20, 4);
static GIANT: ScaledMonoFont = ScaledMonoFont::new(&FONT_10X20, 6);

/// The `font` property's values, in the order docs/app-reference.md lists the fonts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontName {
    Small,
    Normal,
    Medium,
    Large,
    Huge,
    Giant,
    SevenSegment,
    DotMatrix,
    Condensed,
}

impl FontName {
    /// The font a `font` value names; an unknown number gives the normal font.
    pub fn from_value(value: i32) -> FontName {
        use FontName::*;
        match value {
            0 => Small,
            2 => Medium,
            3 => Large,
            4 => Huge,
            5 => Giant,
            6 => SevenSegment,
            7 => DotMatrix,
            8 => Condensed,
            _ => Normal,
        }
    }

    /// The font drawn for this name; the display fonts fall back to the large font for now.
    pub fn metrics(self) -> &'static dyn FontMetrics {
        use FontName::*;
        match self {
            Small => &SMALL,
            Normal => &NORMAL,
            Medium => &MEDIUM,
            Large | SevenSegment | DotMatrix | Condensed => &LARGE,
            Huge => &HUGE,
            Giant => &GIANT,
        }
    }
}

/// How wide a line of text is in a font, in whole pixels.
pub fn text_width(font: &dyn FontMetrics, text: &str) -> i32 {
    (font.measure_fp16(text) + SUBPIXELS_PER_PIXEL as i32 - 1) / SUBPIXELS_PER_PIXEL as i32
}

/// How tall a line of text is in a font.
pub fn line_height(font: &dyn FontMetrics) -> i32 {
    font.line_metrics().line_height as i32
}
