//! The fonts an app picks with its `font` property, drawn with soft edges.
//!
//! Every text size is one font drawn from lines, arcs and dots, made from `fonts/geometric.txt` by
//! `tools/generate_fonts.py` and read in place from flash. Each size scales it and moves its straight
//! strokes and the edges of its arcs onto whole pixels, so stems stay sharp and bowls stay even. The
//! dot-matrix display font is a second such font, made of rows of dots, which an app that uses it
//! carries in its own image. docs/app-runtime.md lists the sizes.

use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU8, AtomicUsize, Ordering},
};

use battletime_runtime::AppImage;
use battletime_ui::{
    geometry::{Color, Rect},
    raster::{floor, square_root},
    renderer::{FontLineMetrics, FontMetrics, GlyphInfo, Renderer},
};

use crate::widgets::sine_of;

// ============================================================================
// The font file
// ============================================================================

const SUBPIXELS_PER_PIXEL: i32 = 16;
const FONT_MAGIC: &[u8; 4] = b"NTGF";
const FONT_VERSION: u8 = 1;
const FONT_HEADER_BYTES: usize = 16;
const GLYPH_ENTRY_BYTES: usize = 6;
const LINE: u8 = 0;
const ARC: u8 = 1;
const DOT: u8 = 2;
const DOTS: u8 = 3;

/// The most shapes a glyph may have, as `tools/generate_fonts.py` checks; a glyph with more draws only these.
const MOST_SHAPES: usize = 8;
/// The most lines one axis puts on whole pixels: two for each shape, and the font's four height lines.
const MOST_CONTROLS: usize = 2 * MOST_SHAPES + 4;
/// The widest piece of a glyph row worked out in one go; wider glyphs are drawn in pieces.
const WIDEST_GLYPH_ROW: usize = 128;
/// How much wider than the line a dot is, so it holds its own beside the strokes.
const DOT_GROWTH: f32 = 1.5;

/// The font made by `tools/generate_fonts.py`, read in place from the shared library once installed.
///
/// Layout measures text all the time, and the font sits in flash, where each read makes the CPU
/// wait, so the first lookup checks the file once and notes which printable ASCII characters it
/// has; later lookups trust that.
pub struct GeometricFont {
    start: AtomicPtr<u8>,
    length: AtomicUsize,
    /// `UNCHECKED`, `CHECKED` or `NOT_A_FONT`.
    state: AtomicU8,
    glyph_count: AtomicUsize,
    /// One bit for each character from `FIRST_ASCII` up, set where the font has a glyph for it.
    ascii: [AtomicU32; ASCII_WORDS],
}

const UNCHECKED: u8 = 0;
const CHECKED: u8 = 1;
const NOT_A_FONT: u8 = 2;
const FIRST_ASCII: u32 = 0x20;
const ASCII_WORDS: usize = 3;

/// Where one glyph's shapes start, how many there are, and how wide it is in font units.
#[derive(Clone, Copy)]
struct GlyphEntry {
    width: u8,
    shape_count: u8,
    offset: usize,
}

/// One shape as the font file keeps it, in font units with y growing upwards.
#[derive(Clone, Copy)]
enum Shape {
    Line {
        x1: i8,
        y1: i8,
        x2: i8,
        y2: i8,
    },
    Arc {
        x: i8,
        y: i8,
        across: u8,
        down: u8,
        start: u8,
        end: u8,
    },
    Dot {
        x: i8,
        y: i8,
    },
    /// A row of `count` evenly spaced dots from one end to the other.
    Dots {
        x1: i8,
        y1: i8,
        x2: i8,
        y2: i8,
        count: u8,
    },
}

fn read_u16(bytes: &[u8], at: usize) -> u16 {
    match bytes.get(at..at + 2) {
        Some(pair) => u16::from_le_bytes([pair[0], pair[1]]),
        None => 0,
    }
}

impl GeometricFont {
    /// A font with no glyphs yet; `install_from_library` fills it.
    pub const fn empty() -> Self {
        GeometricFont {
            start: AtomicPtr::new(core::ptr::null_mut()),
            length: AtomicUsize::new(0),
            state: AtomicU8::new(UNCHECKED),
            glyph_count: AtomicUsize::new(0),
            ascii: [const { AtomicU32::new(0) }; ASCII_WORDS],
        }
    }

    /// A font that starts with its glyphs, for builds that carry the font file.
    #[cfg(any(test, feature = "built-in-fonts"))]
    const fn preinstalled(bytes: &'static [u8]) -> Self {
        GeometricFont {
            start: AtomicPtr::new(bytes.as_ptr() as *mut u8),
            length: AtomicUsize::new(bytes.len()),
            state: AtomicU8::new(UNCHECKED),
            glyph_count: AtomicUsize::new(0),
            ascii: [const { AtomicU32::new(0) }; ASCII_WORDS],
        }
    }

    fn install(&self, bytes: &'static [u8]) {
        // Safety: a `'static` slice stays where it is.
        unsafe { self.install_borrowed(bytes) };
    }

    /// Takes a font file that may move or go away later.
    ///
    /// # Safety
    ///
    /// The bytes must stay where they are until `remove_if_holding` is called with them.
    unsafe fn install_borrowed(&self, bytes: &[u8]) {
        self.state.store(UNCHECKED, Ordering::Release);
        self.length.store(0, Ordering::Release);
        self.start.store(bytes.as_ptr() as *mut u8, Ordering::Release);
        self.length.store(bytes.len(), Ordering::Release);
    }

    /// Empties the font if it holds these bytes, before they go away.
    fn remove_if_holding(&self, bytes: &[u8]) {
        if !bytes.is_empty() && core::ptr::eq(self.start.load(Ordering::Acquire), bytes.as_ptr()) {
            self.install(&[]);
        }
    }

