use alloc::vec;

use crate::memory_need::*;
use crate::resources::ImageFormat;
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
fn a_repeat_builds_its_row_once_for_each_row() {
    let mut image = writer();
    image.widget(WidgetKind::Column, 1, &[]);
    image.widget(WidgetKind::Repeat, 1, &[(Property::RowCount, Constant(5))]);
    image.widget(WidgetKind::Row, 2, &[]);
    image.widget(WidgetKind::Label, 0, &[]);
    image.widget(WidgetKind::Label, 0, &[]);

    assert_eq!(need_of(&image).widget_bytes, (2 + 5 * 3) * NODE_BYTES);
}

#[test]
fn a_worked_out_row_count_is_taken_as_a_few_rows() {
    let mut image = writer();
    let count = image.expression(vec![]);
    image.widget(WidgetKind::Column, 1, &[]);
    image.widget(WidgetKind::Repeat, 1, &[(Property::RowCount, Expression(count))]);
    image.widget(WidgetKind::Label, 0, &[]);

    assert_eq!(need_of(&image).widget_bytes, (2 + ASSUMED_ROWS) * NODE_BYTES);
}

#[test]
fn down_a_list_only_the_rows_in_view_are_built() {
    let mut image = writer();
    image.widget(WidgetKind::List, 1, &[]);
    image.widget(WidgetKind::Repeat, 1, &[(Property::RowCount, Constant(100))]);
    image.widget(WidgetKind::Label, 0, &[(Property::Height, Constant(40))]);

    // Six rows fill the screen, and one more can peek in at each end.
    assert_eq!(need_of(&image).widget_bytes, (2 + 8) * NODE_BYTES);
}

#[test]
fn no_screen_counts_more_nodes_than_the_host_builds() {
    let mut image = writer();
    image.widget(WidgetKind::Column, 1, &[]);
    image.widget(WidgetKind::Repeat, 1, &[(Property::RowCount, Constant(1000))]);
    image.widget(WidgetKind::Label, 0, &[]);

    assert_eq!(
        need_of(&image).widget_bytes,
        crate::widgets::MOST_WIDGETS as u32 * NODE_BYTES
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
    assert_eq!(
        estimate_memory_need(&parsed).working_bytes,
        WORKING_BYTES + SUBSCRIPTION_BYTES + 24 + PASSING_BYTES
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
