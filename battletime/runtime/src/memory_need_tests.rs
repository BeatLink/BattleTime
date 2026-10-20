use alloc::vec;

use crate::library_icons::{
    held_icon_bytes, library_icon_bytes, HELD_ICONS_MOST_BYTES, LIBRARY_ICON_MOST_BYTES,
};
use crate::memory_need::*;
use crate::resources::{ImageFormat, ImageReference};
use crate::subscriptions::{Subscription, SubscriptionKind, SubscriptionValue};
use crate::widgets::{Property, PropertySource::*, WidgetKind};
use crate::writer::{CodeWriter, ImageWriter};
use crate::{instruction::Instruction, AppImage, AppKind, SystemValue, ValueType};

// ============================================================================
// Helpers
// ============================================================================

fn writer() -> ImageWriter {
    ImageWriter::new("sample", "Sample", AppKind::App, 1)
}

fn need_of(writer: &ImageWriter) -> MemoryNeed {
    let bytes = writer.build();
    estimate_memory_need(&AppImage::parse(&bytes).unwrap())
}

// ============================================================================
// The parts
// ============================================================================

#[test]
fn an_app_with_one_empty_screen_needs_its_image_and_the_working_memory() {
    let mut image = writer();
    image.widget(WidgetKind::Column, 0, &[]);
    let bytes = image.build();

    let need = estimate_memory_need(&AppImage::parse(&bytes).unwrap());

    assert_eq!(need.image_bytes, (bytes.len() as u32).div_ceil(4) * 4);
    assert_eq!(need.working_bytes, WORKING_BYTES + PASSING_BYTES);
    assert_eq!(need.widget_bytes, NODE_BYTES);
    assert_eq!(need.text_bytes, 0);
    assert_eq!(need.icon_bytes, 0);
    assert_eq!(
        need.total_bytes,
        need.image_bytes + need.working_bytes + need.widget_bytes + need.text_bytes
    );
    assert!(!need.lists_apps);
}

#[test]
fn the_largest_screen_decides_the_nodes() {
    let mut image = writer();
    image.widget(WidgetKind::Column, 1, &[]);
    image.widget(WidgetKind::Label, 0, &[]);
    image.widget(WidgetKind::Column, 3, &[]);
    for _ in 0..3 {
        image.widget(WidgetKind::Label, 0, &[]);
    }

    assert_eq!(need_of(&image).widget_bytes, 4 * NODE_BYTES);
}

#[test]
fn fixed_text_counts_its_length_and_worked_out_text_a_guess() {
    let mut image = writer();
    let hello = image.string("Hello there") as i32;
    let worked_out = image.expression(vec![]);
    image.widget(WidgetKind::Column, 4, &[]);
    image.widget(WidgetKind::Label, 0, &[(Property::Text, Constant(hello))]);
    image.widget(WidgetKind::Label, 0, &[(Property::Text, Expression(worked_out))]);
    image.widget(WidgetKind::TextFile, 0, &[]);
    image.widget(WidgetKind::BitmapFile, 0, &[]);

    assert_eq!(
        need_of(&image).text_bytes,
        12 + WORKED_OUT_TEXT_BYTES + TEXT_FILE_BYTES + BITMAP_FILE_BYTES
    );
}

#[test]
fn each_library_icon_shown_counts_at_its_size_and_an_apps_own_picture_does_not() {
    let mut image = writer();
    let own = image.image_resource(ImageFormat::Mask, 1, 1, &[255]) as i32;
    let named = image.library_image_reference(4) as i32;
    let library = ImageReference::SharedLibrary(7).to_value();
    let worked_out = image.expression(vec![]);
    image.widget(WidgetKind::Column, 5, &[]);
    image.widget(WidgetKind::Image, 0, &[(Property::Image, Constant(own))]);
    image.widget(WidgetKind::Image, 0, &[(Property::Image, Constant(named))]);
    image.widget(WidgetKind::Icon, 0, &[(Property::Icon, Constant(library))]);
    image.widget(WidgetKind::Icon, 0, &[(Property::Icon, Expression(worked_out))]);
    image.widget(WidgetKind::StatusIcons, 0, &[]);

    let need = need_of(&image);
    let held = |number| held_icon_bytes(library_icon_bytes(number));
    // The named icon and the library one at their sizes, the picked one at the most, and the three status icons.
    assert_eq!(
        need.icon_bytes,
        held(4) + held(7) + held_icon_bytes(LIBRARY_ICON_MOST_BYTES) + status_icons_bytes()
    );
    assert_eq!(
        need.total_bytes,
        need.image_bytes + need.working_bytes + need.widget_bytes + need.text_bytes + need.icon_bytes
    );
}

