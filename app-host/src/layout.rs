//! Places every node of the open screen, in the spirit of a small flexbox.
//!
//! A `Column` stacks its children down and a `Row` across, `gap` apart; spare space goes to children
//! with `grow`, in proportion, or else `justify` places them. Across the flow a child fills the space
//! unless it has a size, an `align`, or its parent an `align_items`. A `Stack` and a `Button` put
//! their children on top of each other, an `Icon` keeping its own size, and a `Grid` in rows of `columns` cells. A `Scroll` and a
//! `List` are columns that clip their children and can be scrolled. A child with `x` or `y` leaves
//! the flow and sits at that offset inside its parent, and so do lines, scales and analog clocks,
//! which are drawn over the whole of their parent unless given a size.

use alloc::boxed::Box;
use neotime_runtime::WidgetKind;
use neotime_ui::geometry::Rect;

use crate::drawing::SCROLL_BAR_ROOM;
use crate::fonts::{line_height, text_width};
use crate::nodes::{inset, row_slots, Align, Length, LongMode, Node, Placement};
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
    /// A column centred both ways, as an Arc holds its children in its middle.
    DownCentered,
    Across,
    OnTop,
    Grid,
    Nothing,
}

fn flow_of(nodes: &[Box<Node>], index: usize) -> Flow {
    use WidgetKind::*;
    if runs_across(nodes, index) {
        return Flow::Across;
    }
    match nodes[index].kind {
        Row => Flow::Across,
        Stack | Button => Flow::OnTop,
        Grid => Flow::Grid,
        // Pages and Tabs have no layout of their own yet, so they stack their children like a column.
        Column | Scroll | List | Pages | Tabs | ClickArea | Repeat => Flow::Down,
        Arc => Flow::DownCentered,
        _ => Flow::Nothing,
    }
}

/// Whether a node's children run across: a Scroll or List set to, or the slotted rows of a Repeat in one.
pub fn runs_across(nodes: &[Box<Node>], index: usize) -> bool {
    let node = &nodes[index];
    node.runs_across()
        || node.kind == WidgetKind::Repeat
            && row_slots(node).is_some()
            && node
                .parent
                .is_some_and(|parent| nodes[usize::from(parent)].runs_across())
}

/// Whether a widget's children are cut off at its edges.
pub fn clips_children(kind: WidgetKind) -> bool {
    matches!(
        kind,
        WidgetKind::Scroll | WidgetKind::List | WidgetKind::AppList | WidgetKind::TextFile
    )
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
        | WidgetKind::ButtonMatrix
        | WidgetKind::Battery
        | WidgetKind::StatusIcons
        | WidgetKind::Badge
        | WidgetKind::TimeRollers
        | WidgetKind::DurationRollers
        | WidgetKind::WeekdayPicker
        | WidgetKind::Stepper
        | WidgetKind::NumberRoller => measured(),
        _ => fixed(0, 0),
    }
}

/// Whether a node is a Repeat whose rows are laid out as its parent's own children.
///
/// Only a Repeat whose rows sit in slots of one height, as in a long list, lays them out itself.
pub fn is_transparent(node: &Node) -> bool {
    node.kind == WidgetKind::Repeat && row_slots(node).is_none()
}

/// The node numbers of a node's children as layout sees them: the rows of a transparent Repeat
/// stand in for the Repeat itself.
fn children(nodes: &[Box<Node>], index: usize) -> impl Iterator<Item = usize> + '_ {
    let end = nodes[index].subtree_end as usize;
    let mut next = index + 1;
    // Inside a transparent Repeat: where its rows end, and where the parent's next child is.
    let mut inside: Option<(usize, usize)> = None;
    core::iter::from_fn(move || loop {
        if let Some((rows_end, resume)) = inside {
            if next < rows_end {
                let row = next;
                next = nodes[row].subtree_end as usize;
                return Some(row);
            }
            inside = None;
            next = resume;
        }
        if next >= end {
            return None;
        }
        let child = next;
        next = nodes[child].subtree_end as usize;
        if is_transparent(&nodes[child]) && nodes[child].settings.visible {
            inside = Some((nodes[child].subtree_end as usize, next));
            next = child + 1;
            continue;
        }
        if is_transparent(&nodes[child]) {
            continue;
        }
        return Some(child);
    })
}

/// The transparent Repeats directly inside a node.
fn transparent_repeats(nodes: &[Box<Node>], index: usize) -> alloc::vec::Vec<usize> {
    let end = nodes[index].subtree_end as usize;
    let mut found = alloc::vec::Vec::new();
    let mut child = index + 1;
    while child < end {
        if is_transparent(&nodes[child]) {
            found.push(child);
        }
        child = nodes[child].subtree_end as usize;
    }
    found
}

