//! Places every node of the open screen, in the spirit of a small flexbox.
//!
//! A `Column` stacks its children down and a `Row` across, `gap` apart; spare space goes to children
//! with `grow`, in proportion, or else `justify` places them. Across the flow a child fills the space
//! unless it has a size, an `align`, or its parent an `align_items`. A `Stack` and a `Button` put
//! their children on top of each other, and a `Grid` in rows of `columns` cells. A `Scroll` and a
//! `List` are columns that clip their children and can be scrolled. A child with `x` or `y` leaves
//! the flow and sits at that offset inside its parent, and so do lines, scales and analog clocks,
//! which are drawn over the whole of their parent unless given a size.

use alloc::boxed::Box;
use neotime_runtime::WidgetKind;
use rlvgl_core::widget::Rect;

use crate::fonts::{line_height, text_width};
use crate::nodes::{inset, Align, Length, LongMode, Node, Placement};
use crate::text::{wrapped_line_count, wrapped_width};

// ============================================================================
// Sizes the widgets want
// ============================================================================

const BUTTON_SIDE_SPACE: i32 = 16;
const BUTTON_SMALLEST_HEIGHT: i32 = 44;
const ARC_SIZE: i32 = 100;
const BAR_WIDTH: i32 = 120;
const BAR_HEIGHT: i32 = 10;
const LED_SIZE: i32 = 16;
const SWITCH_WIDTH: i32 = 52;
const SWITCH_HEIGHT: i32 = 28;
const SLIDER_WIDTH: i32 = 120;
const SLIDER_HEIGHT: i32 = 16;
const CHART_WIDTH: i32 = 200;
const CHART_HEIGHT: i32 = 100;
pub const TICK_BOX_SIZE: i32 = 24;
pub const TICK_BOX_GAP: i32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Size {
    width: i32,
    height: i32,
}

/// How a widget lays out the widgets inside it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Flow {
    Down,
    Across,
    OnTop,
    Grid,
    Nothing,
}

fn flow_of(kind: WidgetKind) -> Flow {
    use WidgetKind::*;
    match kind {
        Row => Flow::Across,
        Stack | Button => Flow::OnTop,
        Grid => Flow::Grid,
        // Pages and Tabs have no layout of their own yet, so they stack their children like a column.
        Column | Scroll | List | Pages | Tabs => Flow::Down,
        _ => Flow::Nothing,
    }
}

/// Whether a widget's children are cut off at its edges.
pub fn clips_children(kind: WidgetKind) -> bool {
    matches!(kind, WidgetKind::Scroll | WidgetKind::List)
}

/// Widgets drawn over their parent rather than placed in its flow.
fn is_overlay(kind: WidgetKind) -> bool {
    matches!(
        kind,
        WidgetKind::Line | WidgetKind::Scale | WidgetKind::AnalogClock
    )
}

