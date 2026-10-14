//! Tests for the text font: reading the file, putting glyphs on whole pixels, and their coverage.

extern crate std;

use super::*;
use std::vec::Vec;

// ============================================================================
// Helpers
// ============================================================================

/// Records every row blended, as a glyph would reach the screen.
#[derive(Default)]
struct RowRecorder {
    rows: Vec<(i32, i32, Vec<u8>)>,
    visible: Option<Rect>,
}

impl Renderer for RowRecorder {
    fn fill_rect(&mut self, _rect: Rect, _color: Color) {}

    fn draw_text(&mut self, _position: (i32, i32), _text: &str, _color: Color) {}

    fn blend_row(&mut self, x: i32, y: i32, _color: Color, coverage: &[u8]) {
        self.rows.push((x, y, coverage.to_vec()));
    }

    fn visible_area(&self) -> Option<Rect> {
        self.visible
    }
}

impl RowRecorder {
    /// The ink at one pixel, or 0 where nothing was drawn.
    fn ink_at(&self, x: i32, y: i32) -> u8 {
        self.rows
            .iter()
            .filter(|(_, row_y, _)| *row_y == y)
            .filter_map(|(left, _, coverage)| coverage.get(usize::try_from(x - left).ok()?).copied())
            .max()
            .unwrap_or(0)
    }

    fn inked_pixels(&self) -> usize {
        self.rows
            .iter()
            .flat_map(|(_, _, coverage)| coverage)
            .filter(|&&ink| ink > 0)
            .count()
    }
}

