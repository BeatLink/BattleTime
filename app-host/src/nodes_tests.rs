use super::*;

// ============================================================================
// Helpers
// ============================================================================

const SCREEN: Rect = Rect {
    x: 0,
    y: 0,
    width: 240,
    height: 240,
};

const RED: i64 = 0xF800;

fn node(kind: WidgetKind) -> Node {
    Node::new(0, kind, 1, None, SCREEN)
}

fn theme_value(color: ThemeColor) -> i64 {
    ColorValue::Theme(color).to_value()
}

fn number(value: i64) -> PropertyValue<'static> {
    PropertyValue::Number(value)
}

// ============================================================================
// Property values
// ============================================================================

#[test]
fn align_reads_start_center_and_end_and_nothing_else() {
    assert_eq!(Align::from_value(0), Some(Align::Start));
    assert_eq!(Align::from_value(1), Some(Align::Center));
    assert_eq!(Align::from_value(2), Some(Align::End));
    assert_eq!(Align::from_value(3), None);
    assert_eq!(Align::from_value(-1), None);
}

#[test]
fn align_puts_a_thing_at_the_start_middle_or_end_of_its_space() {
    assert_eq!(Align::Start.offset(100, 30), 0);
    assert_eq!(Align::Center.offset(100, 30), 35);
    assert_eq!(Align::End.offset(100, 30), 70);
}

#[test]
fn placement_reads_its_five_values_and_nothing_else() {
    let read: [Option<Placement>; 6] = core::array::from_fn(|value| Placement::from_value(value as i32));

    assert_eq!(
        read,
        [
            Some(Placement::Start),
            Some(Placement::Center),
            Some(Placement::End),
            Some(Placement::SpaceBetween),
            Some(Placement::Stretch),
            None,
        ]
    );
}

#[test]
fn long_mode_reads_scroll_and_clip_and_wraps_otherwise() {
    assert_eq!(LongMode::from_value(0), LongMode::Wrap);
    assert_eq!(LongMode::from_value(1), LongMode::Scroll);
    assert_eq!(LongMode::from_value(2), LongMode::Clip);
    assert_eq!(LongMode::from_value(9), LongMode::Wrap);
}

#[test]
fn a_negative_length_is_a_share_of_the_whole() {
    assert_eq!(Length::from_value(-50), Length::Percent(50));
    assert_eq!(Length::from_value(-50).of(240), 120);
    assert_eq!(Length::from_value(30).of(240), 30);
    assert_eq!(Length::from_value(i32::MIN), Length::Percent(i32::MAX));
}

// ============================================================================
// Kinds with extra properties
// ============================================================================

#[test]
fn each_kind_starts_with_its_own_extra_properties_empty() {
    assert!(matches!(
        node(WidgetKind::CellGrid).extra,
        Extra::CellGrid {
            rows: 0,
            palette: None,
            ..
        }
    ));
    assert!(matches!(
        node(WidgetKind::Scale).extra,
        Extra::Scale {
            ticks: 0,
            labels: None,
            ..
        }
    ));
    assert!(matches!(
        node(WidgetKind::AnalogClock).extra,
        Extra::AnalogClock {
            time: 0,
            show_seconds: false,
            ..
        }
    ));
    assert!(matches!(
        node(WidgetKind::Button).extra,
        Extra::Image {
            reference: None,
            recolor: None
        }
    ));
    assert_eq!(node(WidgetKind::Label).extra, Extra::None);
}

#[test]
fn a_cell_grid_stores_its_rows_cells_shape_and_palette() {
    let mut grid = node(WidgetKind::CellGrid);

    let changed = [
        grid.apply(Property::Rows, number(8)),
        grid.apply(Property::CellSize, number(12)),
        grid.apply(Property::CellGap, number(2)),
        grid.apply(Property::CellShape, number(7)),
        grid.apply(Property::Palette, number(3)),
    ];

    assert_eq!(changed, [true; 5]);
    assert_eq!(
        grid.extra,
        Extra::CellGrid {
            rows: 8,
            cell_size: 12,
            cell_gap: 2,
            cell_shape: 2,
            palette: Some(3),
        }
    );
}

