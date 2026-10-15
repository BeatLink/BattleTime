extern crate std;

use std::vec;

use embedded_graphics::pixelcolor::Rgb565;

use super::*;
use crate::geometry::PointF;
use crate::renderer::{FontLineMetrics, FontMetrics, GlyphInfo};

// ============================================================================
// Helpers
// ============================================================================

const BLUE: Color = Color(0, 0, 255, OPAQUE);
const RED: Color = Color(255, 0, 0, OPAQUE);

fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
    Rect { x, y, width, height }
}

/// A 10 by 4 strip at row 20, filled blue.
fn blue_buffer() -> std::vec::Vec<u16> {
    vec![battletime_display::panel_pixel(to_rgb565(BLUE)); 40]
}

fn strip_on(buffer: &mut [u16]) -> Strip<'_> {
    Strip::new(buffer, rect(0, 20, 10, 4))
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn fills_only_inside_both_the_clip_and_the_strip() {
    let mut buffer = blue_buffer();
    let mut strip = strip_on(&mut buffer);

    StripRenderer::new(&mut strip, rect(2, 0, 3, 22), OPAQUE).fill_rect(rect(0, 0, 100, 100), RED);

    assert_eq!(strip.pixel_at(2, 21), Some(to_rgb565(RED)));
    assert_eq!(strip.pixel_at(4, 20), Some(to_rgb565(RED)));
    assert_eq!(strip.pixel_at(5, 20), Some(to_rgb565(BLUE)));
    assert_eq!(strip.pixel_at(2, 22), Some(to_rgb565(BLUE)));
}

#[test]
fn soft_edges_mix_with_the_pixel_really_underneath() {
    let mut buffer = blue_buffer();
    let mut strip = strip_on(&mut buffer);

    StripRenderer::new(&mut strip, rect(0, 0, 240, 240), OPAQUE).blend_row(0, 20, RED, &[0, 128, 255]);

    assert_eq!(strip.pixel_at(0, 20), Some(to_rgb565(BLUE)));
    assert_eq!(strip.pixel_at(1, 20), Some(Rgb565::new(16, 0, 15)));
    assert_eq!(strip.pixel_at(2, 20), Some(to_rgb565(RED)));
}

#[test]
fn a_faded_widget_mixes_even_its_solid_fills() {
    let mut buffer = blue_buffer();
    let mut strip = strip_on(&mut buffer);
    {
        let mut faded = StripRenderer::new(&mut strip, rect(0, 0, 240, 240), 128);
        faded.fill_rect(rect(0, 20, 1, 1), RED);
        faded.draw_pixels((1, 20), &[RED, Color(0, 0, 0, 0)], 2, 1);
    }

    assert_eq!(strip.pixel_at(0, 20), Some(Rgb565::new(16, 0, 15)));
    assert_eq!(strip.pixel_at(1, 20), Some(Rgb565::new(16, 0, 15)));
    assert_eq!(strip.pixel_at(2, 20), Some(to_rgb565(BLUE)));
}

#[test]
fn pixel_blocks_are_cut_to_what_lands() {
    let mut buffer = blue_buffer();
    let mut strip = strip_on(&mut buffer);
    {
        let mut renderer = StripRenderer::new(&mut strip, rect(0, 0, 240, 240), OPAQUE);
        renderer.draw_pixels((-1, 19), &[RED; 9], 3, 3);
        renderer.draw_pixels((5, 20), &[RED], 0, 1);
    }

    assert_eq!(strip.pixel_at(0, 20), Some(to_rgb565(RED)));
    assert_eq!(strip.pixel_at(1, 21), Some(to_rgb565(RED)));
    assert_eq!(strip.pixel_at(2, 20), Some(to_rgb565(BLUE)));
    assert_eq!(strip.pixel_at(0, 22), Some(to_rgb565(BLUE)));
}