#[test]
fn a_paged_app_needs_its_resident_image_and_cache_or_what_opening_holds() {
    let mut image = writer();
    image.widget(WidgetKind::Column, 10, &[]);
    for _ in 0..10 {
        image.widget(WidgetKind::Label, 0, &[]);
    }
    for _ in 0..20 {
        let mut code = CodeWriter::new();
        for _ in 0..100 {
            code.emit(Instruction::PushInt(1));
        }
        image.handler(code.finish().unwrap());
    }
    let bytes = image.build();
    let parsed = AppImage::parse(&bytes).unwrap();
    let need = estimate_memory_need(&parsed);

    let whole = (bytes.len() as u32).div_ceil(4) * 4;
    let resident = (crate::paging::resident_bytes(&parsed) as u32).div_ceil(4) * 4;
    let cache = (crate::paging::cache_bytes(&parsed) as u32).div_ceil(4) * 4;
    assert!(need.pages_code);
    // One screen of a column and ten labels, each record 4 bytes.
    let window = 11 * 4;
    assert_eq!(
        need.image_bytes,
        resident + cache + CODE_CACHE_OVERHEAD_BYTES + window + SCREEN_WINDOW_OVERHEAD_BYTES + 8
    );
    assert_eq!(need.opening_bytes, whole + resident);
    let open = need.image_bytes + need.working_bytes + need.widget_bytes + need.text_bytes + need.icon_bytes;
    assert_eq!(
        need.total_bytes,
        open.max(need.opening_bytes + need.working_bytes)
    );
}

#[test]
fn an_image_a_value_picks_costs_no_library_icon_when_the_app_names_none() {
    let mut code = CodeWriter::new();
    code.emit(Instruction::PushSmall(1));
    let own_picture = code.finish().unwrap();
    let mut image = writer();
    image.image_resource(ImageFormat::Mask, 1, 1, &[255]);
    image.image_resource(ImageFormat::Mask, 1, 1, &[128]);
    let picked = image.expression(own_picture);
    image.widget(WidgetKind::Column, 1, &[]);
    image.widget(WidgetKind::Image, 0, &[(Property::Image, Expression(picked))]);
    assert_eq!(need_of(&image).icon_bytes, 0);

    // Naming a library icon anywhere in the code, such as a handler, makes the value count again.
    let mut code = CodeWriter::new();
    code.emit(Instruction::PushInt(ImageReference::SharedLibrary(3).to_value()))
        .emit(Instruction::Drop);
    image.handler(code.finish().unwrap());
    assert_eq!(
        need_of(&image).icon_bytes,
        held_icon_bytes(LIBRARY_ICON_MOST_BYTES)
    );
}

#[test]
fn reading_an_icon_system_value_or_starting_with_an_icon_counts_too() {
    let screen = |image: &mut ImageWriter, expression| {
        image.widget(WidgetKind::Column, 1, &[]);
        image.widget(WidgetKind::Icon, 0, &[(Property::Icon, Expression(expression))]);
    };
    let mut code = CodeWriter::new();
    code.emit(Instruction::PushSmall(0))
        .emit(Instruction::ReadSystemElement(SystemValue::NotificationIcon));
    let mut reads_system = writer();
    let expression = reads_system.expression(code.finish().unwrap());
    screen(&mut reads_system, expression);
    assert_eq!(
        need_of(&reads_system).icon_bytes,
        held_icon_bytes(LIBRARY_ICON_MOST_BYTES)
    );

    let mut starts_with_one = writer();
    let icon = ImageReference::SharedLibrary(5).to_value() as i64;
    starts_with_one.variable("icon", ValueType::Int, false, icon);
    let mut code = CodeWriter::new();
    code.emit(Instruction::ReadState(0));
    let expression = starts_with_one.expression(code.finish().unwrap());
    screen(&mut starts_with_one, expression);
    assert_eq!(
        need_of(&starts_with_one).icon_bytes,
        held_icon_bytes(LIBRARY_ICON_MOST_BYTES)
    );
}

