//! Tests for functions: the `Call` instruction, its checks and the code walker that follows it.

extern crate std;

use std::vec::Vec;

use super::*;
use crate::bindings::{analyse_expression_in, for_each_reached_instruction, for_each_reached_instruction_in};
use crate::image::{CodeBlock, CodeProblem};
use crate::instruction::{decode, Instruction, TimePart as Part};
use crate::interpreter::{LOCAL_POOL, MOST_CALL_DEPTH, MOST_LOCALS};
use crate::writer::{CodeWriter, ImageWriter};

// ============================================================================
// Helpers
// ============================================================================

#[derive(Default)]
struct System {
    now: Value,
}

impl SystemValues for System {
    fn read(&self, value: SystemValue) -> Value {
        match value {
            SystemValue::Now => self.now,
            _ => 0,
        }
    }

    fn random_number(&self) -> u32 {
        7
    }
}

fn code(instructions: &[Instruction]) -> Vec<u8> {
    let mut writer = CodeWriter::new();
    for instruction in instructions {
        writer.emit(*instruction);
    }
    writer.finish().unwrap()
}

fn new_writer() -> ImageWriter {
    ImageWriter::new("functions", "Functions", AppKind::App, 1)
}

/// A function of two arguments that gives `first * 10 + second`.
fn add_tens_and_units(writer: &mut ImageWriter) -> u8 {
    use Instruction::*;
    writer.function(
        2,
        1,
        2,
        code(&[ReadLocal(0), PushSmall(10), Multiply, ReadLocal(1), Add]),
    )
}

fn evaluate(bytes: &[u8], expression: u16, state: &AppState) -> Result<Value, Failure> {
    let image = AppImage::parse(bytes).unwrap();
    Interpreter::new().evaluate_expression(&image, expression, state, &System::default())
}

fn verify_bytes(bytes: &[u8]) -> Result<(), ImageError> {
    verify(&AppImage::parse(bytes).unwrap())
}

fn code_problem(block: CodeBlock, problem: CodeProblem) -> impl Fn(&ImageError) -> bool {
    move |error| matches!(error, ImageError::BadCode { block: found, problem: found_problem, .. } if *found == block && *found_problem == problem)
}

// ============================================================================
// Running calls
// ============================================================================

#[test]
fn a_call_gives_back_the_functions_result() {
    use Instruction::*;
    let mut writer = new_writer();
    let function = add_tens_and_units(&mut writer);
    let expression = writer.expression(code(&[PushSmall(3), PushSmall(4), Call(function)]));
    let bytes = writer.build();
    let state = AppState::new(&AppImage::parse(&bytes).unwrap()).unwrap();

    assert_eq!(verify_bytes(&bytes), Ok(()));
    assert_eq!(evaluate(&bytes, expression, &state), Ok(34));
}

#[test]
fn a_call_keeps_the_callers_locals_and_stack() {
    use Instruction::*;
    let mut writer = new_writer();
    let function = add_tens_and_units(&mut writer);
    let expression = writer.expression(code(&[
        PushSmall(7),
        WriteLocal(0),
        PushSmall(100),
        PushSmall(1),
        PushSmall(2),
        Call(function),
        Add,
        ReadLocal(0),
        Add,
    ]));
    let bytes = writer.build();
    let state = AppState::new(&AppImage::parse(&bytes).unwrap()).unwrap();

    assert_eq!(verify_bytes(&bytes), Ok(()));
    assert_eq!(evaluate(&bytes, expression, &state), Ok(100 + 12 + 7));
}

#[test]
fn functions_call_lower_numbered_functions_and_loop() {
    use Instruction::*;
    let mut writer = new_writer();
    let tens_and_units = add_tens_and_units(&mut writer);

    // Sums tens_and_units(count, 1) for count from the argument down to 1.
    let mut summing = CodeWriter::new();
    let done = summing.label();
    let top = summing.loop_start();
    summing
        .emit(ReadLocal(0))
        .emit(PushSmall(0))
        .emit(Greater)
        .jump_if_false(done);
    summing
        .emit(ReadLocal(1))
        .emit(ReadLocal(0))
        .emit(PushSmall(1))
        .emit(Call(tens_and_units))
        .emit(Add)
        .emit(WriteLocal(1));
    summing
        .emit(ReadLocal(0))
        .emit(PushSmall(1))
        .emit(Subtract)
        .emit(WriteLocal(0))
        .loop_back(top);
    summing.place(done).emit(ReadLocal(1));
    let sum = writer.function(1, 1, 2, summing.finish().unwrap());
    let expression = writer.expression(code(&[PushSmall(3), Call(sum)]));
    let bytes = writer.build();
    let state = AppState::new(&AppImage::parse(&bytes).unwrap()).unwrap();

    assert_eq!(verify_bytes(&bytes), Ok(()));
    assert_eq!(evaluate(&bytes, expression, &state), Ok(31 + 21 + 11));
}

