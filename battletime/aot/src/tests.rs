extern crate std;

use std::format;
use std::string::String;
use std::vec;
use std::vec::Vec;

use battletime_runtime::instruction::Instruction;
use battletime_runtime::interpreter::{MOST_LOCALS, MOST_STEPS, STACK_DEPTH};
use battletime_runtime::native::{NativeCode, NativeFrame};
use battletime_runtime::paging::CodeUnit;
use battletime_runtime::writer::{CodeWriter, ImageWriter};
use battletime_runtime::{
    AppImage, AppKind, AppState, Failure, HandlerOutcome, Interpreter, SystemValue, SystemValues, Value, NONE,
};

use crate::simulator::{Machine, Region, Stop, HOST_FUNCTIONS_ADDRESS};
use crate::thumb::{Assembler, Condition, DataOperation, LINK, PROGRAM_COUNTER, R0, R1, R2, R4, R6};
use crate::{translate, Context, Exit};

// ============================================================================
// Running native code in the simulator
// ============================================================================

const CODE_ADDRESS: u32 = 0x0080_0000;
const RAM_ADDRESS: u32 = 0x2000_0000;
const CONTEXT_ADDRESS: u32 = RAM_ADDRESS;
const HELPERS_ADDRESS: u32 = RAM_ADDRESS + 0x40;
const RAM_BYTES: usize = 0x2000;
const STACK_ADDRESS: u32 = 0x3000_0000;
const LOCALS_ADDRESS: u32 = 0x3100_0000;

fn write_context(machine: &mut Machine, context: &Context) {
    let words = [
        context.locals,
        context.stack,
        context.stack_room,
        context.steps_left,
        context.resume_offset,
        context.depth,
        context.helpers,
    ];
    for (index, word) in words.iter().enumerate() {
        machine
            .write_word(CONTEXT_ADDRESS + index as u32 * 4, *word)
            .unwrap();
    }
}

fn read_context(machine: &Machine) -> Context {
    let word = |index: u32| machine.read_word(CONTEXT_ADDRESS + index * 4).unwrap();
    Context {
        locals: word(0),
        stack: word(1),
        stack_room: word(2),
        steps_left: word(3),
        resume_offset: word(4),
        depth: word(5),
        helpers: word(6),
    }
}

/// Runs a native unit on `stack` and `locals` from `context`, with `execute` as the execute helper.
fn run_in_simulator(
    native: &[u8],
    stack: &mut [Value],
    locals: &mut [Value],
    context: Context,
    execute: &mut dyn FnMut(u32, u32) -> bool,
) -> (Exit, Context) {
    let mut machine = Machine::new(vec![
        Region::owned(CODE_ADDRESS, native.to_vec()),
        Region::owned(RAM_ADDRESS, vec![0; RAM_BYTES]),
        unsafe { Region::borrowed(STACK_ADDRESS, stack.as_mut_ptr().cast(), stack.len() * 8) },
        unsafe { Region::borrowed(LOCALS_ADDRESS, locals.as_mut_ptr().cast(), locals.len() * 8) },
    ]);
    machine
        .write_word(HELPERS_ADDRESS, HOST_FUNCTIONS_ADDRESS | 1)
        .unwrap();
    write_context(
        &mut machine,
        &Context {
            locals: LOCALS_ADDRESS,
            stack: STACK_ADDRESS,
            stack_room: stack.len() as u32,
            helpers: HELPERS_ADDRESS,
            ..context
        },
    );

    let mut host = |function: u32, machine: &mut Machine| {
        assert_eq!(function, battletime_runtime::native::EXECUTE_HELPER);
        let offset = machine.registers[1];
        let depth = read_context(machine).depth;
        machine.registers[0] = match execute(offset, depth) {
            true => Exit::Finished as u32,
            false => Exit::Failed as u32,
        };
    };
    let stop = machine.call(
        CODE_ADDRESS | 1,
        CONTEXT_ADDRESS,
        RAM_ADDRESS + RAM_BYTES as u32,
        &mut host,
    );
    assert_eq!(stop, Stop::Returned);
    (
        Exit::from_word(machine.registers[0]).unwrap(),
        read_context(&machine),
    )
}

