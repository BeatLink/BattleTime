//! Rounded rectangles and borders built from plain fills, with soft corners.

use crate::geometry::{scale, Color, Rect};
use crate::renderer::Renderer;

// ============================================================================
// Frame style
// ============================================================================

/// How a widget's box looks: its fill, its border and how round its corners are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    pub bg_color: Color,
    pub border_color: Color,
    pub border_width: u8,
    /// Scales the opacity of every colour the widget draws.
    pub alpha: u8,
    pub radius: u8,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            bg_color: Color(255, 255, 255, 255),
            border_color: Color(0, 0, 0, 255),
            border_width: 0,
            alpha: 255,
            radius: 0,
        }
    }
}

/// Draws a widget's fill and then its border; a fully clear part is skipped.
pub fn draw_widget_bg(renderer: &mut dyn Renderer, rect: Rect, style: &Style) {
    let fill = style.bg_color.with_alpha(style.alpha);
    if fill.3 != 0 {
        if style.radius > 0 {
            fill_rounded_rect(renderer, rect, fill, style.radius);
        } else if fill.3 == 255 {
            renderer.fill_rect(rect, fill);
        } else {
            renderer.blend_rect(rect, fill);
        }
    }
    if style.border_width > 0 {
        let border = style.border_color.with_alpha(style.alpha);
        if border.3 != 0 {
            draw_rounded_border(renderer, rect, border, style.border_width, style.radius);
        }
    }
}

// ============================================================================
// Corner arcs
// ============================================================================

fn integer_square_root(value: u32) -> u32 {
    if value == 0 {
        return 0;
    }
    let mut root = value;
    let mut next = value.div_ceil(2);
    while next < root {
        root = next;
        next = (root + value / root) / 2;
    }
    root
}

/// How far across a quarter circle reaches on the row `rows_from_axis` from its centre line.
///
/// Gives whole pixels and the opacity of the pixel just past them, in quarter-pixel steps.
fn arc_reach(radius: i32, rows_from_axis: i32) -> (i32, u8) {
    let radius_in_quarters = radius as u32 * 4;
    let row_in_quarters = rows_from_axis as u32 * 4 + 2;
    let radius_squared = radius_in_quarters * radius_in_quarters;
    let row_squared = row_in_quarters * row_in_quarters;
    if row_squared >= radius_squared {
        return (0, 0);
    }
    let reach_in_quarters = integer_square_root(radius_squared - row_squared);
    ((reach_in_quarters / 4) as i32, (reach_in_quarters % 4) as u8 * 64)
}

/// One pixel at `x`, `y`.
fn dot(x: i32, y: i32) -> Rect {
    Rect {
        x,
        y,
        width: 1,
        height: 1,
    }
}

/// `width` pixels of one row from `x`.
fn row_span(x: i32, y: i32, width: i32) -> Rect {
    Rect {
        x,
        y,
        width,
        height: 1,
    }
}

// ============================================================================
// Filled rounded rectangles
// ============================================================================

/// Fills `rect` with corners of `radius`, cut to half the shorter side so pills work.
pub fn fill_rounded_rect(renderer: &mut dyn Renderer, rect: Rect, color: Color, radius: u8) {
    let radius = (radius as i32).min(rect.width / 2).min(rect.height / 2);
    if radius <= 0 {
        renderer.fill_rect(rect, color);
        return;
    }
    let last_row = rect.y + rect.height - 1;
    let right_corner = rect.x + rect.width - radius;

    if rect.height - 2 * radius > 0 {
        let body = Rect {
            y: rect.y + radius,
            height: rect.height - 2 * radius,
            ..rect
        };
        renderer.fill_rect(body, color);
    }
    if rect.width - 2 * radius > 0 {
        let between = |y| Rect {
            x: rect.x + radius,
            y,
            width: rect.width - 2 * radius,
            height: radius,
        };
        renderer.fill_rect(between(rect.y), color);
        renderer.fill_rect(between(rect.y + rect.height - radius), color);
    }

    for row in 0..radius {
        let (reach, edge_opacity) = arc_reach(radius, radius - 1 - row);
        let (top, bottom) = (rect.y + row, last_row - row);
        if reach > 0 {
            for y in [top, bottom] {
                renderer.fill_rect(row_span(rect.x + radius - reach, y, reach), color);
                renderer.fill_rect(row_span(right_corner, y, reach), color);
            }
        }
        if edge_opacity > 0 {
            let edge = Color(color.0, color.1, color.2, scale(edge_opacity, color.3));
            for y in [top, bottom] {
                renderer.blend_rect(dot(rect.x + radius - reach - 1, y), edge);
                renderer.blend_rect(dot(right_corner + reach, y), edge);
            }
        }
    }
}