    fn bytes(&self) -> &'static [u8] {
        let length = self.length.load(Ordering::Acquire);
        let start = self.start.load(Ordering::Acquire);
        if start.is_null() || length == 0 {
            return &[];
        }
        // Only `install` sets these, from a `'static` slice.
        unsafe { core::slice::from_raw_parts(start, length) }
    }

    /// The installed file, checked the first time it is asked for.
    fn data(&self) -> Option<FontData> {
        match self.state.load(Ordering::Acquire) {
            CHECKED => Some(FontData {
                bytes: self.bytes(),
                glyph_count: self.glyph_count.load(Ordering::Acquire),
            }),
            NOT_A_FONT => None,
            _ => self.check(),
        }
    }

    /// Checks the installed file and notes its glyph count and which printable ASCII it has.
    #[cold]
    fn check(&self) -> Option<FontData> {
        let bytes = self.bytes();
        let is_font =
            bytes.len() >= FONT_HEADER_BYTES && &bytes[..4] == FONT_MAGIC && bytes[4] == FONT_VERSION;
        if !is_font {
            self.state.store(NOT_A_FONT, Ordering::Release);
            return None;
        }
        let data = FontData {
            bytes,
            glyph_count: read_u16(bytes, 6) as usize,
        };
        let mut ascii = [0u32; ASCII_WORDS];
        for index in 0..data.glyph_count {
            let Some((character, _)) = data.entry_at(index) else {
                break;
            };
            let bit = (character as u32).wrapping_sub(FIRST_ASCII);
            if (bit as usize) < ASCII_WORDS * 32 {
                ascii[bit as usize / 32] |= 1 << (bit % 32);
            }
        }
        for (slot, word) in self.ascii.iter().zip(ascii) {
            slot.store(word, Ordering::Release);
        }
        self.glyph_count.store(data.glyph_count, Ordering::Release);
        self.state.store(CHECKED, Ordering::Release);
        Some(data)
    }

    /// Whether the font has a glyph for `character`; printable ASCII needs no read of the file.
    fn has(&self, character: char) -> bool {
        let bit = (character as u32).wrapping_sub(FIRST_ASCII);
        let is_ascii = (bit as usize) < ASCII_WORDS * 32;
        if is_ascii && self.state.load(Ordering::Acquire) == CHECKED {
            return self.ascii[bit as usize / 32].load(Ordering::Acquire) & (1 << (bit % 32)) != 0;
        }
        let Some(data) = self.data() else {
            return false;
        };
        match is_ascii {
            true => self.ascii[bit as usize / 32].load(Ordering::Acquire) & (1 << (bit % 32)) != 0,
            false => data.glyph(character).is_some(),
        }
    }

    #[cfg(test)]
    fn glyph_count(&self) -> usize {
        self.data().map_or(0, |data| data.glyph_count)
    }

    #[cfg(test)]
    fn heights(&self) -> Option<FontHeights> {
        Some(self.data()?.heights())
    }

    #[cfg(test)]
    fn entry_at(&self, index: usize) -> Option<(u16, GlyphEntry)> {
        self.data()?.entry_at(index)
    }

    fn glyph(&self, character: char) -> Option<GlyphEntry> {
        self.data()?.glyph(character)
    }

    #[cfg(test)]
    fn shapes(&self, glyph: GlyphEntry) -> ShapeReader<'static> {
        match self.data() {
            Some(data) => data.shapes(glyph),
            None => ShapeReader { data: &[], left: 0 },
        }
    }
}

/// The font file once checked: its bytes and how many glyphs it lists.
#[derive(Clone, Copy)]
struct FontData {
    bytes: &'static [u8],
    glyph_count: usize,
}

impl FontData {
    /// The font's height lines, standard width and how far its shapes reach, in font units.
    fn heights(&self) -> FontHeights {
        let signed = |at: usize| self.bytes[at] as i8 as f32;
        FontHeights {
            capital: self.bytes[8] as f32,
            x_height: self.bytes[9] as f32,
            standard_width: self.bytes[10] as f32,
            descender: signed(11),
            lowest: signed(12),
            highest: signed(13),
            leftmost: signed(14),
            rightmost: signed(15),
        }
    }

    fn entry_at(&self, index: usize) -> Option<(u16, GlyphEntry)> {
        if index >= self.glyph_count {
            return None;
        }
        let start = FONT_HEADER_BYTES + index * GLYPH_ENTRY_BYTES;
        let entry = self.bytes.get(start..start + GLYPH_ENTRY_BYTES)?;
        Some((
            u16::from_le_bytes([entry[0], entry[1]]),
            GlyphEntry {
                width: entry[2],
                shape_count: entry[3],
                offset: u16::from_le_bytes([entry[4], entry[5]]) as usize,
            },
        ))
    }

    fn glyph(&self, character: char) -> Option<GlyphEntry> {
        let wanted = u16::try_from(character as u32).ok()?;
        // The font holds printable ASCII without gaps from its first glyph, so most characters are found at once.
        let (first, _) = self.entry_at(0)?;
        let guess = wanted.checked_sub(first).map(usize::from);
        if let Some((found, entry)) = guess.and_then(|index| self.entry_at(index)) {
            if found == wanted {
                return Some(entry);
            }
        }
        let mut low = 0;
        let mut high = self.glyph_count;
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

    /// A glyph's shapes, at most `MOST_SHAPES`, stopping early at any that the file cuts short.
    fn shapes(&self, glyph: GlyphEntry) -> ShapeReader<'static> {
        let start = FONT_HEADER_BYTES + self.glyph_count * GLYPH_ENTRY_BYTES + glyph.offset;
        ShapeReader {
            data: self.bytes.get(start..).unwrap_or(&[]),
            left: (glyph.shape_count as usize).min(MOST_SHAPES),
        }
    }
}

/// The font's height lines and standard width, in font units.
#[derive(Clone, Copy)]
struct FontHeights {
    capital: f32,
    x_height: f32,
    standard_width: f32,
    descender: f32,
    /// The lowest, highest, leftmost and rightmost any glyph's centre lines reach.
    lowest: f32,
    highest: f32,
    leftmost: f32,
    rightmost: f32,
}

/// Reads one glyph's shapes in turn.
struct ShapeReader<'font> {
    data: &'font [u8],
    left: usize,
}

