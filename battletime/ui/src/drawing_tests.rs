//! Tests for the geometry, the rasterizers, the renderer's default calls and the shapes.

extern crate std;

use std::{vec, vec::Vec};

use crate::geometry::{divide_by_255, scale, Color, PointF, Rect};
use crate::raster::{rasterize_arc, rasterize_disc, rasterize_line, CoverageSink};
use crate::renderer::{ClipRenderer, FontLineMetrics, FontMetrics, GlyphInfo, Renderer};
use crate::shapes::{draw_border_straight, draw_rounded_border, draw_widget_bg, fill_rounded_rect, Style};

// ============================================================================
// A canvas that remembers each pixel's colour
// ============================================================================

const SIDE: i32 = 40;
const WHITE: Color = Color(255, 255, 255, 255);
const RED: Color = Color(255, 0, 0, 255);

struct Canvas {
    pixels: Vec<Option<Color>>,
    texts: Vec<((i32, i32), std::string::String)>,
}

impl Canvas {
    fn new() -> Canvas {
        Canvas {
            pixels: vec![None; (SIDE * SIDE) as usize],
            texts: Vec::new(),
        }
    }

    fn at(&self, x: i32, y: i32) -> Option<Color> {
        self.pixels[(y * SIDE + x) as usize]
    }

    fn painted(&self) -> usize {
        self.pixels.iter().filter(|pixel| pixel.is_some()).count()
    }

    fn set(&mut self, rect: Rect, color: Color) {
        for y in rect.y.max(0)..rect.bottom().min(SIDE) {
            for x in rect.x.max(0)..rect.right().min(SIDE) {
                self.pixels[(y * SIDE + x) as usize] = Some(color);
            }
        }
    }
}

impl Renderer for Canvas {
    fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.set(rect, color);
    }

    fn blend_rect(&mut self, rect: Rect, color: Color) {
        self.set(rect, color);
    }

    fn draw_text(&mut self, position: (i32, i32), text: &str, _color: Color) {
        self.texts.push((position, text.into()));
    }
}

fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
    Rect { x, y, width, height }
}

// ============================================================================
// Geometry
// ============================================================================

#[test]
fn rectangles_join_and_overlap() {
    let left = rect(0, 0, 10, 10);
    let right = rect(5, 5, 10, 10);

    assert_eq!(left.union(right), rect(0, 0, 15, 15));
    assert_eq!(left.intersect(right), Some(rect(5, 5, 5, 5)));
    assert_eq!(left.intersect(rect(10, 0, 5, 5)), None);
    assert!(rect(0, 0, 0, 4).is_empty());
}

#[test]
fn a_colour_fades_by_a_share() {
    assert_eq!(Color(1, 2, 3, 255).with_alpha(128), Color(1, 2, 3, 128));
    assert_eq!(Color(1, 2, 3, 128).with_alpha(0), Color(1, 2, 3, 0));
}

#[test]
fn every_product_of_two_shares_divides_by_255_to_the_nearest_step() {
    for value in 0..=255u16 {
        for share in 0..=255u16 {
            let product = value * share;
            let nearest = (u32::from(product) * 2 + 255) / 510;
            assert_eq!(u32::from(divide_by_255(product)), nearest, "{value} x {share}");
        }
    }
    assert_eq!(scale(255, 255), 255);
    assert_eq!(scale(200, 255), 200);
    assert_eq!(scale(1, 128), 1);
}

// ============================================================================
// Rasterizers
// ============================================================================

#[derive(Default)]
struct Rows {
    rows: Vec<(i32, i32, Vec<u8>)>,
}

impl Rows {
    fn coverage_at(&self, x: i32, y: i32) -> u8 {
        self.rows
            .iter()
            .filter(|(start, row, _)| *row == y && (*start..*start + 256).contains(&x))
            .find_map(|(start, _, coverage)| coverage.get((x - start) as usize).copied())
            .unwrap_or(0)
    }
}

impl CoverageSink for Rows {
    fn row(&mut self, x: i32, y: i32, coverage: &[u8]) {
        self.rows.push((x, y, coverage.to_vec()));
    }
}

