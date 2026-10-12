//! Tests for values kept in hidden variables: each compiles an app, runs its derive handlers with the
//! runtime's interpreter and reads the hidden variable back.

use neotime_runtime::subscriptions::{SubscriptionKind, SubscriptionValue};
use neotime_runtime::{
    AppImage, AppState, Interpreter, Property, PropertySource, SystemValue, SystemValues, Value, WidgetKind,
    WidgetTree,
};

use crate::compile_source;

// ============================================================================
// Helpers
// ============================================================================

struct FakeSystem;

impl SystemValues for FakeSystem {
    fn read(&self, _value: SystemValue) -> Value {
        0
    }

    fn random_number(&self) -> u32 {
        7
    }
}

/// An app with the given declarations and main screen, granted the `background` permission.
fn app_with(declarations: &str, screen_body: &str, options: &str) -> String {
    format!(
        r#"
import {{ computed, signal }} from "@preact/signals";
import {{ define_app, at, daily, now, type Instant, type TimeOfDay }} from "neotime";
import {{ Chart, Line, Roller, Screen }} from "neotime/widgets";

{declarations}
function tick() {{}}
function Main() {{
  return <Screen>{screen_body}</Screen>;
}}

export default define_app({{
  id: "test", name: "Test", screens: {{ main: Main }}, permissions: ["background"],
  {options}
}});
"#
    )
}

/// An app whose main screen is `screen_body`.
fn screen_app(declarations: &str, screen_body: &str) -> String {
    app_with(declarations, screen_body, "")
}

fn compile(source: &str) -> Vec<u8> {
    let output = compile_source(source);
    let errors: Vec<String> = output.errors().map(|error| error.message.clone()).collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    output.image.expect("an image")
}

fn first_error(source: &str) -> String {
    let output = compile_source(source);
    let message = output.errors().next().expect("an error").message.clone();
    message
}

fn first_warning(source: &str) -> String {
    let output = compile_source(source);
    let message = output.warnings().next().expect("a warning").message.clone();
    message
}

/// The source of a property on the nth widget of a kind that has it.
fn property_of(image: &AppImage, kind: WidgetKind, wanted: Property, nth: usize) -> Option<PropertySource> {
    let tree = WidgetTree::new(*image);
    (0..tree.widget_count())
        .filter_map(|index| tree.widget(index))
        .filter(|widget| widget.kind == kind)
        .filter_map(|widget| {
            widget
                .properties()
                .map(|entry| entry.expect("a property"))
                .find(|entry| entry.property == wanted)
                .map(|entry| entry.source)
        })
        .nth(nth)
}

/// The state variable an array property names.
fn array_variable(image: &AppImage, kind: WidgetKind, wanted: Property, nth: usize) -> u8 {
    match property_of(image, kind, wanted, nth) {
        Some(PropertySource::Constant(variable)) => variable as u8,
        other => panic!("{kind:?} {wanted:?} is {other:?}"),
    }
}

/// The state variable an app keeps under `key`.
fn variable_named(image: &AppImage, key: &str) -> u8 {
    (0..image.state_count())
        .find(|&index| {
            let variable = image.state_variable(index).expect("a variable");
            image.string(variable.key_string) == Some(key)
        })
        .unwrap_or_else(|| panic!("no variable {key}")) as u8
}

/// Runs every derive handler once, as the runtime does when the app opens.
fn run_derive_handlers(image: &AppImage, state: &mut AppState) {
    for index in 0..image.subscription_count() {
        let subscription = image.subscription(index).unwrap();
        if subscription.kind == SubscriptionKind::Derive {
            Interpreter::new()
                .run_handler(image, subscription.handler, state, &[], &FakeSystem)
                .expect("the derive handler runs");
        }
    }
}

/// Every element of an array variable.
fn elements_of(image: &AppImage, state: &AppState, variable: u8) -> Vec<Value> {
    let length = image.state_variable(variable as u16).unwrap().length;
    (0..length as Value)
        .map(|index| state.element(variable, index).unwrap())
        .collect()
}

/// How many derive subscriptions an image lists.
fn derive_count(image: &AppImage) -> usize {
    (0..image.subscription_count())
        .filter(|&index| image.subscription(index).unwrap().kind == SubscriptionKind::Derive)
        .count()
}

/// The state variable the first subscription of a kind waits on.
fn subscription_variable(image: &AppImage, kind: SubscriptionKind) -> u8 {
    let subscription = (0..image.subscription_count())
        .map(|index| image.subscription(index).unwrap())
        .find(|subscription| subscription.kind == kind)
        .expect("the subscription");
    match subscription.value {
        SubscriptionValue::Variable(variable) => variable,
        other => panic!("{kind:?} waits on {other:?}"),
    }
}

// ============================================================================
// Line points
// ============================================================================

