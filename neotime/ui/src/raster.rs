//! Works out how much of each pixel a shape covers, a row at a time, for soft edges.
//!
//! A pixel's coverage comes from the distance between its centre and the shape's nearest edge,
//! over a ramp one pixel wide: a centre right on the edge is half covered. Only the rows and
//! columns inside the clip are visited. The kernels follow rlvgl's (MIT licensed).

use crate::geometry::{PointF, Rect};

// ============================================================================
// Where coverage goes
// ============================================================================

/// Takes one row of coverage at a time, 0 for none and 255 for all of a pixel.
pub trait CoverageSink {
    fn row(&mut self, x: i32, y: i32, coverage: &[u8]);
}

/// The longest run of coverage handed over at once; a longer run goes as several.
const LONGEST_RUN: usize = 256;

/// Collects one row's coverage and hands it on in runs of covered pixels.
struct RowRun<'sink> {
    sink: &'sink mut dyn CoverageSink,
    coverage: [u8; LONGEST_RUN],
    start_x: i32,
    length: usize,
}

impl<'sink> RowRun<'sink> {
    fn new(sink: &'sink mut dyn CoverageSink) -> Self {
        Self {
            sink,
            coverage: [0; LONGEST_RUN],
            start_x: 0,
            length: 0,
        }
    }

    fn push(&mut self, x: i32, y: i32, amount: u8) {
        if self.length == 0 {
            self.start_x = x;
        } else if self.length == LONGEST_RUN {
            self.flush(y);
            self.start_x = x;
        }
        self.coverage[self.length] = amount;
        self.length += 1;
    }

    fn flush(&mut self, y: i32) {
        if self.length > 0 {
            self.sink.row(self.start_x, y, &self.coverage[..self.length]);
            self.length = 0;
        }
    }
}

/// Coverage from the distance into a shape plus half a pixel.
#[inline]
fn coverage_of(distance_plus_half: f32) -> u8 {
    if distance_plus_half <= 0.0 {
        0
    } else if distance_plus_half >= 1.0 {
        255
    } else {
        (distance_plus_half * 255.0 + 0.5) as u8
    }
}

/// The columns and rows from `left`, `top` up to `right`, `bottom` that are inside `clip`.
fn clipped(left: i32, top: i32, right: i32, bottom: i32, clip: Rect) -> Option<Rect> {
    let area = Rect {
        x: left.max(clip.x),
        y: top.max(clip.y),
        width: right.min(clip.right()) - left.max(clip.x),
        height: bottom.min(clip.bottom()) - top.max(clip.y),
    };
    (!area.is_empty()).then_some(area)
}

// ============================================================================
// Turned rectangles and lines
// ============================================================================

/// A rectangle turned about its centre, given by the cosine and sine of the turn.
#[derive(Copy, Clone, Debug)]
struct TurnedBox {
    centre: PointF,
    length: f32,
    width: f32,
    cos_turn: f32,
    sin_turn: f32,
}

impl TurnedBox {
    /// The upright rectangle round it, one pixel larger on each side for the soft edge.
    fn bounds(&self) -> (i32, i32, i32, i32) {
        let half_length = self.length * 0.5;
        let half_width = self.width * 0.5;
        let half_across = self.cos_turn.abs() * half_length + self.sin_turn.abs() * half_width + 1.0;
        let half_down = self.sin_turn.abs() * half_length + self.cos_turn.abs() * half_width + 1.0;
        (
            floor(self.centre.x - half_across) as i32,
            floor(self.centre.y - half_down) as i32,
            ceil(self.centre.x + half_across) as i32,
            ceil(self.centre.y + half_down) as i32,
        )
    }
}