#[test]
fn a_cell_grid_passes_common_properties_on_to_its_settings() {
    let mut grid = node(WidgetKind::CellGrid);

    let changed = grid.apply(Property::Columns, number(5));

    assert!(changed);
    assert_eq!(grid.settings.columns, 5);
}

#[test]
fn a_scale_stores_its_ticks_labels_and_colours() {
    let mut scale = node(WidgetKind::Scale);

    scale.apply(Property::Ticks, number(60));
    scale.apply(Property::MajorEvery, number(5));
    scale.apply(Property::Skip, number(4));
    scale.apply(Property::Labels, number(6));
    scale.apply(Property::NeedleColor, number(RED));
    scale.apply(Property::MajorColor, number(theme_value(ThemeColor::Accent)));

    assert_eq!(
        scale.extra,
        Extra::Scale {
            ticks: 60,
            major_every: 5,
            skip: Some(4),
            labels: Some(6),
            needle_color: Some(ColorValue::Fixed(RED as u16)),
            major_color: Some(ColorValue::Theme(ThemeColor::Accent)),
        }
    );
}

#[test]
fn a_scale_ignores_a_needle_colour_that_is_not_a_colour() {
    let mut scale = node(WidgetKind::Scale);

    let changed = scale.apply(Property::NeedleColor, number(-5));

    assert!(!changed);
    assert!(matches!(
        scale.extra,
        Extra::Scale {
            needle_color: None,
            ..
        }
    ));
}

#[test]
fn an_analog_clock_stores_its_time_hands_and_colours() {
    let mut clock = node(WidgetKind::AnalogClock);

    clock.apply(Property::Time, number(1_000_000));
    clock.apply(Property::ShowSeconds, number(1));
    clock.apply(Property::HandStyle, number(-3));
    clock.apply(Property::HourColor, number(RED));
    clock.apply(Property::MinuteColor, number(theme_value(ThemeColor::Text)));
    clock.apply(Property::SecondColor, number(theme_value(ThemeColor::Danger)));
    clock.apply(Property::HourLength, number(50));
    clock.apply(Property::MinuteLength, number(80));
    clock.apply(Property::SecondLength, number(100_000));

    assert_eq!(
        clock.extra,
        Extra::AnalogClock {
            time: 1_000_000,
            show_seconds: true,
            hand_style: 0,
            hour_color: Some(ColorValue::Fixed(RED as u16)),
            minute_color: Some(ColorValue::Theme(ThemeColor::Text)),
            second_color: Some(ColorValue::Theme(ThemeColor::Danger)),
            hour_length: Some(50),
            minute_length: Some(80),
            second_length: Some(u16::MAX),
        }
    );
}

#[test]
fn an_image_stores_its_picture_and_recolour() {
    let mut image = node(WidgetKind::Image);

    let picture_changed = image.apply(Property::Image, number(2));
    let recolor_changed = image.apply(Property::Recolor, number(RED));
    let bad_recolor_changed = image.apply(Property::Recolor, number(-1));

    assert!(picture_changed && recolor_changed && !bad_recolor_changed);
    assert_eq!(
        image.extra,
        Extra::Image {
            reference: Some(2),
            recolor: Some(ColorValue::Fixed(RED as u16)),
        }
    );
}

// ============================================================================
// Common properties
// ============================================================================

#[test]
fn position_and_spacing_properties_land_in_the_settings() {
    let mut column = node(WidgetKind::Column);

    column.apply(Property::X, number(10));
    column.apply(Property::Y, number(-20));
    column.apply(Property::Gap, number(-4));
    column.apply(Property::BorderWidth, number(3));
    column.apply(Property::Radius, number(-8));

    let settings = column.settings;
    assert_eq!((settings.x, settings.y), (Some(10), Some(-20)));
    assert_eq!(settings.gap, 0);
    assert_eq!(settings.border_width, Some(3));
    assert_eq!(settings.radius, Some(0));
}