#[test]
fn a_function_may_give_back_text() {
    use Instruction::*;
    let mut writer = new_writer();
    let padded = writer.function(1, 1, 1, code(&[ReadLocal(0), FormatNumber(3)]));
    let expression = writer.expression(code(&[PushSmall(7), Call(padded)]));
    let bytes = writer.build();
    let image = AppImage::parse(&bytes).unwrap();
    let state = AppState::new(&image).unwrap();
    let mut interpreter = Interpreter::new();

    let value = interpreter
        .evaluate_expression(&image, expression, &state, &System::default())
        .unwrap();

    assert_eq!(interpreter.text(&image, value), Ok("007"));
}

#[test]
fn a_handler_function_writes_state_and_causes_effects() {
    use Instruction::*;
    let mut writer = new_writer();
    let count = writer.variable("count", ValueType::Int, false, 5);
    let add_and_buzz = writer.function(
        1,
        0,
        1,
        code(&[
            ReadState(count),
            ReadLocal(0),
            Add,
            WriteState(count),
            PushInt(200),
            Effect(EffectKind::Vibrate),
        ]),
    );
    let handler = writer.handler(code(&[PushSmall(3), Call(add_and_buzz)]));
    let read_count = writer.expression(code(&[ReadState(count)]));
    let bytes = writer.build();
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let mut interpreter = Interpreter::new();

    let outcome = interpreter
        .run_handler(&image, handler, &mut state, &[], &System::default())
        .unwrap();

    assert_eq!(verify(&image), Ok(()));
    assert_eq!(outcome.changed_variables(), 1 << count);
    assert_eq!(outcome.effects().len(), 1);
    assert_eq!(
        interpreter.evaluate_expression(&image, read_count, &state, &System::default()),
        Ok(8)
    );
}

#[test]
fn a_function_that_takes_its_callers_values_fails_when_unchecked() {
    use Instruction::*;
    let mut writer = new_writer();
    let greedy = writer.function(0, 1, 0, code(&[Add]));
    let expression = writer.expression(code(&[PushSmall(1), PushSmall(2), Call(greedy)]));
    let bytes = writer.build();
    let state = AppState::new(&AppImage::parse(&bytes).unwrap()).unwrap();

    assert!(verify_bytes(&bytes)
        .is_err_and(|error| code_problem(CodeBlock::Function(0), CodeProblem::StackUnderflow)(&error)));
    assert_eq!(evaluate(&bytes, expression, &state), Err(Failure::StackUnderflow));
}

#[test]
fn a_function_that_leaves_the_wrong_count_fails_when_unchecked() {
    use Instruction::*;
    let mut writer = new_writer();
    let empty = writer.function(0, 1, 0, Vec::new());
    let expression = writer.expression(code(&[PushSmall(1), Call(empty)]));
    let bytes = writer.build();
    let state = AppState::new(&AppImage::parse(&bytes).unwrap()).unwrap();

    assert!(verify_bytes(&bytes)
        .is_err_and(|error| code_problem(CodeBlock::Function(0), CodeProblem::WrongResultCount)(&error)));
    assert_eq!(
        evaluate(&bytes, expression, &state),
        Err(Failure::WrongResultCount)
    );
}

#[test]
fn a_function_that_calls_itself_is_refused_and_stopped() {
    use Instruction::*;
    let mut writer = new_writer();
    let endless = writer.function(0, 1, 0, code(&[Call(0)]));
    let expression = writer.expression(code(&[Call(endless)]));
    let bytes = writer.build();
    let state = AppState::new(&AppImage::parse(&bytes).unwrap()).unwrap();

    assert!(verify_bytes(&bytes)
        .is_err_and(|error| code_problem(CodeBlock::Function(0), CodeProblem::BadCall)(&error)));
    assert_eq!(evaluate(&bytes, expression, &state), Err(Failure::CallsTooDeep));
}

