//! Tests for each part of the language: each compiles a small app, checks the image with the
//! runtime's own checks, and runs its expressions and handlers with the runtime's interpreter.

use neotime_runtime::instruction::EffectKind;
use neotime_runtime::resources::ImageReference;
use neotime_runtime::theme::rgb565;
use neotime_runtime::{
    AppImage, AppState, ColorValue, Interpreter, Property, PropertySource, SystemValue, SystemValues,
    ThemeColor, Value, WidgetKind, WidgetTree,
};

use crate::compile_source;
use crate::diagnostics::Severity;

// ============================================================================
// Helpers
// ============================================================================

const HEADER: &str = r#"
import {
  computed, signal, define_app, format_duration, format_number, format_time, now, persisted, random_int, range, vibrate,
  vibrate_pattern, go, start_of_minute, start_of_hour, text_from_codes, join, to_upper, text_length,
  slice_text, pad_start, next_time_of_day, count_where, HOUR, MINUTE, SECOND, type Duration, type Instant, type SwipeEvent, type Color,
} from "neotime";
import { icons } from "neotime/icons";
import { Button, Column, Icon, Label, Row, Screen } from "neotime/widgets";
"#;

const FOOTER: &str = r#"
export default define_app({ id: "test", name: "Test", screens: { main: Main } });
"#;

/// A whole app made of the header, the code given and the footer.
fn app(body: &str) -> String {
    format!("{HEADER}\n{body}\n{FOOTER}")
}

fn compile(source: &str) -> Vec<u8> {
    let output = compile_source(source);
    let errors: Vec<String> = output
        .errors()
        .map(|error| error.render("test.tsx", source))
        .collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    output.image.expect("an image")
}

fn first_error(source: &str) -> String {
    let output = compile_source(source);
    let error = output.errors().next().expect("an error");
    error.message.clone()
}

struct FakeSystem {
    now: Value,
}

impl SystemValues for FakeSystem {
    fn read(&self, value: SystemValue) -> Value {
        match value {
            SystemValue::Now => self.now,
            SystemValue::Uses24HourClock => 1,
            SystemValue::UsesMetricUnits => 1,
            SystemValue::StartedAt => self.now - 90_000,
            _ => 0,
        }
    }

    fn read_text(&self, value: SystemValue) -> &str {
        match value {
            SystemValue::FirmwareVersion => "0.1.0+1b19aed",
            _ => "",
        }
    }

    fn random_number(&self) -> u32 {
        7
    }
}

const SYSTEM: FakeSystem = FakeSystem { now: 0 };

/// Every widget in the order the image stores them.
fn widgets(image: &AppImage) -> Vec<(WidgetKind, Vec<(Property, PropertySource)>)> {
    let tree = WidgetTree::new(*image);
    (0..tree.widget_count())
        .map(|index| {
            let widget = tree.widget(index).expect("a widget");
            let properties = widget
                .properties()
                .map(|entry| entry.expect("a property"))
                .map(|entry| (entry.property, entry.source))
                .collect();
            (widget.kind, properties)
        })
        .collect()
}

/// The source of a property on the first widget of a kind that has it.
fn property_of(image: &AppImage, kind: WidgetKind, wanted: Property, nth: usize) -> PropertySource {
    widgets(image)
        .into_iter()
        .filter(|(widget_kind, _)| *widget_kind == kind)
        .filter_map(|(_, properties)| {
            properties
                .into_iter()
                .find(|(property, _)| *property == wanted)
                .map(|(_, source)| source)
        })
        .nth(nth)
        .unwrap_or_else(|| panic!("no {kind:?} with {wanted:?}"))
}

/// What a property shows now: a constant, or its expression worked out.
fn value_of(image: &AppImage, state: &AppState, source: PropertySource, system: &FakeSystem) -> Value {
    match source {
        PropertySource::Constant(value) => value as Value,
        PropertySource::Expression(expression) => Interpreter::new()
            .evaluate_expression(image, expression, state, system)
            .expect("the expression runs"),
        PropertySource::Handler(_) => panic!("a handler is not a value"),
    }
}

/// The text a property shows now.
fn text_of(image: &AppImage, state: &AppState, source: PropertySource, system: &FakeSystem) -> String {
    let mut interpreter = Interpreter::new();
    match source {
        PropertySource::Constant(string) => image.string(string as u16).expect("a string").to_string(),
        PropertySource::Expression(expression) => {
            let value = interpreter
                .evaluate_expression(image, expression, state, system)
                .expect("the expression runs");
            interpreter.text(image, value).expect("text").to_string()
        }
        PropertySource::Handler(_) => panic!("a handler is not text"),
    }
}

fn label_text(image: &AppImage, state: &AppState, nth: usize) -> String {
    text_of(
        image,
        state,
        property_of(image, WidgetKind::Label, Property::Text, nth),
        &SYSTEM,
    )
}

fn run(image: &AppImage, state: &mut AppState, kind: WidgetKind, event: Property, event_fields: &[Value]) {
    let PropertySource::Handler(handler) = property_of(image, kind, event, 0) else {
        panic!("not a handler");
    };
    Interpreter::new()
        .run_handler(image, handler, state, event_fields, &SYSTEM)
        .expect("the handler runs");
}

fn variable_named(image: &AppImage, key: &str) -> u8 {
    (0..image.state_count())
        .find(|&index| {
            let variable = image.state_variable(index).expect("a variable");
            image.string(variable.key_string) == Some(key)
        })
        .unwrap_or_else(|| panic!("no variable {key}")) as u8
}

// ============================================================================
// Widgets and text
// ============================================================================

#[test]
fn literal_text_becomes_a_constant_string() {
    let image = compile(&app(
        "function Main() { return <Screen><Label>Hello, world</Label></Screen>; }",
    ));
    let image = AppImage::parse(&image).unwrap();
    let state = AppState::new(&image).unwrap();

    assert_eq!(label_text(&image, &state, 0), "Hello, world");
    assert!(matches!(
        property_of(&image, WidgetKind::Label, Property::Text, 0),
        PropertySource::Constant(_)
    ));
}

#[test]
fn the_screen_root_carries_its_name() {
    let image = compile(&app(
        "function Main() { return <Screen><Label>Hi</Label></Screen>; }",
    ));
    let image = AppImage::parse(&image).unwrap();

    assert_eq!(WidgetTree::new(image).screen_named("main"), Some(0));
}

#[test]
fn template_literals_join_text_and_numbers() {
    let image = compile(&app(
        "const count = signal(3);\nfunction Main() { return <Screen><Label>{`${count.value} steps`}</Label></Screen>; }",
    ));
    let image = AppImage::parse(&image).unwrap();
    let state = AppState::new(&image).unwrap();

    assert_eq!(label_text(&image, &state, 0), "3 steps");
}

#[test]
fn numbers_in_jsx_text_are_shown_as_text() {
    let image = compile(&app(
        "const count = signal(-12);\nfunction Main() { return <Screen><Label>Count: {count.value}</Label></Screen>; }",
    ));
    let image = AppImage::parse(&image).unwrap();
    let state = AppState::new(&image).unwrap();

    assert_eq!(label_text(&image, &state, 0), "Count: -12");
}

