//! How much of the watch's heap an app needs while it is open, worked out from its image alone.
//!
//! The package manager checks it before an app is installed or turned on, and the shell before it
//! opens one. docs/memory-map.md explains each part and how it was measured on the watch.

use crate::image::AppImage;
use crate::instruction::{decode, EffectKind, Instruction, SystemValue};
use crate::interpreter::scratch_bytes_for;
use crate::library_icons::{held_icon_bytes, HELD_ICONS_MOST_BYTES, LIBRARY_ICON_MOST_BYTES};
use crate::resources::{library_image_named_by, ImageReference};
use crate::widgets::{Property, PropertySource, WidgetKind, WidgetTree, DEEPEST_NESTING, MOST_WIDGETS};

// ============================================================================
// Sizes measured on the watch
// ============================================================================

/// The interpreter without its scratch room, which every open app holds whatever its size.
pub const WORKING_BYTES: u32 = 1_156;

/// Each expression's bindings: the state and system values it reads and how often it redraws.
pub const EXPRESSION_BINDING_BYTES: u32 = 16;

/// Each variable's place in the state.
pub const PLACEMENT_BYTES: u32 = 6;

/// One node of the open screen, as `size_of` gives it on the watch, plus its place in the list.
pub const NODE_BYTES: u32 = 132;

/// Each subscription the host keeps track of.
pub const SUBSCRIPTION_BYTES: u32 = 40;

/// What a label whose text is worked out while the app runs is taken to hold.
pub const WORKED_OUT_TEXT_BYTES: u32 = 16;

/// A TextFile's line places, the lines near its view and the buffer it reads into.
pub const TEXT_FILE_BYTES: u32 = 2_560;

/// A BitmapFile's picture, held while its screen is open.
pub const BITMAP_FILE_BYTES: u32 = rounded(crate::widgets::BITMAP_FILE_MOST_BYTES as u32);

/// The most bytes one `prefetch_file` reads ahead; an app that asks for one holds that much until a `file_bytes` takes it.
pub const PREFETCH_BYTES: u32 = 4096;

/// The most entries one folder listing holds; the rest of a larger folder is left out.
pub const MOST_FOLDER_ENTRIES: usize = 64;

/// The bytes a folder listing keeps for all its entries' names together.
pub const FOLDER_NAME_BYTES: usize = 1024;

/// The longest folder path a listing keeps, as long as the flash store's longest path.
pub const LONGEST_FOLDER_PATH_BYTES: usize = 64;

/// What one listed entry holds beside its name: its size, where its name starts, its name's length and whether it is a folder.
pub const FOLDER_ENTRY_BYTES: usize = 8;

/// A folder listing, which an app that calls `open_folder` holds while it is open.
pub const FOLDER_LISTING_BYTES: u32 = rounded(
    (LONGEST_FOLDER_PATH_BYTES + MOST_FOLDER_ENTRIES * FOLDER_ENTRY_BYTES + FOLDER_NAME_BYTES) as u32,
);

/// What drawing borrows from the heap for a moment, and the image checks when the app opens.
pub const PASSING_BYTES: u32 = 1_536;

/// Rows of a Repeat taken to be built when their number is worked out while the app runs.
pub const ASSUMED_ROWS: u32 = 8;

/// The heap the firmware has: 10 KB for the system and 20.25 KB for the UI; the flash store keeps its tables outside it.
pub const HEAP_BYTES: u32 = 30_976;

/// What the system holds on the heap while an app is open: notifications, file transfers and the shell.
pub const SYSTEM_BYTES: u32 = 4_096;

/// The most an open app may need, leaving the system its share.
pub const APP_HEAP_BUDGET_BYTES: u32 = HEAP_BYTES - SYSTEM_BYTES;

