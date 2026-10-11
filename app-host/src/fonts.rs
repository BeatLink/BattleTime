//! The fonts an app picks with its `font` property, drawn through rlvgl's anti-aliased glyph path.
//!
//! The text fonts are bitmaps made from JetBrains Mono and Open Sans by `tools/generate_fonts.py`,
//! with 4-bit coverage and runs of empty and solid pixels shortened, read in place from flash. The
//! seven-segment and dot-matrix display fonts carry no bitmaps: their glyphs are worked out from
//! segments and dots as they are drawn. docs/app-runtime.md lists the sizes.

use rlvgl_core::{
    font::{FontLineMetrics, FontMetrics, GlyphInfo},
    renderer::Renderer,
    widget::{Color, Rect},
};

// ============================================================================
// Bitmap fonts
// ============================================================================

const SUBPIXELS_PER_PIXEL: i32 = 16;
const FONT_HEADER_BYTES: usize = 16;
const GLYPH_ENTRY_BYTES: usize = 10;
const FONT_MAGIC: &[u8; 4] = b"NTFN";
const EMPTY_RUN: u8 = 0;
const SOLID_RUN: u8 = 15;
const COVERAGE_PER_STEP: u8 = 17;

/// The widest glyph row drawn in one go; wider glyphs are drawn in pieces.
const WIDEST_GLYPH_ROW: usize = 128;

/// A font made by `tools/generate_fonts.py`, read in place.
pub struct BitmapFont {
    bytes: &'static [u8],
}

/// Where one glyph's coverage starts, and its size and place.
#[derive(Clone, Copy)]
struct GlyphEntry {
    offset: usize,
    advance: u8,
    left: i8,
    top: i8,
    width: u8,
    height: u8,
}

fn read_u16(bytes: &[u8], at: usize) -> u16 {
    match bytes.get(at..at + 2) {
        Some(pair) => u16::from_le_bytes([pair[0], pair[1]]),
        None => 0,
    }
}

impl BitmapFont {
    pub const fn new(bytes: &'static [u8]) -> Self {
        BitmapFont { bytes }
    }

    fn is_valid(&self) -> bool {
        self.bytes.len() >= FONT_HEADER_BYTES && &self.bytes[..4] == FONT_MAGIC
    }

    fn glyph_count(&self) -> usize {
        if !self.is_valid() {
            return 0;
        }
        read_u16(self.bytes, 6) as usize
    }

    fn data_start(&self) -> usize {
        FONT_HEADER_BYTES + self.glyph_count() * GLYPH_ENTRY_BYTES
    }

    fn entry_at(&self, index: usize) -> Option<(u16, GlyphEntry)> {
        let start = FONT_HEADER_BYTES + index * GLYPH_ENTRY_BYTES;
        let entry = self.bytes.get(start..start + GLYPH_ENTRY_BYTES)?;
        let character = u16::from_le_bytes([entry[0], entry[1]]);
        let offset = entry[2] as usize | (entry[3] as usize) << 8 | (entry[4] as usize) << 16;
        Some((
            character,
            GlyphEntry {
                offset,
                advance: entry[5],
                left: entry[6] as i8,
                top: entry[7] as i8,
                width: entry[8],
                height: entry[9],
            },
        ))
    }

    fn glyph(&self, character: char) -> Option<GlyphEntry> {
        let wanted = u16::try_from(character as u32).ok()?;
        let mut low = 0;
        let mut high = self.glyph_count();
        while low < high {
            let middle = (low + high) / 2;
            let (found, entry) = self.entry_at(middle)?;
            match found.cmp(&wanted) {
                core::cmp::Ordering::Equal => return Some(entry),
                core::cmp::Ordering::Less => low = middle + 1,
                core::cmp::Ordering::Greater => high = middle,
            }
        }
        None
    }