#[test]
fn a_condition_around_a_widget_sets_its_visibility() {
    let image = compile(&app(r#"const shown = signal(false);
        function Main() {
          return (
            <Screen>
              {shown.value && <Label>Hi</Label>}
              <Button onClick={() => { shown.value = !shown.value; }}>Toggle</Button>
            </Screen>
          );
        }"#));
    let image = AppImage::parse(&image).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let visible = property_of(&image, WidgetKind::Label, Property::Visible, 0);

    assert_eq!(value_of(&image, &state, visible, &SYSTEM), 0);
    run(&image, &mut state, WidgetKind::Button, Property::OnClick, &[0, 0]);
    assert_eq!(value_of(&image, &state, visible, &SYSTEM), 1);
}

#[test]
fn a_ternary_between_widgets_shows_one_or_the_other() {
    let image = compile(&app(r#"const on = signal(true);
        function Main() { return <Screen>{on.value ? <Label>On</Label> : <Label>Off</Label>}</Screen>; }"#));
    let image = AppImage::parse(&image).unwrap();
    let state = AppState::new(&image).unwrap();

    let first = property_of(&image, WidgetKind::Label, Property::Visible, 0);
    let second = property_of(&image, WidgetKind::Label, Property::Visible, 1);
    assert_eq!(value_of(&image, &state, first, &SYSTEM), 1);
    assert_eq!(value_of(&image, &state, second, &SYSTEM), 0);
}

#[test]
fn components_are_inlined_with_their_props() {
    let image = compile(&app(
        r#"function Place(props: { place: number }) { return <Label>{`#${props.place + 1}`}</Label>; }
        function Main() { return <Screen><Place place={0} /><Place place={1} /></Screen>; }"#,
    ));
    let image = AppImage::parse(&image).unwrap();
    let state = AppState::new(&image).unwrap();

    assert_eq!(label_text(&image, &state, 0), "#1");
    assert_eq!(label_text(&image, &state, 1), "#2");
}

#[test]
fn map_over_a_constant_array_repeats_widgets() {
    let image = compile(&app(r#"const NAMES = ["a", "b", "c"];
        function Main() { return <Screen>{NAMES.map((name) => <Label>{name}</Label>)}</Screen>; }"#));
    let image = AppImage::parse(&image).unwrap();
    let state = AppState::new(&image).unwrap();

    assert_eq!(label_text(&image, &state, 2), "c");
    assert_eq!(widgets(&image).len(), 4);
}

#[test]
fn colours_become_theme_references_or_rgb565() {
    let image = compile(&app(
        r##"function Main() { return <Screen><Label color="accent" background="#ff0000">Hi</Label></Screen>; }"##,
    ));
    let image = AppImage::parse(&image).unwrap();

    assert_eq!(
        property_of(&image, WidgetKind::Label, Property::Color, 0),
        PropertySource::Constant(ColorValue::Theme(ThemeColor::Accent).to_value() as i32)
    );
    assert_eq!(
        property_of(&image, WidgetKind::Label, Property::Background, 0),
        PropertySource::Constant(rgb565(255, 0, 0) as i32)
    );
}

#[test]
fn fonts_icons_and_lengths_use_their_number_mappings() {
    let image = compile(&app(
        r#"function Main() { return <Screen><Label font="huge" width="100%">Hi</Label><Icon icon={icons.heart} /></Screen>; }"#,
    ));
    let image = AppImage::parse(&image).unwrap();

    assert_eq!(
        property_of(&image, WidgetKind::Label, Property::Font, 0),
        PropertySource::Constant(4)
    );
    assert_eq!(
        property_of(&image, WidgetKind::Label, Property::Width, 0),
        PropertySource::Constant(-100)
    );
    assert_eq!(
        property_of(&image, WidgetKind::Icon, Property::Icon, 0),
        PropertySource::Constant(ImageReference::SharedLibrary(18).to_value())
    );
}

#[test]
fn a_font_chosen_at_run_time_is_an_expression() {
    let image = compile(&app(r#"const big = signal(true);
        function Main() { return <Screen><Label font={big.value ? "large" : "small"}>Hi</Label></Screen>; }"#));
    let image = AppImage::parse(&image).unwrap();
    let state = AppState::new(&image).unwrap();
    let font = property_of(&image, WidgetKind::Label, Property::Font, 0);

    assert_eq!(value_of(&image, &state, font, &SYSTEM), 3);
}

#[test]
fn props_the_format_has_no_place_for_are_left_out_with_a_warning() {
    let source =
        app(r#"function Main() { return <Screen><Row><Label wobble={3}>Hi</Label></Row></Screen>; }"#);
    let output = compile_source(&source);

    assert!(output.image.is_some());
    assert!(output
        .warnings()
        .any(|warning| warning.message.contains("`wobble`")));
}

#[test]
fn text_align_and_icon_size_map_to_their_properties() {
    let image = compile(&app(r#"import { Icon, Label } from "neotime/widgets";
        import { icons } from "neotime/icons";
        function Main() { return <Screen><Label text_align="right">Hi</Label><Icon size={24} icon={icons.heart} /></Screen>; }"#));
    let image = AppImage::parse(&image).unwrap();

    assert_eq!(
        property_of(&image, WidgetKind::Label, Property::Align, 0),
        PropertySource::Constant(2)
    );
    assert_eq!(
        property_of(&image, WidgetKind::Icon, Property::Width, 0),
        PropertySource::Constant(24)
    );
    assert_eq!(
        property_of(&image, WidgetKind::Icon, Property::Height, 0),
        PropertySource::Constant(24)
    );
}

#[test]
fn the_scrolling_props_make_one_set_of_flags() {
    use neotime_runtime::widgets::{SCROLLS_ACROSS, SCROLLS_BY_PAGES, SCROLLS_WITHOUT_BAR};
    let image = compile(&app(r#"import { List, Scroll, TextFile } from "neotime/widgets";
        function Main() { return <Screen>
            <List direction="row" scrolling="pages" scroll_bar={false} />
            <Scroll direction="column" scrolling="smooth" />
            <TextFile path="/notes/note.txt" scrolling="pages" />
        </Screen>; }"#));
    let image = AppImage::parse(&image).unwrap();

    assert_eq!(
        property_of(&image, WidgetKind::List, Property::Scrolling, 0),
        PropertySource::Constant(SCROLLS_ACROSS | SCROLLS_BY_PAGES | SCROLLS_WITHOUT_BAR)
    );
    assert_eq!(
        property_of(&image, WidgetKind::TextFile, Property::Scrolling, 0),
        PropertySource::Constant(SCROLLS_BY_PAGES)
    );
    let scroll = widgets(&image)
        .into_iter()
        .find(|(kind, _)| *kind == WidgetKind::Scroll)
        .unwrap();
    assert!(scroll
        .1
        .iter()
        .all(|(property, _)| *property != Property::Scrolling));
}

#[test]
fn a_page_is_a_growing_column_or_a_scroll_when_it_scrolls() {
    let image = compile(&app(r#"import { Label, Page, Pages } from "neotime/widgets";
        function Main() { return <Screen>
            <Pages value={1} onPageChange={(event) => {}}>
                <Page><Label>One</Label></Page>
                <Page scrolls scroll_bar={false} grow={2}><Label>Two</Label></Page>
                <Page scrolls={false}><Label>Three</Label></Page>
            </Pages>
        </Screen>; }"#));
    let image = AppImage::parse(&image).unwrap();
    let kinds: Vec<(WidgetKind, Option<PropertySource>)> = widgets(&image)
        .into_iter()
        .filter(|(kind, _)| matches!(kind, WidgetKind::Column | WidgetKind::Scroll))
        .skip(1)
        .map(|(kind, properties)| {
            let grow = properties
                .iter()
                .find(|(property, _)| *property == Property::Grow)
                .map(|(_, source)| *source);
            (kind, grow)
        })
        .collect();

    assert_eq!(
        kinds,
        [
            (WidgetKind::Column, Some(PropertySource::Constant(1))),
            (WidgetKind::Scroll, Some(PropertySource::Constant(2))),
            (WidgetKind::Column, Some(PropertySource::Constant(1))),
        ]
    );
    assert_eq!(
        property_of(&image, WidgetKind::Pages, Property::Value, 0),
        PropertySource::Constant(1)
    );
}

#[test]
fn a_page_needs_scrolls_written_out() {
    let source = app(r#"import { signal } from "@preact/signals";
        import { Page, Pages } from "neotime/widgets";
        const long = signal(true);
        function Main() { return <Screen><Pages><Page scrolls={long.value} /></Pages></Screen>; }"#);
    assert!(first_error(&source).contains("`scrolls` on a `Page` must be written out"));
}

#[test]
fn the_scrolling_props_must_be_written_out_and_direction_fits_only_lists() {
    let source = |widget: &str| {
        app(&format!(
            r#"import {{ signal }} from "@preact/signals";
            import {{ List, TextFile }} from "neotime/widgets";
            const paged = signal(true);
            function Main() {{ return <Screen>{widget}</Screen>; }}"#
        ))
    };
    assert!(first_error(&source(r#"<List direction="up" />"#)).contains("`direction` must be"));
    assert!(
        first_error(&source(r#"<TextFile path="/a.txt" direction="row" />"#))
            .contains("only a Scroll or List")
    );
    assert!(first_error(&source(r#"<List scrolling="fast" />"#)).contains("`scrolling` must be"));
    assert!(first_error(&source("<List scroll_bar={paged.value} />")).contains("`scroll_bar` must be"));
}

/// The subscriptions an image lists.
fn subscriptions_of(image: &AppImage) -> Vec<neotime_runtime::subscriptions::Subscription> {
    (0..image.subscription_count())
        .map(|index| image.subscription(index).unwrap())
        .collect()
}

#[test]
fn events_connected_with_on_are_written_in_order() {
    let source = format!(
        "{HEADER}\nimport {{ on }} from \"neotime\";\nconst count = signal(0);\nfunction tick() {{ count.value += 1; }}\nfunction Main() {{ return <Screen><Label>{{count.value}}</Label></Screen>; }}\non(\"open\", tick);\non(\"tick\", tick, {{ every: SECOND, while_open: true }});\non(\"shake\", tick, {{ while_open: true }});\nexport default define_app({{ id: \"t\", name: \"T\", screens: {{ main: Main }} }});"
    );
    let image = compile(&source);
    let image = AppImage::parse(&image).unwrap();
    let kinds: Vec<_> = subscriptions_of(&image)
        .iter()
        .map(|subscription| subscription.kind)
        .collect();
    use neotime_runtime::subscriptions::SubscriptionKind;

    assert_eq!(
        kinds,
        [
            SubscriptionKind::Open,
            SubscriptionKind::Every,
            SubscriptionKind::Motion
        ]
    );
    assert_eq!(subscriptions_of(&image)[2].detail, 1);
}

#[test]
fn a_short_background_period_is_an_error_and_a_background_event_needs_background() {
    let short = format!(
        "{HEADER}\nimport {{ on }} from \"neotime\";\nfunction Main() {{ return <Screen></Screen>; }}\non(\"tick\", () => {{}}, {{ every: SECOND }});\nexport default define_app({{ id: \"t\", name: \"T\", screens: {{ main: Main }} }});"
    );
    let output = compile_source(&short);
    assert!(output.errors().any(|error| error.message.contains("while_open")));

    let background = format!(
        "{HEADER}\nimport {{ on }} from \"neotime\";\nfunction Main() {{ return <Screen></Screen>; }}\non(\"tick\", () => {{}}, {{ every: MINUTE }});\nexport default define_app({{ id: \"t\", name: \"T\", screens: {{ main: Main }} }});"
    );
    let output = compile_source(&background);
    assert_eq!(output.permissions, [crate::permissions::Permission::Background]);
}

#[test]
fn a_computed_array_is_kept_by_a_derive_subscription() {
    let image = compile(&app(r##"import { CellGrid } from "neotime/widgets";
        const shift = signal(0);
        const cells = computed(() => range(4).map((cell) => (cell + shift.value) % 2));
        function Main() { return <Screen><CellGrid columns={2} rows={2} cell_size={20} cells={cells.value} palette={["#000000", "accent"]} /></Screen>; }"##));
    let image = AppImage::parse(&image).unwrap();
    use neotime_runtime::subscriptions::SubscriptionKind;

    assert!(subscriptions_of(&image)
        .iter()
        .any(|subscription| subscription.kind == SubscriptionKind::Derive));
    assert!(matches!(
        property_of(&image, WidgetKind::CellGrid, Property::Cells, 0),
        PropertySource::Constant(_)
    ));
}

#[test]
fn iso_week_and_day_of_year_are_worked_out() {
    // 2021-01-03 at noon UTC is a Sunday in ISO week 53 of 2020, and day 3 of its year.
    let declarations = "import { iso_week_of, day_of_year } from \"neotime\";\nconst week = signal(0);\nconst day = signal(0);\nconst when = signal<Instant>(1609675200000);";
    let handler = "week.value = iso_week_of(when.value); day.value = day_of_year(when.value);";

    assert_eq!(after_click(declarations, handler, "week.value"), "53");
    assert_eq!(after_click(declarations, handler, "day.value"), "3");
}

#[test]
fn the_shared_library_holds_every_icon() {
    let image = crate::library::build_library().expect("the library builds");
    let parsed = AppImage::parse(&image).expect("the library parses");
    neotime_runtime::verify(&parsed).expect("the library passes the install checks");

    assert_eq!(parsed.kind, neotime_runtime::AppKind::Library);
    assert_eq!(parsed.resource_count() as usize, crate::types::ICON_NAMES.len());
    for index in 0..parsed.resource_count() {
        let icon = parsed.resource(index).unwrap();
        assert_eq!(
            (icon.width, icon.height),
            (crate::library::ICON_SIDE, crate::library::ICON_SIDE)
        );
    }
}

// ============================================================================
// Expressions
// ============================================================================

/// Compiles `expression` as a label's text through `format_number` and returns what it shows.
fn number_shown(declarations: &str, expression: &str) -> String {
    let image = compile(&app(&format!(
        "{declarations}\nfunction Main() {{ return <Screen><Label>{{format_number({expression})}}</Label></Screen>; }}"
    )));
    let image = AppImage::parse(&image).unwrap();
    let state = AppState::new(&image).unwrap();
    label_text(&image, &state, 0)
}

#[test]
fn division_rounds_the_way_the_math_function_says() {
    let declarations = "const minus_five = signal(-5);\nconst seven = signal(7);";

    assert_eq!(
        number_shown(declarations, "Math.floor(minus_five.value / 4)"),
        "-2"
    );
    assert_eq!(
        number_shown(declarations, "Math.trunc(minus_five.value / 4)"),
        "-1"
    );
    assert_eq!(number_shown(declarations, "Math.ceil(seven.value / 4)"), "2");
    assert_eq!(number_shown(declarations, "Math.round(seven.value / 2)"), "4");
    assert_eq!(
        number_shown(declarations, "Math.round(minus_five.value / 2)"),
        "-2"
    );
    assert_eq!(
        number_shown(declarations, "Math.floor(seven.value / minus_five.value)"),
        "-2"
    );
}

#[test]
fn arithmetic_bitwise_and_math_functions_work() {
    let declarations = "const six = signal(6);";

    assert_eq!(number_shown(declarations, "six.value * 7 - 2"), "40");
    assert_eq!(number_shown(declarations, "(six.value & 3) | 8"), "10");
    assert_eq!(number_shown(declarations, "six.value << 2"), "24");
    assert_eq!(number_shown(declarations, "Math.max(1, six.value, 4)"), "6");
    assert_eq!(number_shown(declarations, "Math.abs(-six.value)"), "6");
    assert_eq!(number_shown(declarations, "six.value % 4"), "2");
}

#[test]
fn constants_are_worked_out_when_compiling() {
    let image = compile(&app(
        "const TOTAL = 3 * HOUR + Math.floor(90 / 60) * MINUTE;\nfunction Main() { return <Screen><Label>{`${TOTAL}`}</Label></Screen>; }",
    ));
    let image = AppImage::parse(&image).unwrap();
    let state = AppState::new(&image).unwrap();

    assert!(matches!(
        property_of(&image, WidgetKind::Label, Property::Text, 0),
        PropertySource::Constant(_)
    ));
    assert_eq!(label_text(&image, &state, 0), "10860000");
}

#[test]
fn comparing_with_null_tests_for_the_empty_optional() {
    let declarations = "const started = signal<Instant | null>(null);";

    assert_eq!(number_shown(declarations, "started.value === null ? 1 : 0"), "1");
    assert_eq!(number_shown(declarations, "started.value !== null ? 1 : 0"), "0");
    assert_eq!(number_shown(declarations, "started.value ?? 42"), "42");
}

#[test]
fn logical_operators_and_ternaries_choose_values() {
    let declarations = "const yes = signal(true);\nconst no = signal(false);";

    assert_eq!(number_shown(declarations, "yes.value && !no.value ? 1 : 2"), "1");
    assert_eq!(number_shown(declarations, "no.value || yes.value ? 3 : 4"), "3");
}

#[test]
fn computed_values_are_inlined() {
    let declarations = "const base = signal(4);\nconst doubled = computed(() => base.value * 2);\nconst plus = computed(() => { const extra = 1; return doubled.value + extra; });";

    assert_eq!(number_shown(declarations, "plus.value"), "9");
}

#[test]
fn a_constant_table_is_read_at_a_run_time_index() {
    let declarations = "const TABLE = [5, 9, 2, 7];\nconst place = signal(2);\nconst EVEN = [10, 20, 30];";

    assert_eq!(number_shown(declarations, "TABLE[place.value]"), "2");
    assert_eq!(number_shown(declarations, "EVEN[place.value]"), "30");
}

#[test]
fn format_patterns_are_checked_and_formatted() {
    let image = compile(&app(r#"const elapsed = signal<Duration>(75_000);
        const moment = signal<Instant>(0);
        function Main() {
          return (
            <Screen>
              <Label>{format_duration(elapsed.value, "mm:ss")}</Label>
              <Label>{format_time(moment.value, "YYYY-MM-DD HH:mm")}</Label>
              <Label>{format_duration(elapsed.value, elapsed.value > HOUR ? "h:mm:ss" : "m:ss")}</Label>
            </Screen>
          );
        }"#));
    let image = AppImage::parse(&image).unwrap();
    let state = AppState::new(&image).unwrap();

    assert_eq!(label_text(&image, &state, 0), "01:15");
    assert_eq!(label_text(&image, &state, 1), "1970-01-01 00:00");
    assert_eq!(label_text(&image, &state, 2), "1:15");
}

#[test]
fn a_bad_format_pattern_is_an_error() {
    let message = first_error(&app(r#"const elapsed = signal(0);
        function Main() { return <Screen><Label>{format_duration(elapsed.value, "mm:qq")}</Label></Screen>; }"#));

    assert!(message.contains("not a valid pattern"), "{message}");
}

#[test]
fn system_values_are_read_with_value() {
    let image = compile(&app(
        "function Main() { return <Screen><Label>{format_time(now.value, \"HH:mm\")}</Label></Screen>; }",
    ));
    let image = AppImage::parse(&image).unwrap();
    let state = AppState::new(&image).unwrap();
    let source = property_of(&image, WidgetKind::Label, Property::Text, 0);

    let system = FakeSystem {
        now: (13 * 3600 + 5 * 60) * 1000,
    };
    assert_eq!(text_of(&image, &state, source, &system), "13:05");
}

#[test]
fn swipe_directions_are_shown_as_their_names() {
    let image = compile(&app(r#"const direction = signal<SwipeEvent["direction"]>("up");
        function Main() { return <Screen><Label>{`${direction.value}`}</Label></Screen>; }"#));
    let image = AppImage::parse(&image).unwrap();
    let state = AppState::new(&image).unwrap();

    assert_eq!(label_text(&image, &state, 0), "up");
}

// ============================================================================
// Handlers
// ============================================================================

/// Compiles an app with one button whose handler is `handler`, presses it and returns the state.
fn after_click(declarations: &str, handler: &str, shown: &str) -> String {
    let source = app(&format!(
        "{declarations}\nfunction Main() {{ return <Screen><Button onClick={{() => {{ {handler} }}}}>Go</Button><Label>{{format_number({shown})}}</Label></Screen>; }}"
    ));
    let image = compile(&source);
    let image = AppImage::parse(&image).unwrap();
    let mut state = AppState::new(&image).unwrap();
    run(&image, &mut state, WidgetKind::Button, Property::OnClick, &[0, 0]);
    label_text(&image, &state, 0)
}

#[test]
fn handlers_write_state_and_see_their_own_writes() {
    let declarations = "const count = signal(1);\nconst copy = signal(0);";
    let handler = "count.value = count.value + 1; copy.value = count.value * 10;";

    assert_eq!(after_click(declarations, handler, "copy.value"), "20");
}

#[test]
fn locals_if_else_and_early_return_work() {
    let declarations = "const count = signal(5);";
    let handler = "let next = count.value; if (next > 3) { next = next - 3; } else { next = 0; } count.value = next; if (next === 2) return; count.value = 99;";

    assert_eq!(after_click(declarations, handler, "count.value"), "2");
}

#[test]
fn compound_assignments_and_updates_work() {
    let declarations = "const count = signal(5);";

    assert_eq!(
        after_click(
            declarations,
            "count.value += 3; count.value *= 2; count.value++;",
            "count.value"
        ),
        "17"
    );
}

#[test]
fn for_of_loops_over_ranges_and_constant_arrays() {
    let declarations = "const total = signal(0);\nconst PRIMES = [2, 3, 5, 7];";
    let handler = "let sum = 0; for (const n of range(5)) { sum = sum + n; } for (const prime of PRIMES) { if (prime === 5) continue; sum = sum + prime; } total.value = sum;";

    assert_eq!(after_click(declarations, handler, "total.value"), "22");
}

#[test]
fn break_leaves_a_loop() {
    let declarations = "const found = signal(-1);\nconst VALUES = [4, 8, 15, 16];";
    let handler = "for (const value of VALUES) { if (value > 10) { found.value = value; break; } }";

    assert_eq!(after_click(declarations, handler, "found.value"), "15");
}

#[test]
fn functions_called_from_handlers_are_inlined() {
    let declarations = "const count = signal(2);\nfunction triple(value: number): number { if (value < 0) return 0; return value * 3; }\nfunction bump() { count.value = triple(count.value) + 1; }";

    assert_eq!(after_click(declarations, "bump();", "count.value"), "7");
}

#[test]
fn a_whole_array_assignment_reads_the_old_array() {
    let declarations = "const board = signal([0, 0, 0, 0]);\nconst PLACES = [0, 1, 2, 3];";
    let handler = "board.value[0] = 1; board.value[1] = 2; board.value = PLACES.map((place) => board.value[(place + 1) % 4] + 10);";

    assert_eq!(
        after_click(declarations, handler, "board.value[0] * 1000 + board.value[3]"),
        "12011"
    );
}

#[test]
fn an_array_literal_assignment_shifts_elements() {
    let declarations = "const laps = signal<number[]>([1, 1, 1]);";
    let handler = "laps.value = [7, laps.value[0] + 1, laps.value[1] + 2];";

    assert_eq!(
        after_click(
            declarations,
            handler,
            "laps.value[0] * 100 + laps.value[1] * 10 + laps.value[2]"
        ),
        "723"
    );
}

#[test]
fn records_are_kept_field_by_field() {
    let declarations = "const time = signal({ hour: 8, minute: 30 });";
    let handler = "time.value = { ...time.value, hour: time.value.hour + 1 };";

    assert_eq!(
        after_click(declarations, handler, "time.value.hour * 100 + time.value.minute"),
        "930"
    );
}

#[test]
fn effects_are_recorded_with_their_arguments() {
    let image = compile(&app(
        r#"function Main() { return <Screen><Button onClick={() => { vibrate(200); vibrate_pattern("alarm"); go("main"); }}>Go</Button></Screen>; }"#,
    ));
    let image = AppImage::parse(&image).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let PropertySource::Handler(handler) = property_of(&image, WidgetKind::Button, Property::OnClick, 0)
    else {
        panic!("not a handler");
    };

    let outcome = Interpreter::new()
        .run_handler(&image, handler, &mut state, &[0, 0], &SYSTEM)
        .unwrap();
    let effects = outcome.effects();
    assert_eq!(effects[0].kind, EffectKind::Vibrate);
    assert_eq!(effects[0].arguments[0], 200);
    assert_eq!(effects[1].kind, EffectKind::VibratePattern);
    assert_eq!(effects[1].arguments[0], 3);
    assert_eq!(effects[2].kind, EffectKind::Go);
    assert_eq!(image.string(effects[2].arguments[0] as u16), Some("main"));
}

#[test]
fn event_fields_are_read_by_name() {
    let image = compile(&app(r#"const last = signal(0);
        function Main() {
          return (
            <Screen onSwipe={(event) => { if (event.direction === "left") last.value = 1; else last.value = 2; }}>
              <Button onClick={(event) => { last.value = event.y; }}>Go</Button>
            </Screen>
          );
        }"#));
    let image = AppImage::parse(&image).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let last = variable_named(&image, "last");

    run(&image, &mut state, WidgetKind::Column, Property::OnSwipe, &[2]);
    assert_eq!(state.get(last), Some(1));
    run(
        &image,
        &mut state,
        WidgetKind::Button,
        Property::OnClick,
        &[10, 33],
    );
    assert_eq!(state.get(last), Some(33));
}

#[test]
fn random_numbers_are_allowed_in_handlers() {
    let declarations = "const rolled = signal(0);";

    assert_eq!(
        after_click(declarations, "rolled.value = random_int(1, 6);", "rolled.value"),
        "2"
    );
}

// ============================================================================
// State layout
// ============================================================================

#[test]
fn arrays_take_the_narrowest_width_their_values_fit() {
    let image = compile(&app(r#"const small = signal([0, 0, 0, 0]);
        const medium = signal([0, 0]);
        const times = signal<Duration[]>([0, 0]);
        function Main() {
          return (
            <Screen>
              <Button onClick={() => { small.value[1] = 1; medium.value[0] = 1000; times.value[0] = 5; }}>Go</Button>
            </Screen>
          );
        }"#));
    let image = AppImage::parse(&image).unwrap();
    let width = |key: &str| {
        image
            .state_variable(variable_named(&image, key) as u16)
            .unwrap()
            .element_bytes
    };

    assert_eq!(width("small"), 1);
    assert_eq!(width("medium"), 2);
    assert_eq!(width("times"), 8);
}

#[test]
fn persisted_signals_keep_their_key() {
    let image = compile(&app(
        "const best = persisted(\"best_score\", 0);\nfunction Main() { return <Screen><Label>{`${best.value}`}</Label></Screen>; }",
    ));
    let image = AppImage::parse(&image).unwrap();
    let variable = image
        .state_variable(variable_named(&image, "best_score") as u16)
        .unwrap();

    assert!(variable.persists);
}

#[test]
fn too_many_state_variables_is_an_error() {
    let declarations: String = (0..65)
        .map(|index| format!("const value_{index} = signal(0);\n"))
        .collect();
    let message = first_error(&app(&format!(
        "{declarations}function Main() {{ return <Screen><Label>Hi</Label></Screen>; }}"
    )));

    assert!(message.contains("at most 64 state variables"), "{message}");
}

#[test]
fn an_array_too_large_for_state_is_an_error() {
    let message = first_error(&app(
        "const big = signal<Instant[]>(range(200).map(() => 0));\nfunction Main() { return <Screen><Label>Hi</Label></Screen>; }",
    ));

    assert!(message.contains("at most 1024"), "{message}");
}

// ============================================================================
// Imports
// ============================================================================

#[test]
fn signals_imported_from_neotime_build_the_same_image_as_from_preact() {
    let body = r#"
        const count = persisted("count", 2);
        const doubled = computed(() => count.value * 2);
        function Main() {
          const taps = useSignal(0);
          return <Screen><Label>{`${doubled.value} ${taps.value}`}</Label>
            <Button onClick={() => { taps.value++; count.value = signal_start.value; }}>Go</Button></Screen>;
        }
        const signal_start = signal(5);
        export default define_app({ id: "test", name: "Test", screens: { main: Main } });"#;
    let imports = |signals_from: &str| {
        format!(
            r#"import {{ computed, signal, useSignal }} from "{signals_from}";
            import {{ define_app, persisted }} from "neotime";
            import {{ Button, Label, Screen }} from "neotime/widgets";
            {body}"#
        )
    };
    assert_eq!(compile(&imports("neotime")), compile(&imports("@preact/signals")));
}

// ============================================================================
// What the compiler rejects
// ============================================================================

#[test]
fn rejected_forms_name_the_declarative_alternative() {
    let cases = [
        ("const half = signal(4);\nfunction Main() { return <Screen><Label>{`${half.value / 2}`}</Label></Screen>; }", "Math.trunc"),
        ("const n = signal(0);\nfunction Main() { return <Screen><Button onClick={() => { while (n.value < 3) n.value++; }}>Go</Button></Screen>; }", "for ... of"),
        ("import { useState } from \"preact/hooks\";\nfunction Main() { const [a, b] = useState(0); return <Screen><Label>Hi</Label></Screen>; }", "signal"),
        ("import { thing } from \"lodash\";\nfunction Main() { return <Screen><Label>Hi</Label></Screen>; }", "copy the code into the app"),
        ("const n = signal(0);\nfunction Main() { return <Screen><Button onClick={() => { const s = signal(0); }}>Go</Button></Screen>; }", "top of the file"),
        ("const n = signal(0);\nfunction Main() { return <Screen><Button onClick={() => { setTimeout(() => {}, 5); }}>Go</Button></Screen>; }", "`on`"),
        ("const n = signal(0);\nfunction Main() { return <Screen><Label>{`${n.value}`}</Label><Button onClick={() => { n.value = 1.5; }}>Go</Button></Screen>; }", "whole"),
        ("const name = signal(\"a\");\nconst count = signal(0);\nfunction Main() { return <Screen><Button onClick={() => { name.value = `${count.value}`; }}>Go</Button></Screen>; }", "fixed text"),
        ("function count(n: number): number { return n === 0 ? 0 : count(n - 1); }\nconst value = signal(0);\nfunction Main() { return <Screen><Button onClick={() => { value.value = count(3); }}>Go</Button></Screen>; }", "recursion"),
        ("const board = signal([0, 0]);\nfunction Main() { return <Screen><Button onClick={() => { let copy = [1, 2]; }}>Go</Button></Screen>; }", "one value"),
    ];
    for (body, advice) in cases {
        let output = compile_source(&app(body));
        let message = output
            .errors()
            .next()
            .map(|error| error.message.clone())
            .unwrap_or_else(|| panic!("no error for: {body}"));
        assert!(message.contains(advice), "expected `{advice}` in: {message}");
    }
}

#[test]
fn errors_are_printed_with_the_file_line_and_column() {
    let source = "const x = 1;\nconst y = x / 2;\n";
    let output = compile_source(&format!("{HEADER}{source}"));
    let error = output
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.severity == Severity::Error)
        .unwrap();
    let rendered = error.render("app.tsx", &format!("{HEADER}{source}"));

    assert!(rendered.starts_with("app.tsx:"), "{rendered}");
    assert!(rendered.split(':').count() >= 4, "{rendered}");
}

#[test]
fn effects_are_rejected_outside_handlers() {
    let message = first_error(&app(
        "const n = computed(() => { vibrate(10); return 1; });\nfunction Main() { return <Screen><Label>{`${n.value}`}</Label></Screen>; }",
    ));

    assert!(message.contains("only be asked for in a handler"), "{message}");
}

// ============================================================================
// Newer runtime features
// ============================================================================

#[test]
fn start_of_minute_uses_the_runtime_instruction() {
    let declarations = "const moment = signal<Instant>(125_000);";

    assert_eq!(
        number_shown(declarations, "start_of_minute(moment.value)"),
        "120000"
    );
    assert_eq!(number_shown(declarations, "start_of_hour(moment.value)"), "0");
}

#[test]
fn text_from_codes_shows_an_array_of_character_codes() {
    let image = compile(&app(r#"const codes = signal([72, 72, 72, 72]);
        const length = signal(3);
        function Main() { return <Screen><Label>{text_from_codes(codes.value, length.value)}</Label></Screen>; }"#));
    let image = AppImage::parse(&image).unwrap();
    let state = AppState::new(&image).unwrap();

    assert_eq!(label_text(&image, &state, 0), "HHH");
}

#[test]
fn join_unrolls_over_a_fixed_length_array() {
    let image = compile(&app(r#"const hours = signal(7);
        function Main() { return <Screen><Label>{join([`${hours.value}`, "30", "00"], ":")}</Label></Screen>; }"#));
    let image = AppImage::parse(&image).unwrap();
    let state = AppState::new(&image).unwrap();

    assert_eq!(label_text(&image, &state, 0), "7:30:00");
}

#[test]
fn text_functions_work_on_text_known_when_compiling() {
    let image = compile(&app(r#"const NAME = "watch";
        function Main() {
          return (
            <Screen>
              <Label>{`${to_upper(NAME)} ${text_length(NAME)} ${slice_text(NAME, 1, 3)} ${pad_start("7", 3, "0")} ${"ab".toUpperCase()}`}</Label>
            </Screen>
          );
        }"#));
    let image = AppImage::parse(&image).unwrap();
    let state = AppState::new(&image).unwrap();

    assert_eq!(label_text(&image, &state, 0), "WATCH 5 at 007 AB");
}

#[test]
fn records_in_an_array_are_kept_as_one_array_per_field() {
    let declarations = "const alarms = signal([{ hour: 7, enabled: false }, { hour: 7, enabled: false }, { hour: 7, enabled: false }]);\nconst place = signal(1);";
    let handler =
        "alarms.value[place.value].enabled = true; alarms.value[2] = { ...alarms.value[1], hour: 9 };";

    assert_eq!(
        after_click(
            declarations,
            handler,
            "(alarms.value[1].enabled ? 100 : 0) + (alarms.value[2].enabled ? 10 : 0) + alarms.value[2].hour - alarms.value[0].hour"
        ),
        "112"
    );
}

#[test]
fn records_in_an_array_can_be_counted_in_a_loop() {
    let declarations =
        "const alarms = signal([{ enabled: true }, { enabled: true }]);\nconst on = signal(0);";
    let handler =
        "let count = 0; for (const alarm of alarms.value) { if (alarm.enabled) count++; } on.value = count;";

    assert_eq!(after_click(declarations, handler, "on.value"), "2");
}

#[test]
fn array_from_with_a_length_repeats_widgets() {
    let image = compile(&app(
        "function Main() { return <Screen>{Array.from({ length: 5 }, (_, hour) => <Label>{`${hour * 30}`}</Label>)}</Screen>; }",
    ));
    let image = AppImage::parse(&image).unwrap();
    let state = AppState::new(&image).unwrap();
    let PropertySource::Expression(expression) = property_of(&image, WidgetKind::Label, Property::Text, 0)
    else {
        panic!("the label's text is not an expression");
    };
    let mut interpreter = Interpreter::new();
    interpreter.set_row(4);
    let value = interpreter
        .evaluate_expression(&image, expression, &state, &SYSTEM)
        .unwrap();

    assert_eq!(interpreter.text(&image, value), Ok("120"));
    assert_eq!(widgets(&image)[1].0, WidgetKind::Repeat);
}

#[test]
fn an_exact_division_of_constants_is_allowed() {
    assert_eq!(number_shown("const WIDTH = 240;", "(WIDTH - 40) / 2"), "100");
}

#[test]
fn system_text_values_are_read_like_any_other() {
    let source = app(r#"import { current_music } from "neotime";
        function Main() { return <Screen><Label>{current_music.value.artist}</Label></Screen>; }"#);
    let image = compile(&source);
    let image = AppImage::parse(&image).unwrap();

    assert!(matches!(
        property_of(&image, WidgetKind::Label, Property::Text, 0),
        PropertySource::Expression(_)
    ));
}

#[test]
fn the_firmware_version_and_uptime_are_read_without_a_permission() {
    let source = app(r#"import { firmware_version, started_at } from "neotime";
        function Main() {
          return (
            <Screen>
              <Label>{`NeoTime ${firmware_version.value}`}</Label>
              <Label>{format_duration(now.value - started_at.value, "h:mm:ss")}</Label>
            </Screen>
          );
        }"#);
    let image = compile(&source);
    let image = AppImage::parse(&image).unwrap();
    let state = AppState::new(&image).unwrap();

    assert_eq!(label_text(&image, &state, 0), "NeoTime 0.1.0+1b19aed");
    assert_eq!(label_text(&image, &state, 1), "0:01:30");
}

#[test]
fn next_time_of_day_finds_the_next_matching_weekday() {
    // 1970-01-01 was a Thursday, weekday 3; ask for 07:30 on Saturdays only, bit 5.
    let declarations = "const found = signal<Instant>(0);\nconst start = signal<Instant>(10 * HOUR);";
    let handler = "found.value = next_time_of_day(start.value, { hour: 7, minute: 30 }, 1 << 5);";

    let expected = (2 * 86_400_000 + 7 * 3_600_000 + 30 * 60_000).to_string();
    assert_eq!(after_click(declarations, handler, "found.value"), expected);
}

#[test]
fn a_cell_grid_takes_its_cells_and_palette_as_state_arrays() {
    let image = compile(&app(r##"import { CellGrid } from "neotime/widgets";
        const board = signal([0, 1, 1, 0]);
        function Main() {
          return (
            <Screen>
              <CellGrid columns={2} rows={2} cell_size={20} cells={board.value} palette={["#000000", "accent"]} onClick={(event) => { board.value[event.value] = 1; }} />
            </Screen>
          );
        }"##));
    let image = AppImage::parse(&image).unwrap();
    let board = variable_named(&image, "board");
    let PropertySource::Constant(cells) = property_of(&image, WidgetKind::CellGrid, Property::Cells, 0)
    else {
        panic!("cells is a constant");
    };
    let PropertySource::Constant(palette) = property_of(&image, WidgetKind::CellGrid, Property::Palette, 0)
    else {
        panic!("palette is a constant");
    };
    let mut state = AppState::new(&image).unwrap();

    assert_eq!(cells as u8, board);
    assert_eq!(
        state.element(palette as u8, 1),
        Some(ColorValue::Theme(ThemeColor::Accent).to_value())
    );
    assert_eq!(state.element(board, 2), Some(1));
    run(
        &image,
        &mut state,
        WidgetKind::CellGrid,
        Property::OnClick,
        &[0, 0, 3],
    );
    assert_eq!(state.element(board, 3), Some(1));
}

#[test]
fn button_matrix_rows_become_options_with_row_breaks() {
    let image = compile(&app(r#"import { ButtonMatrix } from "neotime/widgets";
        const pressed = signal(0);
        function Main() {
          return <Screen><ButtonMatrix rows={[["1", "2"], ["3"]]} onClick={(event) => { pressed.value = event.value; }} /></Screen>;
        }"#));
    let image = AppImage::parse(&image).unwrap();
    let PropertySource::Constant(options) =
        property_of(&image, WidgetKind::ButtonMatrix, Property::Options, 0)
    else {
        panic!("options is a constant");
    };
    let state = AppState::new(&image).unwrap();
    let labels: Vec<&str> = (0..4)
        .map(|index| {
            image
                .string(state.element(options as u8, index).unwrap() as u16)
                .unwrap()
        })
        .collect();

    assert_eq!(labels, ["1", "2", "\n", "3"]);
}

#[test]
fn button_matrix_highlighted_labels_become_bits_of_its_value() {
    let image = compile(&app(r#"import { ButtonMatrix } from "neotime/widgets";
        const symbol = signal("+");
        const ready = signal(true);
        function Main() {
          return <Screen>
            <ButtonMatrix rows={[["1", "+"], ["Go", "+"]]} highlighted={[symbol.value, "1"]} onClick={() => {}} />
            <ButtonMatrix rows={[["1", "Go"]]} highlighted={ready.value ? ["Go"] : []} onClick={() => {}} />
            <ButtonMatrix rows={[["1", "Go"]]} highlighted={["Go"]} onClick={() => {}} />
            <ButtonMatrix rows={[["1", "Go"]]} onClick={() => {}} />
          </Screen>;
        }"#));
    let image = AppImage::parse(&image).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let value = |state: &AppState, nth: usize| {
        value_of(
            &image,
            state,
            property_of(&image, WidgetKind::ButtonMatrix, Property::Value, nth),
            &SYSTEM,
        )
    };

    assert_eq!(value(&state, 0), 0b1011);
    assert_eq!(value(&state, 1), 0b10);
    assert_eq!(value(&state, 2), 0b10);
    let unhighlighted = widgets(&image)
        .into_iter()
        .filter(|(kind, _)| *kind == WidgetKind::ButtonMatrix)
        .nth(3)
        .unwrap();
    assert!(!unhighlighted
        .1
        .iter()
        .any(|(property, _)| *property == Property::Value));
    let symbol = variable_named(&image, "symbol");
    let go = (0..image.string_count())
        .find(|&index| image.string(index) == Some("Go"))
        .unwrap() as Value;
    state.set(symbol, go).unwrap();
    state.set(variable_named(&image, "ready"), 0).unwrap();
    assert_eq!(value(&state, 0), 0b0101);
    assert_eq!(value(&state, 1), 0);
}

#[test]
fn button_matrix_highlighted_needs_fixed_labels() {
    let rows_unknown = app(r#"import { ButtonMatrix } from "neotime/widgets";
        const label = signal("1");
        function Main() {
          return <Screen><ButtonMatrix rows={[["1"], [label.value]]} highlighted={["1"]} onClick={() => {}} /></Screen>;
        }"#);
    let not_an_array = app(r#"import { ButtonMatrix } from "neotime/widgets";
        const symbol = signal("1");
        function Main() {
          return <Screen><ButtonMatrix rows={[["1"]]} highlighted={symbol.value} onClick={() => {}} /></Screen>;
        }"#);

    assert!(first_error(&rows_unknown).contains("`rows` written out"));
    assert!(first_error(&not_an_array).contains("must be an array of labels"));
}

#[test]
fn arrays_may_start_with_different_values() {
    let declarations = "const tiles = signal([3, 1, 2, 0]);";

    assert_eq!(
        after_click(
            declarations,
            "tiles.value[3] = tiles.value[0];",
            "tiles.value[1] * 10 + tiles.value[3]"
        ),
        "13"
    );
}

#[test]
fn new_widget_props_use_their_number_mappings() {
    let image = compile(&app(r#"import { AnalogClock } from "neotime/widgets";
        function Main() {
          return (
            <Screen justify="between" align_items="center" gap={6}>
              <AnalogClock time={now.value} hand_style="tapered" hour_color="accent" />
              <Button disabled={true}>Go</Button>
            </Screen>
          );
        }"#));
    let image = AppImage::parse(&image).unwrap();

    assert_eq!(
        property_of(&image, WidgetKind::Column, Property::Justify, 0),
        PropertySource::Constant(3)
    );
    assert_eq!(
        property_of(&image, WidgetKind::Column, Property::AlignItems, 0),
        PropertySource::Constant(1)
    );
    assert_eq!(
        property_of(&image, WidgetKind::Column, Property::Gap, 0),
        PropertySource::Constant(6)
    );
    assert_eq!(
        property_of(&image, WidgetKind::AnalogClock, Property::HandStyle, 0),
        PropertySource::Constant(1)
    );
    assert_eq!(
        property_of(&image, WidgetKind::Button, Property::Disabled, 0),
        PropertySource::Constant(1)
    );
}

#[test]
fn text_functions_work_on_text_made_while_running() {
    let image = compile(&app(r#"const count = signal(42);
        function Main() {
          return (
            <Screen>
              <Label>{to_upper(format_time(0, "ddd"))}</Label>
              <Label>{`${text_length(`${count.value}x`)}`}</Label>
              <Label>{slice_text(`${count.value}789`, 1, -1)}</Label>
              <Label>{pad_start(`${count.value}`, 5, "0")}</Label>
              <Label>{`${count.value}`.padStart(3, "*").slice(1)}</Label>
              <Label>{`${`${count.value}`.length}`}</Label>
            </Screen>
          );
        }"#));
    let image = AppImage::parse(&image).unwrap();
    let state = AppState::new(&image).unwrap();

    assert_eq!(label_text(&image, &state, 0), "THU");
    assert_eq!(label_text(&image, &state, 1), "3");
    assert_eq!(label_text(&image, &state, 2), "278");
    assert_eq!(label_text(&image, &state, 3), "00042");
    assert_eq!(label_text(&image, &state, 4), "42");
    assert_eq!(label_text(&image, &state, 5), "2");
}

#[test]
fn includes_and_index_of_search_arrays() {
    let declarations =
        "const board = signal([0, 0, 7, 0, 7]);\nconst wanted = signal(7);\nconst PRIMES = [2, 3, 5, 7];";

    assert_eq!(
        number_shown(declarations, "board.value.indexOf(wanted.value)"),
        "2"
    );
    assert_eq!(number_shown(declarations, "board.value.includes(9) ? 1 : 0"), "0");
    assert_eq!(
        number_shown(declarations, "PRIMES.includes(wanted.value) ? 1 : 0"),
        "1"
    );
    assert_eq!(number_shown(declarations, "PRIMES.indexOf(wanted.value)"), "3");
}

#[test]
fn count_where_counts_the_elements_that_pass() {
    let declarations = "const board = signal([0, 1, 7, 0, 7]);\nconst limit = signal(0);";

    assert_eq!(
        number_shown(
            declarations,
            "count_where(board.value, (cell) => cell > limit.value)"
        ),
        "3"
    );
    assert_eq!(
        after_click(
            declarations,
            "limit.value = count_where(board.value, (cell, place) => place < 2);",
            "limit.value"
        ),
        "2"
    );
}

#[test]
fn join_works_on_a_text_array_signal() {
    let image = compile(&app(r#"const words = signal(["a", "b", "c"]);
        function Main() { return <Screen><Label>{join(words.value, "-")}</Label></Screen>; }"#));
    let image = AppImage::parse(&image).unwrap();
    let state = AppState::new(&image).unwrap();

    assert_eq!(label_text(&image, &state, 0), "a-b-c");
}

#[test]
fn justify_around_is_an_error_naming_the_choices() {
    let message = first_error(&app(
        r#"function Main() { return <Screen justify="around"><Label>Hi</Label></Screen>; }"#,
    ));

    assert!(
        message.contains("start, center, end or space_between"),
        "{message}"
    );
}

#[test]
/// A helper's return, a `.map` result and arithmetic on a counter all keep their ranges, so arrays stay narrow.
fn array_widths_follow_ranges_through_helpers_and_counters() {
    let image = compile(&app(r#"const CELLS = range(8);
        function helper(board: number[], cell: number, live: number): number {
          const n = board[cell] === 0 ? 0 : 1;
          return n === 1 ? live : 0;
        }
        function next(board: number[], live: number): number[] {
          return CELLS.map((cell) => helper(board, cell, live));
        }
        const a = signal<number[]>([0, 0, 0, 0, 0, 0, 0, 0]);
        const b = signal<number[]>([0, 0, 0, 0, 0, 0, 0, 0]);
        const c = signal<number[]>([0, 0, 0, 0, 0, 0, 0, 0]);
        const d = signal<number[]>([0, 0, 0, 0, 0, 0, 0, 0]);
        const generation = signal(0);
        function Main() {
          return (
            <Screen>
              <Button onClick={() => {
                const g = generation.value + 1;
                a.value = CELLS.map((cell) => (a.value[cell] === 0 ? 1 : 0));
                b.value = CELLS.map((cell) => helper(b.value, cell, 1));
                c.value = CELLS.map((cell) => helper(c.value, cell, 1 + (g % 9)));
                d.value = next(d.value, 1 + (g % 9));
                generation.value = g;
              }}>Go</Button>
            </Screen>
          );
        }"#));
    let image = AppImage::parse(&image).unwrap();
    for key in ["a", "b", "c", "d"] {
        let width = image
            .state_variable(variable_named(&image, key) as u16)
            .unwrap()
            .element_bytes;
        assert_eq!(width, 1, "`{key}`");
    }
}

#[test]
fn the_forecast_is_read_one_day_at_a_time() {
    let image = compile(&app(r#"import { current_weather } from "neotime";
        function Day(props: { place: number }) {
          return props.place < current_weather.value.forecast.length
            ? <Label>{`${current_weather.value.forecast[props.place].high}`}</Label>
            : <Label>-</Label>;
        }
        function Main() {
          return <Screen><Day place={1} /></Screen>;
        }"#));
    let image = AppImage::parse(&image).unwrap();

    assert!(reads_system_element(&image, SystemValue::WeatherForecastHigh));
}

/// Whether any expression of the image reads this system list element.
fn reads_system_element(image: &AppImage, value: SystemValue) -> bool {
    (0..image.expression_count()).any(|expression| {
        let code = image.expression_code(expression).unwrap();
        let mut offset = 0;
        while offset < code.len() {
            let (instruction, next) = neotime_runtime::instruction::decode(code, offset).unwrap();
            if instruction == neotime_runtime::instruction::Instruction::ReadSystemElement(value) {
                return true;
            }
            offset = next;
        }
        false
    })
}

const NOTIFICATIONS_SOURCE: &str = r#"import { notifications } from "neotime";
const place = signal(0);
function Main() {
  return (
    <Screen>
      <Label>{place.value < notifications.value.length ? notifications.value[place.value].title : "None"}</Label>
      <Label>{`${notifications.value[0].id}`}</Label>
      <Icon icon={notifications.value[0].icon} />
    </Screen>
  );
}"#;

#[test]
fn the_kept_notifications_are_read_one_field_at_a_time() {
    let source = format!(
        "{HEADER}\n{NOTIFICATIONS_SOURCE}\nexport default define_app({{ id: \"t\", name: \"T\", screens: {{ main: Main }} }});"
    );
    let image = compile(&source);
    let image = AppImage::parse(&image).unwrap();

    assert!(reads_system_element(&image, SystemValue::NotificationTitle));
    assert!(reads_system_element(&image, SystemValue::NotificationId));
    assert!(reads_system_element(&image, SystemValue::NotificationIcon));
}

#[test]
fn reading_the_kept_notifications_needs_the_notifications_permission() {
    let output = compile_source(&app(NOTIFICATIONS_SOURCE));

    assert_eq!(
        output.permissions,
        [crate::permissions::Permission::Notifications]
    );
}

/// Whether any expression or handler of the image runs this instruction.
fn image_runs(image: &AppImage, wanted: neotime_runtime::instruction::Instruction) -> bool {
    let codes = (0..image.expression_count())
        .map(|expression| image.expression_code(expression).unwrap())
        .chain((0..image.handler_count()).map(|handler| image.handler_code(handler).unwrap()));
    for code in codes {
        let mut offset = 0;
        while offset < code.len() {
            let (instruction, next) = neotime_runtime::instruction::decode(code, offset).unwrap();
            if instruction == wanted {
                return true;
            }
            offset = next;
        }
    }
    false
}

#[test]
fn the_step_history_is_read_one_day_or_hour_at_a_time() {
    use neotime_runtime::instruction::Instruction;
    let image = compile(&app(r#"import { reset_step_trip, step_history } from "neotime";
        import { Chart } from "neotime/widgets";
        const hours = computed(() => range(24).map((hour) => step_history.value.hours[24 + hour].steps));
        function Main() {
          return (
            <Screen>
              <Label>{`${step_history.value.days[1].total} ${step_history.value.days.length}`}</Label>
              <Label>{`${step_history.value.trip} ${step_history.value.hours.length}`}</Label>
              <Chart values={hours.value} />
              <Button onClick={reset_step_trip}>Reset</Button>
            </Screen>
          );
        }"#));
    let image = AppImage::parse(&image).unwrap();

    for value in [
        SystemValue::StepTrip,
        SystemValue::StepDayCount,
        SystemValue::StepHourCount,
    ] {
        assert!(image_runs(&image, Instruction::ReadSystem(value)), "{value:?}");
    }
    assert!(image_runs(
        &image,
        Instruction::ReadSystemElement(SystemValue::StepDayTotal)
    ));
    assert!(image_runs(
        &image,
        Instruction::ReadSystemElement(SystemValue::StepHourSteps)
    ));
    assert!(image_runs(&image, Instruction::Effect(EffectKind::ResetStepTrip)));
}

const PACKAGES_SOURCE: &str = r#"import { app_memory_budget, packages, set_app_enabled } from "neotime";
        function toggle(place: number) {
          set_app_enabled(packages.value[place].id, !packages.value[place].enabled);
        }
        function Main() {
          return (
            <Screen>
              <Label>{`${packages.value.length} ${app_memory_budget.value}`}</Label>
              <Label>{packages.value[0].name}</Label>
              <Label>{`${packages.value[0].memory} ${packages.value[0].refusal}`}</Label>
              <Label>{packages.value[0].is_face ? "Face" : "App"}</Label>
              <Button onClick={() => toggle(0)}>Toggle</Button>
            </Screen>
          );
        }"#;

#[test]
fn the_installed_packages_are_read_one_field_at_a_time_and_turned_on_by_id() {
    use neotime_runtime::instruction::Instruction;
    let source = format!(
        "{HEADER}\n{PACKAGES_SOURCE}\nexport default define_app({{ id: \"t\", name: \"T\", screens: {{ main: Main }} }});"
    );
    let image = compile(&source);
    let image = AppImage::parse(&image).unwrap();

    for value in [SystemValue::PackageCount, SystemValue::AppMemoryBudget] {
        assert!(image_runs(&image, Instruction::ReadSystem(value)), "{value:?}");
    }
    for value in [
        SystemValue::PackageName,
        SystemValue::PackageId,
        SystemValue::PackageEnabled,
        SystemValue::PackageIsFace,
        SystemValue::PackageMemory,
        SystemValue::PackageRefusal,
    ] {
        assert!(
            image_runs(&image, Instruction::ReadSystemElement(value)),
            "{value:?}"
        );
    }
    assert!(image_runs(&image, Instruction::Effect(EffectKind::SetAppEnabled)));
}

#[test]
fn the_about_facts_and_the_task_table_are_read_from_the_system() {
    use neotime_runtime::instruction::Instruction;
    let image = compile(&app(
        r#"import { bootloader_version, flash_id, free_heap_bytes, stack_overflows, tasks, touch_chip_id } from "neotime";
        function Main() {
          return (
            <Screen>
              <Label>{`${bootloader_version.value} ${flash_id.value & 255}`}</Label>
              <Label>{`${free_heap_bytes.value} ${stack_overflows.value} ${touch_chip_id.value}`}</Label>
              <Label>{`${tasks.value.length} ${tasks.value[0].name} ${tasks.value[0].runs} ${tasks.value[0].busy_milliseconds}`}</Label>
            </Screen>
          );
        }"#,
    ));
    let image = AppImage::parse(&image).unwrap();

    for value in [
        SystemValue::BootloaderVersion,
        SystemValue::FlashId,
        SystemValue::FreeHeapBytes,
        SystemValue::StackOverflows,
        SystemValue::TouchChipId,
        SystemValue::TaskCount,
    ] {
        assert!(image_runs(&image, Instruction::ReadSystem(value)), "{value:?}");
    }
    for value in [
        SystemValue::TaskName,
        SystemValue::TaskRuns,
        SystemValue::TaskBusyMilliseconds,
    ] {
        assert!(
            image_runs(&image, Instruction::ReadSystemElement(value)),
            "{value:?}"
        );
    }
}

#[test]
fn turning_apps_on_and_off_needs_the_settings_permission() {
    let output = compile_source(&app(PACKAGES_SOURCE));

    assert_eq!(output.permissions, [crate::permissions::Permission::Settings]);
}

#[test]
fn a_twenty_millisecond_timer_needs_while_open() {
    let source = |period: u32| {
        format!(
            r#"{HEADER}
import {{ on }} from "neotime";
const ticks = signal(0);
function tick() {{ ticks.value = ticks.value + 1; }}
function Main() {{ return <Screen><Label>{{`${{ticks.value}}`}}</Label></Screen>; }}
on("tick", tick, {{ every: {period}, while_open: true }});
export default define_app({{ id: "test", name: "Test", screens: {{ main: Main }} }});
"#
        )
    };
    compile(&source(20));
    assert!(first_error(&source(10)).contains("at most every 20 milliseconds"));
}

#[test]
fn an_app_with_too_many_changing_values_is_refused() {
    let labels: String = (0..140)
        .map(|place| format!("<Label>{{`${{ticks.value + {place}}}`}}</Label>"))
        .collect();
    let source = app(&format!(
        "const ticks = signal(0);\nfunction Main() {{ return <Screen>{labels}</Screen>; }}"
    ));
    assert!(first_error(&source).contains("at most 128"));
}

#[test]
fn a_text_file_keeps_its_paths_as_strings() {
    let image = compile(&app(r##"import { TextFile } from "neotime/widgets";
        function Main() { return <Screen><TextFile path="/notes/note.txt" font_path="/notes/font" placeholder="No note" grow={1} /></Screen>; }"##));
    let image = AppImage::parse(&image).unwrap();

    for (property, expected) in [
        (Property::Path, "/notes/note.txt"),
        (Property::FontPath, "/notes/font"),
        (Property::Placeholder, "No note"),
    ] {
        let PropertySource::Constant(string) = property_of(&image, WidgetKind::TextFile, property, 0) else {
            panic!("{property:?} is not a constant");
        };
        assert_eq!(image.string(string as u16), Some(expected));
    }
}

#[test]
fn a_bitmap_file_keeps_its_path_as_a_string() {
    let image = compile(&app(r##"import { BitmapFile } from "neotime/widgets";
        function Main() { return <Screen><BitmapFile path="/qr/code" color="#000000" grow={1} /></Screen>; }"##));
    let image = AppImage::parse(&image).unwrap();

    let PropertySource::Constant(string) = property_of(&image, WidgetKind::BitmapFile, Property::Path, 0)
    else {
        panic!("the path is not a constant");
    };
    assert_eq!(image.string(string as u16), Some("/qr/code"));
}

#[test]
fn a_bitmap_file_takes_a_number_worked_out_from_state() {
    let image = compile(&app(r##"import { BitmapFile } from "neotime/widgets";
        const slot = signal(0);
        function Main() { return <Screen><BitmapFile path="/qr/" value={slot.value} grow={1} /></Screen>; }"##));
    let image = AppImage::parse(&image).unwrap();

    assert!(matches!(
        property_of(&image, WidgetKind::BitmapFile, Property::Value, 0),
        PropertySource::Expression(_)
    ));
}

#[test]
fn a_bitmap_file_path_must_be_written_out() {
    let error = first_error(&app(r##"import { BitmapFile } from "neotime/widgets";
        const name = signal(0);
        function Main() { return <Screen><BitmapFile path={`/qr/${name.value}`} /></Screen>; }"##));

    assert!(
        error.contains("`path` on `BitmapFile` must be text written out"),
        "{error}"
    );
}

#[test]
fn a_text_file_path_must_be_written_out() {
    let error = first_error(&app(r##"import { TextFile } from "neotime/widgets";
        const name = signal(0);
        function Main() { return <Screen><TextFile path={`/notes/${name.value}`} /></Screen>; }"##));

    assert!(
        error.contains("`path` on `TextFile` must be text written out"),
        "{error}"
    );
}

// ============================================================================
// Launcher groups
// ============================================================================

#[test]
fn an_app_declares_its_launcher_group() {
    let source = format!(
        "{HEADER}\nfunction Main() {{ return <Screen />; }}\nexport default define_app({{ id: \"snake\", name: \"Snake\", group: \"games\", screens: {{ main: Main }} }});"
    );
    let bytes = compile(&source);
    let image = AppImage::parse(&bytes).unwrap();
    assert_eq!(
        image.group_string.and_then(|index| image.string(index)),
        Some("games")
    );
}

#[test]
fn a_group_must_be_a_short_plain_id() {
    let source = format!(
        "{HEADER}\nfunction Main() {{ return <Screen />; }}\nexport default define_app({{ id: \"snake\", group: \"Fun games!\", screens: {{ main: Main }} }});"
    );
    assert!(first_error(&source).contains("`group` is up to 16 letters"));
}
