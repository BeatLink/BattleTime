//! The drawing calls every widget makes, and a renderer that keeps them inside a rectangle.
//!
//! Shapes with soft edges are worked out a row at a time and handed over as coverage, how much of
//! each pixel the shape covers, so a renderer only has to blend rows.

use crate::geometry::{scale, Color, PointF, Rect};
use crate::raster::{rasterize_arc, rasterize_disc, rasterize_line, CoverageSink};

// ============================================================================
// Fonts
// ============================================================================

/// One glyph's size and place against its pen position, in whole pixels but for the advance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphInfo {
    /// How far the pen moves, in sixteenths of a pixel.
    pub advance_fp16: u16,
    pub bearing_x: i16,
    pub bearing_y: i16,
    pub width: u16,
    pub height: u16,
}

/// A font's line height and how far it reaches above and below the baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontLineMetrics {
    pub line_height: u16,
    pub ascent: i16,
    pub descent: i16,
}

/// A font a renderer can measure and draw.
pub trait FontMetrics {
    fn glyph_metrics(&self, character: char) -> Option<GlyphInfo>;

    fn line_metrics(&self) -> FontLineMetrics;

    /// How far the pen moves past a character, in sixteenths of a pixel; nothing if the font lacks it.
    ///
    /// Layout measures text with this, so a font that can answer without its glyph's box should.
    fn advance_fp16(&self, character: char) -> Option<u16> {
        self.glyph_metrics(character).map(|glyph| glyph.advance_fp16)
    }

    /// Fills `coverage` with one row of a glyph from `x_offset`; false if the font has no coverage.
    fn glyph_coverage_row(&self, _character: char, _row: u16, _x_offset: u16, _coverage: &mut [u8]) -> bool {
        false
    }
}

// ============================================================================
// Renderer
// ============================================================================

/// The widest run of glyph coverage handed over in one call.
const GLYPH_COVERAGE_RUN: usize = 64;

/// Somewhere widgets draw: a screen, a strip of one, or a wrapper that changes what is drawn.
pub trait Renderer {
    fn fill_rect(&mut self, rect: Rect, color: Color);

    /// Draws plain text with its baseline at `position`.
    fn draw_text(&mut self, position: (i32, i32), text: &str, color: Color);

    /// Mixes `color` over what is there, by its opacity.
    fn blend_rect(&mut self, rect: Rect, color: Color) {
        self.fill_rect(rect, color);
    }

    /// Mixes `color` over one row, each pixel's opacity scaled by its coverage.
    fn blend_row(&mut self, x: i32, y: i32, color: Color, coverage: &[u8]) {
        for (offset, &amount) in coverage.iter().enumerate() {
            if amount == 0 {
                continue;
            }
            let dot = Rect {
                x: x + offset as i32,
                y,
                width: 1,
                height: 1,
            };
            self.blend_rect(dot, Color(color.0, color.1, color.2, scale(color.3, amount)));
        }
    }

    /// Draws a block of pixels row by row from `position`.
    fn draw_pixels(&mut self, position: (i32, i32), pixels: &[Color], width: u32, height: u32) {
        for row in 0..height as i32 {
            for column in 0..width as i32 {
                let index = (row as u32 * width + column as u32) as usize;
                if let Some(&pixel) = pixels.get(index) {
                    let dot = Rect {
                        x: position.0 + column,
                        y: position.1 + row,
                        width: 1,
                        height: 1,
                    };
                    self.fill_rect(dot, pixel);
                }
            }
        }
    }

    /// Draws one glyph with its pen at `origin` on the baseline.
    fn draw_glyph(&mut self, font: &dyn FontMetrics, character: char, origin: (i32, i32), color: Color) {
        let Some(glyph) = font.glyph_metrics(character) else {
            return;
        };
        let extent = Rect {
            x: origin.0 + glyph.bearing_x as i32,
            y: origin.1 - glyph.bearing_y as i32,
            width: glyph.width as i32,
            height: glyph.height as i32,
        };
        if extent.is_empty() {
            return;
        }
        // Only the part that can show is worked out, since a glyph's coverage is costly to sample.
        let Some(shown) = clip_within(self.visible_area(), extent) else {
            return;
        };
        let first_column = (shown.x - extent.x) as u16;
        let end_column = first_column + shown.width as u16;
        let first_row = (shown.y - extent.y) as u16;
        let mut coverage = [0u8; GLYPH_COVERAGE_RUN];
        for row in first_row..first_row + shown.height as u16 {
            let mut x_offset = first_column;
            while x_offset < end_column {
                let run = (end_column - x_offset).min(GLYPH_COVERAGE_RUN as u16) as usize;
                let row_coverage = &mut coverage[..run];
                if !font.glyph_coverage_row(character, row, x_offset, row_coverage) {
                    self.blend_rect(extent, color);
                    return;
                }
                self.blend_row(
                    extent.x + i32::from(x_offset),
                    extent.y + i32::from(row),
                    color,
                    row_coverage,
                );
                x_offset += run as u16;
            }
        }
    }

    /// The only part of the screen drawing can reach, when the renderer knows it, so callers skip the rest.
    fn visible_area(&self) -> Option<Rect> {
        None
    }

    /// Fills a circle with soft edges.
    fn fill_disc_aa(&mut self, centre: PointF, radius: f32, color: Color) {
        let Some(clip) = clip_within(self.visible_area(), padded_square(centre, radius)) else {
            return;
        };
        let mut sink = RowBlender {
            renderer: self,
            color,
        };
        rasterize_disc(centre, radius, clip, &mut sink);
    }