    /// The glyph's coverage, pixel by pixel from its top-left corner, as 0 to 255.
    fn coverage(&self, glyph: GlyphEntry) -> impl Iterator<Item = u8> + '_ {
        let start = self.data_start() + glyph.offset;
        let data = self.bytes.get(start..).unwrap_or(&[]);
        let mut halves = data.iter().flat_map(|byte| [byte >> 4, byte & 0x0F].into_iter());
        let mut repeat_value = 0;
        let mut repeats_left = 0u8;
        let pixel_count = glyph.width as usize * glyph.height as usize;
        (0..pixel_count).map(move |_| {
            if repeats_left > 0 {
                repeats_left -= 1;
                return repeat_value * COVERAGE_PER_STEP;
            }
            let value = halves.next().unwrap_or(0);
            if value == EMPTY_RUN || value == SOLID_RUN {
                repeat_value = value;
                repeats_left = halves.next().unwrap_or(0);
            }
            value * COVERAGE_PER_STEP
        })
    }

    /// Draws one glyph with its pen at `origin` on the baseline, decoding it once.
    fn draw_character(&self, renderer: &mut dyn Renderer, character: char, origin: (i32, i32), color: Color) {
        let Some(glyph) = self.glyph(character) else {
            return;
        };
        let width = glyph.width as usize;
        if width == 0 {
            return;
        }
        let left = origin.0 + glyph.left as i32;
        let top = origin.1 - glyph.top as i32;
        let mut row = [0u8; WIDEST_GLYPH_ROW];
        let mut pixels = self.coverage(glyph);
        for row_index in 0..glyph.height as i32 {
            let mut column = 0;
            while column < width {
                let piece = (width - column).min(WIDEST_GLYPH_ROW);
                for slot in row[..piece].iter_mut() {
                    *slot = pixels.next().unwrap_or(0);
                }
                renderer.blend_row(left + column as i32, top + row_index, color, &row[..piece]);
                column += piece;
            }
        }
    }
}

impl FontMetrics for BitmapFont {
    fn glyph_metrics(&self, character: char) -> Option<GlyphInfo> {
        let glyph = self.glyph(character)?;
        Some(GlyphInfo {
            advance_fp16: glyph.advance as u16 * SUBPIXELS_PER_PIXEL as u16,
            bearing_x: glyph.left as i16,
            bearing_y: glyph.top as i16,
            width: glyph.width as u16,
            height: glyph.height as u16,
        })
    }

    fn line_metrics(&self) -> FontLineMetrics {
        FontLineMetrics {
            line_height: read_u16(self.bytes, 8),
            ascent: read_u16(self.bytes, 10) as i16,
            descent: read_u16(self.bytes, 12) as i16,
        }
    }

    fn glyph_coverage_row(&self, character: char, row: u16, x_offset: u16, coverage: &mut [u8]) -> bool {
        let Some(glyph) = self.glyph(character) else {
            return false;
        };
        let skipped = row as usize * glyph.width as usize + x_offset as usize;
        let mut pixels = self.coverage(glyph).skip(skipped);
        let row_left = (glyph.width as usize).saturating_sub(x_offset as usize);
        for (index, slot) in coverage.iter_mut().enumerate() {
            *slot = if index < row_left {
                pixels.next().unwrap_or(0)
            } else {
                0
            };
        }
        true
    }
}

// ============================================================================
// Shapes worked out as they are drawn
// ============================================================================

/// Samples taken across and down each pixel to find how much of it a shape covers.
const SAMPLES_PER_SIDE: i32 = 4;
const FULL_COVERAGE: i32 = 255;

/// How much of each pixel in one glyph row a shape covers, from a test of points in glyph pixels.
fn sample_row(row: u16, x_offset: u16, coverage: &mut [u8], is_inside: impl Fn(f32, f32) -> bool) {
    let samples = (SAMPLES_PER_SIDE * SAMPLES_PER_SIDE) as f32;
    for (index, slot) in coverage.iter_mut().enumerate() {
        let column = x_offset as f32 + index as f32;
        let mut hits = 0;
        for down in 0..SAMPLES_PER_SIDE {
            for across in 0..SAMPLES_PER_SIDE {
                let x = column + (across as f32 + 0.5) / SAMPLES_PER_SIDE as f32;
                let y = row as f32 + (down as f32 + 0.5) / SAMPLES_PER_SIDE as f32;
                hits += is_inside(x, y) as i32;
            }
        }
        *slot = (hits as f32 * FULL_COVERAGE as f32 / samples) as u8;
    }
}

// ============================================================================
// Seven segments
// ============================================================================

/// Digits built from seven bars with pointed ends, as on a calculator.
pub struct SevenSegmentFont;