#[test]
fn a_function_that_reads_past_its_locals_fails() {
    use Instruction::*;
    let mut writer = new_writer();
    let reader = writer.function(1, 1, 1, code(&[ReadLocal(1)]));
    let expression = writer.expression(code(&[PushSmall(1), Call(reader)]));
    let bytes = writer.build();
    let state = AppState::new(&AppImage::parse(&bytes).unwrap()).unwrap();

    assert!(verify_bytes(&bytes)
        .is_err_and(|error| code_problem(CodeBlock::Function(0), CodeProblem::LocalOutOfRange)(&error)));
    assert_eq!(evaluate(&bytes, expression, &state), Err(Failure::BadLocal));
}

#[test]
fn a_call_to_a_missing_function_is_refused_and_fails() {
    use Instruction::*;
    let mut writer = new_writer();
    let expression = writer.expression(code(&[Call(3)]));
    let bytes = writer.build();
    let state = AppState::new(&AppImage::parse(&bytes).unwrap()).unwrap();

    assert!(verify_bytes(&bytes)
        .is_err_and(|error| code_problem(CodeBlock::Expression(0), CodeProblem::BadCall)(&error)));
    assert_eq!(evaluate(&bytes, expression, &state), Err(Failure::NoSuchCode));
}

// ============================================================================
// Checks
// ============================================================================

#[test]
fn an_expression_may_not_call_a_function_that_acts_like_a_handler() {
    use Instruction::*;
    let mut writer = new_writer();
    let buzz = writer.function(0, 0, 0, code(&[PushInt(100), Effect(EffectKind::Vibrate)]));
    let indirect = writer.function(0, 1, 0, code(&[Call(buzz), PushSmall(1)]));
    writer.expression(code(&[Call(indirect)]));
    let bytes = writer.build();

    assert!(verify_bytes(&bytes).is_err_and(|error| code_problem(
        CodeBlock::Expression(0),
        CodeProblem::HandlerFunctionInExpression
    )(&error)));
}

#[test]
fn a_function_header_must_fit_its_arguments_and_results() {
    use Instruction::*;
    for (arguments, results, locals) in [(0, 2, 0), (2, 0, 1), (0, 0, MOST_LOCALS as u8 + 1)] {
        let mut writer = new_writer();
        writer.function(arguments, results, locals, code(&[PushSmall(1), Drop]));
        let bytes = writer.build();

        assert!(verify_bytes(&bytes)
            .is_err_and(
                |error| code_problem(CodeBlock::Function(0), CodeProblem::BadFunctionHeader)(&error)
            ));
    }
}

#[test]
fn a_function_shorter_than_its_header_is_refused() {
    let mut writer = new_writer();
    writer.function(0, 0, 0, Vec::new());
    let mut bytes = writer.build();
    // The function's table entry is the last 4 bytes before its 3 header bytes; make it 2 long.
    let entry_length = bytes.len() - 3 - 2;
    bytes[entry_length] = 2;
    let crc = crate::image::crc32(&bytes[16..]);
    bytes[12..16].copy_from_slice(&crc.to_le_bytes());

    assert!(verify_bytes(&bytes)
        .is_err_and(|error| code_problem(CodeBlock::Function(0), CodeProblem::BadFunctionHeader)(&error)));
}

#[test]
fn calls_nested_too_deep_are_refused() {
    use Instruction::*;
    let mut writer = new_writer();
    writer.function(0, 1, 0, code(&[PushSmall(1)]));
    for callee in 0..MOST_CALL_DEPTH as u8 {
        writer.function(0, 1, 0, code(&[Call(callee)]));
    }
    let bytes = writer.build();

    assert!(verify_bytes(&bytes).is_err_and(|error| code_problem(
        CodeBlock::Function(MOST_CALL_DEPTH as u16),
        CodeProblem::CallsTooDeep
    )(&error)));
}

#[test]
fn calls_whose_locals_overflow_the_pool_are_refused() {
    use Instruction::*;
    let mut writer = new_writer();
    let full = MOST_LOCALS as u8;
    writer.function(0, 1, full, code(&[PushSmall(1)]));
    let levels = (LOCAL_POOL - MOST_LOCALS) / MOST_LOCALS;
    for callee in 0..levels as u8 {
        writer.function(0, 1, full, code(&[Call(callee)]));
    }
    let bytes = writer.build();

    assert!(verify_bytes(&bytes)
        .is_err_and(
            |error| code_problem(CodeBlock::Function(levels as u16), CodeProblem::CallsTooDeep)(&error)
        ));
}