/// What one installed app costs the shell's full list of apps, beside its id and name.
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
    /// The host's working memory, its subscriptions, its saved state, what drawing borrows, a file part read ahead and a folder listing.
    pub working_bytes: u32,
    /// The nodes of its largest screen.
    pub widget_bytes: u32,
    /// The text those nodes hold.
    pub text_bytes: u32,
    /// The shared library's icons those nodes show, held while the screen is open.
    pub icon_bytes: u32,
    /// Everything above together.
    pub total_bytes: u32,
    /// Whether it shows the launcher's list of apps, which costs more for each app turned on.
    pub lists_apps: bool,
    /// Whether it reads the installed apps, packages or launcher groups, so the shell keeps its full list while it is open.
    pub reads_app_list: bool,
}

/// Works out an app's heap need from its image.
pub fn estimate_memory_need(image: &AppImage) -> MemoryNeed {
    let tree = WidgetTree::new(*image);
    let mut largest = ScreenCost::default();
    // The screen with the most icons may be another one, so taking the most of each is safe.
    let mut most_icons = 0;
    for root in tree.screens() {
        let cost = screen_cost(&tree, root);
        most_icons = most_icons.max(cost.icon_count);
        if cost.node_count > largest.node_count
            || (cost.node_count == largest.node_count && cost.text_bytes > largest.text_bytes)
        {
            largest = cost;
        }
    }

    let image_bytes = rounded(image.byte_count() as u32);
    let working_bytes = working_bytes_of(image);
    let widget_bytes = NODE_BYTES * largest.node_count;
    let text_bytes = largest.text_bytes;
    let icon_bytes = icons_held_bytes(most_icons);
    let reads = system_values_read(image);
    MemoryNeed {
        image_bytes,
        working_bytes,
        widget_bytes,
        text_bytes,
        icon_bytes,
        total_bytes: image_bytes + working_bytes + widget_bytes + text_bytes + icon_bytes,
        lists_apps: largest.has_app_list || reads & bit_of(SystemValue::InstalledAppCount) != 0,
        reads_app_list: reads_app_list_from(image, reads),
    }
}

/// The host's working memory, its subscriptions, its saved state and what drawing borrows.
fn working_bytes_of(image: &AppImage) -> u32 {
    WORKING_BYTES
        + sized_working_bytes(image)
        + SUBSCRIPTION_BYTES * image.subscription_count() as u32
        + rounded(saved_state_bytes(image) as u32)
        + PASSING_BYTES
        + if uses_effect(image, EffectKind::PrefetchFile) {
            PREFETCH_BYTES
        } else {
            0
        }
        + if uses_effect(image, EffectKind::OpenFolder) {
            FOLDER_LISTING_BYTES
        } else {
            0
        }
}

/// The working memory sized to the app: its bindings, its variables' places and bytes, and the interpreter's scratch room.
pub fn sized_working_bytes(image: &AppImage) -> u32 {
    EXPRESSION_BINDING_BYTES * image.expression_count() as u32
        + rounded(PLACEMENT_BYTES * image.state_count() as u32)
        + rounded(image.state_bytes() as u32)
        + rounded(scratch_bytes_for(image) as u32)
}

/// The heap a closed app needs while its subscriptions run: its image and the host's working memory, with no screen.
pub fn closed_run_bytes(image: &AppImage) -> u32 {
    rounded(image.byte_count() as u32) + working_bytes_of(image)
}

/// The system values only the shell's full list of apps answers.
const APP_LIST_VALUES: [SystemValue; 18] = [
    SystemValue::InstalledAppCount,
    SystemValue::InstalledAppName,
    SystemValue::InstalledAppId,
    SystemValue::InstalledAppIcon,
    SystemValue::PackageCount,
    SystemValue::PackageName,
    SystemValue::PackageId,
    SystemValue::PackageEnabled,
    SystemValue::PackageIsFace,
    SystemValue::PackageMemory,
    SystemValue::PackageRefusal,
    SystemValue::LauncherStyle,
    SystemValue::LauncherView,
    SystemValue::LauncherGroupCount,
    SystemValue::LauncherGroupName,
    SystemValue::LauncherGroupIcon,
    SystemValue::LauncherGroupFirst,
    SystemValue::LauncherGroupSize,
];

/// Whether the app reads the installed apps, packages or launcher groups, or shows an `AppList`.
///
/// The shell reads its full list of apps only while such an app is open.
pub fn reads_app_list(image: &AppImage) -> bool {
    reads_app_list_from(image, system_values_read(image))
}

