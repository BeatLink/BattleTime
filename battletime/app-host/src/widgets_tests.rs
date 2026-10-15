//! Tests for the value widgets, drawn onto a canvas that remembers each pixel's colour.

extern crate std;

use std::{vec, vec::Vec};

use battletime_ui::{
    geometry::{Color, Rect},
    renderer::Renderer,
    shapes::Style,
};

use super::*;

// ============================================================================
// Canvas
// ============================================================================

const SIDE: i32 = 60;
const TRACK: Color = Color(40, 40, 40, 255);
const ACCENT: Color = Color(0, 200, 0, 255);

struct Canvas {
    pixels: Vec<Option<Color>>,
}

impl Canvas {
    fn new() -> Canvas {
        Canvas {
            pixels: vec![None; (SIDE * SIDE) as usize],
        }
    }

    fn at(&self, x: i32, y: i32) -> Option<Color> {
        self.pixels[(y * SIDE + x) as usize]
    }

    fn painted(&self) -> usize {
        self.pixels.iter().filter(|pixel| pixel.is_some()).count()
    }
}

impl Renderer for Canvas {
    fn fill_rect(&mut self, rect: Rect, color: Color) {
        for y in rect.y.max(0)..rect.bottom().min(SIDE) {
            for x in rect.x.max(0)..rect.right().min(SIDE) {
                self.pixels[(y * SIDE + x) as usize] = Some(color);
            }
        }
    }

    fn draw_text(&mut self, _position: (i32, i32), _text: &str, _color: Color) {}
}

fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
    Rect { x, y, width, height }
}

fn style(background: Color, radius: u8) -> Style {
    Style {
        bg_color: background,
        border_color: TRACK,
        radius,
        ..Style::default()
    }
}

// ============================================================================
// Arc
// ============================================================================

/// An arc round the whole of a 60-pixel square, from twelve o'clock clockwise, 10 pixels thick.
fn arc_look(value: i32) -> ArcLook {
    ArcLook {
        minimum: 0,
        maximum: 100,
        value,
        start_angle: 0,
        end_angle: 360,
        rotation: -90,
        stroke_width: 10,
        track_color: TRACK,
        indicator_color: ACCENT,
    }
}

#[test]
fn an_arc_fills_its_track_from_the_start_by_the_value() {
    let mut canvas = Canvas::new();

    draw_arc(&mut canvas, rect(0, 0, SIDE, SIDE), &arc_look(25));

    assert_eq!(canvas.at(36, 4), Some(ACCENT));
    assert_eq!(canvas.at(55, 36), Some(TRACK));
    assert_eq!(canvas.at(30, 30), None);
}

#[test]
fn an_arc_full_or_empty_draws_all_or_none_of_its_indicator() {
    let mut full = Canvas::new();
    let mut empty = Canvas::new();

    draw_arc(&mut full, rect(0, 0, SIDE, SIDE), &arc_look(150));
    draw_arc(&mut empty, rect(0, 0, SIDE, SIDE), &arc_look(-5));

    assert_eq!(full.at(4, 36), Some(ACCENT));
    assert_eq!(empty.at(36, 4), Some(TRACK));
}

#[test]
fn an_arc_with_a_gap_fills_across_the_wrap_and_reversed_ranges_count_down() {
    let mut wrapped = Canvas::new();
    let mut reversed = Canvas::new();
    let gap = ArcLook {
        start_angle: 225,
        end_angle: 135,
        rotation: 270,
        ..arc_look(50)
    };

    draw_arc(&mut wrapped, rect(0, 0, SIDE, SIDE), &gap);
    draw_arc(
        &mut reversed,
        rect(0, 0, SIDE, SIDE),
        &ArcLook {
            minimum: 100,
            maximum: 0,
            value: 75,
            ..arc_look(0)
        },
    );

    assert_eq!(wrapped.at(20, 6), Some(ACCENT));
    assert_eq!(wrapped.at(40, 6), Some(TRACK));
    assert_eq!(wrapped.at(30, 56), None);
    assert_eq!(reversed.at(36, 4), Some(ACCENT));
}

#[test]
fn an_arc_whose_start_and_end_meet_runs_the_whole_way_round() {
    let mut canvas = Canvas::new();
    let meeting = ArcLook {
        start_angle: 0,
        end_angle: 0,
        ..arc_look(25)
    };

    draw_arc(&mut canvas, rect(0, 0, SIDE, SIDE), &meeting);

    assert_eq!(canvas.at(36, 4), Some(ACCENT));
    assert_eq!(canvas.at(55, 36), Some(TRACK));
    assert_eq!(canvas.at(4, 24), Some(TRACK));
}

#[test]
fn an_arc_with_no_room_or_no_thickness_draws_nothing() {
    let mut canvas = Canvas::new();

    draw_arc(&mut canvas, rect(0, 0, 0, 10), &arc_look(50));
    draw_arc(
        &mut canvas,
        rect(0, 0, SIDE, SIDE),
        &ArcLook {
            stroke_width: 0,
            ..arc_look(50)
        },
    );
    draw_arc(
        &mut canvas,
        rect(0, 0, SIDE, SIDE),
        &ArcLook {
            minimum: 5,
            maximum: 5,
            track_color: Color(0, 0, 0, 0),
            ..arc_look(5)
        },
    );

    assert_eq!(canvas.painted(), 0);
}