#[test]
fn a_call_whose_stack_would_overflow_its_callers_is_refused() {
    use Instruction::*;
    let mut writer = new_writer();
    let mut deep = std::vec![PushSmall(1); 20];
    deep.extend(std::vec![Add; 19]);
    let function = writer.function(0, 1, 0, code(&deep));
    let mut caller = std::vec![PushSmall(1); 13];
    caller.push(Call(function));
    caller.extend(std::vec![Add; 13]);
    writer.expression(code(&caller));
    let bytes = writer.build();
    let state = AppState::new(&AppImage::parse(&bytes).unwrap()).unwrap();

    assert!(verify_bytes(&bytes)
        .is_err_and(|error| code_problem(CodeBlock::Expression(0), CodeProblem::StackTooDeep)(&error)));
    assert_eq!(evaluate(&bytes, 0, &state), Err(Failure::StackOverflow));
}

#[test]
fn the_deepest_calls_allowed_run() {
    use Instruction::*;
    let mut writer = new_writer();
    writer.function(0, 1, 0, code(&[PushSmall(1)]));
    for callee in 0..MOST_CALL_DEPTH as u8 - 1 {
        writer.function(0, 1, 0, code(&[Call(callee), PushSmall(1), Add]));
    }
    let expression = writer.expression(code(&[Call(MOST_CALL_DEPTH as u8 - 1)]));
    let bytes = writer.build();
    let state = AppState::new(&AppImage::parse(&bytes).unwrap()).unwrap();

    assert_eq!(verify_bytes(&bytes), Ok(()));
    assert_eq!(evaluate(&bytes, expression, &state), Ok(MOST_CALL_DEPTH as Value));
}

#[test]
fn locals_past_the_pool_fail_when_unchecked() {
    use Instruction::*;
    let mut writer = new_writer();
    let full = MOST_LOCALS as u8;
    writer.function(0, 1, full, code(&[PushSmall(1)]));
    for callee in 0..3u8 {
        writer.function(0, 1, full, code(&[Call(callee)]));
    }
    let expression = writer.expression(code(&[Call(3)]));
    let bytes = writer.build();
    let state = AppState::new(&AppImage::parse(&bytes).unwrap()).unwrap();

    assert_eq!(evaluate(&bytes, expression, &state), Err(Failure::CallsTooDeep));
}

#[test]
fn a_function_with_a_bad_header_fails_when_unchecked() {
    use Instruction::*;
    let mut writer = new_writer();
    writer.function(2, 1, 1, code(&[PushSmall(1)]));
    let expression = writer.expression(code(&[PushSmall(1), PushSmall(1), Call(0)]));
    let bytes = writer.build();
    let state = AppState::new(&AppImage::parse(&bytes).unwrap()).unwrap();

    assert_eq!(evaluate(&bytes, expression, &state), Err(Failure::CallsTooDeep));
}

// ============================================================================
// Encoding and walking
// ============================================================================

#[test]
fn a_call_encodes_as_two_bytes() {
    let encoded = Instruction::Call(9).encode();

    assert_eq!(encoded.as_bytes(), &[0x2C, 9]);
    assert_eq!(decode(encoded.as_bytes(), 0), Ok((Instruction::Call(9), 2)));
}

#[test]
fn bindings_see_what_called_functions_read() {
    use Instruction::*;
    let mut writer = new_writer();
    let count = writer.variable("count", ValueType::Int, false, 0);
    let seconds = writer.function(
        0,
        1,
        0,
        code(&[ReadSystem(SystemValue::Now), TimePart(Part::Second)]),
    );
    let both = writer.function(0, 1, 0, code(&[Call(seconds), ReadState(count), Add]));
    let again = writer.function(0, 1, 0, code(&[Call(both), Call(seconds), Add]));
    writer.expression(code(&[Call(again)]));
    let bytes = writer.build();
    let image = AppImage::parse(&bytes).unwrap();

    let info = analyse_expression(&image, image.expression_code(0).unwrap()).unwrap();

    assert!(info.inputs.reads_clock());
    assert_eq!(info.inputs.state_slots, 1 << count);
    assert_eq!(info.redraw_period_in_milliseconds, 1000);
}