const ANYWHERE: Rect = Rect {
    x: -1000,
    y: -1000,
    width: 2000,
    height: 2000,
};

#[test]
fn a_disc_is_solid_inside_soft_at_its_edge_and_empty_outside() {
    let mut rows = Rows::default();

    rasterize_disc(PointF::new(20.0, 20.0), 8.0, ANYWHERE, &mut rows);

    assert_eq!(rows.coverage_at(20, 20), 255);
    let edge = rows.coverage_at(27, 20);
    assert!(edge > 0 && edge < 255, "edge coverage {edge}");
    assert_eq!(rows.coverage_at(30, 20), 0);
    assert_eq!(rows.coverage_at(20, 20), rows.coverage_at(19, 19));
}

#[test]
fn shapes_never_reach_outside_their_clip() {
    let clip = rect(0, 18, 40, 4);
    let mut rows = Rows::default();

    rasterize_disc(PointF::new(20.0, 20.0), 8.0, clip, &mut rows);
    rasterize_line(
        PointF::new(0.0, 0.0),
        PointF::new(39.0, 39.0),
        3.0,
        clip,
        &mut rows,
    );
    rasterize_arc(
        PointF::new(20.0, 20.0),
        10.0,
        6.0,
        1.0,
        0.0,
        1.0,
        0.0,
        6.3,
        clip,
        &mut rows,
    );

    assert!(!rows.rows.is_empty());
    assert!(rows.rows.iter().all(|(_, y, _)| (18..22).contains(y)));
}

#[test]
fn nothing_is_drawn_for_shapes_with_no_size() {
    let mut rows = Rows::default();

    rasterize_disc(PointF::new(20.0, 20.0), 0.0, ANYWHERE, &mut rows);
    rasterize_line(
        PointF::new(5.0, 5.0),
        PointF::new(5.0, 5.0),
        2.0,
        ANYWHERE,
        &mut rows,
    );
    rasterize_line(
        PointF::new(5.0, 5.0),
        PointF::new(9.0, 5.0),
        0.0,
        ANYWHERE,
        &mut rows,
    );
    rasterize_arc(
        PointF::new(20.0, 20.0),
        5.0,
        5.0,
        1.0,
        0.0,
        0.0,
        1.0,
        1.5,
        ANYWHERE,
        &mut rows,
    );
    rasterize_disc(PointF::new(20.0, 20.0), 4.0, rect(100, 100, 5, 5), &mut rows);

    assert!(rows.rows.is_empty());
}

#[test]
fn a_line_covers_its_path_and_nothing_beside_it() {
    let mut rows = Rows::default();

    rasterize_line(
        PointF::new(2.0, 10.5),
        PointF::new(30.0, 10.5),
        2.0,
        ANYWHERE,
        &mut rows,
    );

    assert_eq!(rows.coverage_at(15, 10), 255);
    assert_eq!(rows.coverage_at(15, 14), 0);
    assert_eq!(rows.coverage_at(35, 10), 0);
}

#[test]
fn a_slanted_line_covers_its_middle() {
    let mut rows = Rows::default();

    rasterize_line(
        PointF::new(0.0, 0.0),
        PointF::new(30.0, 30.0),
        3.0,
        ANYWHERE,
        &mut rows,
    );

    assert_eq!(rows.coverage_at(15, 15), 255);
    assert_eq!(rows.coverage_at(25, 5), 0);
}

#[test]
fn a_run_longer_than_the_buffer_is_handed_over_in_pieces() {
    let mut rows = Rows::default();

    rasterize_line(
        PointF::new(0.0, 5.5),
        PointF::new(600.0, 5.5),
        2.0,
        ANYWHERE,
        &mut rows,
    );

    let pieces: Vec<_> = rows.rows.iter().filter(|(_, y, _)| *y == 5).collect();
    assert!(pieces.len() >= 3);
    assert!(pieces
        .windows(2)
        .all(|pair| pair[0].0 + pair[0].2.len() as i32 == pair[1].0));
}