fn reads_app_list_from(image: &AppImage, reads: u128) -> bool {
    let list_values = APP_LIST_VALUES
        .iter()
        .fold(0, |mask, &value| mask | bit_of(value));
    let tree = WidgetTree::new(*image);
    reads & list_values != 0
        || (0..tree.widget_count()).any(|index| {
            tree.widget(index)
                .is_some_and(|widget| widget.kind == WidgetKind::AppList)
        })
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

/// What an app costs the shell's full list of installed apps, kept only while an app that reads it is open.
pub fn listed_app_bytes(id: &str, name: &str) -> u32 {
    LISTED_APP_BYTES + rounded(id.len() as u32) + rounded(name.len() as u32)
}

/// What an app costs the shell's index of app ids, which every open app shares the heap with.
pub fn indexed_app_bytes(id: &str) -> u32 {
    id.len() as u32 + 1
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
    /// How many library icons its widgets may show at once.
    icon_count: u32,
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
        cost.icon_count = cost
            .icon_count
            .saturating_add(copies.saturating_mul(widget_library_icons(tree, &widget)));
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

/// How many of the shared library's icons one node of this widget may show at once.
fn widget_library_icons(tree: &WidgetTree, widget: &crate::widgets::Widget) -> u32 {
    match widget.kind {
        WidgetKind::StatusIcons => 3,
        // Only its charging bolt.
        WidgetKind::Battery => 1,
        // Every cell in view may be an app whose icon is a library icon; the cap on held icons bounds it.
        WidgetKind::AppList => u32::MAX,
        _ => {
            let source = widget
                .property(Property::Image)
                .or_else(|| widget.property(Property::Icon));
            match source {
                Some(PropertySource::Constant(value)) => names_library_icon(tree.image(), value) as u32,
                Some(PropertySource::Expression(_)) => 1,
                _ => 0,
            }
        }
    }
}

/// Whether an image value is a library icon, or an app's image that only names one.
fn names_library_icon(image: &AppImage, value: i32) -> bool {
    match ImageReference::from_value(value) {
        Some(ImageReference::App(index)) => image
            .resource_bytes(index)
            .and_then(library_image_named_by)
            .is_some(),
        Some(ImageReference::SharedLibrary(_) | ImageReference::InstalledApp(_)) => true,
        None => false,
    }
}

/// What the host holds for this many library icons, up to its cap.
fn icons_held_bytes(icon_count: u32) -> u32 {
    icon_count
        .saturating_mul(held_icon_bytes(LIBRARY_ICON_MOST_BYTES))
        .min(HELD_ICONS_MOST_BYTES)
}

/// The text one node of this widget holds.
fn widget_text_bytes(tree: &WidgetTree, widget: &crate::widgets::Widget) -> u32 {
    if widget.kind == WidgetKind::TextFile {
        return TEXT_FILE_BYTES;
    }
    if widget.kind == WidgetKind::BitmapFile {
        return BITMAP_FILE_BYTES;
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
    for_each_instruction(image, |instruction| {
        if let Instruction::ReadSystem(value) | Instruction::ReadSystemElement(value) = instruction {
            reads |= bit_of(value);
        }
    });
    reads
}

/// Whether any of the app's code asks for this effect, such as `prefetch_file` or `open_folder`.
fn uses_effect(image: &AppImage, kind: EffectKind) -> bool {
    let mut found = false;
    for_each_instruction(image, |instruction| {
        found |= instruction == Instruction::Effect(kind);
    });
    found
}

/// Gives every instruction of the image's expressions, handlers and functions to `visit`.
fn for_each_instruction(image: &AppImage, mut visit: impl FnMut(Instruction)) {
    let mut scan = |code: &[u8]| {
        let mut offset = 0;
        while let Ok((instruction, next)) = decode(code, offset) {
            visit(instruction);
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
}

/// A size rounded up to the heap's allocation step, and none for nothing.
const fn rounded(bytes: u32) -> u32 {
    bytes.div_ceil(ALLOCATION_STEP) * ALLOCATION_STEP
}