impl Iterator for ShapeReader<'_> {
    type Item = Shape;

    fn next(&mut self) -> Option<Shape> {
        if self.left == 0 {
            return None;
        }
        let size = match *self.data.first()? {
            LINE => 5,
            ARC => 7,
            DOT => 3,
            DOTS => 6,
            _ => return None,
        };
        let fields = self.data.get(..size)?;
        let signed = |at: usize| fields[at] as i8;
        let shape = match fields[0] {
            LINE => Shape::Line {
                x1: signed(1),
                y1: signed(2),
                x2: signed(3),
                y2: signed(4),
            },
            ARC => Shape::Arc {
                x: signed(1),
                y: signed(2),
                across: fields[3],
                down: fields[4],
                start: fields[5],
                end: fields[6],
            },
            DOTS => Shape::Dots {
                x1: signed(1),
                y1: signed(2),
                x2: signed(3),
                y2: signed(4),
                count: fields[5],
            },
            _ => Shape::Dot {
                x: signed(1),
                y: signed(2),
            },
        };
        self.data = &self.data[size..];
        self.left -= 1;
        Some(shape)
    }
}

static FONT: GeometricFont = font_slot();
/// The dot-matrix font, taken from the image of the app that uses it while that app is loaded.
static DOT_MATRIX_FONT: GeometricFont = dot_matrix_slot();

// The watch carries no font: they come from the shared library and the apps. Tests and the simulator carry them.
#[cfg(not(any(test, feature = "built-in-fonts")))]
const fn font_slot() -> GeometricFont {
    GeometricFont::empty()
}

#[cfg(not(any(test, feature = "built-in-fonts")))]
const fn dot_matrix_slot() -> GeometricFont {
    GeometricFont::empty()
}

#[cfg(any(test, feature = "built-in-fonts"))]
const fn font_slot() -> GeometricFont {
    GeometricFont::preinstalled(include_bytes!("../fonts/geometric.ntfont"))
}

#[cfg(any(test, feature = "built-in-fonts"))]
const fn dot_matrix_slot() -> GeometricFont {
    GeometricFont::preinstalled(include_bytes!("../fonts/dot-matrix.ntfont"))
}

// ============================================================================
// Sizes
// ============================================================================

/// One size of a font: which font, how large it is drawn, how thick its lines are, and the line it sits in.
pub struct StyledFont {
    file: &'static GeometricFont,
    /// From the baseline to the top of a capital's ink, in pixels.
    capital_height: f32,
    /// How far the pen moves past a glyph of the font's standard width, in pixels.
    standard_advance: u8,
    /// Whether every glyph takes the standard advance, as the text sizes do.
    monospaced: bool,
    /// How thick each line is, in pixels.
    stroke: f32,
    line_height: u16,
    ascent: i16,
    descent: i16,
}

// Each size keeps the advance and line of the bitmap font it replaced, so layouts stay where they were.
static SMALL: StyledFont = StyledFont::text(12.0, 10, 2.0, 22, 17, 5);
static NORMAL: StyledFont = StyledFont::text(14.0, 12, 2.5, 27, 21, 6);
static MEDIUM: StyledFont = StyledFont::text(22.0, 18, 4.0, 40, 31, 9);
static LARGE: StyledFont = StyledFont::text(31.0, 25, 6.0, 56, 43, 13);
static HUGE: StyledFont = StyledFont::text(57.0, 46, 6.0, 62, 59, 3);
static CONDENSED: StyledFont = StyledFont::text(35.0, 20, 4.0, 39, 37, 2);
static GIANT: StyledFont = StyledFont {
    file: &FONT,
    capital_height: 87.0,
    standard_advance: 68,
    monospaced: false,
    stroke: 6.0,
    line_height: 92,
    ascent: 89,
    descent: 3,
};
/// Dots 5.5 pixels apart and 4.4 across, in the line the dot-matrix font has always had.
static DOT_MATRIX: StyledFont = StyledFont {
    file: &DOT_MATRIX_FONT,
    capital_height: 6.0 * 5.5 + DOT_MATRIX_STROKE,
    standard_advance: 33,
    monospaced: true,
    stroke: DOT_MATRIX_STROKE,
    line_height: 44,
    ascent: 41,
    descent: 3,
};
/// The line a dot 4.4 pixels across grows from.
const DOT_MATRIX_STROKE: f32 = 4.4 / DOT_GROWTH;

/// How a size maps font units to pixels.
#[derive(Clone, Copy)]
struct Scale {
    across: f32,
    down: f32,
    heights: FontHeights,
}

impl StyledFont {
    const fn text(
        capital_height: f32,
        standard_advance: u8,
        stroke: f32,
        line_height: u16,
        ascent: i16,
        descent: i16,
    ) -> Self {
        StyledFont {
            file: &FONT,
            capital_height,
            standard_advance,
            monospaced: true,
            stroke,
            line_height,
            ascent,
            descent,
        }
    }

    fn scale(&self) -> Option<Scale> {
        self.scale_in(&self.file.data()?)
    }

    fn scale_in(&self, data: &FontData) -> Option<Scale> {
        let heights = data.heights();
        if heights.capital <= 0.0 || heights.standard_width <= 0.0 {
            return None;
        }
        Some(Scale {
            across: self.standard_advance as f32 / heights.standard_width,
            down: (self.capital_height - self.stroke) / heights.capital,
            heights,
        })
    }

    /// How far the pen moves past a character, without working out its box.
    fn advance_of(&self, character: char) -> Option<i32> {
        if self.monospaced {
            return self.file.has(character).then_some(self.standard_advance as i32);
        }
        let data = self.file.data()?;
        let glyph = data.glyph(character)?;
        Some(self.advance(glyph, &self.scale_in(&data)?))
    }

    fn advance(&self, glyph: GlyphEntry, scale: &Scale) -> i32 {
        match self.monospaced {
            true => self.standard_advance as i32,
            false => round(glyph.width as f32 * scale.across) as i32,
        }
    }

    /// Where a glyph's x = 0 falls past the pen: monospaced glyphs sit in the middle of their advance.
    fn shift(&self, glyph: GlyphEntry, scale: &Scale) -> f32 {
        match self.monospaced {
            true => (self.standard_advance as f32 - glyph.width as f32 * scale.across) / 2.0,
            false => 0.0,
        }
    }