struct NativeRun {
    exit: Exit,
    context: Context,
    stack: Vec<Value>,
}

/// Runs a unit that calls no helper on an empty stack.
fn run_native(native: &[u8], steps_left: u32, stack_room: usize) -> NativeRun {
    let mut stack = vec![0; stack_room];
    let mut locals = vec![0; MOST_LOCALS];
    let context = Context {
        steps_left,
        ..Context::default()
    };
    let (exit, context) = run_in_simulator(native, &mut stack, &mut locals, context, &mut |_, _| {
        panic!("the unit called a helper")
    });
    NativeRun {
        exit,
        stack: stack[..context.depth as usize].to_vec(),
        context,
    }
}

/// Native code for an image's units, run in the simulator, as the interpreter's native code.
struct SimulatedNative {
    units: Vec<(CodeUnit, Vec<u8>)>,
    /// How many runs finished, and how many handed over.
    finished: std::rc::Rc<core::cell::Cell<(usize, usize)>>,
}

impl NativeCode for SimulatedNative {
    fn run(&mut self, unit: CodeUnit, frame: &mut NativeFrame<'_>) -> Option<Exit> {
        let native = &self.units.iter().find(|(translated, _)| *translated == unit)?.1;
        let stack = unsafe { core::slice::from_raw_parts_mut(frame.stack, frame.stack_room) };
        let locals = unsafe { core::slice::from_raw_parts_mut(frame.locals, frame.locals_room) };
        let context = Context {
            steps_left: frame.steps_left,
            resume_offset: frame.resume_offset,
            depth: frame.depth,
            ..Context::default()
        };
        let (exit, context) = run_in_simulator(native, stack, locals, context, frame.execute);
        frame.steps_left = context.steps_left;
        frame.resume_offset = context.resume_offset;
        frame.depth = context.depth;
        let (finished, handed_over) = self.finished.get();
        self.finished.set(match exit {
            Exit::Finished => (finished + 1, handed_over),
            _ => (finished, handed_over + 1),
        });
        Some(exit)
    }
}

/// Translates every unit of an image that translates, with what each unit refused.
fn translate_image(image: &AppImage) -> (SimulatedNative, usize) {
    let effects = crate::native_file::call_effects(image);
    let mut units = Vec::new();
    let mut refused = 0;
    let mut add =
        |unit: CodeUnit, code: &[u8], local_count: usize| match translate(code, local_count, &effects) {
            Ok(native) => units.push((unit, native)),
            Err(_) => refused += 1,
        };
    for index in 0..image.expression_count() {
        add(
            CodeUnit::Expression(index),
            image.expression_code(index).unwrap(),
            MOST_LOCALS,
        );
    }
    for index in 0..image.handler_count() {
        add(
            CodeUnit::Handler(index),
            image.handler_code(index).unwrap(),
            MOST_LOCALS,
        );
    }
    for index in 0..image.function_count() {
        let function = image.function(index).unwrap();
        add(
            CodeUnit::Function(index as u8),
            function.code,
            function.local_count as usize,
        );
    }
    let finished = std::rc::Rc::new(core::cell::Cell::new((0, 0)));
    (SimulatedNative { units, finished }, refused)
}

// ============================================================================
// Running the same code in the interpreter
// ============================================================================

#[derive(Default)]
struct FakeSystem;

impl SystemValues for FakeSystem {
    fn read(&self, value: SystemValue) -> Value {
        value as Value * 7 + 3
    }
    fn random_number(&self) -> u32 {
        4
    }
}

fn expression_image(code: &[u8]) -> Vec<u8> {
    let mut writer = ImageWriter::new("test", "Test", AppKind::App, 1);
    writer.expression(code.to_vec());
    writer.build()
}

fn assemble(build: impl FnOnce(&mut CodeWriter)) -> Vec<u8> {
    let mut writer = CodeWriter::new();
    build(&mut writer);
    writer.finish().unwrap()
}