/// Gives each transparent Repeat inside a node the box around its rows, so clipping and lookups still work.
fn place_transparent_repeats(nodes: &mut [Box<Node>], index: usize, inner: Rect, shown: bool, clip: Rect) {
    for repeat in transparent_repeats(nodes, index) {
        let end = nodes[repeat].subtree_end as usize;
        let mut around: Option<Rect> = None;
        let mut row = repeat + 1;
        while row < end {
            let bounds = nodes[row].bounds;
            around = Some(around.map_or(bounds, |around| around.union(bounds)));
            row = nodes[row].subtree_end as usize;
        }
        let node = &mut nodes[repeat];
        node.bounds = around.unwrap_or(Rect {
            x: inner.x,
            y: inner.y,
            width: 0,
            height: 0,
        });
        node.clip = clip;
        node.shown = shown && node.settings.visible;
        if !node.shown {
            let mut row = repeat + 1;
            while row < end {
                let row_clip = nodes[row].clip;
                let row_bounds = nodes[row].bounds;
                place(nodes, row, row_bounds, false, row_clip);
                row = nodes[row].subtree_end as usize;
            }
        }
    }
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
    children(nodes, index).filter(|&child| is_in_flow(&nodes[child]) && !is_empty_repeat(nodes, child))
}

/// A Repeat with no row shown takes no space, so the widgets around it close up as if it were not there.
fn is_empty_repeat(nodes: &[Box<Node>], index: usize) -> bool {
    match row_slots(&nodes[index]) {
        Some(slots) => slots.count == 0,
        None => {
            nodes[index].kind == WidgetKind::Repeat
                && !children(nodes, index).any(|row| nodes[row].settings.visible)
        }
    }
}

/// The total of the gaps between `count` children.
fn gaps(count: i32, gap: i32) -> i32 {
    (count - 1).max(0) * gap
}