    /// Draws a straight line `width` pixels wide with square ends and soft edges.
    fn stroke_line_aa(&mut self, from: PointF, to: PointF, width: f32, color: Color) {
        let clip = self.visible_area().unwrap_or(EVERYWHERE);
        let mut sink = RowBlender {
            renderer: self,
            color,
        };
        rasterize_line(from, to, width, clip, &mut sink);
    }

    /// Fills part of a ring between two angles, whose cosines and sines are given, with soft edges.
    #[allow(clippy::too_many_arguments)]
    fn fill_arc_aa(
        &mut self,
        centre: PointF,
        outer_radius: f32,
        inner_radius: f32,
        start_cos: f32,
        start_sin: f32,
        end_cos: f32,
        end_sin: f32,
        extent_in_radians: f32,
        color: Color,
    ) {
        let Some(clip) = clip_within(self.visible_area(), padded_square(centre, outer_radius)) else {
            return;
        };
        let mut sink = RowBlender {
            renderer: self,
            color,
        };
        rasterize_arc(
            centre,
            outer_radius,
            inner_radius,
            start_cos,
            start_sin,
            end_cos,
            end_sin,
            extent_in_radians,
            clip,
            &mut sink,
        );
    }
}

/// A clip that keeps every pixel, for shapes the renderer clips itself.
const EVERYWHERE: Rect = Rect {
    x: i32::MIN / 4,
    y: i32::MIN / 4,
    width: i32::MAX / 2,
    height: i32::MAX / 2,
};

/// The square round a circle, with room for its soft edge.
fn padded_square(centre: PointF, radius: f32) -> Rect {
    let reach = radius + 1.0;
    Rect {
        x: (centre.x - reach) as i32 - 1,
        y: (centre.y - reach) as i32 - 1,
        width: (reach * 2.0) as i32 + 3,
        height: (reach * 2.0) as i32 + 3,
    }
}

/// A shape's own clip cut to the renderer's visible area, or nothing if they do not meet.
fn clip_within(visible: Option<Rect>, shape: Rect) -> Option<Rect> {
    match visible {
        Some(visible) => shape.intersect(visible),
        None => Some(shape),
    }
}

/// Hands each row of a shape's coverage to a renderer to blend.
struct RowBlender<'renderer, Inner: Renderer + ?Sized> {
    renderer: &'renderer mut Inner,
    color: Color,
}

impl<Inner: Renderer + ?Sized> CoverageSink for RowBlender<'_, Inner> {
    fn row(&mut self, x: i32, y: i32, coverage: &[u8]) {
        self.renderer.blend_row(x, y, self.color, coverage);
    }
}

// ============================================================================
// Clipping
// ============================================================================

/// How far above the baseline plain text is taken to reach, for clipping it whole.
const TEXT_LINE_HEIGHT: i32 = 16;

/// Passes drawing on, dropping every pixel outside a rectangle.
pub struct ClipRenderer<'inner> {
    inner: &'inner mut dyn Renderer,
    clip: Option<Rect>,
}

impl<'inner> ClipRenderer<'inner> {
    pub fn new(inner: &'inner mut dyn Renderer, clip: Rect) -> Self {
        Self {
            inner,
            clip: (!clip.is_empty()).then_some(clip),
        }
    }
}

impl Renderer for ClipRenderer<'_> {
    fn visible_area(&self) -> Option<Rect> {
        let clip = self.clip?;
        clip_within(self.inner.visible_area(), clip)
    }

    fn fill_rect(&mut self, rect: Rect, color: Color) {
        if let Some(visible) = self.clip.and_then(|clip| rect.intersect(clip)) {
            self.inner.fill_rect(visible, color);
        }
    }

    fn blend_rect(&mut self, rect: Rect, color: Color) {
        if let Some(visible) = self.clip.and_then(|clip| rect.intersect(clip)) {
            self.inner.blend_rect(visible, color);
        }
    }

    /// Draws text only when its whole line and its start are inside, rather than cutting letters.
    fn draw_text(&mut self, position: (i32, i32), text: &str, color: Color) {
        let Some(clip) = self.clip else {
            return;
        };
        let (x, y) = position;
        let is_inside_down = y - TEXT_LINE_HEIGHT >= clip.y && y <= clip.bottom();
        let is_inside_across = x >= clip.x && x < clip.right();
        if is_inside_down && is_inside_across {
            self.inner.draw_text(position, text, color);
        }
    }

    fn draw_pixels(&mut self, position: (i32, i32), pixels: &[Color], width: u32, height: u32) {
        let Some(clip) = self.clip else {
            return;
        };
        let block = Rect {
            x: position.0,
            y: position.1,
            width: width as i32,
            height: height as i32,
        };
        if block.is_empty() {
            return;
        }
        let Some(visible) = block.intersect(clip) else {
            return;
        };
        if visible == block {
            self.inner.draw_pixels(position, pixels, width, height);
            return;
        }
        let first_column = (visible.x - block.x) as u32;
        let run = visible.width as u32;
        for row in 0..visible.height {
            let source_row = (visible.y - block.y + row) as u32;
            let start = (source_row * width + first_column) as usize;
            let Some(slice) = pixels.get(start..start + run as usize) else {
                return;
            };
            self.inner
                .draw_pixels((visible.x, visible.y + row), slice, run, 1);
        }
    }

    fn blend_row(&mut self, x: i32, y: i32, color: Color, coverage: &[u8]) {
        let Some(clip) = self.clip else {
            return;
        };
        let row = Rect {
            x,
            y,
            width: coverage.len() as i32,
            height: 1,
        };
        let Some(visible) = row.intersect(clip) else {
            return;
        };
        let start = (visible.x - x) as usize;
        self.inner.blend_row(
            visible.x,
            y,
            color,
            &coverage[start..start + visible.width as usize],
        );
    }
}