#[test]
fn clear_colours_and_clips_outside_the_strip_draw_nothing() {
    let mut buffer = blue_buffer();
    let mut strip = strip_on(&mut buffer);
    let clear = Color(255, 0, 0, 0);
    {
        let mut renderer = StripRenderer::new(&mut strip, rect(0, 0, 240, 240), OPAQUE);
        renderer.blend_rect(rect(0, 20, 10, 4), clear);
        renderer.blend_row(0, 20, clear, &[255; 10]);
        renderer.blend_row(0, 30, RED, &[255; 10]);
        renderer.draw_text((0, 40), "x", clear);
    }
    let mut elsewhere = StripRenderer::new(&mut strip, rect(100, 100, 5, 5), OPAQUE);
    elsewhere.fill_rect(rect(0, 0, 240, 240), RED);
    elsewhere.blend_rect(rect(0, 0, 240, 240), RED);
    elsewhere.draw_text((101, 120), "x", RED);
    assert_eq!(elsewhere.visible_area(), None);

    assert!(strip.pixels().all(|pixel| pixel == to_rgb565(BLUE)));
}

#[test]
fn shapes_are_worked_out_only_where_they_land() {
    let mut buffer = blue_buffer();
    let mut strip = strip_on(&mut buffer);
    {
        let mut renderer = StripRenderer::new(&mut strip, rect(0, 0, 240, 240), OPAQUE);
        renderer.fill_disc_aa(PointF::new(5.0, 22.0), 3.0, RED);
        renderer.fill_disc_aa(PointF::new(5.0, 100.0), 3.0, RED);
        renderer.fill_arc_aa(PointF::new(5.0, 100.0), 3.0, 1.0, 1.0, 0.0, 1.0, 0.0, 6.3, RED);
        renderer.stroke_line_aa(PointF::new(0.0, 23.5), PointF::new(10.0, 23.5), 1.0, RED);
    }

    assert_eq!(strip.pixel_at(5, 22), Some(to_rgb565(RED)));
    assert_eq!(strip.pixel_at(9, 23), Some(to_rgb565(RED)));
}

#[test]
fn plain_text_draws_whole_lines_over_what_is_there() {
    let mut buffer = vec![battletime_display::panel_pixel(to_rgb565(BLUE)); 40 * 30];
    let mut strip = Strip::new(&mut buffer, rect(0, 0, 40, 30));
    {
        let mut renderer = StripRenderer::new(&mut strip, rect(0, 0, 40, 30), OPAQUE);
        renderer.draw_text((0, 20), "x", RED);
        renderer.draw_text((0, 5), "cut", RED);
    }

    assert!(strip.pixels().any(|pixel| pixel == to_rgb565(RED)));
    assert!((0..40).all(|x| strip.pixel_at(x, 0) == Some(to_rgb565(BLUE))));
}

/// A 40 by 40 solid square glyph that counts the pixels it is asked to work out.
struct CountingFont {
    pixels_sampled: core::cell::Cell<usize>,
}

impl FontMetrics for CountingFont {
    fn glyph_metrics(&self, _character: char) -> Option<GlyphInfo> {
        Some(GlyphInfo {
            advance_fp16: 40 * 16,
            bearing_x: 0,
            bearing_y: 40,
            width: 40,
            height: 40,
        })
    }

    fn line_metrics(&self) -> FontLineMetrics {
        FontLineMetrics {
            line_height: 40,
            ascent: 40,
            descent: 0,
        }
    }

    fn glyph_coverage_row(&self, _character: char, _row: u16, _x_offset: u16, coverage: &mut [u8]) -> bool {
        self.pixels_sampled
            .set(self.pixels_sampled.get() + coverage.len());
        coverage.fill(u8::MAX);
        true
    }
}

#[test]
fn a_glyph_is_worked_out_only_where_it_lands() {
    let mut buffer = blue_buffer();
    let mut strip = strip_on(&mut buffer);
    let font = CountingFont {
        pixels_sampled: core::cell::Cell::new(0),
    };
    {
        let mut renderer = StripRenderer::new(&mut strip, rect(0, 0, 240, 240), OPAQUE);
        renderer.draw_glyph(&font, '8', (-5, 50), RED);
    }

    // The strip is 10 by 4 at row 20 and the glyph covers it from column -5, so only those 40 pixels count.
    assert_eq!(font.pixels_sampled.get(), 40);
    assert!(strip.pixels().all(|pixel| pixel == to_rgb565(RED)));
}