    /// The glyph's shapes in pixels, with its pen at `origin` on the baseline; returns how many there are.
    fn place(
        &self,
        data: &FontData,
        glyph: GlyphEntry,
        origin: (i32, i32),
        placed: &mut [Placed; MOST_SHAPES],
    ) -> usize {
        let Some(scale) = self.scale_in(data) else {
            return 0;
        };
        let half = self.stroke / 2.0;
        // Each line's near edge lands on a pixel's edge, so a whole-pixel line covers whole pixels.
        let offset = half - floor(half);
        let mut across = AxisFit::new(origin.0 as f32 + self.shift(glyph, &scale), scale.across);
        let mut down = AxisFit::new(origin.1 as f32 - half, -scale.down);
        let heights = scale.heights;
        for line in [heights.descender, 0.0, heights.x_height, heights.capital] {
            down.add(line);
        }
        for shape in data.shapes(glyph) {
            match shape {
                Shape::Line { x1, y1, x2, y2 } => {
                    if x1 == x2 {
                        across.add(x1 as f32);
                    }
                    if y1 == y2 {
                        down.add(y1 as f32);
                    }
                }
                Shape::Arc {
                    x,
                    y,
                    across: wide,
                    down: tall,
                    ..
                } => {
                    across.add(x as f32 - wide as f32);
                    across.add(x as f32 + wide as f32);
                    down.add(y as f32 - tall as f32);
                    down.add(y as f32 + tall as f32);
                }
                Shape::Dot { .. } | Shape::Dots { .. } => {}
            }
        }
        across.fit(offset);
        down.fit(offset);
        let mut count = 0;
        for (slot, shape) in placed.iter_mut().zip(data.shapes(glyph)) {
            *slot = match shape {
                Shape::Line { x1, y1, x2, y2 } => Placed::line(
                    across.place(x1 as f32),
                    down.place(y1 as f32),
                    across.place(x2 as f32),
                    down.place(y2 as f32),
                ),
                Shape::Arc {
                    x,
                    y,
                    across: wide,
                    down: tall,
                    start,
                    end,
                } => {
                    let left = across.place(x as f32 - wide as f32);
                    let right = across.place(x as f32 + wide as f32);
                    let top = down.place(y as f32 + tall as f32);
                    let bottom = down.place(y as f32 - tall as f32);
                    Placed::Arc {
                        x: (left + right) / 2.0,
                        y: (top + bottom) / 2.0,
                        across: (right - left) / 2.0,
                        down: (bottom - top) / 2.0,
                        start: direction_of(start),
                        end: direction_of(end),
                        sweep: Sweep::of(start, end),
                    }
                }
                Shape::Dot { x, y } => Placed::Dot {
                    x: across.place(x as f32),
                    y: down.place(y as f32),
                },
                Shape::Dots {
                    x1,
                    y1,
                    x2,
                    y2,
                    count,
                } => Placed::line_with_gaps(
                    across.place(x1 as f32),
                    down.place(y1 as f32),
                    across.place(x2 as f32),
                    down.place(y2 as f32),
                    count.saturating_sub(1) as f32,
                ),
            };
            count += 1;
        }
        count
    }

    /// The glyph's shapes placed with its pen at `origin`, taken from the last glyph placed when it is the same.
    fn placed_glyph(&self, data: &FontData, character: char, origin: (i32, i32)) -> Option<PlacedGlyph> {
        let key = PlacedGlyphKey {
            font_address: self as *const StyledFont as usize,
            font_file_address: data.bytes.as_ptr() as usize,
            character,
            origin,
        };
        if let Some(placed) = LAST_PLACED_GLYPH.get(key) {
            return Some(placed);
        }
        let glyph = data.glyph(character)?;
        let mut shapes = [Placed::NOTHING; MOST_SHAPES];
        let count = self.place(data, glyph, origin, &mut shapes);
        let placed = PlacedGlyph {
            key,
            shapes,
            count,
            ink_box: self.ink_box(&shapes[..count]),
        };
        LAST_PLACED_GLYPH.set(placed);
        Some(placed)
    }

    /// The pixels a glyph's ink can reach with its pen at `origin`, with room for its soft edges.
    fn ink_box(&self, shapes: &[Placed]) -> Option<Rect> {
        let reach = self.stroke / 2.0 * DOT_GROWTH + 1.0;
        let mut bounds: Option<(f32, f32, f32, f32)> = None;
        for shape in shapes {
            let (left, top, right, bottom) = shape.bounds();
            bounds = Some(match bounds {
                None => (left, top, right, bottom),
                Some((l, t, r, b)) => (l.min(left), t.min(top), r.max(right), b.max(bottom)),
            });
        }
        let (left, top, right, bottom) = bounds?;
        let x = floor(left - reach) as i32;
        let y = floor(top - reach) as i32;
        Some(Rect {
            x,
            y,
            width: floor(right + reach) as i32 + 1 - x,
            height: floor(bottom + reach) as i32 + 1 - y,
        })
    }

