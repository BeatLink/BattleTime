extern crate std;

use std::vec;
use std::vec::Vec;

use battletime_runtime::instruction::Instruction;
use battletime_runtime::interpreter::{MOST_LOCALS, MOST_STEPS, STACK_DEPTH};
use battletime_runtime::writer::{CodeWriter, ImageWriter};
use battletime_runtime::{
    AppImage, AppKind, AppState, Failure, Interpreter, SystemValue, SystemValues, Value, NONE,
};

use crate::simulator::{Machine, Region, Stop};
use crate::thumb::{Assembler, Condition, DataOperation, LINK, PROGRAM_COUNTER, R0, R1, R2, R4, R6};
use crate::{translate, Context, Exit};

// ============================================================================
// Running native code in the simulator
// ============================================================================

const CODE_ADDRESS: u32 = 0x0080_0000;
const RAM_ADDRESS: u32 = 0x2000_0000;
const CONTEXT_ADDRESS: u32 = RAM_ADDRESS;
const LOCALS_ADDRESS: u32 = RAM_ADDRESS + 0x100;
const STACK_ADDRESS: u32 = RAM_ADDRESS + 0x400;
const RAM_BYTES: usize = 0x2000;

struct NativeRun {
    exit: Exit,
    context: Context,
    stack: Vec<Value>,
}

fn run_native(native: &[u8], steps_left: u32, stack_room: u32) -> NativeRun {
    let mut machine = Machine::new(vec![
        Region {
            address: CODE_ADDRESS,
            bytes: native.to_vec(),
        },
        Region {
            address: RAM_ADDRESS,
            bytes: vec![0; RAM_BYTES],
        },
    ]);
    let context = Context {
        locals: LOCALS_ADDRESS,
        stack: STACK_ADDRESS,
        stack_room,
        steps_left,
        resume_offset: 0,
        depth: 0,
    };
    let words = [
        context.locals,
        context.stack,
        context.stack_room,
        context.steps_left,
        context.resume_offset,
        context.depth,
    ];
    for (index, word) in words.iter().enumerate() {
        machine
            .write_word(CONTEXT_ADDRESS + index as u32 * 4, *word)
            .unwrap();
    }

    let stop = machine.call(CODE_ADDRESS | 1, CONTEXT_ADDRESS, RAM_ADDRESS + RAM_BYTES as u32);
    assert_eq!(stop, Stop::Returned);

    let word = |index: u32| machine.read_word(CONTEXT_ADDRESS + index * 4).unwrap();
    let context = Context {
        locals: word(0),
        stack: word(1),
        stack_room: word(2),
        steps_left: word(3),
        resume_offset: word(4),
        depth: word(5),
    };
    let value = |address: u32| {
        machine.read_word(address).unwrap() as u64 as i64
            | (machine.read_word(address + 4).unwrap() as i64) << 32
    };
    let stack = (0..context.depth)
        .map(|slot| value(STACK_ADDRESS + slot * 8))
        .collect();
    NativeRun {
        exit: Exit::from_word(machine.registers[0]).unwrap(),
        context,
        stack,
    }
}

// ============================================================================
// Running the same code in the interpreter
// ============================================================================

#[derive(Default)]
struct FakeSystem;

impl SystemValues for FakeSystem {
    fn read(&self, _value: SystemValue) -> Value {
        0
    }
    fn random_number(&self) -> u32 {
        4
    }
}

fn interpret(code: &[u8]) -> Result<Value, Failure> {
    let mut writer = ImageWriter::new("test", "Test", AppKind::App, 1);
    writer.expression(code.to_vec());
    let bytes = writer.build();
    let image = AppImage::parse(&bytes).unwrap();
    let state = AppState::new(&image).unwrap();
    Interpreter::for_image(&image).evaluate_expression(&image, 0, &state, &FakeSystem)
}

fn assemble(build: impl FnOnce(&mut CodeWriter)) -> Vec<u8> {
    let mut writer = CodeWriter::new();
    build(&mut writer);
    writer.finish().unwrap()
}