fn rasterize_turned_box(shape: &TurnedBox, clip: Rect, sink: &mut dyn CoverageSink) {
    let (left, top, right, bottom) = shape.bounds();
    let Some(area) = clipped(left, top, right, bottom, clip) else {
        return;
    };
    let half_length = shape.length * 0.5;
    let half_width = shape.width * 0.5;
    let mut run = RowRun::new(sink);

    for y in area.y..area.bottom() {
        let down = y as f32 + 0.5 - shape.centre.y;
        for x in area.x..area.right() {
            let across = x as f32 + 0.5 - shape.centre.x;
            let along_length = across * shape.cos_turn + down * shape.sin_turn;
            let along_width = -across * shape.sin_turn + down * shape.cos_turn;
            let inside_length = half_length - along_length.abs();
            let inside_width = half_width - along_width.abs();
            let inside = if inside_length < inside_width {
                inside_length
            } else {
                inside_width
            };
            let amount = coverage_of(inside + 0.5);
            if amount > 0 {
                run.push(x, y, amount);
            } else if run.length > 0 {
                // The box is convex, so nothing more on this row is covered.
                break;
            }
        }
        run.flush(y);
    }
}

/// A line from `from` to `to`, `width` pixels wide, with square ends.
pub fn rasterize_line(from: PointF, to: PointF, width: f32, clip: Rect, sink: &mut dyn CoverageSink) {
    let across = to.x - from.x;
    let down = to.y - from.y;
    let length_squared = across * across + down * down;
    if length_squared <= 0.0 || width <= 0.0 {
        return;
    }
    let length = square_root(length_squared);
    let shape = TurnedBox {
        centre: PointF::new(0.5 * (from.x + to.x), 0.5 * (from.y + to.y)),
        length,
        width,
        cos_turn: across / length,
        sin_turn: down / length,
    };
    rasterize_turned_box(&shape, clip, sink);
}

// ============================================================================
// Circles
// ============================================================================

/// The distance from the centre, from its square, by two steps of Heron's method from `guess`.
#[inline]
fn distance_near(guess: f32, distance_squared: f32) -> f32 {
    let mut distance = guess;
    distance = 0.5 * (distance + distance_squared / distance);
    0.5 * (distance + distance_squared / distance)
}

/// A filled circle.
pub fn rasterize_disc(centre: PointF, radius: f32, clip: Rect, sink: &mut dyn CoverageSink) {
    if radius <= 0.0 {
        return;
    }
    let reach = radius + 1.0;
    let Some(area) = clipped(
        floor(centre.x - reach) as i32,
        floor(centre.y - reach) as i32,
        ceil(centre.x + reach) as i32,
        ceil(centre.y + reach) as i32,
        clip,
    ) else {
        return;
    };
    let inner = if radius > 1.0 { radius - 1.0 } else { 0.0 };
    let inner_squared = inner * inner;
    let outer = radius + 1.0;
    let outer_squared = outer * outer;
    let mut run = RowRun::new(sink);

    for y in area.y..area.bottom() {
        let down = y as f32 + 0.5 - centre.y;
        let down_squared = down * down;
        for x in area.x..area.right() {
            let across = x as f32 + 0.5 - centre.x;
            let distance_squared = across * across + down_squared;
            let amount = if distance_squared >= outer_squared {
                0
            } else if distance_squared <= inner_squared {
                255
            } else {
                coverage_of(radius - distance_near(radius, distance_squared) + 0.5)
            };
            if amount > 0 {
                run.push(x, y, amount);
            } else if run.length > 0 {
                break;
            }
        }
        run.flush(y);
    }
}

/// How much of a pixel at `distance_squared` from the centre lies inside a ring's two edges.
fn ring_coverage(distance_squared: f32, outer: &Edge, inner: Option<&Edge>) -> u8 {
    let inner_amount = |inner: &Edge| {
        if distance_squared >= inner.outside_squared {
            255
        } else if distance_squared <= inner.inside_squared {
            0
        } else {
            coverage_of(distance_near(inner.radius, distance_squared) - inner.radius + 0.5)
        }
    };
    if distance_squared >= outer.outside_squared {
        0
    } else if distance_squared <= outer.inside_squared {
        inner.map_or(255, inner_amount)
    } else {
        let outer_amount = coverage_of(outer.radius - distance_near(outer.radius, distance_squared) + 0.5);
        match inner {
            None => outer_amount,
            Some(inner) => outer_amount.min(inner_amount(inner)),
        }
    }
}