/// The size a leaf widget wants for its content, before padding, given the width it may take.
fn content_size(node: &Node, available_width: Option<i32>) -> Size {
    let font = node.settings.font.metrics();
    let text_size = || Size {
        width: text_width(font, node.text()),
        height: line_height(font),
    };
    let fixed = |width, height| Size { width, height };
    let measured = || fixed(node.content_width as i32, node.content_height as i32);
    match node.kind {
        WidgetKind::Label => {
            let wraps = node.settings.long_mode == LongMode::Wrap;
            let width = match node.settings.width {
                Some(Length::Pixels(pixels)) => {
                    Some(i32::from(pixels) - 2 * i32::from(node.settings.padding))
                }
                _ => available_width,
            };
            match (wraps, width) {
                (true, Some(width)) if width > 0 => Size {
                    width: wrapped_width(node.text(), node.settings.font, width),
                    height: wrapped_line_count(node.text(), node.settings.font, width) * line_height(font),
                },
                _ => text_size(),
            }
        }
        WidgetKind::Button => {
            let text = text_size();
            let icon = measured();
            let icon_space = if icon.width > 0 && !node.text.is_empty() {
                TICK_BOX_GAP
            } else {
                0
            };
            fixed(
                text.width + icon.width + icon_space + 2 * BUTTON_SIDE_SPACE,
                text.height.max(icon.height).max(BUTTON_SMALLEST_HEIGHT),
            )
        }
        WidgetKind::Checkbox | WidgetKind::Radio => {
            let text = text_size();
            fixed(
                TICK_BOX_SIZE + TICK_BOX_GAP + text.width,
                text.height.max(TICK_BOX_SIZE),
            )
        }
        WidgetKind::Arc => fixed(ARC_SIZE, ARC_SIZE),
        WidgetKind::Bar => fixed(BAR_WIDTH, BAR_HEIGHT),
        WidgetKind::Led => fixed(LED_SIZE, LED_SIZE),
        WidgetKind::Switch => fixed(SWITCH_WIDTH, SWITCH_HEIGHT),
        WidgetKind::Slider => fixed(SLIDER_WIDTH, SLIDER_HEIGHT),
        WidgetKind::Chart => fixed(CHART_WIDTH, CHART_HEIGHT),
        WidgetKind::Image
        | WidgetKind::Icon
        | WidgetKind::CellGrid
        | WidgetKind::Line
        | WidgetKind::Roller
        | WidgetKind::Dropdown
        | WidgetKind::ButtonMatrix => measured(),
        _ => fixed(0, 0),
    }
}

/// The node numbers of a node's direct children.
fn children(nodes: &[Box<Node>], index: usize) -> impl Iterator<Item = usize> + '_ {
    let end = nodes[index].subtree_end as usize;
    let mut next = index + 1;
    core::iter::from_fn(move || {
        if next >= end {
            return None;
        }
        let child = next;
        next = nodes[child].subtree_end as usize;
        Some(child)
    })
}

fn is_positioned(node: &Node) -> bool {
    node.settings.x.map(i32::from).is_some()
        || node.settings.y.map(i32::from).is_some()
        || is_overlay(node.kind)
}

fn is_in_flow(node: &Node) -> bool {
    node.settings.visible && !is_positioned(node)
}

fn flowing(nodes: &[Box<Node>], index: usize) -> impl Iterator<Item = usize> + '_ {
    children(nodes, index).filter(|&child| is_in_flow(&nodes[child]))
}

/// The total of the gaps between `count` children.
fn gaps(count: i32, gap: i32) -> i32 {
    (count - 1).max(0) * gap
}

/// The size a widget wants, padding included and margin left out, within `available_width` if known.
fn natural_size(nodes: &[Box<Node>], index: usize, available_width: Option<i32>) -> Size {
    let node = &nodes[index];
    let padding = 2 * i32::from(node.settings.padding);
    let own_width = match node.settings.width {
        Some(Length::Pixels(pixels)) => Some(pixels.into()),
        _ => available_width,
    };
    let inner_width = own_width.map(|width| (width - padding).max(0));
    let with_margin = |child: usize, width: Option<i32>| {
        let margin = 2 * i32::from(nodes[child].settings.margin);
        let size = natural_size(nodes, child, width.map(|width| width - margin));
        Size {
            width: size.width + margin,
            height: size.height + margin,
        }
    };
    let gap = i32::from(node.settings.gap);
    let count = flowing(nodes, index).count() as i32;

    let own = content_size(node, inner_width);
    let inside = match flow_of(node.kind) {
        Flow::Down => {
            let total = flowing(nodes, index)
                .map(|child| with_margin(child, inner_width))
                .fold(Size { width: 0, height: 0 }, |total, size| Size {
                    width: total.width.max(size.width),
                    height: total.height + size.height,
                });
            Size {
                width: total.width,
                height: total.height + gaps(count, gap),
            }
        }
        Flow::Across => {
            let total = flowing(nodes, index).map(|child| with_margin(child, None)).fold(
                Size { width: 0, height: 0 },
                |total, size| Size {
                    width: total.width + size.width,
                    height: total.height.max(size.height),
                },
            );
            Size {
                width: total.width + gaps(count, gap),
                height: total.height,
            }
        }
        Flow::OnTop => flowing(nodes, index)
            .map(|child| with_margin(child, inner_width))
            .fold(own, |total, size| Size {
                width: total.width.max(size.width),
                height: total.height.max(size.height),
            }),
        Flow::Grid => {
            let columns = grid_columns(node);
            let cell_width = inner_width.map(|width| (width - gaps(columns, gap)) / columns);
            let mut widest = 0;
            let mut height = 0;
            let mut row_height = 0;
            for (position, child) in flowing(nodes, index).enumerate() {
                let size = with_margin(child, cell_width);
                widest = widest.max(size.width);
                row_height = row_height.max(size.height);
                if (position as i32 + 1) % columns == 0 || position as i32 + 1 == count {
                    height += row_height;
                    row_height = 0;
                }
            }
            let rows = (count + columns - 1) / columns;
            Size {
                width: widest * columns + gaps(columns, gap),
                height: height + gaps(rows, gap),
            }
        }
        Flow::Nothing => own,
    };

    Size {
        width: match node.settings.width {
            Some(Length::Pixels(pixels)) => pixels.into(),
            _ => inside.width + padding,
        },
        height: match node.settings.height {
            Some(Length::Pixels(pixels)) => pixels.into(),
            _ => inside.height + padding,
        },
    }
}

