//! How much of the watch's heap an app needs while it is open, worked out from its image alone.
//!
//! The package manager checks it before an app is installed or turned on, and the shell before it
//! opens one. docs/memory-map.md explains each part and how it was measured on the watch.

use crate::image::AppImage;
use crate::instruction::{decode, Instruction, SystemValue};
use crate::widgets::{Property, PropertySource, WidgetKind, WidgetTree, DEEPEST_NESTING, MOST_WIDGETS};

// ============================================================================
// Sizes measured on the watch
// ============================================================================

/// The app host's state, interpreter and bindings, which every open app holds whatever its size.
pub const WORKING_BYTES: u32 = 6_628;

/// One node of the open screen, as `size_of` gives it on the watch, plus its place in the list.
pub const NODE_BYTES: u32 = 164;

/// Each subscription the host keeps track of.
pub const SUBSCRIPTION_BYTES: u32 = 40;

/// What a label whose text is worked out while the app runs is taken to hold.
pub const WORKED_OUT_TEXT_BYTES: u32 = 16;

/// A TextFile's line places, the lines near its view and the buffer it reads into.
pub const TEXT_FILE_BYTES: u32 = 2_560;

/// What drawing borrows from the heap for a moment, and the image checks when the app opens.
pub const PASSING_BYTES: u32 = 1_536;

/// Rows of a Repeat taken to be built when their number is worked out while the app runs.
pub const ASSUMED_ROWS: u32 = 8;

/// The heap the firmware has: 10 KB for the system and 21 KB for the UI; the flash store keeps its tables outside it.
pub const HEAP_BYTES: u32 = 31_744;

/// What the system holds on the heap while an app is open: notifications, file transfers and the shell.
pub const SYSTEM_BYTES: u32 = 4_096;

/// The most an open app may need, leaving the system its share.
pub const APP_HEAP_BUDGET_BYTES: u32 = HEAP_BYTES - SYSTEM_BYTES;

/// What one installed app costs the shell's list of apps, beside its id and name.
pub const LISTED_APP_BYTES: u32 = 48;

/// The screen's height, which bounds the rows of a list that are built at once.
const SCREEN_HEIGHT: i32 = 240;

/// Each heap allocation is rounded up to this many bytes.
const ALLOCATION_STEP: u32 = 4;

// ============================================================================
// The estimate
// ============================================================================

/// An app's heap need, part by part, in bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryNeed {
    /// The image itself, which is read into the heap while the app is open.
    pub image_bytes: u32,
    /// The host's working memory, its subscriptions, its saved state and what drawing borrows.
    pub working_bytes: u32,
    /// The nodes of its largest screen.
    pub widget_bytes: u32,
    /// The text those nodes hold.
    pub text_bytes: u32,
    /// Everything above together.
    pub total_bytes: u32,
    /// Whether it shows the launcher's list of apps, which costs more for each app turned on.
    pub lists_apps: bool,
}

/// Works out an app's heap need from its image.
pub fn estimate_memory_need(image: &AppImage) -> MemoryNeed {
    let tree = WidgetTree::new(*image);
    let mut largest = ScreenCost::default();
    for root in tree.screens() {
        let cost = screen_cost(&tree, root);
        if cost.node_count > largest.node_count
            || (cost.node_count == largest.node_count && cost.text_bytes > largest.text_bytes)
        {
            largest = cost;
        }
    }

    let image_bytes = rounded(image.byte_count() as u32);
    let working_bytes = WORKING_BYTES
        + SUBSCRIPTION_BYTES * image.subscription_count() as u32
        + rounded(saved_state_bytes(image) as u32)
        + PASSING_BYTES;
    let widget_bytes = NODE_BYTES * largest.node_count;
    let text_bytes = largest.text_bytes;
    let reads = system_values_read(image);
    MemoryNeed {
        image_bytes,
        working_bytes,
        widget_bytes,
        text_bytes,
        total_bytes: image_bytes + working_bytes + widget_bytes + text_bytes,
        lists_apps: largest.has_app_list || reads & bit_of(SystemValue::InstalledAppCount) != 0,
    }
}

/// What an app adds to the launcher's list while the launcher is open: its icon and its name.
pub fn launcher_share_bytes(image: &AppImage) -> u32 {
    let icon = image
        .has_icon
        .then(|| image.resource_bytes(0))
        .flatten()
        .map_or(0, |icon| rounded(icon.len() as u32));
    let name = image.string(image.name_string).map_or(0, str::len) as u32;
    // The icon's place in the launcher's list, and the name's line in the list's text.
    icon + 8 + name + 1
}

/// What an app costs the shell's list of installed apps, which every open app shares the heap with.
pub fn listed_app_bytes(id: &str, name: &str) -> u32 {
    LISTED_APP_BYTES + rounded(id.len() as u32) + rounded(name.len() as u32)
}