const SEGMENT_DIGIT_WIDTH: f32 = 34.0;
const SEGMENT_DIGIT_HEIGHT: f32 = 60.0;
const SEGMENT_THICKNESS: f32 = 8.0;
const SEGMENT_GAP: f32 = 1.0;
const SEGMENT_DIGIT_ADVANCE: u16 = 42;
const SEGMENT_SIGN_ADVANCE: u16 = 18;
const SEGMENT_SIDE_BEARING: i16 = 4;
const SEGMENT_ROOM_BELOW: i16 = 4;

/// The lit segments of each digit, bit 0 for the top bar and then clockwise, with bit 6 the middle.
const DIGIT_SEGMENTS: [u8; 10] = [
    0b011_1111, 0b000_0110, 0b101_1011, 0b100_1111, 0b110_0110, 0b110_1101, 0b111_1101, 0b000_0111,
    0b111_1111, 0b110_1111,
];
const MIDDLE_SEGMENT: u8 = 0b100_0000;

/// Whether a point is inside a bar along x from `start` to `end`, centred on `middle`, with pointed ends.
fn is_in_bar(along: f32, across: f32, start: f32, end: f32, middle: f32) -> bool {
    let half = SEGMENT_THICKNESS / 2.0;
    let distance = (across - middle).abs();
    distance <= half && along >= start + distance && along <= end - distance
}

fn is_in_segment(segment: u8, x: f32, y: f32) -> bool {
    let half = SEGMENT_THICKNESS / 2.0;
    let left = half;
    let right = SEGMENT_DIGIT_WIDTH - half;
    let top = half;
    let middle = SEGMENT_DIGIT_HEIGHT / 2.0;
    let bottom = SEGMENT_DIGIT_HEIGHT - half;
    let across = |at: f32| is_in_bar(x, y, left + SEGMENT_GAP, right - SEGMENT_GAP, at);
    let upper = |at: f32| is_in_bar(y, x, top + SEGMENT_GAP, middle - SEGMENT_GAP, at);
    let lower = |at: f32| is_in_bar(y, x, middle + SEGMENT_GAP, bottom - SEGMENT_GAP, at);
    match segment {
        0 => across(top),
        1 => upper(right),
        2 => lower(right),
        3 => across(bottom),
        4 => lower(left),
        5 => upper(left),
        _ => across(middle),
    }
}

/// A seven-segment glyph: which segments are lit, or a sign drawn as dots.
#[derive(Clone, Copy)]
enum SegmentGlyph {
    Segments(u8),
    Colon,
    Point,
    Space,
}

impl SevenSegmentFont {
    fn glyph(character: char) -> Option<SegmentGlyph> {
        Some(match character {
            '0'..='9' => SegmentGlyph::Segments(DIGIT_SEGMENTS[character as usize - '0' as usize]),
            '-' => SegmentGlyph::Segments(MIDDLE_SEGMENT),
            ':' => SegmentGlyph::Colon,
            '.' => SegmentGlyph::Point,
            ' ' => SegmentGlyph::Space,
            _ => return None,
        })
    }
}

impl FontMetrics for SevenSegmentFont {
    fn glyph_metrics(&self, character: char) -> Option<GlyphInfo> {
        let (advance, width, height) = match Self::glyph(character)? {
            SegmentGlyph::Segments(_) => (
                SEGMENT_DIGIT_ADVANCE,
                SEGMENT_DIGIT_WIDTH as u16,
                SEGMENT_DIGIT_HEIGHT as u16,
            ),
            SegmentGlyph::Colon | SegmentGlyph::Point => (
                SEGMENT_SIGN_ADVANCE,
                SEGMENT_THICKNESS as u16,
                SEGMENT_DIGIT_HEIGHT as u16,
            ),
            SegmentGlyph::Space => (SEGMENT_DIGIT_ADVANCE, 0, 0),
        };
        let side_bearing = match Self::glyph(character)? {
            SegmentGlyph::Segments(_) => SEGMENT_SIDE_BEARING,
            _ => (SEGMENT_SIGN_ADVANCE as i16 - SEGMENT_THICKNESS as i16) / 2,
        };
        Some(GlyphInfo {
            advance_fp16: advance * SUBPIXELS_PER_PIXEL as u16,
            bearing_x: side_bearing,
            bearing_y: SEGMENT_DIGIT_HEIGHT as i16,
            width,
            height,
        })
    }