    /// The pixels any glyph's ink can reach with its pen at the origin, from how far the font's shapes reach.
    ///
    /// Layout asks for this for every character it measures, so it reads none of the glyph's shapes.
    /// Placing moves points by about half a pixel, so two pixels more all round hold the placed glyph too.
    fn rough_box(&self, glyph: GlyphEntry, scale: &Scale) -> Option<Rect> {
        if glyph.shape_count == 0 {
            return None;
        }
        let half = self.stroke / 2.0;
        let shift = self.shift(glyph, scale);
        let heights = scale.heights;
        let reach = half * DOT_GROWTH + 2.0;
        let x = floor(shift + heights.leftmost * scale.across - reach) as i32;
        let y = floor(-half - heights.highest * scale.down - reach) as i32;
        let right = floor(shift + heights.rightmost * scale.across + reach) as i32 + 1;
        let bottom = floor(-half - heights.lowest * scale.down + reach) as i32 + 1;
        Some(Rect {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }

    /// Draws one glyph with its pen at `origin` on the baseline, only the rows the renderer shows.
    fn draw_character(&self, renderer: &mut dyn Renderer, character: char, origin: (i32, i32), color: Color) {
        let Some(data) = self.file.data() else {
            return;
        };
        let Some(scale) = self.scale_in(&data) else {
            return;
        };
        // Text is asked to draw in every strip near it, so a strip no glyph can reach costs nothing more.
        if let Some(visible) = renderer.visible_area() {
            let baseline = origin.1 as f32 - self.stroke / 2.0;
            let reach = self.stroke / 2.0 * DOT_GROWTH + 2.0;
            let top = baseline - scale.heights.highest * scale.down - reach;
            let bottom = baseline - scale.heights.lowest * scale.down + reach;
            if bottom < visible.y as f32 || top > visible.bottom() as f32 {
                return;
            }
        }
        let Some(placed) = self.placed_glyph(&data, character, origin) else {
            return;
        };
        let shapes = &placed.shapes[..placed.count];
        let Some(area) = placed.ink_box else {
            return;
        };
        let (first_row, end_row) = rows_to_draw(renderer, area.y, area.height);
        let (first_column, end_column) = columns_to_draw(renderer, area.x, area.width);
        if first_column >= end_column {
            return;
        }
        let mut row = [0u8; WIDEST_GLYPH_ROW];
        for row_index in first_row..end_row {
            let y = area.y + row_index;
            let mut column = first_column;
            while column < end_column {
                let piece = ((end_column - column) as usize).min(WIDEST_GLYPH_ROW);
                let coverage = &mut row[..piece];
                coverage.fill(0);
                cover_row(
                    shapes,
                    self.stroke / 2.0,
                    y as f32 + 0.5,
                    area.x + column,
                    coverage,
                );
                renderer.blend_row(area.x + column, y, color, coverage);
                column += piece as i32;
            }
        }
    }
}

impl FontMetrics for StyledFont {
    fn glyph_metrics(&self, character: char) -> Option<GlyphInfo> {
        let glyph = self.file.glyph(character)?;
        let scale = self.scale()?;
        let advance_fp16 = (self.advance(glyph, &scale) * SUBPIXELS_PER_PIXEL) as u16;
        let Some(area) = self.rough_box(glyph, &scale) else {
            return Some(GlyphInfo {
                advance_fp16,
                bearing_x: 0,
                bearing_y: 0,
                width: 0,
                height: 0,
            });
        };
        Some(GlyphInfo {
            advance_fp16,
            bearing_x: area.x as i16,
            bearing_y: -area.y as i16,
            width: area.width as u16,
            height: area.height as u16,
        })
    }

    fn advance_fp16(&self, character: char) -> Option<u16> {
        Some((self.advance_of(character)? * SUBPIXELS_PER_PIXEL) as u16)
    }

    fn line_metrics(&self) -> FontLineMetrics {
        FontLineMetrics {
            line_height: self.line_height,
            ascent: self.ascent,
            descent: self.descent,
        }
    }

    fn glyph_coverage_row(&self, character: char, row: u16, x_offset: u16, coverage: &mut [u8]) -> bool {
        let Some(info) = self.glyph_metrics(character) else {
            return false;
        };
        let Some(data) = self.file.data() else {
            return false;
        };
        let Some(glyph) = data.glyph(character) else {
            return false;
        };
        // The glyph's box starts at its top-left corner, so its pen sits at its bearings.
        let mut placed = [Placed::NOTHING; MOST_SHAPES];
        let count = self.place(
            &data,
            glyph,
            (-(info.bearing_x as i32), info.bearing_y as i32),
            &mut placed,
        );
        coverage.fill(0);
        cover_row(
            &placed[..count],
            self.stroke / 2.0,
            row as f32 + 0.5,
            x_offset as i32,
            coverage,
        );
        true
    }
}

/// Which of `height` rows from `top` the renderer can show, counted from `top`, end not included.
pub fn rows_to_draw(renderer: &dyn Renderer, top: i32, height: i32) -> (i32, i32) {
    match renderer.visible_area() {
        Some(visible) => (
            (visible.y - top).clamp(0, height),
            (visible.bottom() - top).clamp(0, height),
        ),
        None => (0, height),
    }
}

/// Which of `width` columns from `left` the renderer can show, counted from `left`, end not included.
pub fn columns_to_draw(renderer: &dyn Renderer, left: i32, width: i32) -> (i32, i32) {
    match renderer.visible_area() {
        Some(visible) => (
            (visible.x - left).clamp(0, width),
            (visible.right() - left).clamp(0, width),
        ),
        None => (0, width),
    }
}

// ============================================================================
// The last placed glyph
// ============================================================================

/// What a placed glyph depends on: the font size, the font file, the character and where its pen sits.
#[derive(Clone, Copy, PartialEq)]
struct PlacedGlyphKey {
    font_address: usize,
    font_file_address: usize,
    character: char,
    origin: (i32, i32),
}

/// A glyph's shapes in pixels and the box their ink reaches.
#[derive(Clone, Copy)]
struct PlacedGlyph {
    key: PlacedGlyphKey,
    shapes: [Placed; MOST_SHAPES],
    count: usize,
    ink_box: Option<Rect>,
}

/// The last glyph placed, since each strip a glyph crosses draws it again from the same place.
///
/// Whoever finds it in use, such as a second test thread, places the glyph itself instead of waiting.
struct LastPlacedGlyph {
    is_in_use: AtomicBool,
    entry: UnsafeCell<KeptGlyph>,
}

/// The kept glyph, its tag a byte of its own that is zero while empty, so the static takes no flash.
#[repr(u8)]
#[derive(Clone, Copy)]
enum KeptGlyph {
    Nothing,
    Glyph(PlacedGlyph),
}

// The entry is only read or written by whoever set `is_in_use`.
unsafe impl Sync for LastPlacedGlyph {}

static LAST_PLACED_GLYPH: LastPlacedGlyph = LastPlacedGlyph {
    is_in_use: AtomicBool::new(false),
    entry: UnsafeCell::new(KeptGlyph::Nothing),
};

impl LastPlacedGlyph {
    /// The glyph kept for `key`, if it is the one kept and no one else is using it.
    fn get(&self, key: PlacedGlyphKey) -> Option<PlacedGlyph> {
        if self.is_in_use.swap(true, Ordering::Acquire) {
            return None;
        }
        // Safety: `is_in_use` was false, so nothing else holds the entry until it is cleared below.
        let found = match unsafe { &*self.entry.get() } {
            KeptGlyph::Glyph(placed) if placed.key == key => Some(*placed),
            _ => None,
        };
        self.is_in_use.store(false, Ordering::Release);
        found
    }

    /// Keeps `placed` in place of the last glyph, unless someone else is using it.
    fn set(&self, placed: PlacedGlyph) {
        if self.is_in_use.swap(true, Ordering::Acquire) {
            return;
        }
        // Safety: `is_in_use` was false, so nothing else holds the entry until it is cleared below.
        unsafe { *self.entry.get() = KeptGlyph::Glyph(placed) };
        self.is_in_use.store(false, Ordering::Release);
    }
}

// ============================================================================
// Putting strokes on whole pixels
// ============================================================================

/// Moves chosen lines of a glyph onto whole pixels along one axis, and stretches what lies between them.
struct AxisFit {
    /// Where each chosen line falls unmoved, in pixels, in order.
    unmoved: [f32; MOST_CONTROLS],
    /// Where each one is moved to.
    moved: [f32; MOST_CONTROLS],
    count: usize,
    /// The whole pixels of the origin, added only at the end, so a glyph snaps the same wherever it is.
    base: f32,
    /// The rest of the origin, which the lines are worked out from.
    origin: f32,
    scale: f32,
}

impl AxisFit {
    fn new(origin: f32, scale: f32) -> Self {
        let base = floor(origin);
        AxisFit {
            unmoved: [0.0; MOST_CONTROLS],
            moved: [0.0; MOST_CONTROLS],
            count: 0,
            base,
            origin: origin - base,
            scale,
        }
    }

    fn unmoved_at(&self, units: f32) -> f32 {
        self.origin + units * self.scale
    }

    /// Chooses a line in font units to put on the pixel grid.
    fn add(&mut self, units: f32) {
        if self.count < MOST_CONTROLS {
            self.unmoved[self.count] = self.unmoved_at(units);
            self.count += 1;
        }
    }

    /// Sorts the chosen lines and moves each to the nearest place `offset` past a pixel's edge.
    fn fit(&mut self, offset: f32) {
        let lines = &mut self.unmoved[..self.count];
        lines.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
        let mut kept = 0;
        for index in 0..self.count {
            let line = self.unmoved[index];
            if kept > 0 && line == self.unmoved[kept - 1] {
                continue;
            }
            let mut moved = round(line - offset) + offset;
            if kept > 0 {
                // Lines a pixel or more apart stay apart, so snapping never merges two strokes.
                let apart = line - self.unmoved[kept - 1] >= 1.0;
                let lowest = self.moved[kept - 1] + if apart { 1.0 } else { 0.0 };
                moved = moved.max(lowest);
            }
            self.unmoved[kept] = line;
            self.moved[kept] = moved;
            kept += 1;
        }
        self.count = kept;
    }

    /// Where a point in font units lands, moved as the chosen lines around it were.
    fn place(&self, units: f32) -> f32 {
        self.base + self.place_from_base(units)
    }

    fn place_from_base(&self, units: f32) -> f32 {
        let at = self.unmoved_at(units);
        if self.count == 0 {
            return at;
        }
        let last = self.count - 1;
        if at <= self.unmoved[0] {
            return at + self.moved[0] - self.unmoved[0];
        }
        if at >= self.unmoved[last] {
            return at + self.moved[last] - self.unmoved[last];
        }
        let mut index = 0;
        while self.unmoved[index + 1] < at {
            index += 1;
        }
        let (from, to) = (self.unmoved[index], self.unmoved[index + 1]);
        let along = (at - from) / (to - from);
        self.moved[index] + along * (self.moved[index + 1] - self.moved[index])
    }
}

fn round(value: f32) -> f32 {
    floor(value + 0.5)
}

// ============================================================================
// Coverage from the nearest shape
// ============================================================================

/// How much of a turn an arc covers, which decides how a point is tested against its ends.
#[derive(Clone, Copy)]
enum Sweep {
    Whole,
    UpToHalf,
    MoreThanHalf,
}

impl Sweep {
    fn of(start: u8, end: u8) -> Sweep {
        match end.wrapping_sub(start) {
            0 => Sweep::Whole,
            1..=128 => Sweep::UpToHalf,
            _ => Sweep::MoreThanHalf,
        }
    }
}

/// The cosine and sine of an angle given in 256ths of a turn, to the nearest degree.
fn direction_of(steps: u8) -> (f32, f32) {
    let degrees = (steps as i32 * 360 + 128) / 256;
    (sine_of(degrees + 90), sine_of(degrees))
}

/// A shape placed in pixels, with y growing downwards.
#[derive(Clone, Copy)]
enum Placed {
    /// A line, or a row of dots when `gaps`, the spaces between its dots, is more than zero.
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        inverse_length_squared: f32,
        gaps: f32,
    },
    Arc {
        x: f32,
        y: f32,
        across: f32,
        down: f32,
        start: (f32, f32),
        end: (f32, f32),
        sweep: Sweep,
    },
    Dot {
        x: f32,
        y: f32,
    },
}

impl Placed {
    /// A line from one point to another, with what working out a pixel's distance from it needs.
    fn line(x1: f32, y1: f32, x2: f32, y2: f32) -> Placed {
        Placed::line_with_gaps(x1, y1, x2, y2, 0.0)
    }

