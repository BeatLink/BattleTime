//! Places every node of the open screen, in the spirit of a small flexbox.
//!
//! A `Column` stacks its children down and a `Row` across; spare space goes to children with
//! `grow`, in proportion. Across the flow a child fills the space unless it has a size or an
//! `align`. A `Stack` and a `Button` put their children on top of each other. A child with `x` or
//! `y` leaves the flow and sits at that offset inside its parent.

use neotime_runtime::WidgetKind;
use rlvgl_core::widget::Rect;

use crate::fonts::{line_height, text_width};
use crate::nodes::{inset, Align, Length, Node};

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
    Nothing,
}

fn flow_of(kind: WidgetKind) -> Flow {
    use WidgetKind::*;
    match kind {
        Row => Flow::Across,
        Stack | Button => Flow::OnTop,
        // Containers without their own layout yet stack their children like a column.
        Column | Grid | Scroll | List | Pages | Tabs => Flow::Down,
        _ => Flow::Nothing,
    }
}

/// The size a leaf widget wants for its content, before padding.
fn content_size(node: &Node) -> Size {
    let font = node.settings.font.metrics();
    let text_size = || Size {
        width: text_width(font, node.text()),
        height: line_height(font),
    };
    let fixed = |width, height| Size { width, height };
    match node.kind {
        WidgetKind::Label => text_size(),
        WidgetKind::Button => {
            let text = text_size();
            fixed(
                text.width + 2 * BUTTON_SIDE_SPACE,
                text.height.max(BUTTON_SMALLEST_HEIGHT),
            )
        }
        WidgetKind::Arc => fixed(ARC_SIZE, ARC_SIZE),
        WidgetKind::Bar => fixed(BAR_WIDTH, BAR_HEIGHT),
        WidgetKind::Led => fixed(LED_SIZE, LED_SIZE),
        WidgetKind::Switch => fixed(SWITCH_WIDTH, SWITCH_HEIGHT),
        WidgetKind::Slider => fixed(SLIDER_WIDTH, SLIDER_HEIGHT),
        _ => fixed(0, 0),
    }
}

/// The node numbers of a node's direct children.
fn children(nodes: &[Node], index: usize) -> impl Iterator<Item = usize> + '_ {
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
    node.settings.x.is_some() || node.settings.y.is_some()
}

fn is_in_flow(node: &Node) -> bool {
    node.settings.visible && !is_positioned(node)
}

/// The size a widget wants, padding included and margin left out.
fn natural_size(nodes: &[Node], index: usize) -> Size {
    let node = &nodes[index];
    let flowing = || children(nodes, index).filter(|&child| is_in_flow(&nodes[child]));
    let with_margin = |child: usize| {
        let size = natural_size(nodes, child);
        let margin = 2 * nodes[child].settings.margin;
        Size {
            width: size.width + margin,
            height: size.height + margin,
        }
    };

    let own = content_size(node);
    let inside = match flow_of(node.kind) {
        Flow::Down => flowing()
            .map(with_margin)
            .fold(Size { width: 0, height: 0 }, |total, size| Size {
                width: total.width.max(size.width),
                height: total.height + size.height,
            }),
        Flow::Across => flowing()
            .map(with_margin)
            .fold(Size { width: 0, height: 0 }, |total, size| Size {
                width: total.width + size.width,
                height: total.height.max(size.height),
            }),
        Flow::OnTop => flowing().map(with_margin).fold(own, |total, size| Size {
            width: total.width.max(size.width),
            height: total.height.max(size.height),
        }),
        Flow::Nothing => own,
    };

    let padding = 2 * node.settings.padding;
    Size {
        width: match node.settings.width {
            Some(Length::Pixels(pixels)) => pixels,
            _ => inside.width + padding,
        },
        height: match node.settings.height {
            Some(Length::Pixels(pixels)) => pixels,
            _ => inside.height + padding,
        },
    }
}

// ============================================================================
// Placing
// ============================================================================

/// Places the screen's root in `screen` and everything inside it.
pub fn lay_out(nodes: &mut [Node], screen: Rect) {
    if nodes.is_empty() {
        return;
    }
    let root = place_positioned_box(nodes, 0, screen, true);
    place(nodes, 0, root, true);
}