#[test]
fn the_walker_visits_each_called_function_once() {
    use Instruction::*;
    let mut writer = new_writer();
    let leaf = writer.function(0, 1, 0, code(&[PushSmall(1)]));
    writer.function(0, 1, 0, code(&[Call(leaf), Call(leaf), Add]));
    let bytes = writer.build();
    let image = AppImage::parse(&bytes).unwrap();
    let top = code(&[Call(1), Call(0), Add]);
    let mut pushes = 0;

    let walked = for_each_reached_instruction(&image, &top, |instruction| {
        if instruction == PushSmall(1) {
            pushes += 1;
        }
    });

    assert_eq!(walked, Some(()));
    assert_eq!(pushes, 1);
}

#[test]
fn the_walker_gives_up_on_a_missing_function() {
    let bytes = new_writer().build();
    let image = AppImage::parse(&bytes).unwrap();

    let walked = for_each_reached_instruction(&image, &code(&[Instruction::Call(0)]), |_| {});

    assert_eq!(walked, None);
}

// ============================================================================
// Calls into the shared library
// ============================================================================

fn new_library_writer() -> ImageWriter {
    ImageWriter::new("library", "Library", AppKind::Library, 1)
}

fn evaluate_with_library(bytes: &[u8], library: Option<&[u8]>, expression: u16) -> Result<Value, Failure> {
    let image = AppImage::parse(bytes).unwrap();
    let library = library.map(|bytes| AppImage::parse(bytes).unwrap());
    let state = AppState::new(&image).unwrap();
    Interpreter::new().evaluate_expression_with_library(
        &image,
        library.as_ref(),
        expression,
        &state,
        &System::default(),
    )
}

/// An app whose only expression gives `call_library(3, 4)`.
fn app_calling_the_library(function: u16, arguments: u8, results: u8) -> Vec<u8> {
    use Instruction::*;
    let mut writer = new_writer();
    writer.expression(code(&[
        PushSmall(3),
        PushSmall(4),
        CallLibrary {
            function,
            arguments,
            results,
        },
    ]));
    writer.build()
}

#[test]
fn an_app_runs_a_library_function_that_calls_another() {
    use Instruction::*;
    let mut library = new_library_writer();
    let tens = add_tens_and_units(&mut library);
    library.function(
        2,
        1,
        2,
        code(&[ReadLocal(0), ReadLocal(1), Call(tens), PushSmall(1), Add]),
    );
    let library = library.build();
    let app = app_calling_the_library(1, 2, 1);

    assert_eq!(verify_bytes(&library), Ok(()));
    assert_eq!(verify_bytes(&app), Ok(()));
    assert_eq!(evaluate_with_library(&app, Some(&library), 0), Ok(35));
}

#[test]
fn a_library_call_without_a_library_fails() {
    let app = app_calling_the_library(0, 2, 1);

    assert_eq!(evaluate_with_library(&app, None, 0), Err(Failure::NoLibrary));
}

#[test]
fn a_library_call_whose_shape_does_not_match_the_function_fails() {
    let mut library = new_library_writer();
    add_tens_and_units(&mut library);
    let library = library.build();

    assert_eq!(
        evaluate_with_library(&app_calling_the_library(0, 1, 1), Some(&library), 0),
        Err(Failure::NoSuchCode)
    );
    assert_eq!(
        evaluate_with_library(&app_calling_the_library(5, 2, 1), Some(&library), 0),
        Err(Failure::NoSuchCode)
    );
}

#[test]
fn library_text_is_copied_out_for_the_app() {
    use Instruction::*;
    let mut library = new_library_writer();
    let greeting = library.string("hello");
    library.function(0, 1, 0, code(&[PushText(greeting)]));
    let library_bytes = library.build();
    let mut app = new_writer();
    app.expression(code(&[CallLibrary {
        function: 0,
        arguments: 0,
        results: 1,
    }]));
    let app_bytes = app.build();
    let image = AppImage::parse(&app_bytes).unwrap();
    let library = AppImage::parse(&library_bytes).unwrap();
    let state = AppState::new(&image).unwrap();
    let mut interpreter = Interpreter::new();

    let value = interpreter
        .evaluate_expression_with_library(&image, Some(&library), 0, &state, &System::default())
        .unwrap();

    assert_eq!(interpreter.text(&image, value), Ok("hello"));
}