/// What an expression gives, with a text result as its text.
fn evaluated(interpreter: &mut Interpreter, image: &AppImage, expression: u16) -> Result<String, Failure> {
    let state = AppState::new(image).unwrap();
    let value = interpreter.evaluate_expression(image, expression, &state, &FakeSystem)?;
    Ok(match interpreter.text(image, value) {
        Ok(text) => format!("{value} {text:?}"),
        Err(_) => format!("{value}"),
    })
}

/// Evaluates an expression with and without native code, which must agree, and gives what it gave.
fn compare(code: &[u8]) -> Result<String, Failure> {
    let bytes = expression_image(code);
    let image = AppImage::parse(&bytes).unwrap();
    let interpreted = evaluated(&mut Interpreter::for_image(&image), &image, 0);

    let mut native = Interpreter::for_image(&image);
    native.set_native_code(Some(std::boxed::Box::new(translate_image(&image).0)));
    assert_eq!(evaluated(&mut native, &image, 0), interpreted, "code {code:?}");
    interpreted
}

// ============================================================================
// The assembler
// ============================================================================

#[test]
fn writes_the_encodings_the_architecture_gives() {
    let mut assembler = Assembler::new();
    assembler.move_small(R0, 1);
    assembler.push(&[R4, LINK]);
    assembler.pop(&[R4, PROGRAM_COUNTER]);
    assembler.load_pair(R0, R1, R6, 8);
    assembler.store_pair(R0, R1, R6, 16);
    assembler.move_wide(R0, 0x1234);
    assembler.move_top(R1, 0xFFFF);
    assembler.data(DataOperation::AddWithCarry, R1, R2);
    assembler.load_word(R0, R4, 12);
    assembler.add_to_stack_pointer(8);
    let bytes = assembler.finish().unwrap();
    let halfwords: Vec<u16> = bytes
        .chunks(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();

    assert_eq!(
        halfwords,
        vec![
            0x2001, 0xB510, 0xBD10, 0xE9D6, 0x0102, 0xE9C6, 0x0104, 0xF241, 0x2034, 0xF6CF, 0x71FF, 0x4151,
            0x68E0, 0xB002
        ]
    );
}

#[test]
fn refuses_an_operand_too_large_for_its_instruction() {
    let mut assembler = Assembler::new();
    assembler.move_small(R0, 256);
    assert!(assembler.finish().is_err());
}

#[test]
fn long_branches_and_calls_reach_both_ways() {
    for gap in [0, 1, 300, 70_000] {
        let mut assembler = Assembler::new();
        let ahead = assembler.label();
        let back = assembler.label();
        let done = assembler.label();
        let routine = assembler.label();
        assembler.push(&[R4, LINK]);
        assembler.move_small(R0, 0);
        assembler.branch(ahead);
        assembler.place(back);
        assembler.add_small(R0, 1);
        assembler.branch(done);
        for _ in 0..gap {
            assembler.move_small(R1, 0);
        }
        assembler.place(routine);
        assembler.add_small(R0, 10);
        assembler.call_register(LINK);
        assembler.place(ahead);
        assembler.compare_small(R0, 0);
        assembler.branch_if(Condition::Equal, back);
        assembler.place(done);
        assembler.branch_and_link(routine);
        assembler.add_small(R0, 2);
        assembler.pop(&[R4, PROGRAM_COUNTER]);
        let code = assembler.finish().unwrap();

        let mut machine = Machine::new(vec![
            Region::owned(CODE_ADDRESS, code),
            Region::owned(RAM_ADDRESS, vec![0; 64]),
        ]);
        let stop = machine.call(CODE_ADDRESS | 1, 0, RAM_ADDRESS + 64, &mut |_, _| {});
        assert_eq!(stop, Stop::Returned, "gap {gap}");
        assert_eq!(machine.registers[0], 13, "gap {gap}");
    }
}

// ============================================================================
// Translated code
// ============================================================================

const BINARY: [Instruction; 18] = [
    Instruction::Add,
    Instruction::Subtract,
    Instruction::Multiply,
    Instruction::Divide,
    Instruction::Remainder,
    Instruction::ShiftLeft,
    Instruction::ShiftRight,
    Instruction::Minimum,
    Instruction::Maximum,
    Instruction::BitAnd,
    Instruction::BitOr,
    Instruction::BitXor,
    Instruction::Equal,
    Instruction::NotEqual,
    Instruction::Less,
    Instruction::LessOrEqual,
    Instruction::Greater,
    Instruction::GreaterOrEqual,
];

const UNARY: [Instruction; 5] = [
    Instruction::Negate,
    Instruction::Absolute,
    Instruction::BitNot,
    Instruction::Not,
    Instruction::IsNone,
];

#[test]
fn works_out_arithmetic_as_the_interpreter_does() {
    let pairs = [
        (5, 7),
        (-3, 9),
        (i64::MAX, 1),
        (i64::MIN, -1),
        (i64::MIN, 1),
        (0, i64::MIN),
        (NONE, 0),
        (9, 0),
    ];
    for (left, right) in pairs {
        for operation in BINARY {
            let code = assemble(|writer| {
                writer
                    .emit(Instruction::PushLong(left))
                    .emit(Instruction::PushLong(right))
                    .emit(operation);
            });
            let _ = compare(&code);
        }
    }
}

#[test]
fn works_out_single_values_as_the_interpreter_does() {
    let values = [0, 1, -1, 255, -256, 1 << 40, i64::MAX, i64::MIN, NONE];
    for value in values {
        for operation in UNARY {
            let code = assemble(|writer| {
                writer.emit(Instruction::PushLong(value)).emit(operation);
            });
            let _ = compare(&code);
        }
    }
}

#[test]
fn hands_an_overflow_to_the_interpreter_at_the_instruction() {
    let code = assemble(|writer| {
        writer
            .emit(Instruction::PushLong(i64::MAX))
            .emit(Instruction::PushSmall(1))
            .emit(Instruction::Add)
            .emit(Instruction::PushSmall(2));
    });
    let run = run_native(
        &translate(&code, MOST_LOCALS, &[]).unwrap(),
        MOST_STEPS,
        STACK_DEPTH,
    );

    assert_eq!(compare(&code), Err(Failure::Overflow));
    assert_eq!(run.exit, Exit::Interpret);
    assert_eq!(run.context.resume_offset, 11);
    assert_eq!(run.stack, vec![i64::MAX, 1]);
    assert_eq!(run.context.steps_left, MOST_STEPS - 2);
}

#[test]
fn hands_a_call_to_the_interpreter_with_its_step_given_back() {
    let code = assemble(|writer| {
        writer
            .emit(Instruction::PushSmall(6))
            .emit(Instruction::PushSmall(7))
            .emit(Instruction::CallLibrary {
                function: 0,
                arguments: 2,
                results: 1,
            });
    });
    let run = run_native(
        &translate(&code, MOST_LOCALS, &[]).unwrap(),
        MOST_STEPS,
        STACK_DEPTH,
    );

    assert_eq!(run.exit, Exit::Interpret);
    assert_eq!(run.context.resume_offset, 4);
    assert_eq!(run.stack, vec![6, 7]);
    assert_eq!(run.context.steps_left, MOST_STEPS - 2);
}

#[test]
fn runs_what_it_does_not_inline_through_the_interpreter() {
    let code = assemble(|writer| {
        writer
            .emit(Instruction::PushSmall(6))
            .emit(Instruction::PushSmall(7))
            .emit(Instruction::Multiply)
            .emit(Instruction::ReadSystem(SystemValue::StepsToday))
            .emit(Instruction::Add);
    });
    assert_eq!(
        compare(&code),
        Ok(format!("{}", 42 + SystemValue::StepsToday as Value * 7 + 3))
    );
}

#[test]
fn hands_over_before_a_block_that_would_pass_the_step_limit() {
    let code = assemble(|writer| {
        writer
            .emit(Instruction::PushSmall(1))
            .emit(Instruction::PushSmall(2))
            .emit(Instruction::Add);
    });
    let run = run_native(&translate(&code, MOST_LOCALS, &[]).unwrap(), 2, STACK_DEPTH);

    assert_eq!(run.exit, Exit::Interpret);
    assert_eq!(run.context.resume_offset, 0);
    assert_eq!(run.context.steps_left, 2);
}

#[test]
fn hands_over_at_the_start_when_the_stack_has_no_room() {
    let code = assemble(|writer| {
        writer
            .emit(Instruction::PushSmall(1))
            .emit(Instruction::PushSmall(2))
            .emit(Instruction::Add);
    });
    let run = run_native(&translate(&code, MOST_LOCALS, &[]).unwrap(), MOST_STEPS, 1);

    assert_eq!(run.exit, Exit::Interpret);
    assert_eq!(run.context.resume_offset, 0);
    assert_eq!(run.context.depth, 0);
}

#[test]
fn counts_a_loop_step_for_step() {
    let code = assemble(|writer| counting_loop(writer, 1000));
    let run = run_native(
        &translate(&code, MOST_LOCALS, &[]).unwrap(),
        MOST_STEPS,
        STACK_DEPTH,
    );

    assert_eq!(compare(&code), Ok(String::from("1000")));
    let loop_steps = 12;
    assert_eq!(MOST_STEPS - run.context.steps_left, 2 + 1000 * loop_steps + 5 + 1);
}

#[test]
fn a_loop_that_runs_too_long_fails_as_in_the_interpreter() {
    let code = assemble(|writer| counting_loop(writer, 100_000));
    assert_eq!(compare(&code), Err(Failure::TooManySteps));
}

/// Counts local 0 up to `times` and leaves it on the stack.
fn counting_loop(writer: &mut CodeWriter, times: i32) {
    writer
        .emit(Instruction::PushSmall(0))
        .emit(Instruction::WriteLocal(0));
    let start = writer.loop_start();
    let end = writer.label();
    writer
        .emit(Instruction::ReadLocal(0))
        .emit(Instruction::PushInt(times))
        .emit(Instruction::Less)
        .jump_if_false(end)
        .emit(Instruction::ReadLocal(0))
        .emit(Instruction::PushSmall(1))
        .emit(Instruction::Add)
        .emit(Instruction::WriteLocal(0))
        .emit(Instruction::PushNone)
        .emit(Instruction::Drop)
        .loop_back(start);
    writer.place(end);
    writer.emit(Instruction::ReadLocal(0));
}

#[test]
fn refuses_code_whose_paths_disagree_on_the_stack() {
    let code = assemble(|writer| {
        let skip = writer.label();
        writer
            .emit(Instruction::PushSmall(1))
            .jump_if_false(skip)
            .emit(Instruction::PushSmall(2));
        writer.place(skip);
    });
    assert!(translate(&code, MOST_LOCALS, &[]).is_err());
}

// ============================================================================
// Random programs
// ============================================================================

/// A small generator of repeatable random numbers.
struct Random {
    state: u64,
}

impl Random {
    fn next(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    fn below(&mut self, limit: u64) -> u64 {
        self.next() % limit
    }

    fn value(&mut self) -> i64 {
        match self.below(8) {
            0 => 0,
            1 => 1,
            2 => -1,
            3 => i64::MAX,
            4 => i64::MIN,
            5 => self.below(200) as i64 - 100,
            6 => self.next() as i64 >> self.below(64),
            _ => self.next() as i64,
        }
    }
}

/// Writes random code that starts and ends at `floor` values on the stack and never takes below it.
fn random_segment(random: &mut Random, writer: &mut CodeWriter, floor: usize, length: usize, nesting: u8) {
    let mut depth = floor;
    for _ in 0..length {
        let above = depth - floor;
        match random.below(12) {
            0 | 1 if depth < 20 => {
                writer.emit(Instruction::PushLong(random.value()));
                depth += 1;
            }
            2 if depth < 20 => {
                writer.emit(Instruction::ReadLocal(random.below(4) as u8));
                depth += 1;
            }
            3 if above >= 1 => {
                writer.emit(Instruction::WriteLocal(random.below(4) as u8));
                depth -= 1;
            }
            4..=6 if above >= 2 => {
                writer.emit(BINARY[random.below(BINARY.len() as u64) as usize]);
                depth -= 1;
            }
            7 if above >= 1 => {
                writer.emit(UNARY[random.below(UNARY.len() as u64) as usize]);
            }
            8 if above >= 1 && depth < 20 => {
                writer.emit(Instruction::Duplicate);
                depth += 1;
            }
            9 if above >= 2 => {
                writer.emit(Instruction::Swap);
            }
            10 if above >= 1 && nesting < 3 => {
                let otherwise = writer.label();
                let end = writer.label();
                writer.jump_if_false(otherwise);
                depth -= 1;
                random_segment(random, writer, depth, length / 2, nesting + 1);
                writer.jump(end);
                writer.place(otherwise);
                random_segment(random, writer, depth, length / 2, nesting + 1);
                writer.place(end);
            }
            11 if depth == 0 && nesting < 2 => {
                let counter = 8 + nesting;
                writer
                    .emit(Instruction::PushSmall(0))
                    .emit(Instruction::WriteLocal(counter));
                let start = writer.loop_start();
                let end = writer.label();
                writer
                    .emit(Instruction::ReadLocal(counter))
                    .emit(Instruction::PushSmall(random.below(20) as i8))
                    .emit(Instruction::Less)
                    .jump_if_false(end);
                random_segment(random, writer, 0, length / 2, nesting + 1);
                writer
                    .emit(Instruction::ReadLocal(counter))
                    .emit(Instruction::PushSmall(1))
                    .emit(Instruction::Add)
                    .emit(Instruction::WriteLocal(counter))
                    .loop_back(start);
                writer.place(end);
            }
            _ => {}
        }
    }
    while depth > floor {
        writer.emit(Instruction::Drop);
        depth -= 1;
    }
}

#[test]
fn random_programs_run_as_in_the_interpreter() {
    let mut random = Random {
        state: 0x5EED_1234_ABCD,
    };
    let mut succeeded = 0;
    for _ in 0..3000 {
        let code = assemble(|writer| {
            random_segment(&mut random, writer, 0, 24, 0);
            writer.emit(Instruction::ReadLocal(random.below(4) as u8));
        });
        if compare(&code).is_ok() {
            succeeded += 1;
        }
    }
    assert!(succeeded > 1000, "only {succeeded} ran to the end");
}

// ============================================================================
// Every app in the repository
// ============================================================================

fn outcome_text(outcome: &Result<HandlerOutcome, Failure>) -> String {
    match outcome {
        Ok(outcome) => format!(
            "changed {:#x}, effects {:?}",
            outcome.changed_variables(),
            outcome.effects()
        ),
        Err(failure) => format!("{failure:?}"),
    }
}

fn state_text(image: &AppImage, state: &AppState) -> String {
    (0..image.state_count() as u8)
        .map(|variable| format!("{:?}", state.variable_bytes(variable)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Runs every expression, and every handler three times over, with and without native code, which must agree.
fn compare_app(path: &std::path::Path, library: &AppImage) -> (usize, usize, (usize, usize)) {
    let (_, output) = battletime_compiler::compile_file(path).unwrap();
    let bytes = output
        .image
        .unwrap_or_else(|| panic!("{} does not compile", path.display()));
    let image = AppImage::parse(&bytes).unwrap();
    let (native, refused) = translate_image(&image);
    let translated = native.units.len();
    let runs = native.finished.clone();

    let mut interpreted = Interpreter::for_image(&image);
    let mut with_native = Interpreter::for_image(&image);
    with_native.set_native_code(Some(std::boxed::Box::new(native)));
    let mut interpreted_state = AppState::new(&image).unwrap();
    let mut native_state = AppState::new(&image).unwrap();
    for round in 0..3 {
        for handler in 0..image.handler_count() {
            let event = [round, handler as Value, 3, 4, 5, 6, 7, 8];
            let run = |interpreter: &mut Interpreter, state: &mut AppState| {
                let outcome = interpreter.run_handler_with_library(
                    &image,
                    Some(library),
                    handler,
                    state,
                    &event,
                    &["text"],
                    &FakeSystem,
                );
                outcome_text(&outcome)
            };
            let expected = run(&mut interpreted, &mut interpreted_state);
            let got = run(&mut with_native, &mut native_state);
            let place = format!("{} handler {handler} round {round}", path.display());
            assert_eq!(got, expected, "{place}");
            assert_eq!(
                state_text(&image, &native_state),
                state_text(&image, &interpreted_state),
                "{place}"
            );
        }
        for expression in 0..image.expression_count() {
            let evaluate = |interpreter: &mut Interpreter, state: &AppState| {
                let value = interpreter.evaluate_expression_with_library(
                    &image,
                    Some(library),
                    expression,
                    state,
                    &FakeSystem,
                );
                value.map(|value| match interpreter.text(&image, value) {
                    Ok(text) => format!("{value} {text:?}"),
                    Err(_) => format!("{value}"),
                })
            };
            let expected = evaluate(&mut interpreted, &interpreted_state);
            let got = evaluate(&mut with_native, &native_state);
            assert_eq!(got, expected, "{} expression {expression}", path.display());
        }
    }
    (translated, refused, runs.get())
}

#[test]
fn every_app_runs_natively_as_in_the_interpreter() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let library_bytes = battletime_compiler::library::build_library().unwrap();
    let library = AppImage::parse(&library_bytes).unwrap();
    let mut translated = 0;
    let mut refused = 0;
    let (mut finished, mut handed_over) = (0, 0);
    for (folder, file) in [("apps", "app.tsx"), ("faces", "face.tsx"), ("shell", "app.tsx")] {
        let mut entries: Vec<_> = std::fs::read_dir(root.join(folder)).unwrap().flatten().collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path().join(file);
            if path.exists() {
                let (units, refusals, (finished_runs, handed_over_runs)) = compare_app(&path, &library);
                translated += units;
                refused += refusals;
                finished += finished_runs;
                handed_over += handed_over_runs;
            }
        }
    }
    std::eprintln!("{translated} units, native runs: {finished} finished, {handed_over} handed over");
    assert!(translated > 1000, "only {translated} units translated");
    assert!(
        finished > handed_over,
        "{finished} runs finished, {handed_over} handed over"
    );
    assert_eq!(refused, 0, "{refused} units were refused");
}

// ============================================================================
// The native file
// ============================================================================

fn game_of_life() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../apps/game-of-life/app.tsx");
    battletime_compiler::compile_file(&path).unwrap().1.image.unwrap()
}

#[test]
fn a_native_file_holds_the_units_with_loops() {
    let image_bytes = game_of_life();
    let image = AppImage::parse(&image_bytes).unwrap();

    let file_bytes = crate::native_file::write(&image_bytes, &image);
    let file = crate::native_file::NativeFile::parse(&file_bytes, &image_bytes).unwrap();

    assert!(file.unit_count() > 0);
    let effects = crate::native_file::call_effects(&image);
    for (unit, code) in file.units() {
        assert_eq!(code.as_ptr() as usize % 4, file_bytes.as_ptr() as usize % 4);
        let (bytecode, local_count) = match unit {
            CodeUnit::Expression(index) => (image.expression_code(index).unwrap(), MOST_LOCALS),
            CodeUnit::Handler(index) => (image.handler_code(index).unwrap(), MOST_LOCALS),
            CodeUnit::Function(index) => {
                let function = image.function(index as u16).unwrap();
                (function.code, function.local_count as usize)
            }
        };
        assert_eq!(code, translate(bytecode, local_count, &effects).unwrap());
    }
}

#[test]
fn a_native_file_belongs_to_one_image() {
    let image_bytes = game_of_life();
    let image = AppImage::parse(&image_bytes).unwrap();
    let file_bytes = crate::native_file::write(&image_bytes, &image);

    let mut other_image = image_bytes.clone();
    other_image[12] ^= 1;
    assert!(crate::native_file::NativeFile::parse(&file_bytes, &other_image).is_none());
    assert!(crate::native_file::NativeFile::parse(&file_bytes[..20], &image_bytes).is_none());
}