fn drawn(font: FontName, character: char) -> RowRecorder {
    let mut recorder = RowRecorder::default();
    font.draw_character(&mut recorder, character, (20, 100), Color(255, 255, 255, 255));
    recorder
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

fn white() -> Color {
    Color(255, 255, 255, 255)
}

// ============================================================================
// The font file
// ============================================================================

#[test]
fn the_font_holds_ascii_and_the_extra_signs() {
    for code in 0x20..0x7F {
        let character = char::from_u32(code).unwrap();
        assert!(FONT.glyph(character).is_some(), "{character:?} is missing");
    }
    for character in "°±×÷←✓✕".chars() {
        assert!(FONT.glyph(character).is_some(), "{character:?} is missing");
    }
    assert!(FONT.glyph('é').is_none());
    assert!(FONT.glyph('\u{1F600}').is_none());
}

#[test]
fn the_font_is_small_enough_to_ship_in_place_of_the_bitmaps() {
    assert!(text_font_bytes() < 4 * 1024, "{} bytes", text_font_bytes());
}

#[test]
fn every_glyph_reads_back_whole() {
    for index in 0..FONT.glyph_count() {
        let (character, entry) = FONT.entry_at(index).unwrap();
        assert!(entry.shape_count as usize <= MOST_SHAPES, "{character:#x}");
        assert_eq!(
            FONT.shapes(entry).count(),
            entry.shape_count as usize,
            "{character:#x}"
        );
    }
}

#[test]
fn a_file_that_is_not_the_font_has_no_glyphs() {
    static NOT_A_FONT: [u8; 16] = *b"NTFN\x01\x04\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00";
    let font = GeometricFont::empty();
    assert_eq!(font.glyph_count(), 0);
    assert!(font.heights().is_none());
    font.install(&NOT_A_FONT);
    assert_eq!(font.glyph_count(), 0);
    assert!(font.glyph('A').is_none());
}

#[test]
fn a_glyph_cut_short_stops_at_its_last_whole_shape() {
    // One glyph, 'A', with two shapes listed but only one line and half of another in the file.
    static CUT_SHORT: [u8; 34] = [
        b'N', b'T', b'G', b'F', 1, 0, 1, 0, 100, 76, 82, 0xE6, 0, 0, 0, 0, //
        b'A', 0, 82, 2, 0, 0, //
        LINE, 1, 2, 3, 4, //
        LINE, 5, 6, 7, 8, 9, 10,
    ];
    static TRUNCATED: [u8; 30] = [
        b'N', b'T', b'G', b'F', 1, 0, 1, 0, 100, 76, 82, 0xE6, 0, 0, 0, 0, //
        b'A', 0, 82, 2, 0, 0, //
        LINE, 1, 2, 3, 4, //
        LINE, 5, 6,
    ];
    let font = GeometricFont::empty();
    font.install(&TRUNCATED);
    let glyph = font.glyph('A').unwrap();
    assert_eq!(font.shapes(glyph).count(), 1);
    font.install(&CUT_SHORT);
    assert_eq!(font.shapes(font.glyph('A').unwrap()).count(), 2);
}

#[test]
fn an_unknown_shape_ends_the_glyph() {
    static ODD_SHAPE: [u8; 27] = [
        b'N', b'T', b'G', b'F', 1, 0, 1, 0, 100, 76, 82, 0xE6, 0, 0, 0, 0, //
        b'A', 0, 82, 2, 0, 0, //
        DOT, 1, 2, //
        9, 0,
    ];
    let font = GeometricFont::empty();
    font.install(&ODD_SHAPE);
    assert_eq!(font.shapes(font.glyph('A').unwrap()).count(), 1);
}

#[test]
fn the_font_notes_which_ascii_it_has_and_looks_up_the_rest() {
    assert!(FONT.has('A') && FONT.has(' ') && FONT.has('~'));
    assert!(!FONT.has('\u{7F}'));
    assert!(FONT.has('°') && FONT.has('✓'));
    assert!(!FONT.has('é'));
}

#[test]
fn installing_a_file_that_is_not_the_font_forgets_what_was_checked() {
    static NOT_A_FONT: [u8; 16] = *b"NTFN\x01\x04\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00";
    static FONT_FILE: &[u8] = include_bytes!("../fonts/geometric.ntfont");
    let font = GeometricFont::empty();
    font.install(FONT_FILE);
    assert!(font.has('A'));
    font.install(&NOT_A_FONT);
    assert!(!font.has('A'));
    assert!(font.data().is_none());
    font.install(FONT_FILE);
    assert!(font.has('A'));
}

#[test]
fn glyphs_are_found_past_a_gap_in_the_characters() {
    // The font jumps from '~' to '°', so the quick guess misses and the search finds it.
    let degree = FONT.glyph('°').unwrap();
    assert!(degree.shape_count > 0);
    assert!(FONT.glyph('\u{7F}').is_none());
}

// ============================================================================
// Sizes
// ============================================================================

#[test]
fn each_size_keeps_the_advance_and_line_of_the_bitmap_it_replaced() {
    let expected = [
        (FontName::Small, 10, 22),
        (FontName::Normal, 12, 27),
        (FontName::Medium, 18, 40),
        (FontName::Large, 25, 56),
        (FontName::Huge, 46, 62),
        (FontName::Giant, 68, 92),
        (FontName::Condensed, 20, 39),
    ];
    for (font, advance, height) in expected {
        let metrics = font.metrics();
        assert_eq!(character_advance(metrics, '0'), advance, "{font:?}");
        assert_eq!(line_height(metrics), height, "{font:?}");
    }
}

#[test]
fn text_sizes_are_monospaced_and_giant_is_not() {
    let small = FontName::Small.metrics();
    assert_eq!(character_advance(small, ':'), character_advance(small, 'W'));
    let giant = FontName::Giant.metrics();
    assert!(character_advance(giant, ':') < character_advance(giant, '0'));
    assert_eq!(text_width(giant, "10:09"), 4 * 68 + 30);
}

#[test]
fn a_capital_reaches_its_capital_height_and_sits_on_the_baseline() {
    for (font, capital) in [
        (FontName::Small, 12),
        (FontName::Large, 31),
        (FontName::Giant, 87),
    ] {
        let recorder = drawn(font, 'H');
        let rows: Vec<i32> = recorder
            .rows
            .iter()
            .filter(|(_, _, coverage)| coverage.iter().any(|&ink| ink > 128))
            .map(|(_, y, _)| *y)
            .collect();
        let (top, bottom) = (*rows.iter().min().unwrap(), *rows.iter().max().unwrap());
        assert_eq!(bottom, 99, "{font:?} should end on the row above the baseline");
        assert_eq!(100 - top, capital, "{font:?}");
    }
}

#[test]
fn hinting_puts_a_stem_on_whole_pixels() {
    // Small's stroke is two pixels, so the stem of an l covers exactly two whole pixels in a row.
    let recorder = drawn(FontName::Small, 'l');
    let row = recorder.rows.iter().find(|(_, y, _)| *y == 95).unwrap();
    let inked: Vec<u8> = row.2.iter().copied().filter(|&ink| ink > 0).collect();
    assert_eq!(inked, [255, 255]);
}

#[test]
fn a_half_pixel_stroke_keeps_one_edge_sharp() {
    // Normal's stroke is two and a half pixels: two solid pixels and one half covered.
    let recorder = drawn(FontName::Normal, 'l');
    let row = recorder.rows.iter().find(|(_, y, _)| *y == 94).unwrap();
    let inked: Vec<u8> = row.2.iter().copied().filter(|&ink| ink > 0).collect();
    assert_eq!(inked.len(), 3, "{inked:?}");
    assert_eq!(inked.iter().filter(|&&ink| ink == 255).count(), 2, "{inked:?}");
}

#[test]
fn a_bowl_comes_out_even_on_both_sides() {
    let recorder = drawn(FontName::Large, 'o');
    let row = recorder.rows.iter().find(|(_, y, _)| *y == 88).unwrap();
    let inked: Vec<usize> = (0..row.2.len()).filter(|&index| row.2[index] > 128).collect();
    let (first, last) = (inked[0], *inked.last().unwrap());
    let middle = (first + last) as f32 / 2.0;
    let left_stem = inked.iter().filter(|&&index| (index as f32) < middle).count();
    let right_stem = inked.iter().filter(|&&index| (index as f32) > middle).count();
    assert_eq!(left_stem, right_stem);
}

#[test]
fn a_space_draws_nothing_but_moves_the_pen() {
    let recorder = drawn(FontName::Normal, ' ');
    assert_eq!(recorder.inked_pixels(), 0);
    assert_eq!(character_advance(FontName::Normal.metrics(), ' '), 12);
}

#[test]
fn a_character_the_font_lacks_draws_nothing_and_takes_no_room() {
    let recorder = drawn(FontName::Normal, 'é');
    assert!(recorder.rows.is_empty());
    assert_eq!(character_advance(FontName::Normal.metrics(), 'é'), 0);
}

#[test]
fn only_the_rows_the_renderer_shows_are_worked_out() {
    let mut recorder = RowRecorder {
        visible: Some(Rect {
            x: 0,
            y: 90,
            width: 240,
            height: 4,
        }),
        ..Default::default()
    };
    FontName::Giant.draw_character(&mut recorder, '8', (20, 100), white());
    let rows: Vec<i32> = recorder.rows.iter().map(|(_, y, _)| *y).collect();
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|y| (90..94).contains(y)), "{rows:?}");
}