#[test]
fn library_code_may_not_touch_the_app() {
    use Instruction::*;
    let mut library = new_library_writer();
    library.function(0, 1, 0, code(&[ReadState(0)]));
    let library = library.build();
    let mut app = new_writer();
    app.variable("count", ValueType::Int, false, 0);
    app.expression(code(&[CallLibrary {
        function: 0,
        arguments: 0,
        results: 1,
    }]));
    let app = app.build();

    assert_eq!(
        evaluate_with_library(&app, Some(&library), 0),
        Err(Failure::NotAllowedHere)
    );
}

#[test]
fn a_library_refuses_functions_that_are_not_pure() {
    use Instruction::*;
    let mut draws_random_numbers = new_library_writer();
    draws_random_numbers.function(0, 1, 0, code(&[PushSmall(1), PushSmall(6), RandomInt]));
    let mut calls_the_library = new_library_writer();
    calls_the_library.function(
        0,
        1,
        0,
        code(&[CallLibrary {
            function: 0,
            arguments: 0,
            results: 1,
        }]),
    );

    let refused = |bytes: &[u8]| {
        verify_bytes(bytes).err().is_some_and(|error| {
            matches!(
                error,
                ImageError::BadCode {
                    problem: CodeProblem::LibraryFunctionNotPure,
                    ..
                }
            )
        })
    };
    assert!(refused(&draws_random_numbers.build()));
    assert!(refused(&calls_the_library.build()));
}

#[test]
fn a_library_call_encodes_as_five_bytes() {
    let call = Instruction::CallLibrary {
        function: 0x0102,
        arguments: 2,
        results: 1,
    };
    let encoded = call.encode();

    assert_eq!(encoded.as_bytes(), &[0x2E, 0x02, 0x01, 2, 1]);
    assert_eq!(decode(encoded.as_bytes(), 0), Ok((call, 5)));
}

#[test]
fn the_walker_follows_calls_into_the_library() {
    use Instruction::*;
    let mut library = new_library_writer();
    library.function(0, 1, 0, code(&[ReadSystem(SystemValue::Now)]));
    let library_bytes = library.build();
    let app_bytes = new_writer().build();
    let image = AppImage::parse(&app_bytes).unwrap();
    let library = AppImage::parse(&library_bytes).unwrap();
    let top = code(&[CallLibrary {
        function: 0,
        arguments: 0,
        results: 1,
    }]);
    let mut clock_reads = 0;

    let walked = for_each_reached_instruction_in(&image, Some(&library), &top, |instruction, _| {
        if instruction == ReadSystem(SystemValue::Now) {
            clock_reads += 1;
        }
    });
    let without_library = for_each_reached_instruction_in(&image, None, &top, |_, _| {});

    assert_eq!(walked, Some(()));
    assert_eq!(clock_reads, 1);
    assert_eq!(without_library, Some(()));
    assert!(analyse_expression_in(&image, Some(&library), &top)
        .unwrap()
        .inputs
        .reads_clock());
}

// ============================================================================
// Sharing repeated code
// ============================================================================

/// Code that compares a number with 5 and formats 1 or 2, with jumps over pushes of any width.
fn compare_and_format(number: i32) -> Vec<u8> {
    use Instruction::*;
    let mut writer = CodeWriter::new();
    let small = writer.label();
    let done = writer.label();
    writer
        .emit(PushInt(number))
        .emit(PushSmall(5))
        .emit(Greater)
        .jump_if_false(small);
    writer.emit(PushSmall(1)).jump(done);
    writer.place(small).emit(PushSmall(2));
    writer.place(done).emit(FormatNumber(2));
    let code = writer.finish().unwrap();
    // Re-encode the first push at its smallest, as the compiler would write it.
    let mut decoded: Vec<u8> = Vec::new();
    match i8::try_from(number) {
        Ok(small) => {
            decoded.extend([0x01, small as u8]);
            decoded.extend(&code[5..]);
        }
        Err(_) => decoded.extend(&code),
    }
    decoded
}