#[test]
fn a_border_colour_must_be_a_colour() {
    let mut column = node(WidgetKind::Column);

    let bad = column.apply(Property::BorderColor, number(-1));
    let good = column.apply(Property::BorderColor, number(theme_value(ThemeColor::Muted)));

    assert!(!bad && good);
    assert_eq!(
        column.settings.border_color,
        Some(ColorValue::Theme(ThemeColor::Muted))
    );
}

#[test]
fn flow_properties_fall_back_when_out_of_range() {
    let mut row = node(WidgetKind::Row);

    row.apply(Property::Justify, number(3));
    row.apply(Property::AlignItems, number(1));
    row.apply(Property::LongMode, number(2));
    let justify_after_space_between = row.settings.justify;
    row.apply(Property::Justify, number(42));
    row.apply(Property::AlignItems, number(42));

    assert_eq!(justify_after_space_between, Placement::SpaceBetween);
    assert_eq!(row.settings.justify, Placement::Start);
    assert_eq!(row.settings.align_items, None);
    assert_eq!(row.settings.long_mode, LongMode::Clip);
}

#[test]
fn value_range_disabled_and_opacity_land_in_the_settings() {
    let mut slider = node(WidgetKind::Slider);

    slider.apply(Property::Minimum, number(-10));
    slider.apply(Property::Maximum, number(10));
    slider.apply(Property::Value, number(i64::MAX));
    slider.apply(Property::Disabled, number(1));
    slider.apply(Property::Opacity, number(1_000));

    let settings = slider.settings;
    assert_eq!((settings.minimum, settings.maximum), (-10, 10));
    assert_eq!(settings.value, i32::MAX);
    assert!(settings.disabled);
    assert_eq!(settings.opacity, OPAQUE);
}

#[test]
fn an_array_property_names_a_state_variable_only_if_it_fits_in_a_byte() {
    let mut chart = node(WidgetKind::Chart);

    chart.apply(Property::Points, number(7));
    let named = chart.settings.array;
    chart.apply(Property::Series, number(300));

    assert_eq!(named, Some(7));
    assert_eq!(chart.settings.array, None);
}

#[test]
fn setting_the_same_value_again_changes_nothing() {
    let mut label = node(WidgetKind::Label);

    let first = label.apply(Property::Padding, number(6));
    let second = label.apply(Property::Padding, number(6));

    assert!(first);
    assert!(!second);
}

#[test]
fn a_property_the_widget_lacks_changes_nothing() {
    let mut label = node(WidgetKind::Label);

    let ticks_changed = label.apply(Property::Ticks, number(12));
    let handler_changed = label.apply(Property::OnClick, number(0));

    assert!(!ticks_changed && !handler_changed);
    assert_eq!(label.settings, Settings::default());
}

#[test]
fn text_is_stored_only_for_the_text_property_and_only_when_it_differs() {
    let mut label = node(WidgetKind::Label);

    let first = label.apply(Property::Text, PropertyValue::Text("Hello"));
    let again = label.apply(Property::Text, PropertyValue::Text("Hello"));
    let wrong_property = label.apply(Property::Width, PropertyValue::Text("Bye"));

    assert!(first && !again && !wrong_property);
    assert_eq!(label.text(), "Hello");
}

// ============================================================================
// Colours
// ============================================================================

#[test]
fn rgb565_white_and_black_spread_to_full_and_empty_channels() {
    assert_eq!(to_color(0xFFFF), Color(255, 255, 255, OPAQUE));
    assert_eq!(to_color(0x0000), Color(0, 0, 0, OPAQUE));
    assert_eq!(to_color(0xF800), Color(255, 0, 0, OPAQUE));
}

#[test]
fn an_unset_colour_resolves_to_the_fallback_or_to_clear() {
    let theme = neotime_runtime::DEFAULT_THEME;
    let surface = to_color(theme.color(ThemeColor::Surface));

    assert_eq!(resolve(None, ThemeColor::Surface, &theme), surface);
    assert_eq!(resolve_or_clear(None, &theme), TRANSPARENT);
    assert_eq!(
        resolve_or_clear(Some(ColorValue::Fixed(0xFFFF)), &theme),
        Color(255, 255, 255, OPAQUE)
    );
}