/// The size a widget wants, padding included and margin left out, within `available_width` if known.
fn natural_size(nodes: &[Box<Node>], index: usize, available_width: Option<i32>) -> Size {
    let node = &nodes[index];
    if let Some(slots) = row_slots(node) {
        let gap = i32::from(node.settings.gap);
        let length = slots.count * slots.extent + gaps(slots.count, gap);
        if runs_across(nodes, index) {
            let tallest = flowing(nodes, index)
                .map(|row| natural_size(nodes, row, None).height + 2 * i32::from(nodes[row].settings.margin))
                .max()
                .unwrap_or(0);
            return Size {
                width: length,
                height: tallest,
            };
        }
        return Size {
            width: available_width.unwrap_or(0),
            height: length,
        };
    }
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
    let inside = match flow_of(nodes, index) {
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
        // An Arc keeps its own size, with its children inside it.
        Flow::Nothing | Flow::DownCentered => own,
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
    let across = runs_across(nodes, index);
    nodes[index].bounds = bounds;
    let shows_scroll_bar = wants_scroll_bar_room(nodes, index);
    let node = &mut nodes[index];
    node.shows_scroll_bar = shows_scroll_bar;
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
    if shows_scroll_bar {
        match across {
            true => inner.height = (inner.height - SCROLL_BAR_ROOM).max(0),
            false => inner.width = (inner.width - SCROLL_BAR_ROOM).max(0),
        }
    }
    if has_page_bar(node) {
        inner.height = (inner.height - SCROLL_BAR_ROOM).max(0);
    }
    if clips_children(node.kind) {
        match across {
            true => inner.x -= node.scroll_offset as i32,
            false => inner.y -= node.scroll_offset as i32,
        }
    }
    if let Some(slots) = row_slots(node) {
        let skipped = slots.first_built * (slots.extent + i32::from(node.settings.gap));
        match across {
            true => inner.x += skipped,
            false => inner.y += skipped,
        }
    }

    match flow_of(nodes, index) {
        Flow::Down => place_flow(nodes, index, inner, true, false, shown, children_clip),
        Flow::DownCentered => place_flow(nodes, index, inner, true, true, shown, children_clip),
        Flow::Across => place_flow(nodes, index, inner, false, false, shown, children_clip),
        Flow::OnTop => place_on_top(nodes, index, inner, shown, children_clip),
        Flow::Grid => place_grid(nodes, index, inner, shown, children_clip),
        Flow::Nothing => {}
    }

    let kids: alloc::vec::Vec<usize> = children(nodes, index).collect();
    for child in kids {
        if !is_in_flow(&nodes[child]) {
            let fills = false;
            let child_bounds = place_positioned_box(nodes, child, inner, fills);
            place(nodes, child, child_bounds, shown, children_clip);
        }
    }
    place_transparent_repeats(nodes, index, inner, shown, children_clip);
}

/// Lays children one after another, down when `is_down`, otherwise across; `centered` puts them in the middle both ways.
fn place_flow(
    nodes: &mut [Box<Node>],
    index: usize,
    inner: Rect,
    is_down: bool,
    centered: bool,
    shown: bool,
    clip: Rect,
) {
    let main_of = |size: Size| if is_down { size.height } else { size.width };
    let cross_of = |size: Size| if is_down { size.width } else { size.height };
    let inner_size = Size {
        width: inner.width,
        height: inner.height,
    };
    let main_space = main_of(inner_size);
    let cross_space = cross_of(inner_size);
    let gap = i32::from(nodes[index].settings.gap);
    let (justify, align_items) = match centered {
        true => (Placement::Center, Some(Placement::Center)),
        false => (nodes[index].settings.justify, nodes[index].settings.align_items),
    };

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
            // A growing Scroll or List fills the space left and scrolls the rest, and a growing Pages fills it
            // with one page, so their content never sizes them.
            None if (clips_children(nodes[child].kind) || nodes[child].kind == WidgetKind::Pages)
                && settings.grow > 0 =>
            {
                2 * i32::from(settings.padding)
            }
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

    let kids: alloc::vec::Vec<usize> = children(nodes, index).collect();
    for child in kids {
        if !is_in_flow(&nodes[child]) {
            continue;
        }
        if is_empty_repeat(nodes, child) {
            let here = if is_down {
                Rect {
                    x: inner.x,
                    y: inner.y + cursor,
                    width: inner.width,
                    height: 0,
                }
            } else {
                Rect {
                    x: inner.x + cursor,
                    y: inner.y,
                    width: 0,
                    height: inner.height,
                }
            };
            place(nodes, child, here, shown, clip);
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
    }
}

/// Places a child inside `room`, filling it unless it has a size or an `align`.
fn place_in_room(nodes: &mut [Box<Node>], child: usize, room: Rect, shown: bool, clip: Rect) {
    let settings = nodes[child].settings;
    let room = inset(room, i32::from(settings.margin));
    let natural = natural_size(nodes, child, Some(room.width));
    // An icon keeps the 32 pixels the sdk promises, so a button's icon leaves room around it.
    let keeps_natural = settings.align.is_some() || nodes[child].kind == WidgetKind::Icon;
    let length = |wanted: Option<Length>, whole: i32, natural: i32| match (wanted, keeps_natural) {
        (Some(length), _) => length.of(whole),
        (None, true) => natural.min(whole),
        (None, false) => whole,
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
    let kids: alloc::vec::Vec<usize> = children(nodes, index).collect();
    for child in kids {
        if is_in_flow(&nodes[child]) {
            place_in_room(nodes, child, inner, shown, clip);
        }
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

/// How long the content of a Scroll, List, AppList or TextFile is the way it scrolls, padding included.
pub fn content_length(nodes: &[Box<Node>], index: usize) -> i32 {
    match nodes[index].kind {
        WidgetKind::AppList => return crate::drawing::app_list_height(&nodes[index]),
        WidgetKind::TextFile => return crate::drawing::text_file_height(&nodes[index]),
        _ => {}
    }
    let settings = &nodes[index].settings;
    let padding = 2 * i32::from(settings.padding);
    let count = flowing(nodes, index).count() as i32;
    let gap = gaps(count, i32::from(settings.gap));
    if nodes[index].runs_across() {
        let children: i32 = flowing(nodes, index)
            .map(|child| natural_size(nodes, child, None).width + 2 * i32::from(nodes[child].settings.margin))
            .sum();
        return children + gap + padding;
    }
    let inner_width = (nodes[index].bounds.width - padding).max(0);
    let children: i32 = flowing(nodes, index)
        .map(|child| {
            let margin = 2 * i32::from(nodes[child].settings.margin);
            natural_size(nodes, child, Some(inner_width - margin)).height + margin
        })
        .sum();
    children + gap + padding
}

/// How much of a scrolling widget's content shows at once, along the way it scrolls.
pub fn visible_length(node: &Node) -> i32 {
    match node.runs_across() {
        true => node.bounds.width,
        false => node.bounds.height,
    }
}

/// How far a scrolling widget can scroll: how much longer its content is than the widget.
pub fn farthest_scroll(nodes: &[Box<Node>], index: usize) -> i32 {
    (content_length(nodes, index) - visible_length(&nodes[index])).max(0)
}

/// Whether a scrolling widget draws a scroll bar, and so keeps room beside its content for it.
/// Whether a widget is a Pages with more than one page, so it keeps room along its bottom for its page bar.
pub fn has_page_bar(node: &Node) -> bool {
    node.kind == WidgetKind::Pages && node.option_count > 1
}

fn wants_scroll_bar_room(nodes: &[Box<Node>], index: usize) -> bool {
    let node = &nodes[index];
    let overflows = match node.kind {
        WidgetKind::AppList => crate::drawing::app_list_height_without_bar(node) > visible_length(node),
        _ => farthest_scroll(nodes, index) > 0,
    };
    clips_children(node.kind) && node.wants_scroll_bar() && overflows
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod layout_tests;
