//! The value widgets drawn straight from a node's values: arcs, bars, LEDs, switches and sliders.
//!
//! Each is a function of what it shows, so nothing is built or kept between draws.

use libm::{cosf, sinf};
use neotime_ui::{
    geometry::{Color, PointF, Rect},
    renderer::Renderer,
    shapes::{draw_widget_bg, fill_rounded_rect, Style},
};

// ============================================================================
// Ranges
// ============================================================================

/// How far `value` is from `minimum` towards `maximum`, as a fraction kept whole, and clamped.
fn range_fraction(value: i32, minimum: i32, maximum: i32) -> (i64, i64) {
    let span = maximum as i64 - minimum as i64;
    if span > 0 {
        ((value as i64 - minimum as i64).clamp(0, span), span)
    } else if span < 0 {
        ((minimum as i64 - value as i64).clamp(0, -span), -span)
    } else {
        (0, 1)
    }
}

fn clamp_to_range(value: i32, minimum: i32, maximum: i32) -> i32 {
    value.clamp(minimum.min(maximum), minimum.max(maximum))
}

// ============================================================================
// Arc
// ============================================================================

/// What an arc shows: a track between two angles, and how much of it the value fills.
pub struct ArcLook {
    pub minimum: i32,
    pub maximum: i32,
    pub value: i32,
    /// Where the track starts and ends, in degrees clockwise from three o'clock before `rotation`.
    pub start_angle: i32,
    pub end_angle: i32,
    pub rotation: i32,
    pub stroke_width: u8,
    pub track_color: Color,
    pub indicator_color: Color,
}

fn turn_of(degrees: i32) -> i32 {
    degrees.rem_euclid(360)
}

/// An angle as kept: within one turn, but a whole turn stays 360.
fn stored_angle(degrees: i32) -> i32 {
    match degrees {
        360 => 360,
        other => turn_of(other),
    }
}

/// How many degrees clockwise from `start` to `end`, a whole turn kept as 360.
fn sweep_in_degrees(start: i32, end: i32) -> i32 {
    let difference = end - start;
    match (0..=360).contains(&difference) {
        true => difference,
        false => difference.rem_euclid(360),
    }
}

fn angle_vector(degrees: i32) -> (f32, f32) {
    let radians = turn_of(degrees) as f32 * core::f32::consts::PI / 180.0;
    (cosf(radians), sinf(radians))
}

/// Draws the track, then the part the value fills from the start.
pub fn draw_arc(renderer: &mut dyn Renderer, bounds: Rect, look: &ArcLook) {
    let track_start = stored_angle(look.start_angle);
    let track_end = stored_angle(look.end_angle);
    let rotation = turn_of(look.rotation);
    let value = clamp_to_range(look.value, look.minimum, look.maximum);

    let (filled, whole) = range_fraction(value, look.minimum, look.maximum);
    let value_angle = if filled == 0 {
        track_start
    } else if filled == whole {
        track_end
    } else {
        let unwrapped_end = match track_end < track_start {
            true => track_end as i64 + 360,
            false => track_end as i64,
        };
        let span = unwrapped_end - track_start as i64;
        (track_start as i64 + span * filled / whole).rem_euclid(360) as i32
    };

    let ring = ArcRing::of(bounds, look.stroke_width);
    ring_part(
        renderer,
        ring,
        track_start + rotation,
        track_end + rotation,
        look.track_color,
    );
    ring_part(
        renderer,
        ring,
        track_start + rotation,
        value_angle + rotation,
        look.indicator_color,
    );
}

/// The ring an arc is drawn on: its centre and its outer and inner radii.
#[derive(Clone, Copy)]
struct ArcRing {
    centre: PointF,
    outer_radius: f32,
    inner_radius: f32,
}

impl ArcRing {
    fn of(bounds: Rect, stroke_width: u8) -> Option<ArcRing> {
        if bounds.width <= 0 || bounds.height <= 0 || stroke_width == 0 {
            return None;
        }
        let outer_radius = bounds.width.min(bounds.height) as f32 * 0.5;
        let thickness = (stroke_width as f32).min(outer_radius);
        Some(ArcRing {
            centre: PointF::new(
                bounds.x as f32 + bounds.width as f32 * 0.5,
                bounds.y as f32 + bounds.height as f32 * 0.5,
            ),
            outer_radius,
            inner_radius: outer_radius - thickness,
        })
    }
}

fn ring_part(renderer: &mut dyn Renderer, ring: Option<ArcRing>, start: i32, end: i32, color: Color) {
    let Some(ring) = ring else {
        return;
    };
    let sweep = sweep_in_degrees(start, end);
    if sweep == 0 || color.3 == 0 {
        return;
    }
    let (start_cos, start_sin) = angle_vector(start);
    let (end_cos, end_sin) = angle_vector(end);
    let extent_in_radians = sweep as f32 * core::f32::consts::PI / 180.0;
    renderer.fill_arc_aa(
        ring.centre,
        ring.outer_radius,
        ring.inner_radius,
        start_cos,
        start_sin,
        end_cos,
        end_sin,
        extent_in_radians,
        color,
    );
}