/// A circular edge and the squares of the distances where its soft band starts and ends.
struct Edge {
    radius: f32,
    inside_squared: f32,
    outside_squared: f32,
}

impl Edge {
    fn new(radius: f32) -> Edge {
        let inside = if radius > 1.0 { radius - 1.0 } else { 0.0 };
        let outside = radius + 1.0;
        Edge {
            radius,
            inside_squared: inside * inside,
            outside_squared: outside * outside,
        }
    }
}

/// Part of a ring between two rays, or a pie slice when the inner radius is 0.
///
/// The rays are given by their cosines and sines, and `extent` is the signed angle between them
/// in radians, positive turning from the start ray towards the end ray.
#[allow(clippy::too_many_arguments)]
pub fn rasterize_arc(
    centre: PointF,
    outer_radius: f32,
    inner_radius: f32,
    start_cos: f32,
    start_sin: f32,
    end_cos: f32,
    end_sin: f32,
    extent: f32,
    clip: Rect,
    sink: &mut dyn CoverageSink,
) {
    if outer_radius <= 0.0 || outer_radius <= inner_radius {
        return;
    }
    let reach = outer_radius + 1.0;
    let Some(area) = clipped(
        floor(centre.x - reach) as i32,
        floor(centre.y - reach) as i32,
        ceil(centre.x + reach) as i32,
        ceil(centre.y + reach) as i32,
        clip,
    ) else {
        return;
    };
    let outer = Edge::new(outer_radius);
    let inner = (inner_radius > 0.0).then(|| Edge::new(inner_radius));
    let sweep = extent.abs();
    let is_full_circle = sweep >= core::f32::consts::TAU - 1e-3;
    let is_more_than_half = sweep > core::f32::consts::PI;
    // A clockwise arc is the anticlockwise one with its rays swapped.
    let (first_cos, first_sin, last_cos, last_sin) = if extent < 0.0 {
        (end_cos, end_sin, start_cos, start_sin)
    } else {
        (start_cos, start_sin, end_cos, end_sin)
    };
    let mut run = RowRun::new(sink);

    for y in area.y..area.bottom() {
        let down = y as f32 + 0.5 - centre.y;
        let down_squared = down * down;
        for x in area.x..area.right() {
            let across = x as f32 + 0.5 - centre.x;
            let radial = ring_coverage(across * across + down_squared, &outer, inner.as_ref());
            let angular = if is_full_circle {
                255
            } else {
                let past_first = coverage_of(first_cos * down - first_sin * across + 0.5);
                let before_last = coverage_of(-(last_cos * down - last_sin * across) + 0.5);
                if is_more_than_half {
                    past_first.max(before_last)
                } else {
                    past_first.min(before_last)
                }
            };
            let amount = radial.min(angular);
            if amount > 0 {
                run.push(x, y, amount);
            } else {
                // A thin ring can cover two separate runs of one row.
                run.flush(y);
            }
        }
        run.flush(y);
    }
}

// ============================================================================
// Arithmetic without a maths library
// ============================================================================

#[inline]
fn square_root(value: f32) -> f32 {
    if value <= 0.0 {
        return 0.0;
    }
    let mut root = f32::from_bits((value.to_bits() + (127 << 23)) >> 1);
    root = 0.5 * (root + value / root);
    0.5 * (root + value / root)
}

#[inline]
fn floor(value: f32) -> f32 {
    let whole = value as i32 as f32;
    if value < whole {
        whole - 1.0
    } else {
        whole
    }
}

#[inline]
fn ceil(value: f32) -> f32 {
    let whole = value as i32 as f32;
    if value > whole {
        whole + 1.0
    } else {
        whole
    }
}