fn texts_of_every_expression(bytes: &[u8]) -> Vec<std::string::String> {
    let image = AppImage::parse(bytes).unwrap();
    let state = AppState::new(&image).unwrap();
    let mut interpreter = Interpreter::new();
    (0..image.expression_count())
        .map(|expression| {
            let value = interpreter
                .evaluate_expression(&image, expression, &state, &System::default())
                .unwrap();
            interpreter.text(&image, value).unwrap().into()
        })
        .collect()
}

#[test]
fn expressions_that_differ_only_in_constants_share_one_function() {
    let numbers = [3, 9, 1000, -4];
    let mut plain = new_writer();
    for number in numbers {
        plain.expression(compare_and_format(number));
    }
    let mut shared = new_writer();
    for number in numbers {
        shared.expression(compare_and_format(number));
    }

    shared.share_repeated_code();
    let plain_bytes = plain.build();
    let shared_bytes = shared.build();
    let image = AppImage::parse(&shared_bytes).unwrap();

    assert_eq!(verify(&image), Ok(()));
    assert_eq!(image.function_count(), 1);
    assert_eq!(image.function(0).unwrap().argument_count, 1);
    assert!(shared_bytes.len() < plain_bytes.len());
    assert_eq!(
        texts_of_every_expression(&shared_bytes),
        texts_of_every_expression(&plain_bytes)
    );
    assert_eq!(texts_of_every_expression(&shared_bytes), ["02", "01", "01", "02"]);
}

#[test]
fn code_too_small_to_gain_is_left_alone() {
    use Instruction::*;
    let mut writer = new_writer();
    writer.expression(code(&[PushSmall(1)]));
    writer.expression(code(&[PushSmall(2)]));

    writer.share_repeated_code();
    let bytes = writer.build();
    let image = AppImage::parse(&bytes).unwrap();

    assert_eq!(image.function_count(), 0);
    assert_eq!(image.expression_code(1), Some(&[0x01, 2][..]));
}

#[test]
fn shared_handlers_keep_their_locals_after_the_arguments() {
    use Instruction::*;
    let mut writer = new_writer();
    let total = writer.variable("total", ValueType::Int, false, 0);
    let mut handlers = Vec::new();
    for amount in [3, 40] {
        handlers.push(writer.handler(code(&[
            PushSmall(amount),
            PushSmall(2),
            Multiply,
            WriteLocal(0),
            ReadState(total),
            ReadLocal(0),
            Add,
            PushSmall(100),
            Add,
            WriteState(total),
        ])));
    }

    writer.share_repeated_code();
    let bytes = writer.build();
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let mut interpreter = Interpreter::new();
    for handler in handlers {
        interpreter
            .run_handler(&image, handler, &mut state, &[], &System::default())
            .unwrap();
    }
    let read_total = AppImage::parse(&bytes).unwrap();

    assert_eq!(verify(&image), Ok(()));
    assert_eq!(image.function_count(), 1);
    let function = image.function(0).unwrap();
    assert_eq!(
        (
            function.argument_count,
            function.result_count,
            function.local_count
        ),
        (1, 0, 2)
    );
    let placement = state.placement(total).unwrap();
    let stored = &state.variable_bytes(total).unwrap()[placement.element_range(0).unwrap()];
    assert_eq!(crate::state::read_packed(stored), Some(6 + 100 + 80 + 100));
    assert_eq!(read_total.handler_count(), 2);
}

#[test]
fn shared_code_takes_differing_texts_as_arguments() {
    use Instruction::*;
    let mut plain = new_writer();
    let mut shared = new_writer();
    for writer in [&mut plain, &mut shared] {
        let names = ["north", "south", "east", "west"].map(|name| writer.string(name));
        let separator = writer.string(": ");
        for name in names {
            writer.expression(code(&[
                PushText(name),
                PushText(separator),
                Join,
                PushInt(70_000),
                FormatNumber(6),
                Join,
            ]));
        }
    }

    shared.share_repeated_code();
    let shared_bytes = shared.build();

    assert_eq!(verify_bytes(&shared_bytes), Ok(()));
    assert_eq!(
        AppImage::parse(&shared_bytes)
            .unwrap()
            .function(0)
            .unwrap()
            .argument_count,
        1
    );
    assert_eq!(
        texts_of_every_expression(&shared_bytes),
        texts_of_every_expression(&plain.build())
    );
    assert_eq!(
        texts_of_every_expression(&shared_bytes),
        ["north: 070000", "south: 070000", "east: 070000", "west: 070000"]
    );
}
