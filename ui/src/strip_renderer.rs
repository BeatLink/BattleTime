//! A renderer that draws straight into a strip, clipped and faded as one widget needs.
//!
//! The strip is in memory, so a see-through colour is mixed with the pixel really underneath it,
//! and every call works on whole runs of a row rather than one pixel at a time.

use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    text::{Baseline, Text},
};

use neotime_display::{color_of_panel_pixel, panel_pixel};

use crate::geometry::{scale, Color, Rect, OPAQUE};
use crate::renderer::Renderer;
use crate::strip::Strip;

/// How far above the baseline plain text is taken to reach, for clipping it whole.
const TEXT_LINE_HEIGHT: i32 = 16;

// ============================================================================
// Strip renderer
// ============================================================================

/// Draws into `strip` inside `clip`, every colour's opacity scaled by `opacity`.
pub struct StripRenderer<'strip, 'buffer> {
    strip: &'strip mut Strip<'buffer>,
    /// Where drawing lands: the clip within the strip, or nothing when they do not meet.
    visible: Option<Rect>,
    opacity: u8,
}

impl<'strip, 'buffer> StripRenderer<'strip, 'buffer> {
    pub fn new(strip: &'strip mut Strip<'buffer>, clip: Rect, opacity: u8) -> Self {
        let visible = clip.intersect(strip.area());
        Self {
            strip,
            visible,
            opacity,
        }
    }

    /// The columns of row `y` from `x` for `length` pixels that land, as a start offset and a range.
    fn visible_run(&self, x: i32, y: i32, length: i32) -> Option<(usize, core::ops::Range<i32>)> {
        let visible = self.visible?;
        if y < visible.y || y >= visible.bottom() {
            return None;
        }
        let first = x.max(visible.x);
        let end = (x + length).min(visible.right());
        (first < end).then(|| ((first - x) as usize, first..end))
    }

    fn row_mut(&mut self, y: i32, columns: core::ops::Range<i32>) -> &mut [u16] {
        self.strip.row_mut(y, columns)
    }
}

impl Renderer for StripRenderer<'_, '_> {
    fn fill_rect(&mut self, rect: Rect, color: Color) {
        if self.opacity != OPAQUE || color.3 != OPAQUE {
            self.blend_rect(rect, color);
            return;
        }
        let Some(area) = self.visible.and_then(|visible| rect.intersect(visible)) else {
            return;
        };
        let pixel = panel_pixel(to_rgb565(color));
        for y in area.y..area.bottom() {
            self.row_mut(y, area.x..area.right()).fill(pixel);
        }
    }

    fn blend_rect(&mut self, rect: Rect, color: Color) {
        let alpha = scale(color.3, self.opacity);
        if alpha == 0 {
            return;
        }
        let Some(area) = self.visible.and_then(|visible| rect.intersect(visible)) else {
            return;
        };
        for y in area.y..area.bottom() {
            for pixel in self.row_mut(y, area.x..area.right()) {
                *pixel = panel_pixel(mix(color, alpha, color_of_panel_pixel(*pixel)));
            }
        }
    }

    fn blend_row(&mut self, x: i32, y: i32, color: Color, coverage: &[u8]) {
        let alpha = scale(color.3, self.opacity);
        if alpha == 0 {
            return;
        }
        let Some((skipped, columns)) = self.visible_run(x, y, coverage.len() as i32) else {
            return;
        };
        let solid = panel_pixel(to_rgb565(color));
        let pixels = self.row_mut(y, columns);
        for (pixel, &amount) in pixels.iter_mut().zip(&coverage[skipped..]) {
            match scale(alpha, amount) {
                0 => {}
                OPAQUE => *pixel = solid,
                share => *pixel = panel_pixel(mix(color, share, color_of_panel_pixel(*pixel))),
            }
        }
    }

    fn draw_pixels(&mut self, position: (i32, i32), pixels: &[Color], width: u32, height: u32) {
        let width = width as usize;
        if width == 0 {
            return;
        }
        let opacity = self.opacity;
        for (row_index, row) in pixels.chunks(width).take(height as usize).enumerate() {
            let y = position.1 + row_index as i32;
            let Some((skipped, columns)) = self.visible_run(position.0, y, row.len() as i32) else {
                continue;
            };
            for (pixel, &color) in self.row_mut(y, columns).iter_mut().zip(&row[skipped..]) {
                match scale(color.3, opacity) {
                    0 => {}
                    OPAQUE => *pixel = panel_pixel(to_rgb565(color)),
                    share => *pixel = panel_pixel(mix(color, share, color_of_panel_pixel(*pixel))),
                }
            }
        }
    }

    /// Draws text only when its whole line and its start are inside, rather than cutting letters.
    fn draw_text(&mut self, position: (i32, i32), text: &str, color: Color) {
        let Some(visible) = self.visible else {
            return;
        };
        let (x, y) = position;
        let is_inside_down = y - TEXT_LINE_HEIGHT >= visible.y && y <= visible.bottom();
        let is_inside_across = x >= visible.x && x < visible.right();
        let alpha = scale(color.3, self.opacity);
        if !is_inside_down || !is_inside_across || alpha == 0 {
            return;
        }
        let under = self.strip.pixel_at(x, y - 1).unwrap_or(Rgb565::BLACK);
        let style = MonoTextStyle::new(&FONT_10X20, mix(color, alpha, under));
        // Renderer letters stand on the row above the baseline, embedded-graphics letters on it.
        let baseline_start = Point::new(x, y - 1);
        let _ = Text::with_baseline(text, baseline_start, style, Baseline::Alphabetic).draw(&mut *self.strip);
    }

    fn visible_area(&self) -> Option<Rect> {
        self.visible
    }
}

// ============================================================================
// Colours
// ============================================================================

pub(crate) fn to_rgb565(color: Color) -> Rgb565 {
    Rgb565::new(color.0 >> 3, color.1 >> 2, color.2 >> 3)
}

/// `color` at opacity `alpha` over `under`, mixed eight bits a channel.
fn mix(color: Color, alpha: u8, under: Rgb565) -> Rgb565 {
    let front_share = u16::from(alpha);
    let back_share = u16::from(OPAQUE - alpha);
    let channel = |front: u8, back: u8| {
        ((u16::from(front) * front_share + u16::from(back) * back_share) / u16::from(OPAQUE)) as u8
    };
    let red = under.r();
    let green = under.g();
    let blue = under.b();
    Rgb565::new(
        channel(color.0, red << 3 | red >> 2) >> 3,
        channel(color.1, green << 2 | green >> 4) >> 2,
        channel(color.2, blue << 3 | blue >> 2) >> 3,
    )
}

#[cfg(test)]
#[path = "strip_renderer_tests.rs"]
mod strip_renderer_tests;