/// Runs an expression both ways: a finished native run gives the interpreter's value, and a handover happens only where the interpreter fails.
fn check_against_interpreter(code: &[u8]) -> (Result<Value, Failure>, NativeRun) {
    let native = translate(code, MOST_LOCALS).unwrap();
    let interpreted = interpret(code);
    let run = run_native(&native, MOST_STEPS, STACK_DEPTH as u32);
    match (interpreted, run.exit) {
        (Ok(value), Exit::Finished) => assert_eq!(run.stack, vec![value], "code {code:?}"),
        (Err(Failure::Overflow | Failure::TooManySteps), Exit::Interpret) => {}
        (interpreted, exit) => {
            panic!("the interpreter gave {interpreted:?} and native code {exit:?}, code {code:?}")
        }
    }
    (interpreted, run)
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
    let bytes = assembler.finish().unwrap();
    let halfwords: Vec<u16> = bytes
        .chunks(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();

    assert_eq!(
        halfwords,
        vec![
            0x2001, 0xB510, 0xBD10, 0xE9D6, 0x0102, 0xE9C6, 0x0104, 0xF241, 0x2034, 0xF6CF, 0x71FF, 0x4151,
            0x68E0
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
fn long_branches_reach_both_ways() {
    for gap in [0, 1, 300, 70_000] {
        let mut assembler = Assembler::new();
        let ahead = assembler.label();
        let back = assembler.label();
        let done = assembler.label();
        assembler.move_small(R0, 0);
        assembler.branch(ahead);
        assembler.place(back);
        assembler.add_small(R0, 1);
        assembler.branch(done);
        for _ in 0..gap {
            assembler.move_small(R1, 0);
        }
        assembler.place(ahead);
        assembler.compare_small(R0, 0);
        assembler.branch_if(Condition::Equal, back);
        assembler.place(done);
        assembler.add_small(R0, 2);
        assembler.pop(&[PROGRAM_COUNTER]);
        let code = assembler.finish().unwrap();

        let mut machine = Machine::new(vec![
            Region {
                address: CODE_ADDRESS,
                bytes: code,
            },
            Region {
                address: RAM_ADDRESS,
                bytes: vec![0; 64],
            },
        ]);
        machine.write_word(RAM_ADDRESS + 60, 0xFFFF_FFFF).unwrap();
        assert_eq!(
            machine.call(CODE_ADDRESS | 1, 0, RAM_ADDRESS + 60),
            Stop::Returned
        );
        assert_eq!(machine.registers[0], 3, "gap {gap}");
    }
}

// ============================================================================
// Translated code
// ============================================================================

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
    ];
    let operations = [
        Instruction::Add,
        Instruction::Subtract,
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
    for (left, right) in pairs {
        for operation in operations {
            let code = assemble(|writer| {
                writer
                    .emit(Instruction::PushLong(left))
                    .emit(Instruction::PushLong(right))
                    .emit(operation);
            });
            let _ = check_against_interpreter(&code);
        }
    }
}

#[test]
fn works_out_single_values_as_the_interpreter_does() {
    let values = [0, 1, -1, 255, -256, 1 << 40, i64::MAX, i64::MIN, NONE];
    let operations = [
        Instruction::Negate,
        Instruction::Absolute,
        Instruction::BitNot,
        Instruction::Not,
        Instruction::IsNone,
    ];
    for value in values {
        for operation in operations {
            let code = assemble(|writer| {
                writer.emit(Instruction::PushLong(value)).emit(operation);
            });
            let _ = check_against_interpreter(&code);
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
    let (interpreted, run) = check_against_interpreter(&code);

    assert_eq!(interpreted, Err(Failure::Overflow));
    assert_eq!(run.exit, Exit::Interpret);
    assert_eq!(run.context.resume_offset, 11);
    assert_eq!(run.context.depth, 2);
    assert_eq!(run.stack, vec![i64::MAX, 1]);
    assert_eq!(run.context.steps_left, MOST_STEPS - 2);
}

#[test]
fn hands_an_instruction_it_does_not_translate_to_the_interpreter() {
    let code = assemble(|writer| {
        writer
            .emit(Instruction::PushSmall(6))
            .emit(Instruction::PushSmall(7))
            .emit(Instruction::Multiply);
    });
    let native = translate(&code, MOST_LOCALS).unwrap();
    let run = run_native(&native, MOST_STEPS, STACK_DEPTH as u32);

    assert_eq!(run.exit, Exit::Interpret);
    assert_eq!(run.context.resume_offset, 4);
    assert_eq!(run.stack, vec![6, 7]);
    assert_eq!(run.context.steps_left, MOST_STEPS - 2);
}

#[test]
fn hands_over_before_a_block_that_would_pass_the_step_limit() {
    let code = assemble(|writer| {
        writer
            .emit(Instruction::PushSmall(1))
            .emit(Instruction::PushSmall(2))
            .emit(Instruction::Add);
    });
    let native = translate(&code, MOST_LOCALS).unwrap();
    let run = run_native(&native, 2, STACK_DEPTH as u32);

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
    let native = translate(&code, MOST_LOCALS).unwrap();
    let run = run_native(&native, MOST_STEPS, 1);

    assert_eq!(run.exit, Exit::Interpret);
    assert_eq!(run.context.resume_offset, 0);
    assert_eq!(run.context.depth, 0);
}

#[test]
fn counts_a_loop_step_for_step() {
    let code = assemble(|writer| counting_loop(writer, 1000));
    let (interpreted, run) = check_against_interpreter(&code);

    assert_eq!(interpreted, Ok(1000));
    let loop_steps = 12;
    assert_eq!(MOST_STEPS - run.context.steps_left, 2 + 1000 * loop_steps + 5 + 1);
}

#[test]
fn hands_a_loop_that_runs_too_long_to_the_interpreter() {
    let code = assemble(|writer| counting_loop(writer, 100_000));
    let (interpreted, run) = check_against_interpreter(&code);

    assert_eq!(interpreted, Err(Failure::TooManySteps));
    assert_eq!(run.exit, Exit::Interpret);
    assert!(run.context.steps_left < 12);
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
    assert!(translate(&code, MOST_LOCALS).is_err());
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

const BINARY: [Instruction; 13] = [
    Instruction::Add,
    Instruction::Subtract,
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
fn random_programs_finish_as_the_interpreter_does() {
    let mut random = Random {
        state: 0x5EED_1234_ABCD,
    };
    let mut finished = 0;
    for _ in 0..3000 {
        let code = assemble(|writer| {
            random_segment(&mut random, writer, 0, 24, 0);
            writer.emit(Instruction::ReadLocal(random.below(4) as u8));
        });
        if check_against_interpreter(&code).1.exit == Exit::Finished {
            finished += 1;
        }
    }
    assert!(finished > 1000, "only {finished} finished natively");
}