/// The most bytes the app's saved state can take: a header, then each persistent variable with its key.
pub fn saved_state_bytes(image: &AppImage) -> usize {
    const SAVED_HEADER_BYTES: usize = 6;
    const ENTRY_HEADER_BYTES: usize = 5;
    let mut bytes = 0;
    for index in 0..image.state_count() {
        let Some(variable) = image.state_variable(index).filter(|variable| variable.persists) else {
            continue;
        };
        let key = image.string(variable.key_string).map_or(0, str::len);
        bytes += ENTRY_HEADER_BYTES + key + variable.stored_bytes();
    }
    if bytes == 0 {
        0
    } else {
        SAVED_HEADER_BYTES + bytes
    }
}

// ============================================================================
// Screens
// ============================================================================

#[derive(Clone, Copy, Debug, Default)]
struct ScreenCost {
    node_count: u32,
    text_bytes: u32,
    has_app_list: bool,
}

/// One open widget whose children are still being counted.
#[derive(Clone, Copy)]
struct OpenWidget {
    children_left: u8,
    /// How many copies of each child are built, more than one inside a Repeat.
    copies: u32,
}

/// The nodes and text one screen builds, counting every widget as shown and each Repeat's rows.
fn screen_cost(tree: &WidgetTree, root: u16) -> ScreenCost {
    let mut cost = ScreenCost::default();
    let end = tree.subtree_end(root).unwrap_or(root);
    let mut open = [OpenWidget {
        children_left: 0,
        copies: 1,
    }; DEEPEST_NESTING];
    let mut depth = 0;
    let mut parent_kinds = [WidgetKind::Column; DEEPEST_NESTING];

    for index in root..end {
        let Some(widget) = tree.widget(index) else {
            break;
        };
        let copies = if depth > 0 { open[depth - 1].copies } else { 1 };
        let parent_kind = if depth > 0 {
            Some(parent_kinds[depth - 1])
        } else {
            None
        };
        cost.node_count += copies;
        cost.text_bytes += copies * widget_text_bytes(tree, &widget);
        cost.has_app_list |= widget.kind == WidgetKind::AppList;

        if depth > 0 {
            open[depth - 1].children_left -= 1;
        }
        if widget.child_count > 0 && depth < DEEPEST_NESTING {
            let rows = match widget.kind {
                WidgetKind::Repeat => repeat_rows(tree, &widget, index, parent_kind),
                _ => 1,
            };
            open[depth] = OpenWidget {
                children_left: widget.child_count,
                copies: copies.saturating_mul(rows),
            };
            parent_kinds[depth] = widget.kind;
            depth += 1;
        }
        while depth > 0 && open[depth - 1].children_left == 0 {
            depth -= 1;
        }
    }
    // The host builds no more nodes than this for one screen.
    cost.node_count = cost.node_count.min(MOST_WIDGETS as u32);
    cost
}

/// How many rows of a Repeat are built: all of them, or only those in view down a list of fixed-height rows.
fn repeat_rows(
    tree: &WidgetTree,
    repeat: &crate::widgets::Widget,
    index: u16,
    parent_kind: Option<WidgetKind>,
) -> u32 {
    let rows = match repeat.property(Property::RowCount) {
        Some(PropertySource::Constant(count)) => count.max(0) as u32,
        _ => ASSUMED_ROWS,
    };
    let in_a_list = matches!(parent_kind, Some(WidgetKind::List | WidgetKind::Scroll));
    let row_height = tree
        .widget(index + 1)
        .and_then(|row| row.property(Property::Height));
    match row_height {
        Some(PropertySource::Constant(height)) if height > 0 && in_a_list => {
            rows.min((SCREEN_HEIGHT / height) as u32 + 2)
        }
        _ => rows,
    }
}

/// The text one node of this widget holds.
fn widget_text_bytes(tree: &WidgetTree, widget: &crate::widgets::Widget) -> u32 {
    if widget.kind == WidgetKind::TextFile {
        return TEXT_FILE_BYTES;
    }
    match widget.property(Property::Text) {
        Some(PropertySource::Constant(string)) => tree
            .image()
            .string(string as u16)
            .map_or(0, |text| rounded(text.len() as u32)),
        Some(PropertySource::Expression(_)) => WORKED_OUT_TEXT_BYTES,
        _ => 0,
    }
}

// ============================================================================
// What the code reads
// ============================================================================

fn bit_of(value: SystemValue) -> u128 {
    1 << (value as u8)
}

/// One bit for each system value any of the app's code reads, by the value's number.
fn system_values_read(image: &AppImage) -> u128 {
    let mut reads = 0;
    let mut scan = |code: &[u8]| {
        let mut offset = 0;
        while let Ok((instruction, next)) = decode(code, offset) {
            if let Instruction::ReadSystem(value) | Instruction::ReadSystemElement(value) = instruction {
                reads |= bit_of(value);
            }
            offset = next;
        }
    };
    for expression in 0..image.expression_count() {
        scan(image.expression_code(expression).unwrap_or(&[]));
    }
    for handler in 0..image.handler_count() {
        scan(image.handler_code(handler).unwrap_or(&[]));
    }
    for function in 0..image.function_count() {
        scan(image.function(function).map_or(&[][..], |function| function.code));
    }
    reads
}

/// A size rounded up to the heap's allocation step, and none for nothing.
fn rounded(bytes: u32) -> u32 {
    bytes.div_ceil(ALLOCATION_STEP) * ALLOCATION_STEP
}