#[test]
fn an_app_list_holds_icons_up_to_the_cap() {
    let mut image = writer();
    image.widget(WidgetKind::Column, 1, &[]);
    image.widget(WidgetKind::AppList, 0, &[]);

    assert_eq!(need_of(&image).icon_bytes, HELD_ICONS_MOST_BYTES);
}

#[test]
fn the_screen_with_the_most_icons_decides_them() {
    let mut image = writer();
    image.widget(WidgetKind::Column, 3, &[]);
    for _ in 0..3 {
        image.widget(WidgetKind::Label, 0, &[]);
    }
    image.widget(WidgetKind::Column, 1, &[]);
    image.widget(WidgetKind::StatusIcons, 0, &[]);

    let need = need_of(&image);
    assert_eq!(need.widget_bytes, 4 * NODE_BYTES);
    assert_eq!(need.icon_bytes, status_icons_bytes());
}

/// The notification, Bluetooth and alarm icons a `StatusIcons` shows, at their sizes.
fn status_icons_bytes() -> u32 {
    use crate::resources::library_icon::{ALARM, BLUETOOTH, NOTIFICATION};
    [NOTIFICATION, BLUETOOTH, ALARM]
        .map(|number| held_icon_bytes(library_icon_bytes(number)))
        .iter()
        .sum()
}

#[test]
fn a_node_that_keeps_an_extra_part_costs_that_too() {
    let mut image = writer();
    image.widget(WidgetKind::Column, 2, &[]);
    image.widget(WidgetKind::Label, 0, &[]);
    image.widget(WidgetKind::Button, 0, &[]);

    assert_eq!(need_of(&image).widget_bytes, 3 * NODE_BYTES + EXTRA_BYTES);
}

#[test]
fn a_node_whose_widget_sets_a_rare_setting_keeps_them_too() {
    let mut image = writer();
    image.widget(WidgetKind::Column, 2, &[]);
    image.widget(WidgetKind::Label, 0, &[(Property::Margin, Constant(4))]);
    image.widget(WidgetKind::Label, 0, &[(Property::Width, Constant(4))]);

    assert_eq!(need_of(&image).widget_bytes, 3 * NODE_BYTES + RARE_SETTINGS_BYTES);
}

#[test]
fn a_repeat_builds_its_row_once_for_each_row() {
    let mut image = writer();
    image.widget(WidgetKind::Column, 1, &[]);
    image.widget(WidgetKind::Repeat, 1, &[(Property::RowCount, Constant(5))]);
    image.widget(WidgetKind::Row, 2, &[]);
    image.widget(WidgetKind::Label, 0, &[]);
    image.widget(WidgetKind::Label, 0, &[]);

    // The Repeat keeps an extra part.
    assert_eq!(
        need_of(&image).widget_bytes,
        (2 + 5 * 3) * NODE_BYTES + EXTRA_BYTES
    );
}

#[test]
fn a_worked_out_row_count_is_taken_as_a_few_rows() {
    let mut image = writer();
    let count = image.expression(vec![]);
    image.widget(WidgetKind::Column, 1, &[]);
    image.widget(WidgetKind::Repeat, 1, &[(Property::RowCount, Expression(count))]);
    image.widget(WidgetKind::Label, 0, &[]);

    assert_eq!(
        need_of(&image).widget_bytes,
        (2 + ASSUMED_ROWS) * NODE_BYTES + EXTRA_BYTES
    );
}

#[test]
fn down_a_list_only_the_rows_in_view_are_built() {
    let mut image = writer();
    image.widget(WidgetKind::List, 1, &[]);
    image.widget(WidgetKind::Repeat, 1, &[(Property::RowCount, Constant(100))]);
    image.widget(WidgetKind::Label, 0, &[(Property::Height, Constant(40))]);

    // Six rows fill the screen, and one more can peek in at each end.
    assert_eq!(need_of(&image).widget_bytes, (2 + 8) * NODE_BYTES + EXTRA_BYTES);
}