// ============================================================================
// Bar
// ============================================================================

/// Draws the box, then the part the value fills: from the left when wide, from the bottom when tall.
pub fn draw_bar(
    renderer: &mut dyn Renderer,
    bounds: Rect,
    style: &Style,
    (minimum, maximum, value): (i32, i32, i32),
    indicator_color: Color,
) {
    draw_widget_bg(renderer, bounds, style);

    let color = indicator_color.with_alpha(style.alpha);
    if color.3 == 0 || bounds.width <= 0 || bounds.height <= 0 || minimum == maximum {
        return;
    }
    let value = clamp_to_range(value, minimum, maximum);
    let filled_length = |length: i32| {
        let (filled, whole) = range_fraction(value, minimum, maximum);
        (filled * i64::from(length) / whole) as i32
    };
    let indicator = match bounds.width >= bounds.height {
        true => Rect {
            width: filled_length(bounds.width),
            ..bounds
        },
        false => {
            let height = filled_length(bounds.height);
            Rect {
                y: bounds.y + bounds.height - height,
                height,
                ..bounds
            }
        }
    };
    if !indicator.is_empty() {
        fill_rounded_rect(renderer, indicator, color, style.radius);
    }
}

// ============================================================================
// LED
// ============================================================================

/// How dim an LED gets at its lowest brightness, out of 255.
const LED_LOWEST_BRIGHTNESS: u8 = 80;

/// Draws a round light on a white disc, dimmed by `brightness`.
pub fn draw_led(renderer: &mut dyn Renderer, bounds: Rect, color: Color, brightness: u8) {
    let style = Style {
        radius: u8::MAX,
        ..Style::default()
    };
    draw_widget_bg(renderer, bounds, &style);

    if bounds.is_empty() || color.3 == 0 {
        return;
    }
    let brightness = u16::from(brightness.max(LED_LOWEST_BRIGHTNESS));
    let dim = |channel: u8| (u16::from(channel) * brightness / 255) as u8;
    let lit = Color(dim(color.0), dim(color.1), dim(color.2), color.3);
    fill_rounded_rect(renderer, bounds, lit, style.radius);
}

// ============================================================================
// Switch
// ============================================================================

/// Draws the track, then a knob over its left half when off and its right half when on.
pub fn draw_switch(renderer: &mut dyn Renderer, bounds: Rect, style: &Style, knob_color: Color, is_on: bool) {
    draw_widget_bg(renderer, bounds, style);

    let knob_width = bounds.width / 2;
    let knob = Rect {
        x: match is_on {
            true => bounds.x + bounds.width - knob_width,
            false => bounds.x,
        },
        width: knob_width,
        ..bounds
    };
    fill_rounded_rect(renderer, knob, knob_color.with_alpha(style.alpha), style.radius);
}

// ============================================================================
// Slider
// ============================================================================

const SLIDER_TRACK_HEIGHT: i32 = 4;
const SLIDER_KNOB_SIZE: i32 = 10;

/// Draws the box, a thin track across its middle in the border colour, and a knob at the value.
pub fn draw_slider(
    renderer: &mut dyn Renderer,
    bounds: Rect,
    style: &Style,
    (minimum, maximum, value): (i32, i32, i32),
    knob_color: Color,
) {
    draw_widget_bg(renderer, bounds, style);
    let is_round = style.radius > 0;

    let track = Rect {
        y: bounds.y + (bounds.height - SLIDER_TRACK_HEIGHT) / 2,
        height: SLIDER_TRACK_HEIGHT,
        ..bounds
    };
    let track_radius = if is_round {
        (SLIDER_TRACK_HEIGHT / 2) as u8
    } else {
        0
    };
    fill_rounded_rect(
        renderer,
        track,
        style.border_color.with_alpha(style.alpha),
        track_radius,
    );

    let value = clamp_to_range(value, minimum, maximum);
    let knob_x = match maximum - minimum {
        0 => bounds.x,
        range => bounds.x + ((value - minimum) as f32 / range as f32 * bounds.width as f32) as i32,
    };
    let knob = Rect {
        x: knob_x - SLIDER_KNOB_SIZE / 2,
        y: bounds.y + (bounds.height - SLIDER_KNOB_SIZE) / 2,
        width: SLIDER_KNOB_SIZE,
        height: SLIDER_KNOB_SIZE,
    };
    let knob_radius = if is_round { (SLIDER_KNOB_SIZE / 2) as u8 } else { 0 };
    fill_rounded_rect(renderer, knob, knob_color.with_alpha(style.alpha), knob_radius);
}

#[cfg(test)]
#[path = "widgets_tests.rs"]
mod widgets_tests;