    /// Dots evenly spaced from one point to another, `gaps` spaces apart, or a line for no gaps.
    fn line_with_gaps(x1: f32, y1: f32, x2: f32, y2: f32, gaps: f32) -> Placed {
        let length_squared = (x2 - x1) * (x2 - x1) + (y2 - y1) * (y2 - y1);
        let inverse_length_squared = match length_squared > 0.0 {
            true => 1.0 / length_squared,
            false => 0.0,
        };
        Placed::Line {
            x1,
            y1,
            x2,
            y2,
            inverse_length_squared,
            gaps,
        }
    }

    const NOTHING: Placed = Placed::Dot { x: 0.0, y: 0.0 };

    /// The shape's centre-line box: left, top, right, bottom.
    fn bounds(&self) -> (f32, f32, f32, f32) {
        match *self {
            Placed::Line { x1, y1, x2, y2, .. } => (x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2)),
            Placed::Arc {
                x, y, across, down, ..
            } => (x - across, y - down, x + across, y + down),
            Placed::Dot { x, y } => (x, y, x, y),
        }
    }

    /// How far from the centre line its ink reaches.
    fn radius(&self, half: f32) -> f32 {
        match *self {
            Placed::Dot { .. } => half * DOT_GROWTH,
            Placed::Line { gaps, .. } if gaps > 0.0 => half * DOT_GROWTH,
            _ => half,
        }
    }

