//! Tests for the `neotime` array functions the interpreter answers natively.

use neotime_runtime::instruction::{decode, ArrayQuestion, ArrayWork, Instruction};
use neotime_runtime::{AppImage, AppState, Interpreter, SystemValue, SystemValues, Value};

use crate::compile_source;

struct System;

impl SystemValues for System {
    fn read(&self, _value: SystemValue) -> Value {
        0
    }

    fn random_number(&self) -> u32 {
        7
    }
}

/// An app with a `board` signal, `body` before its screen, and a screen holding `screen`.
fn source(body: &str, screen: &str) -> String {
    format!(
        r#"
import {{ signal }} from "@preact/signals";
import {{ define_app, count_of, nth_index_of, line_through, can_slide, slide_changes, slide_points, slide_grid, life_step, arrays_equal, push_shift, unshift_pop, type SwipeEvent }} from "neotime";
import {{ Label, Screen }} from "neotime/widgets";

const board = signal([1, 0, 1, 2]);
const other = signal([1, 0, 1, 2]);
{body}

function Main() {{
  return <Screen onSwipe={{swipe}}>{screen}</Screen>;
}}

export default define_app({{ id: "test", name: "Test", screens: {{ main: Main }} }});
"#
    )
}

fn compiled(body: &str, screen: &str) -> Vec<u8> {
    let source = source(body, screen);
    let output = compile_source(&source);
    let errors: Vec<String> = output
        .errors()
        .map(|error| error.render("test.tsx", &source))
        .collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    output.image.expect("an image")
}

fn first_error(body: &str, screen: &str) -> String {
    let output = compile_source(&source(body, screen));
    let message = output.errors().next().expect("an error").message.clone();
    message
}

fn instructions(code: &[u8]) -> Vec<Instruction> {
    let mut found = Vec::new();
    let mut offset = 0;
    while offset < code.len() {
        let (instruction, next) = decode(code, offset).unwrap();
        found.push(instruction);
        offset = next;
    }
    found
}

const NO_SWIPE: &str = "function swipe(event: SwipeEvent) {}";

fn shown(image_bytes: &[u8]) -> String {
    let image = AppImage::parse(image_bytes).unwrap();
    let state = AppState::new(&image).unwrap();
    let mut interpreter = Interpreter::new();
    let value = interpreter
        .evaluate_expression(&image, 0, &state, &System)
        .unwrap();
    interpreter.text(&image, value).unwrap().to_string()
}

#[test]
fn questions_about_an_array_signal_are_asked_natively() {
    let bytes = compiled(
        NO_SWIPE,
        "<Label>{`${count_of(board.value, 1)} ${nth_index_of(board.value, 1, 1)} ${line_through(board.value, 2, 0, 1)} ${board.value.indexOf(2)} ${board.value.includes(5) ? 1 : 0} ${arrays_equal(board.value, other.value) ? 1 : 0}`}</Label>",
    );
    let image = AppImage::parse(&bytes).unwrap();
    let code = instructions(image.expression_code(0).unwrap());

    assert_eq!(shown(&bytes), "2 2 2 3 0 1");
    assert!(!code.contains(&Instruction::LoopStart));
    assert!(code.iter().any(|instruction| matches!(
        instruction,
        Instruction::AskArray {
            question: ArrayQuestion::NthIndexOf,
            ..
        }
    )));
}

#[test]
fn slide_questions_take_a_direction_and_can_slide_asks_about_all_four() {
    let bytes = compiled(
        NO_SWIPE,
        r#"<Label>{`${slide_points(board.value, 2, "left")} ${slide_changes(board.value, 2, "up") ? 1 : 0} ${can_slide(board.value, 2) ? 1 : 0}`}</Label>"#,
    );

    assert_eq!(shown(&bytes), "0 1 1");
}

#[test]
fn a_handler_rewrites_a_grid_straight_into_its_signal() {
    let bytes = compiled(
        "function swipe(event: SwipeEvent) { board.value = slide_grid(board.value, 2, event.direction); other.value = life_step(board.value, 2, 1); }",
        "<Label>{`${board.value[0]}`}</Label>",
    );
    let image = AppImage::parse(&bytes).unwrap();
    let code = instructions(image.handler_code(0).unwrap());
    let works: Vec<ArrayWork> = code
        .iter()
        .filter_map(|instruction| match instruction {
            Instruction::WorkArray { work, .. } => Some(*work),
            _ => None,
        })
        .collect();

    assert_eq!(works, [ArrayWork::Slide, ArrayWork::LifeStep]);
    assert!(!code.contains(&Instruction::LoopStart));
}

#[test]
fn a_history_takes_a_new_value_without_a_copy() {
    let bytes = compiled(
        "function swipe(event: SwipeEvent) { board.value = push_shift(board.value, 7); other.value = unshift_pop(other.value, board.value[0]); }",
        "<Label>{`${board.value[0]}`}</Label>",
    );
    let image = AppImage::parse(&bytes).unwrap();
    let code = instructions(image.handler_code(0).unwrap());
    let works: Vec<ArrayWork> = code
        .iter()
        .filter_map(|instruction| match instruction {
            Instruction::WorkArray { work, .. } => Some(*work),
            _ => None,
        })
        .collect();

    assert_eq!(works, [ArrayWork::PushLast, ArrayWork::PushFirst]);
    assert_eq!(image.state_count(), 2);
    assert!(!code.contains(&Instruction::LoopStart));
}

#[test]
fn array_functions_need_array_signals_and_new_grids_need_an_assignment() {
    assert!(
        first_error(NO_SWIPE, "<Label>{`${count_of([1, 2], 1)}`}</Label>").contains("needs an array signal")
    );
    let error = first_error(
        "function swipe(event: SwipeEvent) { life_step(board.value, 2, 1); }",
        "<Label>{`${board.value[0]}`}</Label>",
    );
    assert!(error.contains("assign what"), "{error}");
    let error = first_error(NO_SWIPE, "<Label>{`${life_step(board.value, 2, 1)[0]}`}</Label>");
    assert!(error.contains("keep the array in a signal"), "{error}");
    assert!(
        first_error(NO_SWIPE, "<Label>{`${count_of(board.value)}`}</Label>")
            .contains("takes an array and 1 more")
    );
    let short = "const short = signal([1, 2]);\nfunction swipe(event: SwipeEvent) { board.value = slide_grid(short.value, 2, event.direction); }";
    assert!(first_error(short, "<Label>{`${board.value[0]}`}</Label>").contains("as long as"));
}

#[test]
fn an_array_is_stored_wide_enough_for_what_native_work_writes_into_it() {
    let bytes = compiled(
        "function swipe(event: SwipeEvent) { board.value = push_shift(board.value, 980); other.value = life_step(other.value, 2, 300); }",
        "<Label>{`${board.value[0]}`}</Label>",
    );
    let image = AppImage::parse(&bytes).unwrap();
    let width_of = |variable: u16| image.state_variable(variable).unwrap().element_bytes;

    assert!(width_of(0) >= 2);
    assert!(width_of(1) >= 2);
}