    fn line_metrics(&self) -> FontLineMetrics {
        let ascent = SEGMENT_DIGIT_HEIGHT as i16 + SEGMENT_ROOM_BELOW;
        FontLineMetrics {
            line_height: (ascent + SEGMENT_ROOM_BELOW) as u16,
            ascent,
            descent: SEGMENT_ROOM_BELOW,
        }
    }

    fn glyph_coverage_row(&self, character: char, row: u16, x_offset: u16, coverage: &mut [u8]) -> bool {
        let Some(glyph) = Self::glyph(character) else {
            return false;
        };
        let half = SEGMENT_THICKNESS / 2.0;
        let is_in_dot =
            |x: f32, y: f32, centre_y: f32| (x - half).abs() <= half && (y - centre_y).abs() <= half;
        sample_row(row, x_offset, coverage, |x, y| match glyph {
            SegmentGlyph::Segments(lit) => {
                (0..7).any(|segment| lit & (1 << segment) != 0 && is_in_segment(segment, x, y))
            }
            SegmentGlyph::Colon => {
                is_in_dot(x, y, SEGMENT_DIGIT_HEIGHT * 0.3) || is_in_dot(x, y, SEGMENT_DIGIT_HEIGHT * 0.7)
            }
            SegmentGlyph::Point => is_in_dot(x, y, SEGMENT_DIGIT_HEIGHT - half),
            SegmentGlyph::Space => false,
        });
        true
    }
}

// ============================================================================
// Dot matrix
// ============================================================================

/// Digits and capitals drawn as round dots on a 5 by 7 grid, as on an old pager.
pub struct DotMatrixFont;

const DOT_COLUMNS: u16 = 5;
const DOT_ROWS: u16 = 7;
const DOT_PITCH: f32 = 5.5;
const DOT_RADIUS: f32 = 2.2;
const DOT_ROOM_AROUND: i16 = 3;