/// The box of a child placed by `x` and `y`, or of the root, which fills the screen by default.
fn place_positioned_box(nodes: &[Node], index: usize, inner: Rect, fills: bool) -> Rect {
    let settings = &nodes[index].settings;
    let natural = natural_size(nodes, index);
    let length = |length: Option<Length>, whole: i32, natural: i32| match length {
        Some(length) => length.of(whole),
        None if fills => whole,
        None => natural,
    };
    Rect {
        x: inner.x + settings.x.unwrap_or(0),
        y: inner.y + settings.y.unwrap_or(0),
        width: length(settings.width, inner.width, natural.width),
        height: length(settings.height, inner.height, natural.height),
    }
}

fn place(nodes: &mut [Node], index: usize, bounds: Rect, parent_shown: bool) {
    let node = &mut nodes[index];
    node.bounds = bounds;
    node.shown = parent_shown && node.settings.visible;
    let shown = node.shown;
    let inner = inset(bounds, node.settings.padding);
    let flow = flow_of(node.kind);

    match flow {
        Flow::Down => place_flow(nodes, index, inner, true, shown),
        Flow::Across => place_flow(nodes, index, inner, false, shown),
        Flow::OnTop => place_on_top(nodes, index, inner, shown),
        Flow::Nothing => {}
    }

    let mut child = index + 1;
    while child < nodes[index].subtree_end as usize {
        if !is_in_flow(&nodes[child]) {
            let fills = false;
            let child_bounds = place_positioned_box(nodes, child, inner, fills);
            place(nodes, child, child_bounds, shown);
        }
        child = nodes[child].subtree_end as usize;
    }
}

/// Lays children one after another, down when `is_down`, otherwise across.
fn place_flow(nodes: &mut [Node], index: usize, inner: Rect, is_down: bool, shown: bool) {
    let main_of = |size: Size| if is_down { size.height } else { size.width };
    let cross_of = |size: Size| if is_down { size.width } else { size.height };
    let inner_size = Size {
        width: inner.width,
        height: inner.height,
    };
    let main_space = main_of(inner_size);
    let cross_space = cross_of(inner_size);

    let main_length = |nodes: &[Node], child: usize| {
        let settings = &nodes[child].settings;
        let wanted = if is_down { settings.height } else { settings.width };
        match wanted {
            Some(length) => length.of(main_space),
            None => main_of(natural_size(nodes, child)),
        }
    };

    let mut used = 0;
    let mut total_grow = 0;
    for child in children(nodes, index).filter(|&child| is_in_flow(&nodes[child])) {
        used += main_length(nodes, child) + 2 * nodes[child].settings.margin;
        total_grow += nodes[child].settings.grow;
    }
    let spare = (main_space - used).max(0);

    let mut cursor = 0;
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
            total => spare * settings.grow / total,
        };
        let main = main_length(nodes, child) + grown;

        let cross_room = cross_space - 2 * settings.margin;
        let wanted_cross = if is_down { settings.width } else { settings.height };
        let cross = match (wanted_cross, settings.align) {
            (Some(length), _) => length.of(cross_space),
            (None, Some(_)) => cross_of(natural_size(nodes, child)).min(cross_room),
            (None, None) => cross_room,
        };
        let cross_offset =
            settings.margin + settings.align.unwrap_or(Align::Center).offset(cross_room, cross);
        let main_offset = cursor + settings.margin;

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
        place(nodes, child, child_bounds, shown);
        cursor += main + 2 * settings.margin;
        child = next_child;
    }
}

/// Puts children on top of each other, each filling the inside unless it has a size or `align`.
fn place_on_top(nodes: &mut [Node], index: usize, inner: Rect, shown: bool) {
    let mut child = index + 1;
    while child < nodes[index].subtree_end as usize {
        let next_child = nodes[child].subtree_end as usize;
        if !is_in_flow(&nodes[child]) {
            child = next_child;
            continue;
        }
        let settings = nodes[child].settings;
        let room = inset(inner, settings.margin);
        let natural = natural_size(nodes, child);
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
        place(nodes, child, child_bounds, shown);
        child = next_child;
    }
}