#[test]
fn buttons_paint_the_surface_and_indicators_paint_no_background_of_their_own() {
    let theme = neotime_runtime::DEFAULT_THEME;
    let mut bar = node(WidgetKind::Bar);
    bar.apply(Property::Background, number(RED));
    let mut label = node(WidgetKind::Label);
    let unpainted = label.own_background(&theme);
    label.apply(Property::Background, number(RED));

    assert_eq!(
        node(WidgetKind::Button).own_background(&theme),
        Some(to_color(theme.color(ThemeColor::Surface)))
    );
    assert_eq!(bar.own_background(&theme), None);
    assert_eq!(unpainted, None);
    assert_eq!(label.own_background(&theme), Some(to_color(RED as u16)));
}

// ============================================================================
// Areas and arrays
// ============================================================================

#[test]
fn a_hidden_widget_covers_nothing() {
    let label = node(WidgetKind::Label);

    assert_eq!(label.covered_area(), None);
}

#[test]
fn a_shown_widget_covers_its_box_cut_to_its_clip() {
    let mut label = node(WidgetKind::Label);
    label.shown = true;
    label.bounds = Rect {
        x: 200,
        y: 10,
        width: 100,
        height: 20,
    };

    let area = label.covered_area();

    assert_eq!(
        area,
        Some(Rect {
            x: 200,
            y: 10,
            width: 40,
            height: 20,
        })
    );
}

#[test]
fn an_open_dropdown_covers_its_list_below_its_box() {
    let mut dropdown = node(WidgetKind::Dropdown);
    dropdown.shown = true;
    dropdown.option_count = 3;
    dropdown.bounds = Rect {
        x: 0,
        y: 0,
        width: 100,
        height: 40,
    };
    let closed = dropdown.covered_area().unwrap();

    dropdown.is_open = true;
    let open = dropdown.covered_area().unwrap();

    assert_eq!(closed.height, 40);
    assert!(open.height > closed.height);
    assert_eq!((open.x, open.y, open.width), (0, 0, 100));
}

#[test]
fn a_cell_grid_reads_its_cells_and_its_palette() {
    let mut grid = node(WidgetKind::CellGrid);
    grid.apply(Property::Cells, number(1));
    grid.apply(Property::Palette, number(4));

    assert_eq!(grid.arrays_read(), (1 << 1) | (1 << 4));
}

#[test]
fn a_scale_reads_its_skip_and_labels_arrays() {
    let mut scale = node(WidgetKind::Scale);
    scale.apply(Property::Skip, number(2));
    scale.apply(Property::Labels, number(5));

    assert_eq!(scale.arrays_read(), (1 << 2) | (1 << 5));
}

#[test]
fn arrays_past_the_sixty_fourth_variable_are_not_tracked() {
    let mut chart = node(WidgetKind::Chart);
    chart.apply(Property::Points, number(64));

    assert_eq!(chart.arrays_read(), 0);
    assert_eq!(node(WidgetKind::Label).arrays_read(), 0);
}

// ============================================================================
// Helpers for drawing
// ============================================================================

#[test]
fn insetting_shrinks_every_side_and_never_goes_negative() {
    let shrunk = inset(SCREEN, 10);
    let crushed = inset(SCREEN, 200);

    assert_eq!(
        shrunk,
        Rect {
            x: 10,
            y: 10,
            width: 220,
            height: 220,
        }
    );
    assert_eq!((crushed.width, crushed.height), (0, 0));
}

#[test]
fn containers_and_controls_are_mapped_but_pages_are_not() {
    assert!(is_mapped(WidgetKind::Column));
    assert!(is_mapped(WidgetKind::Dropdown));
    assert!(!is_mapped(WidgetKind::Pages));
    assert!(!is_mapped(WidgetKind::Tabs));
}