/// Each glyph's rows from the top, bit 4 the leftmost dot.
fn dot_rows(character: char) -> Option<[u8; 7]> {
    Some(match character.to_ascii_uppercase() {
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        'A' => [0x0E, 0x11, 0x11, 0x11, 0x1F, 0x11, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' => [0x1C, 0x12, 0x11, 0x11, 0x11, 0x12, 0x1C],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x11, 0x19, 0x15, 0x13, 0x11, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0A],
        'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x11, 0x0A, 0x04, 0x04, 0x04],
        'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        '-' => [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
        ':' => [0x00, 0x0C, 0x0C, 0x00, 0x0C, 0x0C, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C],
        '/' => [0x00, 0x01, 0x02, 0x04, 0x08, 0x10, 0x00],
        ' ' => [0; 7],
        _ => return None,
    })
}

impl FontMetrics for DotMatrixFont {
    fn glyph_metrics(&self, character: char) -> Option<GlyphInfo> {
        dot_rows(character)?;
        Some(GlyphInfo {
            advance_fp16: ((DOT_COLUMNS + 1) as f32 * DOT_PITCH * SUBPIXELS_PER_PIXEL as f32) as u16,
            bearing_x: 0,
            bearing_y: (DOT_ROWS as f32 * DOT_PITCH) as i16,
            width: (DOT_COLUMNS as f32 * DOT_PITCH) as u16,
            height: (DOT_ROWS as f32 * DOT_PITCH) as u16,
        })
    }

    fn line_metrics(&self) -> FontLineMetrics {
        let ascent = (DOT_ROWS as f32 * DOT_PITCH) as i16 + DOT_ROOM_AROUND;
        FontLineMetrics {
            line_height: (ascent + DOT_ROOM_AROUND) as u16,
            ascent,
            descent: DOT_ROOM_AROUND,
        }
    }

    fn glyph_coverage_row(&self, character: char, row: u16, x_offset: u16, coverage: &mut [u8]) -> bool {
        let Some(rows) = dot_rows(character) else {
            return false;
        };
        sample_row(row, x_offset, coverage, |x, y| {
            let column = (x / DOT_PITCH) as i32;
            let dot_row = (y / DOT_PITCH) as i32;
            if !(0..DOT_COLUMNS as i32).contains(&column) || !(0..DOT_ROWS as i32).contains(&dot_row) {
                return false;
            }
            let is_lit = rows[dot_row as usize] & (1 << (DOT_COLUMNS as i32 - 1 - column)) != 0;
            let centre_x = (column as f32 + 0.5) * DOT_PITCH;
            let centre_y = (dot_row as f32 + 0.5) * DOT_PITCH;
            let distance_squared = (x - centre_x) * (x - centre_x) + (y - centre_y) * (y - centre_y);
            is_lit && distance_squared <= DOT_RADIUS * DOT_RADIUS
        });
        true
    }
}

// ============================================================================
// Font numbers
// ============================================================================

static SMALL: BitmapFont = BitmapFont::new(include_bytes!("../fonts/small.ntfont"));
static NORMAL: BitmapFont = BitmapFont::new(include_bytes!("../fonts/normal.ntfont"));
static MEDIUM: BitmapFont = BitmapFont::new(include_bytes!("../fonts/medium.ntfont"));
static LARGE: BitmapFont = BitmapFont::new(include_bytes!("../fonts/large.ntfont"));
static HUGE: BitmapFont = BitmapFont::new(include_bytes!("../fonts/huge.ntfont"));
static GIANT: BitmapFont = BitmapFont::new(include_bytes!("../fonts/giant.ntfont"));
static CONDENSED: BitmapFont = BitmapFont::new(include_bytes!("../fonts/condensed.ntfont"));
static SEVEN_SEGMENT: SevenSegmentFont = SevenSegmentFont;
static DOT_MATRIX: DotMatrixFont = DotMatrixFont;

/// How many bytes of flash the bitmap fonts take, for the memory map.
pub fn bitmap_font_bytes() -> usize {
    [&SMALL, &NORMAL, &MEDIUM, &LARGE, &HUGE, &GIANT, &CONDENSED]
        .iter()
        .map(|font| font.bytes.len())
        .sum()
}

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

    fn bitmap(self) -> Option<&'static BitmapFont> {
        use FontName::*;
        match self {
            Small => Some(&SMALL),
            Normal => Some(&NORMAL),
            Medium => Some(&MEDIUM),
            Large => Some(&LARGE),
            Huge => Some(&HUGE),
            Giant => Some(&GIANT),
            Condensed => Some(&CONDENSED),
            SevenSegment | DotMatrix => None,
        }
    }

    /// The font drawn for this name.
    pub fn metrics(self) -> &'static dyn FontMetrics {
        match self {
            FontName::SevenSegment => &SEVEN_SEGMENT,
            FontName::DotMatrix => &DOT_MATRIX,
            other => other.bitmap().unwrap_or(&NORMAL),
        }
    }

    /// Draws one character with its pen at `origin` on the baseline; returns how far the pen moves.
    pub fn draw_character(
        self,
        renderer: &mut dyn Renderer,
        character: char,
        origin: (i32, i32),
        color: Color,
    ) -> i32 {
        let advance = character_advance(self.metrics(), character);
        match self.bitmap() {
            Some(font) => font.draw_character(renderer, character, origin, color),
            None => renderer.draw_glyph(self.metrics(), character, origin, color),
        }
        advance
    }
}

/// How far the pen moves past one character, in whole pixels.
pub fn character_advance(font: &dyn FontMetrics, character: char) -> i32 {
    font.glyph_metrics(character).map_or(0, |glyph| {
        (glyph.advance_fp16 as i32 + SUBPIXELS_PER_PIXEL / 2) / SUBPIXELS_PER_PIXEL
    })
}

/// How wide a line of text is in a font, in whole pixels.
pub fn text_width(font: &dyn FontMetrics, text: &str) -> i32 {
    text.chars()
        .map(|character| character_advance(font, character))
        .sum()
}

/// How tall a line of text is in a font.
pub fn line_height(font: &dyn FontMetrics) -> i32 {
    font.line_metrics().line_height as i32
}

/// A glyph's box with its pen at `origin`, for tests.
pub fn glyph_box(font: &dyn FontMetrics, character: char, origin: (i32, i32)) -> Option<Rect> {
    let glyph = font.glyph_metrics(character)?;
    Some(Rect {
        x: origin.0 + glyph.bearing_x as i32,
        y: origin.1 - glyph.bearing_y as i32,
        width: glyph.width as i32,
        height: glyph.height as i32,
    })
}
