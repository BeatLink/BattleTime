//! Tests for the app's own functions compiled once and called: each compiles an app, checks how many
//! functions its image holds, and runs its code with the runtime's interpreter.

use battletime_runtime::{
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

fn app(body: &str) -> String {
    format!(
        r#"
import {{ signal }} from "@preact/signals";
import {{ define_app, range }} from "battletime";
import {{ Button, Label, Screen }} from "battletime/widgets";

{body}

export default define_app({{ id: "test", name: "Test", screens: {{ main: Main }} }});
"#
    )
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

/// The sources of every property of one kind on widgets of one kind, in order.
fn sources(image: &AppImage, kind: WidgetKind, wanted: Property) -> Vec<PropertySource> {
    let tree = WidgetTree::new(*image);
    (0..tree.widget_count())
        .filter_map(|index| tree.widget(index))
        .filter(|widget| widget.kind == kind)
        .filter_map(|widget| {
            widget
                .properties()
                .flatten()
                .find(|entry| entry.property == wanted)
                .map(|entry| entry.source)
        })
        .collect()
}

fn click_every_button(image: &AppImage, state: &mut AppState) {
    for source in sources(image, WidgetKind::Button, Property::OnClick) {
        let PropertySource::Handler(handler) = source else {
            panic!("a button without a handler");
        };
        Interpreter::new()
            .run_handler(image, handler, state, &[0, 0, 0], &FakeSystem)
            .expect("the handler runs");
    }
}

fn label_texts(image: &AppImage, state: &AppState) -> Vec<String> {
    sources(image, WidgetKind::Label, Property::Text)
        .into_iter()
        .map(|source| match source {
            PropertySource::Expression(expression) => {
                let mut interpreter = Interpreter::new();
                let value = interpreter
                    .evaluate_expression(image, expression, state, &FakeSystem)
                    .expect("the expression runs");
                interpreter.text(image, value).expect("text").to_string()
            }
            PropertySource::Constant(string) => image.string(string as u16).expect("a string").to_string(),
            PropertySource::Handler(_) => panic!("a label with a handler"),
        })
        .collect()
}

fn read_variable(image: &AppImage, state: &AppState, key: &str, index: i64) -> Value {
    let variable = (0..image.state_count())
        .find(|&number| {
            let variable = image.state_variable(number).expect("a variable");
            image.string(variable.key_string) == Some(key)
        })
        .unwrap_or_else(|| panic!("no variable {key}")) as u8;
    match image
        .state_variable(variable as u16)
        .expect("a variable")
        .is_array()
    {
        true => state.element(variable, index).expect("an element"),
        false => state.get(variable).expect("a value"),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn a_function_called_from_several_handlers_is_compiled_once() {
    let source = app(r#"
const total = signal(0);
const board = signal([0, 0, 0, 0, 0, 0, 0, 0, 0]);
function score_of(cell: number, weight: number): number {
  let sum = 0;
  for (const other of range(9)) {
    if (board.value[other] === board.value[cell]) {
      sum += weight;
    }
  }
  return sum + cell * weight;
}
function add_a() { total.value += score_of(1, 2); }
function add_b() { total.value += score_of(4, 3); }
function add_c() { total.value += score_of(7, 5); }
function Main() {
  return <Screen>
    <Button onClick={add_a}>A</Button>
    <Button onClick={add_b}>B</Button>
    <Button onClick={add_c}>C</Button>
  </Screen>;
}
"#);
    let bytes = compile(&source);
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();

    click_every_button(&image, &mut state);

    assert_eq!(image.function_count(), 1);
    assert_eq!(image.function(0).unwrap().argument_count, 2);
    assert_eq!(
        read_variable(&image, &state, "total", 0),
        (18 + 2) + (27 + 12) + (45 + 35)
    );
}

#[test]
fn expressions_call_a_function_that_makes_text() {
    let source = app(r#"
const a = signal(5);
const b = signal(7);
const c = signal(9);
function describe(n: number): string {
  return n > 6 ? `big ${n * 10 + 3}` : `small ${n - 1}`;
}
function Main() {
  return <Screen>
    <Label>{describe(a.value)}</Label>
    <Label>{describe(b.value)}</Label>
    <Label>{describe(c.value)}</Label>
  </Screen>;
}
"#);
    let bytes = compile(&source);
    let image = AppImage::parse(&bytes).unwrap();
    let state = AppState::new(&image).unwrap();

    assert_eq!(image.function_count(), 1);
    assert_eq!(label_texts(&image, &state), ["small 4", "big 73", "big 93"]);
}

#[test]
fn arrays_a_called_function_writes_keep_the_width_its_arguments_need() {
    let source = app(r#"
const cells = signal([0, 0, 0, 0]);
function paint(cell: number, colour: number) {
  cells.value[cell] = colour;
  cells.value[(cell + 1) % 4] = colour;
  cells.value[(cell + 2) % 4] = colour;
  cells.value[(cell + 3) % 4] = colour + 1;
}
function first() { paint(0, 1); }
function second() { paint(1, 2); }
function third() { paint(2, 3); }
function Main() {
  return <Screen>
    <Button onClick={first}>1</Button>
    <Button onClick={second}>2</Button>
    <Button onClick={third}>3</Button>
  </Screen>;
}
"#);
    let bytes = compile(&source);
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let cells = (0..image.state_count())
        .filter_map(|number| image.state_variable(number))
        .find(|variable| image.string(variable.key_string) == Some("cells"))
        .unwrap();

    click_every_button(&image, &mut state);

    assert_eq!(image.function_count(), 1);
    assert_eq!(cells.element_bytes, 1);
    let painted: Vec<Value> = (0..4)
        .map(|index| read_variable(&image, &state, "cells", index))
        .collect();
    assert_eq!(painted, [3, 4, 3, 3]);
}

#[test]
fn a_function_that_needs_its_argument_while_compiling_is_inlined() {
    let source = app(r#"
const total = signal(0);
function sum_below(count: number): number {
  let sum = 0;
  for (const number of range(count)) {
    sum += number;
  }
  return sum;
}
function add_three() { total.value += sum_below(3); }
function add_five() { total.value += sum_below(5); }
function add_seven() { total.value += sum_below(7); }
function Main() {
  return <Screen>
    <Button onClick={add_three}>3</Button>
    <Button onClick={add_five}>5</Button>
    <Button onClick={add_seven}>7</Button>
  </Screen>;
}
"#);
    let bytes = compile(&source);
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();

    click_every_button(&image, &mut state);

    assert_eq!(read_variable(&image, &state, "total", 0), 3 + 10 + 21);
}

#[test]
fn a_function_smaller_than_its_calls_stays_inlined() {
    let source = app(r#"
const first = signal(0);
const second = signal(1);
function next(value: number): number { return (value + 1) % 3; }
function step_first() { first.value = next(first.value); }
function step_second() { second.value = next(second.value); }
function Main() {
  return <Screen>
    <Button onClick={step_first}>1</Button>
    <Button onClick={step_second}>2</Button>
  </Screen>;
}
"#);
    let bytes = compile(&source);
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();

    click_every_button(&image, &mut state);

    assert_eq!(image.function_count(), 0);
    assert_eq!(read_variable(&image, &state, "first", 0), 1);
    assert_eq!(read_variable(&image, &state, "second", 0), 2);
}

#[test]
fn a_called_function_may_call_another() {
    let source = app(r#"
const total = signal(0);
const weights = signal([3, 1, 4, 1, 5, 9, 2, 6]);
function weight_at(cell: number): number {
  let best = 0;
  for (const index of range(8)) {
    if (index <= cell && weights.value[index] > best) {
      best = weights.value[index];
    }
  }
  return best;
}
function both(first: number, second: number): number {
  let sum = 0;
  for (const index of range(2)) {
    sum += index === 0 ? weight_at(first) : weight_at(second);
  }
  return sum * 10 + first;
}
function add_a() { total.value += both(1, 4); }
function add_b() { total.value += both(2, 6); }
function add_c() { total.value += both(0, 7); }
function Main() {
  return <Screen>
    <Button onClick={add_a}>A</Button>
    <Button onClick={add_b}>B</Button>
    <Button onClick={add_c}>C</Button>
  </Screen>;
}
"#);
    let bytes = compile(&source);
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();

    click_every_button(&image, &mut state);

    assert!(image.function_count() >= 2);
    assert_eq!(
        read_variable(&image, &state, "total", 0),
        (3 + 5) * 10 + 1 + (4 + 9) * 10 + 2 + (3 + 9) * 10
    );
}