#[test]
fn no_screen_counts_more_nodes_than_the_host_builds() {
    let mut image = writer();
    image.widget(WidgetKind::Column, 1, &[]);
    image.widget(WidgetKind::Repeat, 1, &[(Property::RowCount, Constant(1000))]);
    image.widget(WidgetKind::Label, 0, &[]);

    assert_eq!(
        need_of(&image).widget_bytes,
        crate::widgets::MOST_WIDGETS as u32 * NODE_BYTES + EXTRA_BYTES
    );
}

#[test]
fn subscriptions_and_saved_state_add_to_the_working_memory() {
    let mut image = writer();
    image.variable("count", ValueType::Int, true, 0);
    image.variable("scratch", ValueType::Int, false, 0);
    image.subscription(Subscription {
        kind: SubscriptionKind::Every,
        only_while_open: false,
        handler: 0,
        detail: 0,
        days: 0b111_1111,
        value: SubscriptionValue::Constant(1000),
    });
    image.handler(vec![]);
    image.widget(WidgetKind::Column, 0, &[]);
    let bytes = image.build();
    let parsed = AppImage::parse(&bytes).unwrap();

    // The header, then one entry: its header, the key "count" and eight bytes.
    assert_eq!(saved_state_bytes(&parsed), 6 + 5 + 5 + 8);
    // Two places of six bytes, two eight-byte variables and their scratch copies.
    let sized = 12 + 16 + 16;
    assert_eq!(sized_working_bytes(&parsed), sized);
    assert_eq!(
        estimate_memory_need(&parsed).working_bytes,
        WORKING_BYTES + sized + SUBSCRIPTION_BYTES + 24 + PASSING_BYTES
    );
}

#[test]
fn a_closed_run_needs_the_image_and_the_working_memory_but_no_screen() {
    let mut image = writer();
    image.handler(vec![]);
    image.widget(WidgetKind::Column, 1, &[]);
    image.widget(WidgetKind::Label, 0, &[]);
    let bytes = image.build();
    let parsed = AppImage::parse(&bytes).unwrap();

    let need = estimate_memory_need(&parsed);

    assert_eq!(closed_run_bytes(&parsed), need.image_bytes + need.working_bytes);
    assert!(closed_run_bytes(&parsed) < need.total_bytes);
}

#[test]
fn an_app_without_persistent_state_saves_nothing() {
    let mut image = writer();
    image.variable("scratch", ValueType::Int, false, 0);
    image.widget(WidgetKind::Column, 0, &[]);
    let bytes = image.build();

    assert_eq!(saved_state_bytes(&AppImage::parse(&bytes).unwrap()), 0);
}

// ============================================================================
// Lists of apps
// ============================================================================

#[test]
fn an_app_list_or_reading_the_installed_apps_marks_a_launcher() {
    let mut with_widget = writer();
    with_widget.widget(WidgetKind::AppList, 0, &[]);
    assert!(need_of(&with_widget).lists_apps);

    for code in [
        Instruction::ReadSystem(SystemValue::InstalledAppCount),
        Instruction::ReadSystemElement(SystemValue::InstalledAppCount),
    ] {
        let mut reading = writer();
        let mut writer_code = CodeWriter::new();
        writer_code.emit(Instruction::PushSmall(0)).emit(code);
        reading.expression(writer_code.finish().unwrap());
        reading.widget(WidgetKind::Column, 0, &[]);
        assert!(need_of(&reading).lists_apps, "{code:?}");
    }
}

#[test]
fn reading_packages_or_launcher_groups_reads_the_app_list_without_marking_a_launcher() {
    for value in [
        SystemValue::PackageCount,
        SystemValue::PackageName,
        SystemValue::LauncherGroupCount,
    ] {
        let mut reading = writer();
        let mut code = CodeWriter::new();
        code.emit(Instruction::ReadSystem(value));
        reading.expression(code.finish().unwrap());
        reading.widget(WidgetKind::Column, 0, &[]);
        let bytes = reading.build();
        let image = AppImage::parse(&bytes).unwrap();

        let need = estimate_memory_need(&image);

        assert!(need.reads_app_list, "{value:?}");
        assert!(reads_app_list(&image), "{value:?}");
        assert!(!need.lists_apps, "{value:?}");
    }
}