    /// The square of a pixel centre's distance from a line or dot, which needs no root; nothing for an arc.
    fn distance_squared(&self, px: f32, py: f32) -> Option<f32> {
        match *self {
            Placed::Line {
                x1,
                y1,
                x2,
                y2,
                inverse_length_squared,
                gaps,
            } => {
                let (dx, dy) = (x2 - x1, y2 - y1);
                let along = nearest_along(px - x1, py - y1, dx, dy, inverse_length_squared, gaps);
                let (ex, ey) = (px - x1 - along * dx, py - y1 - along * dy);
                Some(ex * ex + ey * ey)
            }
            Placed::Dot { x, y } => Some((px - x) * (px - x) + (py - y) * (py - y)),
            Placed::Arc { .. } => None,
        }
    }

    /// The ink of a pixel plainly outside a circle's stroke, or plainly inside it away from its ends.
    ///
    /// Squared distances from the centre settle most pixels of a ring without a root; nothing for the rest.
    fn ring_ink(&self, px: f32, py: f32, outer: f32, inner: f32) -> Option<u8> {
        let Placed::Arc {
            x,
            y,
            across,
            down,
            start,
            end,
            sweep,
        } = *self
        else {
            return None;
        };
        if (across - down).abs() >= 0.01 {
            return None;
        }
        let (dx, dy) = (px - x, y - py);
        let squared = dx * dx + dy * dy;
        // An arc's round ends lie on its circle, so nothing outside the ring's band has ink.
        let nearest = (across - outer).max(0.0);
        if squared <= nearest * nearest || squared >= (across + outer) * (across + outer) {
            return Some(0);
        }
        let past_start = start.0 * dy - start.1 * dx >= 0.0;
        let before_end = dx * end.1 - dy * end.0 >= 0.0;
        let inside = match sweep {
            Sweep::Whole => true,
            Sweep::UpToHalf => past_start && before_end,
            Sweep::MoreThanHalf => past_start || before_end,
        };
        let (low, high) = (across - inner, across + inner);
        match inside && low > 0.0 && squared >= low * low && squared <= high * high {
            true => Some(255),
            false => None,
        }
    }

    /// How far a pixel centre is from the shape's centre line.
    fn distance(&self, px: f32, py: f32) -> f32 {
        match *self {
            Placed::Line {
                x1,
                y1,
                x2,
                y2,
                inverse_length_squared,
                gaps,
            } => {
                let (dx, dy) = (x2 - x1, y2 - y1);
                let along = nearest_along(px - x1, py - y1, dx, dy, inverse_length_squared, gaps);
                length(px - x1 - along * dx, py - y1 - along * dy)
            }
            Placed::Dot { x, y } => length(px - x, py - y),
            Placed::Arc {
                x,
                y,
                across,
                down,
                start,
                end,
                sweep,
            } => {
                if across <= 0.0 || down <= 0.0 {
                    return length(px - x, py - y);
                }
                let (dx, dy) = (px - x, y - py);
                // Which side of each end the point lies on, without trigonometry or division:
                // the sides in the circle the arc is squeezed from, scaled by both radii.
                let past_start = || start.0 * dy * across - start.1 * dx * down >= 0.0;
                let before_end = || dx * down * end.1 - dy * across * end.0 >= 0.0;
                let inside = match sweep {
                    Sweep::Whole => true,
                    Sweep::UpToHalf => past_start() && before_end(),
                    Sweep::MoreThanHalf => past_start() || before_end(),
                };
                if !inside {
                    let to_start = length(px - (x + across * start.0), py - (y - down * start.1));
                    let to_end = length(px - (x + across * end.0), py - (y - down * end.1));
                    return to_start.min(to_end);
                }
                if (across - down).abs() < 0.01 {
                    return (length(dx, dy) - across).abs();
                }
                let (qx, qy) = (dx / across, dy / down);
                // An ellipse has no exact distance in closed form; its value over its slope is close near the curve.
                let reach = length(qx, qy);
                if reach < 1e-4 {
                    return across.min(down);
                }
                let slope = length(qx / across, qy / down) / reach;
                (reach - 1.0).abs() / slope
            }
        }
    }

    /// The columns on the row through `y` where the shape's ink can reach, as up to two ranges.
    fn columns(&self, y: f32, reach: f32) -> [Option<(f32, f32)>; 2] {
        match *self {
            Placed::Line { x1, y1, x2, y2, .. } => {
                if y + reach < y1.min(y2) || y - reach > y1.max(y2) {
                    return [None, None];
                }
                let rise = y2 - y1;
                let (low, high) = match rise.abs() < 1e-4 {
                    true => (0.0, 1.0),
                    false => {
                        let a = ((y - reach - y1) / rise).clamp(0.0, 1.0);
                        let b = ((y + reach - y1) / rise).clamp(0.0, 1.0);
                        (a.min(b), a.max(b))
                    }
                };
                let (xa, xb) = (x1 + low * (x2 - x1), x1 + high * (x2 - x1));
                [Some((xa.min(xb) - reach, xa.max(xb) + reach)), None]
            }
            Placed::Dot { x, y: centre } => match (y - centre).abs() <= reach {
                true => [Some((x - reach, x + reach)), None],
                false => [None, None],
            },
            Placed::Arc {
                x,
                y: centre,
                across,
                down,
                ..
            } => {
                let rise = y - centre;
                if rise.abs() > down + reach {
                    return [None, None];
                }
                if (across - down).abs() >= 0.01 {
                    return [Some((x - across - reach, x + across + reach)), None];
                }
                // A circle's ring meets the row in a chord with its middle cut out.
                let outer = (across + reach) * (across + reach) - rise * rise;
                if outer <= 0.0 {
                    return [None, None];
                }
                let outer = square_root(outer);
                let inner = (across - reach).max(0.0);
                let inner = inner * inner - rise * rise;
                match inner > 0.0 {
                    true => {
                        let inner = square_root(inner);
                        [Some((x - outer, x - inner)), Some((x + inner, x + outer))]
                    }
                    false => [Some((x - outer, x + outer)), None],
                }
            }
        }
    }
}