// ============================================================================
// Bar
// ============================================================================

#[test]
fn a_wide_bar_fills_from_the_left_and_a_tall_one_from_the_bottom() {
    let mut wide = Canvas::new();
    let mut tall = Canvas::new();

    draw_bar(
        &mut wide,
        rect(0, 0, 40, 10),
        &style(TRACK, 0),
        (0, 100, 50),
        ACCENT,
    );
    draw_bar(
        &mut tall,
        rect(0, 0, 10, 40),
        &style(TRACK, 0),
        (0, 100, 25),
        ACCENT,
    );

    assert_eq!(wide.at(19, 5), Some(ACCENT));
    assert_eq!(wide.at(20, 5), Some(TRACK));
    assert_eq!(tall.at(5, 39), Some(ACCENT));
    assert_eq!(tall.at(5, 29), Some(TRACK));
}

#[test]
fn a_bar_with_no_range_or_a_clear_indicator_shows_only_its_box() {
    let mut no_range = Canvas::new();
    let mut clear = Canvas::new();
    let mut reversed = Canvas::new();

    draw_bar(
        &mut no_range,
        rect(0, 0, 40, 10),
        &style(TRACK, 0),
        (5, 5, 5),
        ACCENT,
    );
    draw_bar(
        &mut clear,
        rect(0, 0, 40, 10),
        &style(TRACK, 0),
        (0, 10, 5),
        Color(0, 0, 0, 0),
    );
    draw_bar(
        &mut reversed,
        rect(0, 0, 40, 10),
        &style(TRACK, 0),
        (10, 0, 0),
        ACCENT,
    );

    assert!(no_range.pixels.iter().flatten().all(|&color| color == TRACK));
    assert!(clear.pixels.iter().flatten().all(|&color| color == TRACK));
    assert_eq!(reversed.at(39, 5), Some(ACCENT));
}

// ============================================================================
// LED, switch and slider
// ============================================================================

#[test]
fn an_led_is_a_round_light_dimmed_but_never_dark() {
    let mut bright = Canvas::new();
    let mut dim = Canvas::new();
    let mut clear = Canvas::new();

    draw_led(&mut bright, rect(0, 0, 20, 20), ACCENT, 255);
    draw_led(&mut dim, rect(0, 0, 20, 20), ACCENT, 0);
    draw_led(&mut clear, rect(0, 0, 20, 20), Color(0, 0, 0, 0), 255);

    assert_eq!(bright.at(10, 10), Some(ACCENT));
    assert_eq!(bright.at(0, 0), None);
    assert_eq!(dim.at(10, 10), Some(Color(0, 62, 0, 255)));
    assert_eq!(clear.at(10, 10), Some(Color(255, 255, 255, 255)));
}

#[test]
fn a_switch_puts_its_knob_left_when_off_and_right_when_on() {
    let mut off = Canvas::new();
    let mut on = Canvas::new();

    draw_switch(&mut off, rect(0, 0, 40, 20), &style(TRACK, 0), ACCENT, false);
    draw_switch(&mut on, rect(0, 0, 40, 20), &style(TRACK, 0), ACCENT, true);

    assert_eq!(off.at(5, 10), Some(ACCENT));
    assert_eq!(off.at(35, 10), Some(TRACK));
    assert_eq!(on.at(35, 10), Some(ACCENT));
}

#[test]
fn a_slider_draws_its_track_and_a_knob_at_the_value() {
    let mut square = Canvas::new();
    let mut round = Canvas::new();
    let mut no_range = Canvas::new();
    let background = Color(90, 90, 90, 255);

    draw_slider(
        &mut square,
        rect(0, 0, 50, 20),
        &style(background, 0),
        (0, 100, 50),
        ACCENT,
    );
    draw_slider(
        &mut round,
        rect(0, 0, 50, 20),
        &style(background, 10),
        (0, 100, 50),
        ACCENT,
    );
    draw_slider(
        &mut no_range,
        rect(0, 0, 50, 20),
        &style(background, 0),
        (3, 3, 3),
        ACCENT,
    );

    assert_eq!(square.at(25, 10), Some(ACCENT));
    assert_eq!(square.at(5, 10), Some(TRACK));
    assert_eq!(square.at(5, 2), Some(background));
    assert_eq!(round.at(25, 10), Some(ACCENT));
    assert_eq!(no_range.at(1, 10), Some(ACCENT));
}

#[test]
fn the_sine_table_matches_the_sine_of_every_whole_degree_on_any_turn() {
    for degrees in -720..=720 {
        let exact = (degrees as f64).to_radians().sin();
        let (cosine, sine) = angle_vector(degrees);
        assert!((sine as f64 - exact).abs() < 1e-7, "sine of {degrees}");
        let exact_cosine = (degrees as f64).to_radians().cos();
        assert!((cosine as f64 - exact_cosine).abs() < 1e-7, "cosine of {degrees}");
    }
}