#[test]
fn a_quarter_arc_covers_only_its_quarter_of_the_ring() {
    let mut rows = Rows::default();

    // From three o'clock to six o'clock, turning clockwise on screen.
    rasterize_arc(
        PointF::new(20.0, 20.0),
        12.0,
        6.0,
        1.0,
        0.0,
        0.0,
        1.0,
        core::f32::consts::FRAC_PI_2,
        ANYWHERE,
        &mut rows,
    );

    assert_eq!(rows.coverage_at(26, 26), 255);
    assert_eq!(rows.coverage_at(13, 13), 0);
    assert_eq!(rows.coverage_at(20, 20), 0);
}

#[test]
fn a_ring_has_a_hole_and_an_arc_backwards_covers_the_other_side() {
    let mut whole = Rows::default();
    let mut backwards = Rows::default();
    let mut most = Rows::default();

    rasterize_arc(
        PointF::new(20.0, 20.0),
        12.0,
        6.0,
        1.0,
        0.0,
        1.0,
        0.0,
        6.3,
        ANYWHERE,
        &mut whole,
    );
    rasterize_arc(
        PointF::new(20.0, 20.0),
        12.0,
        0.0,
        1.0,
        0.0,
        0.0,
        -1.0,
        -core::f32::consts::FRAC_PI_2,
        ANYWHERE,
        &mut backwards,
    );
    rasterize_arc(
        PointF::new(20.0, 20.0),
        12.0,
        1.5,
        1.0,
        0.0,
        0.0,
        -1.0,
        4.7,
        ANYWHERE,
        &mut most,
    );

    assert_eq!(whole.coverage_at(20, 20), 0);
    assert_eq!(whole.coverage_at(29, 20), 255);
    assert_eq!(backwards.coverage_at(26, 13), 255);
    assert_eq!(backwards.coverage_at(26, 26), 0);
    assert_eq!(most.coverage_at(13, 26), 255);
    assert_eq!(most.coverage_at(26, 13), 0);
}

#[test]
fn a_ring_hands_over_each_row_as_a_run_either_side_of_its_hole() {
    let mut ring = Rows::default();

    rasterize_arc(
        PointF::new(20.0, 20.0),
        18.0,
        14.0,
        1.0,
        0.0,
        1.0,
        0.0,
        6.3,
        ANYWHERE,
        &mut ring,
    );

    let middle_row: Vec<_> = ring.rows.iter().filter(|(_, y, _)| *y == 20).collect();
    assert_eq!(middle_row.len(), 2);
    assert!(middle_row[0].0 < 20 - 14 && middle_row[1].0 > 20 + 12);
    assert!(ring
        .rows
        .iter()
        .all(|(_, _, coverage)| coverage.iter().all(|&amount| amount > 0)));
}

// ============================================================================
// The renderer's default calls
// ============================================================================

#[test]
fn soft_shapes_reach_a_renderer_through_its_rows() {
    let mut canvas = Canvas::new();

    canvas.fill_disc_aa(PointF::new(10.0, 10.0), 4.0, RED);
    canvas.stroke_line_aa(PointF::new(20.0, 30.5), PointF::new(38.0, 30.5), 2.0, RED);
    canvas.fill_arc_aa(PointF::new(30.0, 10.0), 6.0, 3.0, 1.0, 0.0, 1.0, 0.0, 6.3, RED);

    assert_eq!(canvas.at(10, 10), Some(RED));
    assert_eq!(canvas.at(30, 30), Some(RED));
    assert_eq!(canvas.at(34, 10), Some(RED));
    assert_eq!(canvas.at(30, 10), None);
}

#[test]
fn the_default_row_blend_fades_each_pixel_by_its_coverage() {
    let mut canvas = Canvas::new();

    canvas.blend_row(0, 0, RED, &[255, 0, 128]);

    assert_eq!(canvas.at(0, 0), Some(RED));
    assert_eq!(canvas.at(1, 0), None);
    assert_eq!(canvas.at(2, 0), Some(Color(255, 0, 0, 128)));
}