fn grid_columns(node: &Node) -> i32 {
    (node.settings.columns as i32).max(1)
}

// ============================================================================
// Placing
// ============================================================================

/// Places the screen's root in `screen` and everything inside it.
pub fn lay_out(nodes: &mut [Box<Node>], screen: Rect) {
    if nodes.is_empty() {
        return;
    }
    let root = place_positioned_box(nodes, 0, screen, true);
    place(nodes, 0, root, true, screen);
}

/// The box of a child placed by `x` and `y`, or of the root, which fills the screen by default.
fn place_positioned_box(nodes: &[Box<Node>], index: usize, inner: Rect, fills: bool) -> Rect {
    let settings = &nodes[index].settings;
    let fills = fills || matches!(nodes[index].kind, WidgetKind::Scale | WidgetKind::AnalogClock);
    let natural = natural_size(nodes, index, settings.width.map(|width| width.of(inner.width)));
    let length = |length: Option<Length>, whole: i32, natural: i32| match length {
        Some(length) => length.of(whole),
        None if fills => whole,
        None => natural,
    };
    Rect {
        x: inner.x + settings.x.map(i32::from).unwrap_or(0),
        y: inner.y + settings.y.map(i32::from).unwrap_or(0),
        width: length(settings.width, inner.width, natural.width),
        height: length(settings.height, inner.height, natural.height),
    }
}

fn place(nodes: &mut [Box<Node>], index: usize, bounds: Rect, parent_shown: bool, clip: Rect) {
    let node = &mut nodes[index];
    node.bounds = bounds;
    node.clip = clip;
    node.shown = parent_shown && node.settings.visible;
    let shown = node.shown;
    let mut inner = inset(bounds, i32::from(node.settings.padding));
    let children_clip = match clips_children(node.kind) {
        true => clip.intersect(bounds).unwrap_or(Rect {
            x: bounds.x,
            y: bounds.y,
            width: 0,
            height: 0,
        }),
        false => clip,
    };
    if clips_children(node.kind) {
        inner.y -= node.scroll_offset as i32;
    }

    match flow_of(node.kind) {
        Flow::Down => place_flow(nodes, index, inner, true, shown, children_clip),
        Flow::Across => place_flow(nodes, index, inner, false, shown, children_clip),
        Flow::OnTop => place_on_top(nodes, index, inner, shown, children_clip),
        Flow::Grid => place_grid(nodes, index, inner, shown, children_clip),
        Flow::Nothing => {}
    }

    let mut child = index + 1;
    while child < nodes[index].subtree_end as usize {
        if !is_in_flow(&nodes[child]) {
            let fills = false;
            let child_bounds = place_positioned_box(nodes, child, inner, fills);
            place(nodes, child, child_bounds, shown, children_clip);
        }
        child = nodes[child].subtree_end as usize;
    }
}