#[test]
fn a_moving_line_point_is_worked_out_by_a_derive_handler() {
    let bytes = compile(&screen_app(
        "const offset = signal(0);",
        "<Line points={[{ x: 10, y: offset.value }, { x: 20, y: 30 }]} />",
    ));
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let points = array_variable(&image, WidgetKind::Line, Property::Points, 0);
    state.set(variable_named(&image, "offset"), 7).unwrap();

    run_derive_handlers(&image, &mut state);

    assert_eq!(elements_of(&image, &state, points), vec![10, 7, 20, 30]);
}

#[test]
fn a_point_known_whole_sits_beside_one_worked_out() {
    let bytes = compile(&screen_app(
        "const offset = signal(0); const corner = { x: 1, y: 2 };",
        "<Line points={[corner, { x: offset.value, y: 3 }]} />",
    ));
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let points = array_variable(&image, WidgetKind::Line, Property::Points, 0);
    state.set(variable_named(&image, "offset"), 9).unwrap();

    run_derive_handlers(&image, &mut state);

    assert_eq!(elements_of(&image, &state, points), vec![1, 2, 9, 3]);
}

#[test]
fn line_points_named_by_a_local_constant_are_followed() {
    let source = r#"
import { signal } from "@preact/signals";
import { define_app } from "neotime";
import { Line, Screen } from "neotime/widgets";
const offset = signal(0);
function Main() {
  const path = [{ x: offset.value, y: 4 }, { x: 5, y: 6 }];
  return <Screen><Line points={path} /></Screen>;
}
export default define_app({ id: "test", name: "Test", screens: { main: Main } });
"#;
    let bytes = compile(source);
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let points = array_variable(&image, WidgetKind::Line, Property::Points, 0);
    state.set(variable_named(&image, "offset"), 3).unwrap();

    run_derive_handlers(&image, &mut state);

    assert_eq!(elements_of(&image, &state, points), vec![3, 4, 5, 6]);
}

#[test]
fn line_points_not_written_as_points_are_left_out_with_a_warning() {
    let source = screen_app("const offset = signal(0);", "<Line points={[offset.value, 4]} />");

    let warning = first_warning(&source);

    assert!(warning.starts_with("`points` on `Line` is left out"), "{warning}");
}

#[test]
fn a_point_missing_its_y_is_left_out_with_a_warning() {
    let source = screen_app(
        "const offset = signal(0);",
        "<Line points={[{ x: offset.value }]} />",
    );

    let warning = first_warning(&source);

    assert!(warning.starts_with("`points` on `Line` is left out"), "{warning}");
}

// ============================================================================
// Derived arrays
// ============================================================================

#[test]
fn the_same_computed_shown_twice_shares_one_hidden_array() {
    let bytes = compile(&screen_app(
        "const count = signal(0); const totals = computed(() => [count.value, 2, 3]);",
        "<Chart values={totals.value} /><Chart values={totals.value} />",
    ));
    let image = AppImage::parse(&bytes).unwrap();

    let first = array_variable(&image, WidgetKind::Chart, Property::Series, 0);
    let second = array_variable(&image, WidgetKind::Chart, Property::Series, 1);

    assert_eq!(first, second);
    assert_eq!(derive_count(&image), 1);
}

#[test]
fn a_conditional_array_is_as_long_as_its_longer_branch() {
    let bytes = compile(&screen_app(
        "const flag = signal(true); const count = signal(0);",
        "<Chart values={flag.value ? [count.value, 1, 2] : [count.value, 5, 6]} />",
    ));
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let values = array_variable(&image, WidgetKind::Chart, Property::Series, 0);
    state.set(variable_named(&image, "flag"), 0).unwrap();
    state.set(variable_named(&image, "count"), 4).unwrap();

    run_derive_handlers(&image, &mut state);

    assert_eq!(elements_of(&image, &state, values), vec![4, 5, 6]);
}

#[test]
fn shorter_text_options_are_padded_with_empty_text() {
    let bytes = compile(&screen_app(
        "const flag = signal(true);",
        r#"<Roller options={flag.value ? ["one", "two", "three"] : ["only"]} />"#,
    ));
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let options = array_variable(&image, WidgetKind::Roller, Property::Options, 0);
    state.set(variable_named(&image, "flag"), 0).unwrap();

    run_derive_handlers(&image, &mut state);

    let texts: Vec<Option<&str>> = elements_of(&image, &state, options)
        .into_iter()
        .map(|string| image.string(string as u16))
        .collect();
    assert_eq!(texts, vec![Some("only"), Some(""), Some("")]);
}

#[test]
fn an_array_returned_by_a_function_has_that_functions_length() {
    let bytes = compile(&screen_app(
        "const count = signal(0); function week() { const doubled = count.value * 2; return [doubled, 1]; }",
        "<Chart values={week()} />",
    ));
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let values = array_variable(&image, WidgetKind::Chart, Property::Series, 0);
    state.set(variable_named(&image, "count"), 5).unwrap();

    run_derive_handlers(&image, &mut state);

    assert_eq!(elements_of(&image, &state, values), vec![10, 1]);
}