#[test]
fn the_default_pixel_block_fills_each_pixel() {
    let mut canvas = Canvas::new();

    canvas.draw_pixels((1, 1), &[RED, WHITE, WHITE, RED], 2, 2);

    assert_eq!(canvas.at(1, 1), Some(RED));
    assert_eq!(canvas.at(2, 1), Some(WHITE));
    assert_eq!(canvas.at(2, 2), Some(RED));
}

/// A font of 3x2 glyphs, solid for "a" and without coverage for "b".
struct TinyFont;

impl FontMetrics for TinyFont {
    fn glyph_metrics(&self, character: char) -> Option<GlyphInfo> {
        let size = if character == ' ' { 0 } else { 3 };
        ("ab ".contains(character)).then_some(GlyphInfo {
            advance_fp16: 64,
            bearing_x: 0,
            bearing_y: 2,
            width: size,
            height: 2,
        })
    }

    fn line_metrics(&self) -> FontLineMetrics {
        FontLineMetrics {
            line_height: 3,
            ascent: 2,
            descent: 1,
        }
    }

    fn glyph_coverage_row(&self, character: char, _row: u16, _x_offset: u16, coverage: &mut [u8]) -> bool {
        coverage.fill(255);
        character == 'a'
    }
}

#[test]
fn a_glyph_draws_its_coverage_or_else_its_box() {
    let mut canvas = Canvas::new();

    canvas.draw_glyph(&TinyFont, 'a', (0, 2), RED);
    canvas.draw_glyph(&TinyFont, 'b', (10, 2), WHITE);
    canvas.draw_glyph(&TinyFont, ' ', (20, 2), WHITE);
    canvas.draw_glyph(&TinyFont, 'z', (30, 2), WHITE);

    assert_eq!(canvas.at(2, 1), Some(RED));
    assert_eq!(canvas.at(12, 1), Some(WHITE));
    assert_eq!(canvas.painted(), 12);
    assert_eq!(TinyFont.line_metrics().line_height, 3);
}

// ============================================================================
// Clipping
// ============================================================================

#[test]
fn a_clip_drops_every_pixel_outside_it() {
    let mut canvas = Canvas::new();
    {
        let mut clipped = ClipRenderer::new(&mut canvas, rect(10, 10, 10, 10));
        clipped.fill_rect(rect(0, 0, 40, 40), RED);
        clipped.blend_rect(rect(0, 0, 12, 12), WHITE);
        clipped.blend_row(5, 15, WHITE, &[255; 10]);
        clipped.blend_row(5, 30, WHITE, &[255; 10]);
        clipped.fill_rect(rect(30, 30, 2, 2), WHITE);
    }

    assert_eq!(canvas.painted(), 100);
    assert_eq!(canvas.at(10, 10), Some(WHITE));
    assert_eq!(canvas.at(14, 15), Some(WHITE));
    assert_eq!(canvas.at(15, 15), Some(RED));
}

#[test]
fn a_clip_cuts_pixel_blocks_row_by_row() {
    let mut canvas = Canvas::new();
    let block = [RED; 16];
    {
        let mut clipped = ClipRenderer::new(&mut canvas, rect(2, 2, 2, 2));
        clipped.draw_pixels((0, 0), &block, 4, 4);
        clipped.draw_pixels((2, 2), &block[..4], 2, 2);
        clipped.draw_pixels((20, 20), &block, 4, 4);
        clipped.draw_pixels((0, 0), &block, 0, 4);
        clipped.draw_pixels((0, 0), &block[..9], 4, 4);
    }

    assert_eq!(canvas.painted(), 4);
    assert_eq!(canvas.at(3, 3), Some(RED));
}

#[test]
fn a_clip_keeps_only_text_whose_whole_line_fits() {
    let mut canvas = Canvas::new();
    {
        let mut clipped = ClipRenderer::new(&mut canvas, rect(0, 0, 30, 30));
        clipped.draw_text((2, 20), "fits", RED);
        clipped.draw_text((2, 10), "too high", RED);
        clipped.draw_text((35, 20), "too far right", RED);
    }
    {
        let mut nowhere = ClipRenderer::new(&mut canvas, rect(0, 0, 0, 0));
        nowhere.draw_text((2, 20), "no clip", RED);
        nowhere.fill_rect(rect(0, 0, 4, 4), RED);
        nowhere.blend_row(0, 0, RED, &[255]);
        nowhere.draw_pixels((0, 0), &[RED], 1, 1);
    }

    assert_eq!(canvas.texts, vec![((2, 20), "fits".into())]);
    assert_eq!(canvas.painted(), 0);
}