#[test]
fn an_app_list_on_any_screen_reads_the_app_list_and_other_apps_do_not() {
    let mut with_widget = writer();
    with_widget.widget(WidgetKind::Column, 0, &[]);
    with_widget.widget(WidgetKind::AppList, 0, &[]);
    assert!(need_of(&with_widget).reads_app_list);

    let mut plain = writer();
    plain.widget(WidgetKind::Column, 0, &[]);
    assert!(!need_of(&plain).reads_app_list);
    assert_eq!(indexed_app_bytes("timer"), 6);
}

#[test]
fn code_in_handlers_and_functions_is_read_too() {
    let mut code = CodeWriter::new();
    code.emit(Instruction::ReadSystem(SystemValue::InstalledAppCount))
        .emit(Instruction::Drop);
    let code = code.finish().unwrap();

    let mut in_handler = writer();
    in_handler.handler(code.clone());
    in_handler.widget(WidgetKind::Column, 0, &[]);
    assert!(need_of(&in_handler).lists_apps);

    let mut in_function = writer();
    in_function.function(0, 0, 0, code);
    in_function.widget(WidgetKind::Column, 0, &[]);
    assert!(need_of(&in_function).lists_apps);
}

#[test]
fn the_launcher_share_is_the_icon_and_the_name() {
    let mut image = writer();
    image.set_has_icon();
    image.image_resource(ImageFormat::Mask, 4, 4, &[0; 16]);
    image.widget(WidgetKind::Column, 0, &[]);
    let bytes = image.build();
    let parsed = AppImage::parse(&bytes).unwrap();
    let icon = parsed.resource_bytes(0).unwrap().len() as u32;

    assert_eq!(
        launcher_share_bytes(&parsed),
        icon.div_ceil(4) * 4 + 8 + "Sample".len() as u32 + 1
    );

    let mut no_icon = writer();
    no_icon.widget(WidgetKind::Column, 0, &[]);
    let bytes = no_icon.build();
    assert_eq!(launcher_share_bytes(&AppImage::parse(&bytes).unwrap()), 8 + 6 + 1);
}

#[test]
fn a_listed_app_costs_its_entry_and_its_words() {
    assert_eq!(listed_app_bytes("timer", "Timer"), LISTED_APP_BYTES + 8 + 8);
}

#[test]
fn the_budget_leaves_the_system_its_share() {
    assert_eq!(APP_HEAP_BUDGET_BYTES + SYSTEM_BYTES, HEAP_BYTES);
}

#[test]
fn an_app_that_reads_ahead_holds_room_for_one_prefetch() {
    let mut plain = writer();
    plain.widget(WidgetKind::Column, 0, &[]);
    let mut reads_ahead = writer();
    reads_ahead.widget(WidgetKind::Column, 0, &[]);
    let mut code = CodeWriter::new();
    code.emit(Instruction::PushSmall(0));
    code.emit(Instruction::PushSmall(0));
    code.emit(Instruction::PushSmall(0));
    code.emit(Instruction::Effect(crate::EffectKind::PrefetchFile));
    reads_ahead.handler(code.finish().unwrap());

    assert_eq!(
        need_of(&reads_ahead).working_bytes,
        need_of(&plain).working_bytes + PREFETCH_BYTES
    );
}

#[test]
fn an_app_that_opens_folders_holds_room_for_one_listing() {
    let mut plain = writer();
    plain.widget(WidgetKind::Column, 0, &[]);
    let mut lists_folders = writer();
    lists_folders.widget(WidgetKind::Column, 0, &[]);
    let mut code = CodeWriter::new();
    code.emit(Instruction::PushSmall(0));
    code.emit(Instruction::Effect(crate::EffectKind::OpenFolder));
    lists_folders.handler(code.finish().unwrap());

    assert_eq!(
        need_of(&lists_folders).working_bytes,
        need_of(&plain).working_bytes + FOLDER_LISTING_BYTES
    );
    assert_eq!(FOLDER_LISTING_BYTES, 1_600);
}