/// How far along a line, from 0 at its start to 1 at its end, its nearest point to a pixel lies.
///
/// A row of dots has points only at its dots, so the answer snaps to the nearest one.
fn nearest_along(x: f32, y: f32, dx: f32, dy: f32, inverse_length_squared: f32, gaps: f32) -> f32 {
    let along = ((x * dx + y * dy) * inverse_length_squared).clamp(0.0, 1.0);
    match gaps > 0.0 {
        true => round(along * gaps) / gaps,
        false => along,
    }
}

fn length(x: f32, y: f32) -> f32 {
    square_root(x * x + y * y)
}

/// Ink from the distance into a shape plus half a pixel, over a ramp one pixel wide.
fn ink_of(depth_plus_half: f32) -> u8 {
    if depth_plus_half <= 0.0 {
        0
    } else if depth_plus_half >= 1.0 {
        255
    } else {
        (depth_plus_half * 255.0 + 0.5) as u8
    }
}

/// Works out one row of a glyph from `left`, each pixel taking its ink from the nearest shape.
///
/// Only the columns each shape can reach are visited, so a row costs about as much as its strokes.
fn cover_row(shapes: &[Placed], half: f32, y: f32, left: i32, coverage: &mut [u8]) {
    let end = left + coverage.len() as i32;
    for shape in shapes {
        let radius = shape.radius(half);
        // Past half a pixel beyond the edge the ramp gives no ink, and inside half a pixel all of it.
        let (outer, inner) = (radius + 0.5, (radius - 0.5).max(0.0));
        let (outer_squared, inner_squared) = (outer * outer, inner * inner);
        for (from, to) in shape.columns(y, outer).into_iter().flatten() {
            let first = (floor(from) as i32).max(left);
            let last = (floor(to) as i32 + 1).min(end);
            for column in first..last {
                let px = column as f32 + 0.5;
                let ink = match shape.distance_squared(px, y) {
                    Some(squared) if squared >= outer_squared => 0,
                    Some(squared) if squared <= inner_squared => 255,
                    Some(squared) => ink_of(radius - square_root(squared) + 0.5),
                    None => match shape.ring_ink(px, y, outer, inner) {
                        Some(ink) => ink,
                        None => ink_of(radius - shape.distance(px, y) + 0.5),
                    },
                };
                let slot = &mut coverage[(column - left) as usize];
                *slot = (*slot).max(ink);
            }
        }
    }
}

// ============================================================================
// Font numbers
// ============================================================================

/// Takes the text font from the shared library, which must stay where it is, such as in internal flash.
///
/// Until then text takes no room and draws nothing; returns how many fonts the library had.
pub fn install_from_library(library: &AppImage<'static>) -> usize {
    // A library without the font empties it, so it never points into an older library.
    let bytes = library.font(0).unwrap_or(&[]);
    FONT.install(bytes);
    usize::from(!bytes.is_empty())
}

/// Takes the dot-matrix font from an app's image, if the image carries it, as the font the
/// `dot_matrix` value draws.
///
/// # Safety
///
/// The image's bytes must stay where they are until `remove_app_font` is called with the same image.
pub unsafe fn install_app_font(image: &AppImage<'_>) {
    // Tests and the simulator keep their built-in copy, which every image's copy matches.
    if cfg!(not(any(test, feature = "built-in-fonts"))) {
        if let Some(bytes) = image.font(0) {
            DOT_MATRIX_FONT.install_borrowed(bytes);
        }
    }
}

/// Lets go of an app's dot-matrix font before its image goes away; `dot_matrix` then draws as `normal`.
pub fn remove_app_font(image: &AppImage<'_>) {
    if cfg!(not(any(test, feature = "built-in-fonts"))) {
        if let Some(bytes) = image.font(0) {
            DOT_MATRIX_FONT.remove_if_holding(bytes);
        }
    }
}

/// How many bytes of flash the text font takes, for the memory map.
pub fn text_font_bytes() -> usize {
    FONT.bytes().len()
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
            // Apps built before 6 was retired drew big digits with it, and huge is the same height.
            6 => Huge,
            7 => DotMatrix,
            8 => Condensed,
            _ => Normal,
        }
    }

    /// The font and size this name draws; `dot_matrix` draws as `normal` while no app has given its font.
    fn styled(self) -> &'static StyledFont {
        use FontName::*;
        match self {
            Small => &SMALL,
            Normal => &NORMAL,
            Medium => &MEDIUM,
            Large => &LARGE,
            Huge => &HUGE,
            Giant => &GIANT,
            Condensed => &CONDENSED,
            DotMatrix => match DOT_MATRIX_FONT.data() {
                Some(_) => &DOT_MATRIX,
                None => &NORMAL,
            },
        }
    }

    /// The font drawn for this name.
    pub fn metrics(self) -> &'static dyn FontMetrics {
        self.styled()
    }

    /// Draws one character with its pen at `origin` on the baseline; returns how far the pen moves.
    pub fn draw_character(
        self,
        renderer: &mut dyn Renderer,
        character: char,
        origin: (i32, i32),
        color: Color,
    ) -> i32 {
        let font = self.styled();
        font.draw_character(renderer, character, origin, color);
        font.advance_of(character).unwrap_or(0)
    }
}

/// How far the pen moves past one character, in whole pixels.
pub fn character_advance(font: &dyn FontMetrics, character: char) -> i32 {
    font.advance_fp16(character).map_or(0, |advance| {
        (advance as i32 + SUBPIXELS_PER_PIXEL / 2) / SUBPIXELS_PER_PIXEL
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

#[cfg(test)]
#[path = "fonts_tests.rs"]
mod fonts_tests;