// ============================================================================
// Borders
// ============================================================================

/// Draws a border `border_width` thick that follows corners of `radius`.
pub fn draw_rounded_border(
    renderer: &mut dyn Renderer,
    rect: Rect,
    color: Color,
    border_width: u8,
    radius: u8,
) {
    let thickness = border_width as i32;
    if thickness == 0 {
        return;
    }
    let outer_radius = (radius as i32).min(rect.width / 2).min(rect.height / 2);
    if outer_radius <= 0 {
        draw_border_straight(renderer, rect, color, border_width);
        return;
    }
    let inner_radius = (outer_radius - thickness).max(0);
    let last_row = rect.y + rect.height - 1;
    let right_corner = rect.x + rect.width - outer_radius;

    for row in 0..outer_radius {
        let rows_from_axis = outer_radius - 1 - row;
        let (outer_reach, outer_opacity) = arc_reach(outer_radius, rows_from_axis);
        let (inner_reach, inner_opacity) = match inner_radius > 0 {
            true => arc_reach(inner_radius, rows_from_axis),
            false => (0, 0),
        };
        let (top, bottom) = (rect.y + row, last_row - row);

        let ring_width = outer_reach - inner_reach;
        if ring_width > 0 {
            for y in [top, bottom] {
                renderer.fill_rect(
                    row_span(rect.x + outer_radius - outer_reach, y, ring_width),
                    color,
                );
                renderer.fill_rect(row_span(right_corner + inner_reach, y, ring_width), color);
            }
        }
        if outer_opacity > 0 {
            let edge = Color(color.0, color.1, color.2, scale(outer_opacity, color.3));
            for y in [top, bottom] {
                renderer.blend_rect(dot(rect.x + outer_radius - outer_reach - 1, y), edge);
                renderer.blend_rect(dot(right_corner + outer_reach, y), edge);
            }
        }
        if inner_reach > 0 && inner_opacity > 0 {
            let edge = Color(color.0, color.1, color.2, scale(255 - inner_opacity, color.3));
            for y in [top, bottom] {
                renderer.blend_rect(dot(rect.x + outer_radius - inner_reach, y), edge);
                renderer.blend_rect(dot(right_corner + inner_reach - 1, y), edge);
            }
        }
    }

    let straight_height = rect.height - 2 * outer_radius;
    if straight_height > 0 {
        let side = |x| Rect {
            x,
            y: rect.y + outer_radius,
            width: thickness,
            height: straight_height,
        };
        renderer.fill_rect(side(rect.x), color);
        renderer.fill_rect(side(rect.x + rect.width - thickness), color);
    }
    let straight_width = rect.width - 2 * outer_radius;
    if straight_width > 0 {
        let edge = |y| Rect {
            x: rect.x + outer_radius,
            y,
            width: straight_width,
            height: thickness,
        };
        renderer.fill_rect(edge(rect.y), color);
        renderer.fill_rect(edge(rect.y + rect.height - thickness), color);
    }
}

/// Draws a square-cornered border as four fills.
pub fn draw_border_straight(renderer: &mut dyn Renderer, rect: Rect, color: Color, border_width: u8) {
    let thickness = border_width as i32;
    if thickness == 0 {
        return;
    }
    let across = |y| Rect {
        height: thickness,
        y,
        ..rect
    };
    let down = |x| Rect {
        x,
        y: rect.y + thickness,
        width: thickness,
        height: rect.height - 2 * thickness,
    };
    renderer.fill_rect(across(rect.y), color);
    renderer.fill_rect(across(rect.y + rect.height - thickness), color);
    renderer.fill_rect(down(rect.x), color);
    renderer.fill_rect(down(rect.x + rect.width - thickness), color);
}