/// Lays children one after another, down when `is_down`, otherwise across.
fn place_flow(nodes: &mut [Box<Node>], index: usize, inner: Rect, is_down: bool, shown: bool, clip: Rect) {
    let main_of = |size: Size| if is_down { size.height } else { size.width };
    let cross_of = |size: Size| if is_down { size.width } else { size.height };
    let inner_size = Size {
        width: inner.width,
        height: inner.height,
    };
    let main_space = main_of(inner_size);
    let cross_space = cross_of(inner_size);
    let gap = i32::from(nodes[index].settings.gap);
    let justify = nodes[index].settings.justify;
    let align_items = nodes[index].settings.align_items;

    // Across the flow: the child's size, and where it starts, inside its margin.
    let cross_place = |nodes: &[Box<Node>], child: usize| {
        let settings = &nodes[child].settings;
        let cross_room = cross_space - 2 * i32::from(settings.margin);
        let wanted_cross = if is_down { settings.width } else { settings.height };
        let placement = match (settings.align, align_items) {
            (Some(align), _) => Some(align),
            (None, Some(Placement::Start)) => Some(Align::Start),
            (None, Some(Placement::Center)) => Some(Align::Center),
            (None, Some(Placement::End)) => Some(Align::End),
            _ => None,
        };
        let available_width = if is_down { Some(cross_room) } else { None };
        let cross = match (wanted_cross, placement) {
            (Some(length), _) => length.of(cross_space),
            (None, Some(_)) => cross_of(natural_size(nodes, child, available_width)).min(cross_room),
            (None, None) => cross_room,
        };
        let offset =
            i32::from(settings.margin) + placement.unwrap_or(Align::Center).offset(cross_room, cross);
        (cross, offset)
    };
    let main_length = |nodes: &[Box<Node>], child: usize, cross: i32| {
        let settings = &nodes[child].settings;
        let wanted = if is_down { settings.height } else { settings.width };
        match wanted {
            Some(length) => length.of(main_space),
            None => main_of(natural_size(nodes, child, is_down.then_some(cross))),
        }
    };

    let mut used = 0;
    let mut total_grow = 0;
    let mut count = 0;
    for child in flowing(nodes, index) {
        let (cross, _) = cross_place(nodes, child);
        used += main_length(nodes, child, cross) + 2 * i32::from(nodes[child].settings.margin);
        total_grow += i32::from(nodes[child].settings.grow);
        count += 1;
    }
    used += gaps(count, gap);
    let spare = (main_space - used).max(0);

    let (mut cursor, extra_gap) = match (total_grow, justify) {
        (0, Placement::Center) => (spare / 2, 0),
        (0, Placement::End) => (spare, 0),
        (0, Placement::SpaceBetween) if count > 1 => (0, spare / (count - 1)),
        _ => (0, 0),
    };

    let mut child = index + 1;
    while child < nodes[index].subtree_end as usize {
        let next_child = nodes[child].subtree_end as usize;
        if !is_in_flow(&nodes[child]) {
            child = next_child;
            continue;
        }
        let settings = nodes[child].settings;
        let grown = match total_grow {
            0 => 0,
            total => spare * i32::from(settings.grow) / total,
        };
        let (cross, cross_offset) = cross_place(nodes, child);
        let main = main_length(nodes, child, cross) + grown;
        let main_offset = cursor + i32::from(settings.margin);

        let child_bounds = if is_down {
            Rect {
                x: inner.x + cross_offset,
                y: inner.y + main_offset,
                width: cross,
                height: main,
            }
        } else {
            Rect {
                x: inner.x + main_offset,
                y: inner.y + cross_offset,
                width: main,
                height: cross,
            }
        };
        place(nodes, child, child_bounds, shown, clip);
        cursor += main + 2 * i32::from(settings.margin) + gap + extra_gap;
        child = next_child;
    }
}