#[test]
fn an_array_returned_by_an_arrow_function_has_its_length() {
    let bytes = compile(&screen_app(
        "const count = signal(0); const week = () => [count.value, 8, 9];",
        "<Chart values={week()} />",
    ));
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let values = array_variable(&image, WidgetKind::Chart, Property::Series, 0);
    state.set(variable_named(&image, "count"), 2).unwrap();

    run_derive_handlers(&image, &mut state);

    assert_eq!(elements_of(&image, &state, values), vec![2, 8, 9]);
}

#[test]
fn an_array_from_a_function_without_a_final_return_is_left_out_with_a_warning() {
    let source = screen_app(
        "const count = signal(0); function week() { if (count.value > 0) { return [1, 2]; } count.value; }",
        "<Chart values={week()} />",
    );

    let warning = first_warning(&source);

    assert!(
        warning.starts_with("`values` on `Chart` is left out"),
        "{warning}"
    );
}

#[test]
fn a_mapped_signal_array_keeps_the_signals_length() {
    let bytes = compile(&screen_app(
        "const levels = signal([1, 2, 3, 4]);",
        "<Chart values={levels.value.map((level) => level * 10)} />",
    ));
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let values = array_variable(&image, WidgetKind::Chart, Property::Series, 0);

    run_derive_handlers(&image, &mut state);

    assert_eq!(elements_of(&image, &state, values), vec![10, 20, 30, 40]);
}

#[test]
fn a_signal_array_in_a_conditional_counts_with_its_own_length() {
    let bytes = compile(&screen_app(
        "const flag = signal(true); const levels = signal([1, 2, 3]);",
        "<Chart values={flag.value ? levels.value : [7, 8, 9]} />",
    ));
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let values = array_variable(&image, WidgetKind::Chart, Property::Series, 0);

    run_derive_handlers(&image, &mut state);

    assert_eq!(elements_of(&image, &state, values), vec![1, 2, 3]);
}

// ============================================================================
// Derived scalars
// ============================================================================

#[test]
fn at_a_number_signal_copies_it_into_a_hidden_instant() {
    let bytes = compile(&app_with(
        "const count = signal(0);",
        "",
        "subscriptions: [at(count, tick)],",
    ));
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let hidden = subscription_variable(&image, SubscriptionKind::At);
    state.set(variable_named(&image, "count"), 1234).unwrap();

    run_derive_handlers(&image, &mut state);

    assert_ne!(hidden, variable_named(&image, "count"));
    assert_eq!(state.get(hidden), Some(1234));
}

#[test]
fn at_a_computed_time_keeps_what_the_computed_gives() {
    let bytes = compile(&app_with(
        "const start = signal<Instant>(0); const ring_at = computed(() => start.value + 60000);",
        "",
        "subscriptions: [at(ring_at, tick)],",
    ));
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let hidden = subscription_variable(&image, SubscriptionKind::At);
    state.set(variable_named(&image, "start"), 1000).unwrap();

    run_derive_handlers(&image, &mut state);

    assert_eq!(state.get(hidden), Some(61_000));
}

#[test]
fn at_an_inline_expression_keeps_its_value() {
    let bytes = compile(&app_with(
        "const start = signal<Instant>(0);",
        "",
        "subscriptions: [at(start.value + 500, tick)],",
    ));
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let hidden = subscription_variable(&image, SubscriptionKind::At);
    state.set(variable_named(&image, "start"), 2000).unwrap();

    run_derive_handlers(&image, &mut state);

    assert_eq!(state.get(hidden), Some(2500));
}

#[test]
fn daily_at_a_time_of_day_signal_keeps_its_minute_of_the_day() {
    let bytes = compile(&app_with(
        "const wake = signal<TimeOfDay>({ hour: 8, minute: 0 });",
        "",
        "subscriptions: [daily(wake, tick)],",
    ));
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let hidden = subscription_variable(&image, SubscriptionKind::Daily);

    run_derive_handlers(&image, &mut state);

    assert_eq!(state.get(hidden), Some(8 * 60));
}

#[test]
fn at_a_computed_that_gives_text_is_refused() {
    let source = app_with(
        r#"const count = signal(0); const label = computed(() => count.value > 0 ? "soon" : "later");"#,
        "",
        "subscriptions: [at(label, tick)],",
    );

    let message = first_error(&source);

    assert!(message.starts_with("expected"), "{message}");
}

#[test]
fn at_the_clock_itself_cannot_be_kept_in_a_variable() {
    let source = app_with("", "", "subscriptions: [at(now, tick)],");

    assert_eq!(first_error(&source), "this cannot be kept in a state variable");
}