// ============================================================================
// Shapes
// ============================================================================

#[test]
fn a_rounded_box_leaves_its_corners_clear_and_softens_them() {
    let mut canvas = Canvas::new();

    fill_rounded_rect(&mut canvas, rect(0, 0, 30, 20), RED, 8);

    assert_eq!(canvas.at(0, 0), None);
    assert_eq!(canvas.at(29, 19), None);
    assert_eq!(canvas.at(15, 0), Some(RED));
    assert_eq!(canvas.at(0, 10), Some(RED));
    let softened = canvas
        .pixels
        .iter()
        .flatten()
        .filter(|color| color.3 < 255)
        .count();
    assert!(softened > 0);
}

#[test]
fn a_box_without_room_for_corners_is_square() {
    let mut canvas = Canvas::new();

    fill_rounded_rect(&mut canvas, rect(0, 0, 1, 10), RED, 8);
    fill_rounded_rect(&mut canvas, rect(5, 5, 4, 4), RED, 0);

    assert_eq!(canvas.painted(), 26);
}

#[test]
fn a_pill_is_round_at_both_ends() {
    let mut canvas = Canvas::new();

    fill_rounded_rect(&mut canvas, rect(0, 0, 20, 20), RED, 255);

    assert_eq!(canvas.at(10, 10), Some(RED));
    assert_eq!(canvas.at(0, 0), None);
    assert_eq!(canvas.at(19, 0), None);
}

#[test]
fn a_rounded_border_follows_the_corners_and_leaves_the_middle() {
    let mut canvas = Canvas::new();

    draw_rounded_border(&mut canvas, rect(0, 0, 30, 30), RED, 3, 10);

    assert_eq!(canvas.at(15, 15), None);
    assert_eq!(canvas.at(15, 1), Some(RED));
    assert_eq!(canvas.at(1, 15), Some(RED));
    assert_eq!(canvas.at(0, 0), None);
}

#[test]
fn borders_without_rounding_or_width_draw_square_or_nothing() {
    let mut square = Canvas::new();
    let mut thin = Canvas::new();
    let mut none = Canvas::new();

    draw_rounded_border(&mut square, rect(0, 0, 10, 10), RED, 2, 0);
    draw_rounded_border(&mut thin, rect(0, 0, 1, 10), RED, 1, 6);
    draw_rounded_border(&mut none, rect(0, 0, 10, 10), RED, 0, 4);
    draw_border_straight(&mut none, rect(0, 0, 10, 10), RED, 0);

    assert_eq!(square.painted(), 100 - 36);
    assert_eq!(square.at(0, 0), Some(RED));
    assert_eq!(thin.painted(), 10);
    assert_eq!(none.painted(), 0);
}

#[test]
fn a_frame_skips_clear_parts_and_blends_see_through_ones() {
    let mut clear = Canvas::new();
    let mut faded = Canvas::new();
    let mut bordered = Canvas::new();
    let see_through = Style {
        bg_color: Color(10, 20, 30, 100),
        ..Style::default()
    };

    draw_widget_bg(
        &mut clear,
        rect(0, 0, 10, 10),
        &Style {
            alpha: 0,
            border_width: 2,
            ..Style::default()
        },
    );
    draw_widget_bg(&mut faded, rect(0, 0, 10, 10), &see_through);
    draw_widget_bg(
        &mut bordered,
        rect(0, 0, 10, 10),
        &Style {
            bg_color: Color(0, 0, 0, 0),
            border_width: 1,
            radius: 3,
            ..Style::default()
        },
    );

    assert_eq!(clear.painted(), 0);
    assert_eq!(faded.at(5, 5), Some(Color(10, 20, 30, 100)));
    assert_eq!(bordered.at(5, 5), None);
    assert_eq!(bordered.at(5, 0), Some(Color(0, 0, 0, 255)));
}