/// Places a child inside `room`, filling it unless it has a size or an `align`.
fn place_in_room(nodes: &mut [Box<Node>], child: usize, room: Rect, shown: bool, clip: Rect) {
    let settings = nodes[child].settings;
    let room = inset(room, i32::from(settings.margin));
    let natural = natural_size(nodes, child, Some(room.width));
    let length = |wanted: Option<Length>, whole: i32, natural: i32| match (wanted, settings.align) {
        (Some(length), _) => length.of(whole),
        (None, Some(_)) => natural.min(whole),
        (None, None) => whole,
    };
    let width = length(settings.width, room.width, natural.width);
    let height = length(settings.height, room.height, natural.height);
    let align = settings.align.unwrap_or(Align::Center);
    let child_bounds = Rect {
        x: room.x + align.offset(room.width, width),
        y: room.y + align.offset(room.height, height),
        width,
        height,
    };
    place(nodes, child, child_bounds, shown, clip);
}

/// Puts children on top of each other, each filling the inside unless it has a size or `align`.
fn place_on_top(nodes: &mut [Box<Node>], index: usize, inner: Rect, shown: bool, clip: Rect) {
    let mut child = index + 1;
    while child < nodes[index].subtree_end as usize {
        let next_child = nodes[child].subtree_end as usize;
        if is_in_flow(&nodes[child]) {
            place_in_room(nodes, child, inner, shown, clip);
        }
        child = next_child;
    }
}

/// Puts children in rows of `columns` equal cells; rows share out any spare height.
fn place_grid(nodes: &mut [Box<Node>], index: usize, inner: Rect, shown: bool, clip: Rect) {
    let columns = grid_columns(&nodes[index]);
    let gap = i32::from(nodes[index].settings.gap);
    let cell_width = ((inner.width - gaps(columns, gap)) / columns).max(0);
    let flowing_children: alloc::vec::Vec<usize> = flowing(nodes, index).collect();
    let count = flowing_children.len() as i32;
    let rows = (count + columns - 1) / columns;
    if rows == 0 {
        return;
    }

    let row_height = |nodes: &[Box<Node>], row: i32| {
        flowing_children
            .iter()
            .skip((row * columns) as usize)
            .take(columns as usize)
            .map(|&child| {
                let margin = 2 * i32::from(nodes[child].settings.margin);
                natural_size(nodes, child, Some(cell_width - margin)).height + margin
            })
            .max()
            .unwrap_or(0)
    };
    let natural_total: i32 = (0..rows).map(|row| row_height(nodes, row)).sum::<i32>() + gaps(rows, gap);
    let spare_per_row = ((inner.height - natural_total) / rows).max(0);

    let mut top = inner.y;
    for row in 0..rows {
        let height = row_height(nodes, row) + spare_per_row;
        for column in 0..columns {
            let Some(&child) = flowing_children.get((row * columns + column) as usize) else {
                break;
            };
            let room = Rect {
                x: inner.x + column * (cell_width + gap),
                y: top,
                width: cell_width,
                height,
            };
            place_in_room(nodes, child, room, shown, clip);
        }
        top += height + gap;
    }
}

/// How tall the content of a Scroll or List is, to bound how far it scrolls.
pub fn content_height(nodes: &[Box<Node>], index: usize) -> i32 {
    let settings = &nodes[index].settings;
    let inner_width = (nodes[index].bounds.width - 2 * i32::from(settings.padding)).max(0);
    let count = flowing(nodes, index).count() as i32;
    let children: i32 = flowing(nodes, index)
        .map(|child| {
            let margin = 2 * i32::from(nodes[child].settings.margin);
            natural_size(nodes, child, Some(inner_width - margin)).height + margin
        })
        .sum();
    children + gaps(count, i32::from(settings.gap)) + 2 * i32::from(settings.padding)
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod layout_tests;