#[test]
fn a_glyph_drawn_in_pieces_matches_one_drawn_whole() {
    let whole = drawn(FontName::Giant, 'W');
    // The giant W is wider than one piece would be if pieces were small; check every pixel against the metrics box.
    let glyph = FontName::Giant.metrics().glyph_metrics('W').unwrap();
    let left = 20 + glyph.bearing_x as i32;
    let top = 100 - glyph.bearing_y as i32;
    for y in top..top + glyph.height as i32 {
        let mut coverage = std::vec![0u8; glyph.width as usize];
        assert!(FontName::Giant
            .metrics()
            .glyph_coverage_row('W', (y - top) as u16, 0, &mut coverage));
        for (index, &ink) in coverage.iter().enumerate() {
            // The two place the pen from different origins, so rounding may differ by one step of ink.
            let other = whole.ink_at(left + index as i32, y);
            assert!(
                ink.abs_diff(other) <= 1,
                "at {}, {y}: {ink} and {other}",
                left + index as i32
            );
        }
    }
}

#[test]
fn the_metrics_box_holds_all_of_a_glyphs_ink() {
    for font in [
        FontName::Small,
        FontName::Normal,
        FontName::Large,
        FontName::Giant,
        FontName::Condensed,
    ] {
        for character in "AMWgjy@&{}0".chars() {
            let glyph = font.metrics().glyph_metrics(character).unwrap();
            let left = 20 + glyph.bearing_x as i32;
            let top = 100 - glyph.bearing_y as i32;
            let recorder = drawn(font, character);
            for (x, y, coverage) in &recorder.rows {
                for (index, &ink) in coverage.iter().enumerate() {
                    if ink > 0 {
                        let column = x + index as i32;
                        assert!(
                            (left..left + glyph.width as i32).contains(&column),
                            "{font:?} {character:?}"
                        );
                        assert!(
                            (top..top + glyph.height as i32).contains(y),
                            "{font:?} {character:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn an_empty_font_has_no_glyphs_and_no_shapes() {
    let font = GeometricFont::empty();
    assert!(font.glyph('A').is_none());
    let entry = GlyphEntry {
        width: 82,
        shape_count: 1,
        offset: 0,
    };
    assert_eq!(font.shapes(entry).count(), 0);
}

// ============================================================================
// Putting strokes on whole pixels
// ============================================================================

#[test]
fn chosen_lines_land_on_the_grid_and_points_between_stretch() {
    let mut fit = AxisFit::new(0.3, 1.0);
    fit.add(0.0);
    fit.add(10.0);
    fit.fit(0.0);
    assert!(close(fit.place(0.0), 0.0));
    assert!(close(fit.place(10.0), 10.0));
    assert!(close(fit.place(5.0), 5.0));
    // Outside the chosen lines, points move as the nearest one did.
    assert!(close(fit.place(-2.0), -2.0));
    assert!(close(fit.place(12.0), 12.0));
}

#[test]
fn lines_a_pixel_apart_are_never_merged() {
    let mut fit = AxisFit::new(0.0, 1.0);
    fit.add(0.4);
    fit.add(1.45);
    fit.fit(0.0);
    assert!(close(fit.place(0.4), 0.0));
    assert!(close(fit.place(1.45), 1.0));
    // Lines closer than a pixel may share one.
    let mut near = AxisFit::new(0.0, 1.0);
    near.add(0.4);
    near.add(0.45);
    near.fit(0.0);
    assert!(close(near.place(0.4), near.place(0.45)));
}

#[test]
fn a_falling_axis_fits_as_well_as_a_rising_one() {
    // y grows down the screen while font units grow up it.
    let mut fit = AxisFit::new(100.0, -0.37);
    for line in [0.0, 76.0, 100.0, -26.0, 76.0] {
        fit.add(line);
    }
    fit.fit(0.5);
    for line in [0.0, 76.0, 100.0, -26.0] {
        let placed = fit.place(line);
        assert!(close(placed - floor(placed), 0.5), "{line}: {placed}");
    }
    assert!(fit.place(100.0) < fit.place(76.0));
    assert_eq!(fit.count, 4, "the repeated line is kept once");
}

#[test]
fn an_axis_with_no_lines_leaves_points_where_they_were() {
    let fit = AxisFit::new(3.0, 2.0);
    assert!(close(fit.place(1.5), 6.0));
}

#[test]
fn an_axis_takes_only_as_many_lines_as_it_has_room_for() {
    let mut fit = AxisFit::new(0.0, 1.0);
    for line in 0..MOST_CONTROLS + 5 {
        fit.add(line as f32 * 3.0);
    }
    assert_eq!(fit.count, MOST_CONTROLS);
}

// ============================================================================
// Coverage
// ============================================================================

#[test]
fn arcs_tell_their_inside_from_their_ends() {
    // A quarter arc from 0 to 90 degrees, radius 10, centred at (50, 50) on screen.
    let arc = Placed::Arc {
        x: 50.0,
        y: 50.0,
        across: 10.0,
        down: 10.0,
        start: direction_of(0),
        end: direction_of(64),
        sweep: Sweep::of(0, 64),
    };
    // On the arc, upper right of the centre.
    assert!(arc.distance(50.0 + 7.07, 50.0 - 7.07) < 0.05);
    // Lower right is past the start, so the distance is to the end at (60, 50).
    assert!((arc.distance(60.0, 55.0) - 5.0).abs() < 0.05);
    // A three-quarter arc includes that point.
    let long = Placed::Arc {
        x: 50.0,
        y: 50.0,
        across: 10.0,
        down: 10.0,
        start: direction_of(0),
        end: direction_of(192),
        sweep: Sweep::of(0, 192),
    };
    assert!(long.distance(50.0 - 7.07, 50.0 + 7.07) < 0.05);
    assert!((long.distance(57.07, 57.07) - long.distance(57.07, 57.07)).abs() < 1e-6);
    assert!(long.distance(57.07, 57.07) > 1.0);
}

#[test]
fn sweeps_are_sorted_by_how_much_of_a_turn_they_cover() {
    assert!(matches!(Sweep::of(10, 10), Sweep::Whole));
    assert!(matches!(Sweep::of(0, 128), Sweep::UpToHalf));
    assert!(matches!(Sweep::of(200, 72), Sweep::UpToHalf));
    assert!(matches!(Sweep::of(0, 129), Sweep::MoreThanHalf));
}

#[test]
fn an_ellipse_is_near_its_curve_and_far_from_its_middle() {
    let ellipse = Placed::Arc {
        x: 0.0,
        y: 0.0,
        across: 20.0,
        down: 10.0,
        start: direction_of(0),
        end: direction_of(0),
        sweep: Sweep::Whole,
    };
    assert!(ellipse.distance(20.0, 0.0) < 0.01);
    assert!(ellipse.distance(0.0, -10.0) < 0.01);
    assert!((ellipse.distance(0.0, -12.0) - 2.0).abs() < 0.1);
    assert_eq!(ellipse.distance(0.0, 0.0), 10.0);
    let flat = Placed::Arc {
        x: 3.0,
        y: 4.0,
        across: 0.0,
        down: 10.0,
        start: direction_of(0),
        end: direction_of(0),
        sweep: Sweep::Whole,
    };
    assert_eq!(flat.distance(0.0, 0.0), 5.0);
}

#[test]
fn a_ring_meets_a_row_in_two_pieces_or_one() {
    let ring = Placed::Arc {
        x: 50.0,
        y: 50.0,
        across: 20.0,
        down: 20.0,
        start: direction_of(0),
        end: direction_of(0),
        sweep: Sweep::Whole,
    };
    let [left, right] = ring.columns(50.0, 2.0);
    let (left, right) = (left.unwrap(), right.unwrap());
    assert!((left.0 - 28.0).abs() < 0.01 && (left.1 - 32.0).abs() < 0.01);
    assert!((right.0 - 68.0).abs() < 0.01 && (right.1 - 72.0).abs() < 0.01);
    // Across the top the hole is gone, so the row is one piece.
    let [top, none] = ring.columns(30.0, 2.0);
    assert!(top.is_some() && none.is_none());
    assert_eq!(ring.columns(10.0, 2.0), [None, None]);
}

#[test]
fn a_line_reaches_only_the_columns_near_it_on_each_row() {
    let line = Placed::line(0.0, 0.0, 100.0, 100.0);
    let [span, none] = line.columns(50.0, 2.0);
    let span = span.unwrap();
    assert!(none.is_none());
    assert!((span.0 - 46.0).abs() < 0.01 && (span.1 - 54.0).abs() < 0.01);
    assert_eq!(line.columns(120.0, 2.0), [None, None]);
    let flat = Placed::line(10.0, 5.0, 30.0, 5.0);
    assert_eq!(flat.columns(6.0, 1.0)[0], Some((9.0, 31.0)));
    let dot = Placed::Dot { x: 5.0, y: 5.0 };
    assert_eq!(dot.columns(5.5, 2.0)[0], Some((3.0, 7.0)));
    assert_eq!(dot.columns(9.0, 2.0), [None, None]);
}

#[test]
fn joins_take_the_nearest_shape_so_they_are_no_darker() {
    // Two lines meeting at a corner: the corner pixel is solid, never more than solid.
    let shapes = [
        Placed::line(10.5, 10.5, 20.5, 10.5),
        Placed::line(10.5, 10.5, 10.5, 20.5),
    ];
    let mut coverage = [0u8; 16];
    cover_row(&shapes, 0.5, 10.5, 8, &mut coverage);
    assert_eq!(coverage[2], 255);
    assert_eq!(coverage[1], 0);
    let inked = coverage.iter().filter(|&&ink| ink > 0).count();
    assert_eq!(inked, 11);
}

#[test]
fn rings_settled_without_a_root_match_their_exact_distance() {
    // A whole ring and a part of one, each pixel worked out quickly and by its exact distance.
    for (start, end) in [(0, 0), (20, 150), (200, 100)] {
        let ring = Placed::Arc {
            x: 30.0,
            y: 30.0,
            across: 12.0,
            down: 12.0,
            start: direction_of(start),
            end: direction_of(end),
            sweep: Sweep::of(start, end),
        };
        for half in [1.0, 2.5] {
            for row in 10..50 {
                let y = row as f32 + 0.5;
                let mut quick = [0u8; 60];
                cover_row(&[ring], half, y, 0, &mut quick);
                for (column, &ink) in quick.iter().enumerate() {
                    let exact = ink_of(half - ring.distance(column as f32 + 0.5, y) + 0.5);
                    assert!(
                        ink.abs_diff(exact) <= 1,
                        "{start}-{end} at {column}, {row}: {ink} and {exact}"
                    );
                }
            }
        }
    }
}

#[test]
fn an_ellipse_is_never_settled_without_its_distance() {
    let ellipse = Placed::Arc {
        x: 0.0,
        y: 0.0,
        across: 20.0,
        down: 10.0,
        start: direction_of(0),
        end: direction_of(0),
        sweep: Sweep::Whole,
    };
    assert_eq!(ellipse.ring_ink(20.0, 0.0, 2.0, 1.0), None);
    assert_eq!(Placed::Dot { x: 0.0, y: 0.0 }.ring_ink(0.0, 0.0, 2.0, 1.0), None);
}

#[test]
fn coverage_ramps_over_one_pixel() {
    assert_eq!(ink_of(-0.1), 0);
    assert_eq!(ink_of(0.5), 128);
    assert_eq!(ink_of(1.2), 255);
}

#[test]
fn the_unused_font_number_draws_as_huge() {
    assert_eq!(FontName::from_value(6), FontName::Huge);
}
