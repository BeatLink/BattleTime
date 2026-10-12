extern crate std;

use std::{string::String, vec, vec::Vec};

use super::*;
use crate::image::{CodeBlock, CodeProblem};
use crate::widgets::WidgetProblem;

// ============================================================================
// Building images by hand
// ============================================================================

/// Writes bytecode one instruction at a time.
#[derive(Clone, Default)]
struct Code {
    bytes: Vec<u8>,
}

impl Code {
    fn new() -> Self {
        Code::default()
    }
    fn op(mut self, opcode: u8) -> Self {
        self.bytes.push(opcode);
        self
    }
    fn op_byte(mut self, opcode: u8, operand: u8) -> Self {
        self.bytes.extend([opcode, operand]);
        self
    }
    fn op_u16(mut self, opcode: u8, operand: u16) -> Self {
        self.bytes.push(opcode);
        self.bytes.extend(operand.to_le_bytes());
        self
    }
    fn small(self, value: i8) -> Self {
        self.op_byte(0x01, value as u8)
    }
    fn int(mut self, value: i32) -> Self {
        self.bytes.push(0x02);
        self.bytes.extend(value.to_le_bytes());
        self
    }
    fn long(mut self, value: i64) -> Self {
        self.bytes.push(0x03);
        self.bytes.extend(value.to_le_bytes());
        self
    }
    fn none(self) -> Self {
        self.op(0x04)
    }
    fn text(self, index: u16) -> Self {
        self.op_u16(0x05, index)
    }
    fn state(self, slot: u8) -> Self {
        self.op_byte(0x06, slot)
    }
    fn system(self, value: SystemValue) -> Self {
        self.op_byte(0x07, value as u8)
    }
    fn event(self, field: u8) -> Self {
        self.op_byte(0x08, field)
    }
    fn drop_value(self) -> Self {
        self.op(0x0A)
    }
    fn add(self) -> Self {
        self.op(0x10)
    }
    fn subtract(self) -> Self {
        self.op(0x11)
    }
    fn multiply(self) -> Self {
        self.op(0x12)
    }
    fn divide(self) -> Self {
        self.op(0x13)
    }
    fn test_none(self) -> Self {
        self.op(0x27)
    }
    fn jump(self, distance: u16) -> Self {
        self.op_u16(0x28, distance)
    }
    fn jump_if_false(self, distance: u16) -> Self {
        self.op_u16(0x29, distance)
    }
    fn time_part(self, part: u8) -> Self {
        self.op_byte(0x30, part)
    }
    fn format_number(self, minimum_digits: u8) -> Self {
        self.op_byte(0x40, minimum_digits)
    }
    fn format_duration(self, pattern: u16) -> Self {
        self.op_u16(0x41, pattern)
    }
    fn format_time(self, pattern: u16) -> Self {
        self.op_u16(0x42, pattern)
    }
    fn join(self) -> Self {
        self.op(0x43)
    }
    fn read_local(self, local: u8) -> Self {
        self.op_byte(0x0C, local)
    }
    fn write_local(self, local: u8) -> Self {
        self.op_byte(0x0D, local)
    }
    fn loop_start(self) -> Self {
        self.op(0x2A)
    }
    fn loop_back(self, distance: u16) -> Self {
        self.op_u16(0x2B, distance)
    }
    fn element(self, array: u8) -> Self {
        self.op_byte(0x60, array)
    }
    fn write_element(self, array: u8) -> Self {
        self.op_byte(0x61, array)
    }
    fn copy_array(mut self, destination: u8, source: u8) -> Self {
        self.bytes.extend([0x62, destination, source]);
        self
    }
    fn fill(self, array: u8) -> Self {
        self.op_byte(0x63, array)
    }
    fn then(mut self, other: Code) -> Self {
        self.bytes.extend(other.bytes);
        self
    }
    fn write_state(self, slot: u8) -> Self {
        self.op_byte(0x50, slot)
    }
    fn effect(self, kind: EffectKind) -> Self {
        self.op_byte(0x51, kind as u8)
    }
    fn len(&self) -> u16 {
        self.bytes.len() as u16
    }
}

/// Patterns every test image holds, at these string numbers.
const MINUTES_SECONDS: u16 = 2;
const MINUTES_SECONDS_HUNDREDTHS: u16 = 3;
const CLOCK: u16 = 4;
const CLOCK_WITH_SECONDS: u16 = 5;
const DATE: u16 = 6;
const COMMON_PATTERNS: [&str; 5] = ["mm:ss", "mm:ss.cc", "clock", "clock_with_seconds", "date"];

struct Variable {
    key: &'static str,
    value_type: u8,
    persists: bool,
    length: u16,
    /// 0 for 8 bytes, 1 for 1, 2 for 2 and 3 for 4.
    width_code: u8,
    default_value: i64,
}

/// Lays out an image the way the compiler will.
#[derive(Default)]
struct ImageBuilder {
    strings: Vec<String>,
    state: Vec<Variable>,
    expressions: Vec<Vec<u8>>,
    handlers: Vec<Vec<u8>>,
    widgets: Vec<Vec<u8>>,
}

impl ImageBuilder {
    fn new() -> Self {
        let mut builder = ImageBuilder::default();
        builder.string("test-app");
        builder.string("Test app");
        for pattern in COMMON_PATTERNS {
            builder.string(pattern);
        }
        builder
    }

    fn string(&mut self, text: &str) -> u16 {
        self.strings.push(text.into());
        (self.strings.len() - 1) as u16
    }

    fn variable(&mut self, key: &'static str, value_type: u8, persists: bool, default_value: i64) -> u8 {
        self.string(key);
        self.state.push(Variable {
            key,
            value_type,
            persists,
            length: 0,
            width_code: 0,
            default_value,
        });
        (self.state.len() - 1) as u8
    }

    /// An array of `length` elements, each `element_bytes` wide.
    fn array(&mut self, key: &'static str, length: u16, element_bytes: u8, default_value: i64) -> u8 {
        let width_code = match element_bytes {
            1 => 1,
            2 => 2,
            4 => 3,
            _ => 0,
        };
        self.string(key);
        self.state.push(Variable {
            key,
            value_type: INT_TYPE,
            persists: false,
            length,
            width_code,
            default_value,
        });
        (self.state.len() - 1) as u8
    }

    fn expression(&mut self, code: Code) -> u16 {
        self.expressions.push(code.bytes);
        (self.expressions.len() - 1) as u16
    }

    fn widget(
        &mut self,
        kind: WidgetKind,
        child_count: u8,
        properties: &[(Property, PropertySource)],
    ) -> u16 {
        let mut bytes = vec![kind as u8, child_count, properties.len() as u8, 0];
        for (property, source) in properties {
            let (source_byte, value) = match *source {
                PropertySource::Constant(value) => (0u8, value),
                PropertySource::Expression(index) => (1, index as i32),
                PropertySource::Handler(index) => (2, index as i32),
            };
            bytes.push(*property as u8);
            bytes.push(source_byte);
            bytes.extend(value.to_le_bytes());
        }
        self.widgets.push(bytes);
        (self.widgets.len() - 1) as u16
    }

    fn handler(&mut self, code: Code) -> u16 {
        self.handlers.push(code.bytes);
        (self.handlers.len() - 1) as u16
    }

    fn table(entries: &[Vec<u8>]) -> Vec<u8> {
        let mut table: Vec<u8> = Vec::new();
        let mut data: Vec<u8> = Vec::new();
        for entry in entries {
            table.extend((data.len() as u16).to_le_bytes());
            table.extend((entry.len() as u16).to_le_bytes());
            data.extend(entry);
        }
        table.extend(data);
        table
    }

    fn build(&self) -> Vec<u8> {
        let strings: Vec<Vec<u8>> = self.strings.iter().map(|text| text.as_bytes().to_vec()).collect();
        let mut state: Vec<u8> = Vec::new();
        for variable in &self.state {
            let key_string = self.strings.iter().position(|text| text == variable.key).unwrap() as u16;
            state.extend(key_string.to_le_bytes());
            state.push(variable.value_type);
            state.push(variable.persists as u8 | variable.width_code << 1);
            state.extend(variable.length.to_le_bytes());
            state.extend([0, 0]);
            state.extend(variable.default_value.to_le_bytes());
        }
        let sections: Vec<(u16, u16, Vec<u8>)> = vec![
            (1, strings.len() as u16, Self::table(&strings)),
            (2, self.state.len() as u16, state),
            (3, self.expressions.len() as u16, Self::table(&self.expressions)),
            (4, self.handlers.len() as u16, Self::table(&self.handlers)),
            (5, self.widgets.len() as u16, Self::table(&self.widgets)),
        ];

        let mut image: Vec<u8> = Vec::new();
        image.extend(b"NTAP");
        image.extend(1u16.to_le_bytes());
        image.extend([0, 0]);
        image.extend(1u16.to_le_bytes());
        image.extend((sections.len() as u16).to_le_bytes());
        image.extend([0; 4]);
        image.extend(0u16.to_le_bytes());
        image.extend(1u16.to_le_bytes());

        let mut offset = 20 + sections.len() * 12;
        for (number, count, bytes) in &sections {
            image.extend(number.to_le_bytes());
            image.extend(count.to_le_bytes());
            image.extend((offset as u32).to_le_bytes());
            image.extend((bytes.len() as u32).to_le_bytes());
            offset += bytes.len();
        }
        for (_, _, bytes) in &sections {
            image.extend(bytes);
        }
        seal(&mut image);
        image
    }
}

fn seal(image: &mut [u8]) {
    let crc = image::crc32(&image[16..]);
    image[12..16].copy_from_slice(&crc.to_le_bytes());
}

const INSTANT_TYPE: u8 = 2;
const DURATION_TYPE: u8 = 3;
const INT_TYPE: u8 = 0;

// ============================================================================
// A fake system
// ============================================================================

#[derive(Default)]
struct FakeSystem {
    now: i64,
    utc_offset: i64,
    uses_24_hour_clock: bool,
}

impl SystemValues for FakeSystem {
    fn read(&self, value: SystemValue) -> Value {
        match value {
            SystemValue::Now => self.now,
            SystemValue::UtcOffset => self.utc_offset,
            SystemValue::Uses24HourClock => self.uses_24_hour_clock as Value,
            SystemValue::BatteryPercent => 80,
            _ => 0,
        }
    }

    fn random_number(&self) -> u32 {
        1_000_003
    }

    fn read_text(&self, value: SystemValue) -> &str {
        match value {
            SystemValue::MusicTrack => "Blue Monday",
            _ => "",
        }
    }

    /// A two-day forecast whose highs are 21 and 23.
    fn read_element(&self, value: SystemValue, index: Value) -> Value {
        match (value, index) {
            (SystemValue::WeatherForecastHigh, 0) => 21,
            (SystemValue::WeatherForecastHigh, 1) => 23,
            _ => NONE,
        }
    }
}

/// 2026-10-11 14:05:09 UTC, a Sunday.
const SUNDAY_AFTERNOON: i64 = 1_791_727_509_000;

/// The image's state with the first variables set to `values`.
fn state_with(image: &AppImage, values: &[Value]) -> AppState {
    let mut state = AppState::new(image).unwrap();
    for (variable, value) in values.iter().enumerate() {
        state.set(variable as u8, *value).unwrap();
    }
    state
}

fn evaluate(code: Code) -> Result<Value, Failure> {
    let mut builder = ImageBuilder::new();
    builder.expression(code);
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();
    Interpreter::new().evaluate_expression(&image, 0, &AppState::new(&image).unwrap(), &FakeSystem::default())
}

fn evaluate_text(code: Code, system: &FakeSystem) -> String {
    let mut builder = ImageBuilder::new();
    builder.expression(code);
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();
    let mut interpreter = Interpreter::new();
    let value = interpreter
        .evaluate_expression(&image, 0, &AppState::new(&image).unwrap(), system)
        .unwrap();
    interpreter.text(&image, value).unwrap().into()
}

fn verify_problem(builder: &ImageBuilder) -> Option<CodeProblem> {
    let bytes = builder.build();
    match verify(&AppImage::parse(&bytes).unwrap()) {
        Err(ImageError::BadCode { problem, .. }) => Some(problem),
        Err(other) => panic!("unexpected error {other:?}"),
        Ok(()) => None,
    }
}

// ============================================================================
// The stopwatch from docs/app-runtime.md
// ============================================================================

struct Stopwatch {
    bytes: Vec<u8>,
    started_at: u8,
    stopped_after: u8,
}

/// `started_at`, `stopped_after`, the elapsed-time label and the toggle handler.
fn stopwatch() -> Stopwatch {
    let mut builder = ImageBuilder::new();
    let started_at = builder.variable("started_at", INSTANT_TYPE, true, NONE);
    let stopped_after = builder.variable("stopped_after", DURATION_TYPE, true, 0);

    // started_at is none ? stopped_after : now - started_at, then formatted as mm:ss.
    let running = Code::new().system(SystemValue::Now).state(started_at).subtract();
    let stopped = Code::new().state(stopped_after);
    let stopped_branch = stopped.jump(running.len());
    let label = Code {
        bytes: [
            Code::new()
                .state(started_at)
                .test_none()
                .jump_if_false(stopped_branch.len())
                .bytes,
            stopped_branch.bytes,
            running.bytes,
            Code::new().format_duration(MINUTES_SECONDS).bytes,
        ]
        .concat(),
    };
    builder.expression(label);

    // When running: stopped_after = now - started_at; started_at = none.
    // When stopped: started_at = now - stopped_after.
    let stop = Code::new()
        .system(SystemValue::Now)
        .state(started_at)
        .subtract()
        .write_state(stopped_after)
        .none()
        .write_state(started_at);
    let start = Code::new()
        .system(SystemValue::Now)
        .state(stopped_after)
        .subtract()
        .write_state(started_at);
    let start_branch = start.clone().jump(stop.len());
    let toggle = Code {
        bytes: [
            Code::new()
                .state(started_at)
                .test_none()
                .jump_if_false(start_branch.len())
                .bytes,
            start_branch.bytes,
            stop.bytes,
        ]
        .concat(),
    };
    builder.handler(toggle);

    Stopwatch {
        bytes: builder.build(),
        started_at,
        stopped_after,
    }
}

#[test]
fn stopwatch_image_passes_verification() {
    let stopwatch = stopwatch();
    let image = AppImage::parse(&stopwatch.bytes).unwrap();

    assert_eq!(verify(&image), Ok(()));
}

#[test]
fn stopwatch_shows_the_time_since_it_started() {
    let stopwatch = stopwatch();
    let image = AppImage::parse(&stopwatch.bytes).unwrap();
    let state = state_with(&image, &[1_000_000, 0]);
    let system = FakeSystem {
        now: 1_000_000 + 75_400,
        ..Default::default()
    };
    let mut interpreter = Interpreter::new();

    let value = interpreter
        .evaluate_expression(&image, 0, &state, &system)
        .unwrap();

    assert_eq!(interpreter.text(&image, value), Ok("01:15"));
}

#[test]
fn stopped_stopwatch_shows_the_time_it_ran() {
    let stopwatch = stopwatch();
    let image = AppImage::parse(&stopwatch.bytes).unwrap();
    let state = state_with(&image, &[NONE, 9_000]);
    let mut interpreter = Interpreter::new();

    let value = interpreter
        .evaluate_expression(&image, 0, &state, &FakeSystem::default())
        .unwrap();

    assert_eq!(interpreter.text(&image, value), Ok("00:09"));
}

#[test]
fn toggling_a_stopped_stopwatch_starts_it_from_where_it_stopped() {
    let stopwatch = stopwatch();
    let image = AppImage::parse(&stopwatch.bytes).unwrap();
    let mut state = state_with(&image, &[NONE, 9_000]);
    let system = FakeSystem {
        now: 50_000,
        ..Default::default()
    };

    let outcome = Interpreter::new()
        .run_handler(&image, 0, &mut state, &[], &system)
        .unwrap();

    assert_eq!(state.get(stopwatch.started_at), Some(41_000));
    assert_eq!(outcome.changed_variables(), 1 << stopwatch.started_at);
}

#[test]
fn toggling_a_running_stopwatch_stops_it() {
    let stopwatch = stopwatch();
    let image = AppImage::parse(&stopwatch.bytes).unwrap();
    let mut state = state_with(&image, &[41_000, 9_000]);
    let system = FakeSystem {
        now: 60_000,
        ..Default::default()
    };

    let outcome = Interpreter::new()
        .run_handler(&image, 0, &mut state, &[], &system)
        .unwrap();

    assert_eq!(state.get(stopwatch.stopped_after), Some(19_000));
    assert_eq!(state.get(stopwatch.started_at), Some(NONE));
    assert_eq!(outcome.changed_variables(), 0b11);
}

#[test]
fn stopwatch_label_redraws_each_second_from_its_inputs() {
    let stopwatch = stopwatch();
    let image = AppImage::parse(&stopwatch.bytes).unwrap();

    let bindings = Bindings::load(&image).unwrap();
    let info = bindings.info(0).unwrap();

    assert_eq!(info.redraw_period_in_milliseconds, 1000);
    assert_eq!(info.inputs.state_slots, 0b11);
    assert!(info.inputs.reads_clock());
}

// ============================================================================
// Parsing images
// ============================================================================

#[test]
fn parses_the_header() {
    let bytes = ImageBuilder::new().build();

    let image = AppImage::parse(&bytes).unwrap();

    assert_eq!(image.kind, AppKind::App);
    assert_eq!(image.shared_library_version, 1);
    assert_eq!(image.string(image.id_string), Some("test-app"));
    assert_eq!(image.string(image.name_string), Some("Test app"));
}

#[test]
fn reads_state_variables() {
    let mut builder = ImageBuilder::new();
    builder.variable("count", INT_TYPE, true, 7);
    let bytes = builder.build();

    let image = AppImage::parse(&bytes).unwrap();
    let variable = image.state_variable(0).unwrap();

    assert_eq!(image.string(variable.key_string), Some("count"));
    assert_eq!(variable.value_type, ValueType::Int);
    assert!(variable.persists);
    assert_eq!(variable.default_value, 7);
}

#[test]
fn refuses_the_wrong_magic() {
    let mut bytes = ImageBuilder::new().build();
    bytes[0] = b'X';

    assert_eq!(AppImage::parse(&bytes).err(), Some(ImageError::WrongMagic));
}

#[test]
fn refuses_a_corrupted_image() {
    let mut bytes = ImageBuilder::new().build();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;

    assert_eq!(AppImage::parse(&bytes).err(), Some(ImageError::WrongCrc));
}

#[test]
fn refuses_a_section_past_the_end() {
    let mut bytes = ImageBuilder::new().build();
    bytes[28..32].copy_from_slice(&10_000u32.to_le_bytes());
    seal(&mut bytes);

    assert_eq!(
        AppImage::parse(&bytes).err(),
        Some(ImageError::SectionOutOfBounds)
    );
}

#[test]
fn refuses_a_truncated_image() {
    let bytes = ImageBuilder::new().build();

    assert_eq!(AppImage::parse(&bytes[..10]).err(), Some(ImageError::TooShort));
}

#[test]
fn refuses_a_bool_default_that_is_not_zero_or_one() {
    let mut builder = ImageBuilder::new();
    builder.variable("flag", 1, false, 2);
    let bytes = builder.build();

    assert_eq!(
        AppImage::parse(&bytes).err(),
        Some(ImageError::BadDefaultValue(0))
    );
}

#[test]
fn never_panics_on_any_single_corrupted_byte() {
    let stopwatch = stopwatch();
    for index in 0..stopwatch.bytes.len() {
        for flip in [0x01, 0x80, 0xFF] {
            let mut bytes = stopwatch.bytes.clone();
            bytes[index] ^= flip;
            seal(&mut bytes);
            if let Ok(image) = AppImage::parse(&bytes) {
                if verify(&image).is_ok() {
                    let mut interpreter = Interpreter::new();
                    let system = FakeSystem::default();
                    if let Ok(mut state) = AppState::new(&image) {
                        let _ = interpreter.evaluate_expression(&image, 0, &state, &system);
                        let _ = interpreter.run_handler(&image, 0, &mut state, &[], &system);
                    }
                }
            }
        }
    }
}

// ============================================================================
// Verifying code
// ============================================================================

#[test]
fn refuses_an_unknown_instruction() {
    let mut builder = ImageBuilder::new();
    builder.expression(Code::new().op(0xEE));

    assert_eq!(verify_problem(&builder), Some(CodeProblem::Undecodable));
}

#[test]
fn refuses_taking_from_an_empty_stack() {
    let mut builder = ImageBuilder::new();
    builder.expression(Code::new().small(1).add());

    assert_eq!(verify_problem(&builder), Some(CodeProblem::StackUnderflow));
}

#[test]
fn refuses_a_stack_deeper_than_the_interpreter_has() {
    let mut builder = ImageBuilder::new();
    let mut code = Code::new();
    for _ in 0..33 {
        code = code.small(1);
    }
    builder.expression(code);

    assert_eq!(verify_problem(&builder), Some(CodeProblem::StackTooDeep));
}

#[test]
fn refuses_an_expression_that_leaves_two_values() {
    let mut builder = ImageBuilder::new();
    builder.expression(Code::new().small(1).small(2));

    assert_eq!(verify_problem(&builder), Some(CodeProblem::WrongResultCount));
}

#[test]
fn refuses_a_handler_that_leaves_a_value() {
    let mut builder = ImageBuilder::new();
    builder.handler(Code::new().small(1));

    assert_eq!(verify_problem(&builder), Some(CodeProblem::WrongResultCount));
}

#[test]
fn refuses_a_state_write_in_an_expression() {
    let mut builder = ImageBuilder::new();
    builder.variable("count", INT_TYPE, false, 0);
    builder.expression(Code::new().small(1).write_state(0).small(1));

    assert_eq!(verify_problem(&builder), Some(CodeProblem::WriteInExpression));
}

#[test]
fn refuses_an_effect_in_an_expression() {
    let mut builder = ImageBuilder::new();
    builder.expression(Code::new().effect(EffectKind::Back).small(1));

    assert_eq!(verify_problem(&builder), Some(CodeProblem::EffectInExpression));
}

#[test]
fn refuses_a_state_slot_the_app_does_not_have() {
    let mut builder = ImageBuilder::new();
    builder.expression(Code::new().state(3));

    assert_eq!(verify_problem(&builder), Some(CodeProblem::StateSlotOutOfRange));
}

#[test]
fn refuses_a_string_the_app_does_not_have() {
    let mut builder = ImageBuilder::new();
    builder.expression(Code::new().text(40));

    assert_eq!(verify_problem(&builder), Some(CodeProblem::StringOutOfRange));
}

#[test]
fn refuses_a_jump_into_the_middle_of_an_instruction() {
    let mut builder = ImageBuilder::new();
    builder.expression(Code::new().small(1).jump_if_false(1).int(5).small(1).add());

    assert_eq!(verify_problem(&builder), Some(CodeProblem::JumpIntoInstruction));
}

#[test]
fn refuses_a_jump_past_the_end() {
    let mut builder = ImageBuilder::new();
    builder.expression(Code::new().small(1).jump(50));

    assert_eq!(verify_problem(&builder), Some(CodeProblem::JumpPastEnd));
}

#[test]
fn refuses_branches_that_leave_different_stacks() {
    let mut builder = ImageBuilder::new();
    // When false, skip pushing a value, so the two paths meet with different depths.
    let push = Code::new().small(9);
    builder.expression(Code {
        bytes: [Code::new().small(0).jump_if_false(push.len()).bytes, push.bytes].concat(),
    });

    assert_eq!(verify_problem(&builder), Some(CodeProblem::MismatchedStackAtJump));
}

#[test]
fn refuses_reading_an_event_in_an_expression() {
    let mut builder = ImageBuilder::new();
    builder.expression(Code::new().event(0));

    assert_eq!(verify_problem(&builder), Some(CodeProblem::EventInExpression));
}

#[test]
fn reports_where_the_problem_is() {
    let mut builder = ImageBuilder::new();
    builder.expression(Code::new().small(1));
    builder.handler(Code::new().small(1).small(2).add().op(0xEE));
    let bytes = builder.build();

    let error = verify(&AppImage::parse(&bytes).unwrap());

    assert_eq!(
        error,
        Err(ImageError::BadCode {
            block: CodeBlock::Handler(0),
            offset: 5,
            problem: CodeProblem::Undecodable
        })
    );
}

// ============================================================================
// Running code
// ============================================================================

#[test]
fn does_arithmetic() {
    assert_eq!(
        evaluate(Code::new().small(6).small(7).multiply().int(-2).add()),
        Ok(40)
    );
}

#[test]
fn division_by_zero_fails() {
    assert_eq!(
        evaluate(Code::new().small(1).small(0).divide()),
        Err(Failure::DivideByZero)
    );
}

#[test]
fn division_truncates_toward_zero() {
    assert_eq!(evaluate(Code::new().small(-7).small(2).divide()), Ok(-3));
}

#[test]
fn overflow_fails() {
    assert_eq!(
        evaluate(Code::new().long(i64::MAX).small(1).add()),
        Err(Failure::Overflow)
    );
}

#[test]
fn takes_the_branch_the_condition_picks() {
    let then_branch = Code::new().small(10);
    let else_branch = Code::new().small(20);
    let choose = |condition: i8| {
        let then_with_jump = then_branch.clone().jump(else_branch.len());
        Code {
            bytes: [
                Code::new()
                    .small(condition)
                    .jump_if_false(then_with_jump.len())
                    .bytes,
                then_with_jump.bytes,
                else_branch.bytes.clone(),
            ]
            .concat(),
        }
    };

    assert_eq!(evaluate(choose(1)), Ok(10));
    assert_eq!(evaluate(choose(0)), Ok(20));
}

#[test]
fn rolls_a_random_number_inside_the_range() {
    let mut builder = ImageBuilder::new();
    let roll = builder.variable("roll", INT_TYPE, false, 0);
    builder.handler(Code::new().small(1).small(6).op(0x19).write_state(roll));
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();

    let mut state = state_with(&image, &[0]);

    Interpreter::new()
        .run_handler(&image, 0, &mut state, &[], &FakeSystem::default())
        .unwrap();

    assert_eq!(state.get(roll), Some(1 + 1_000_003 % 6));
}

#[test]
fn refuses_randomness_in_an_expression() {
    let mut builder = ImageBuilder::new();
    builder.expression(Code::new().small(1).small(6).op(0x19));

    assert_eq!(verify_problem(&builder), Some(CodeProblem::RandomInExpression));
}

#[test]
fn does_bitwise_arithmetic_for_day_masks() {
    // Is Wednesday (2) in the mask for Monday, Wednesday and Friday?
    let wednesday_set = Code::new().small(0b10101).small(1).small(2).op(0x1E).op(0x1A);
    let all_but_one = Code::new().small(1).op(0x1D);

    assert_eq!(evaluate(wednesday_set), Ok(4));
    assert_eq!(evaluate(all_but_one), Ok(-2));
    assert_eq!(
        evaluate(Code::new().small(1).small(64).op(0x1E)),
        Err(Failure::Overflow)
    );
}

#[test]
fn reads_system_values() {
    assert_eq!(evaluate(Code::new().system(SystemValue::BatteryPercent)), Ok(80));
}

#[test]
fn handler_reads_its_own_writes() {
    let mut builder = ImageBuilder::new();
    let count = builder.variable("count", INT_TYPE, false, 0);
    builder.handler(
        Code::new()
            .state(count)
            .small(1)
            .add()
            .write_state(count)
            .state(count)
            .small(1)
            .add()
            .write_state(count),
    );
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();

    let mut state = state_with(&image, &[5]);

    Interpreter::new()
        .run_handler(&image, 0, &mut state, &[], &FakeSystem::default())
        .unwrap();

    assert_eq!(state.get(count), Some(7));
}

#[test]
fn handler_reads_event_fields() {
    let mut builder = ImageBuilder::new();
    let total = builder.variable("total", INT_TYPE, false, 0);
    builder.handler(Code::new().event(1).write_state(total));
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();

    let mut state = state_with(&image, &[0]);

    Interpreter::new()
        .run_handler(&image, 0, &mut state, &[3, 42], &FakeSystem::default())
        .unwrap();

    assert_eq!(state.get(total), Some(42));
}

#[test]
fn handler_collects_effects_in_order() {
    let mut builder = ImageBuilder::new();
    let title = builder.string("Timer done");
    builder.handler(
        Code::new()
            .int(200)
            .effect(EffectKind::Vibrate)
            .text(title)
            .text(title)
            .effect(EffectKind::Notify)
            .effect(EffectKind::Close),
    );
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();

    let mut state = state_with(&image, &[]);

    let outcome = Interpreter::new()
        .run_handler(&image, 0, &mut state, &[], &FakeSystem::default())
        .unwrap();

    let kinds: Vec<EffectKind> = outcome.effects().iter().map(|effect| effect.kind).collect();
    assert_eq!(
        kinds,
        [EffectKind::Vibrate, EffectKind::Notify, EffectKind::Close]
    );
    assert_eq!(outcome.effects()[0].arguments[0], 200);
}

#[test]
fn failing_handler_changes_nothing() {
    let mut builder = ImageBuilder::new();
    let count = builder.variable("count", INT_TYPE, false, 0);
    builder.handler(
        Code::new()
            .small(1)
            .write_state(count)
            .small(100)
            .effect(EffectKind::Vibrate)
            .small(1)
            .small(0)
            .divide()
            .drop_value(),
    );
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();

    let mut state = state_with(&image, &[0]);

    let result = Interpreter::new().run_handler(&image, 0, &mut state, &[], &FakeSystem::default());

    assert_eq!(result.err(), Some(Failure::DivideByZero));
    assert_eq!(state.get(count), Some(0));
}

#[test]
fn too_many_effects_fail() {
    let mut builder = ImageBuilder::new();
    let mut code = Code::new();
    for _ in 0..9 {
        code = code.effect(EffectKind::Back);
    }
    builder.handler(code);
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();

    let mut state = state_with(&image, &[]);

    let result = Interpreter::new().run_handler(&image, 0, &mut state, &[], &FakeSystem::default());

    assert_eq!(result.err(), Some(Failure::TooManyEffects));
}

// ============================================================================
// Text and time
// ============================================================================

#[test]
fn formats_numbers_with_leading_zeros() {
    assert_eq!(
        evaluate_text(Code::new().small(7).format_number(3), &FakeSystem::default()),
        "007"
    );
    assert_eq!(
        evaluate_text(Code::new().small(-42).format_number(0), &FakeSystem::default()),
        "-42"
    );
}

#[test]
fn formats_hundredths_for_a_stopwatch() {
    let code = Code::new()
        .int(83_456)
        .format_duration(MINUTES_SECONDS_HUNDREDTHS);

    assert_eq!(evaluate_text(code, &FakeSystem::default()), "01:23.45");
}

#[test]
fn formats_the_clock_in_24_and_12_hours() {
    let clock = Code::new().system(SystemValue::Now).format_time(CLOCK);
    let twenty_four = FakeSystem {
        now: SUNDAY_AFTERNOON,
        uses_24_hour_clock: true,
        ..Default::default()
    };
    let twelve = FakeSystem {
        now: SUNDAY_AFTERNOON,
        uses_24_hour_clock: false,
        ..Default::default()
    };

    assert_eq!(evaluate_text(clock.clone(), &twenty_four), "14:05");
    assert_eq!(evaluate_text(clock, &twelve), "2:05");
}

#[test]
fn applies_the_time_zone() {
    let clock = Code::new()
        .system(SystemValue::Now)
        .format_time(CLOCK_WITH_SECONDS);
    let system = FakeSystem {
        now: SUNDAY_AFTERNOON,
        utc_offset: -4 * 3_600_000,
        uses_24_hour_clock: true,
    };

    assert_eq!(evaluate_text(clock, &system), "10:05:09");
}

#[test]
fn formats_the_date() {
    let date = Code::new().system(SystemValue::Now).format_time(DATE);
    let system = FakeSystem {
        now: SUNDAY_AFTERNOON,
        ..Default::default()
    };

    assert_eq!(evaluate_text(date, &system), "2026-10-11");
}

#[test]
fn splits_a_time_into_parts() {
    let system = FakeSystem {
        now: SUNDAY_AFTERNOON,
        ..Default::default()
    };
    let part = |part: u8| {
        let mut builder = ImageBuilder::new();
        builder.expression(Code::new().system(SystemValue::Now).time_part(part));
        let bytes = builder.build();
        let image = AppImage::parse(&bytes).unwrap();
        Interpreter::new()
            .evaluate_expression(&image, 0, &AppState::new(&image).unwrap(), &system)
            .unwrap()
    };

    assert_eq!(
        [part(0), part(1), part(2), part(3), part(4), part(5), part(6)],
        [2026, 10, 11, 14, 5, 9, 6]
    );
}

#[test]
fn handles_dates_before_1970_and_leap_days() {
    let leap_day = text::DateTime::from_local_milliseconds(951_782_400_000);
    let before_1970 = text::DateTime::from_local_milliseconds(-86_400_000);

    assert_eq!((leap_day.year, leap_day.month, leap_day.day), (2000, 2, 29));
    assert_eq!(
        (
            before_1970.year,
            before_1970.month,
            before_1970.day,
            before_1970.weekday
        ),
        (1969, 12, 31, 2)
    );
}

#[test]
fn joins_image_strings_and_formatted_text() {
    let mut builder = ImageBuilder::new();
    let steps = builder.string(" steps");
    builder.expression(Code::new().int(8_421).format_number(0).text(steps).join());
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();
    let mut interpreter = Interpreter::new();

    let value = interpreter
        .evaluate_expression(&image, 0, &AppState::new(&image).unwrap(), &FakeSystem::default())
        .unwrap();

    assert_eq!(interpreter.text(&image, value), Ok("8421 steps"));
}

#[test]
fn text_that_overflows_the_buffer_fails() {
    let mut builder = ImageBuilder::new();
    let long = builder.string(&"x".repeat(200));
    builder.expression(Code::new().text(long).text(long).join());
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();

    let result = Interpreter::new().evaluate_expression(
        &image,
        0,
        &AppState::new(&image).unwrap(),
        &FakeSystem::default(),
    );

    assert_eq!(result, Err(Failure::TextBufferFull));
}

#[test]
fn a_number_is_not_mistaken_for_text() {
    let bytes = ImageBuilder::new().build();
    let image = AppImage::parse(&bytes).unwrap();

    assert_eq!(Interpreter::new().text(&image, 1 << 50), Err(Failure::BadText));
}

// ============================================================================
// Bindings
// ============================================================================

fn bindings_for(expressions: &[Code]) -> (Vec<u8>, usize) {
    let mut builder = ImageBuilder::new();
    builder.variable("a", INT_TYPE, false, 0);
    builder.variable("b", INT_TYPE, false, 0);
    for code in expressions {
        builder.expression(code.clone());
    }
    (builder.build(), expressions.len())
}

#[test]
fn a_clock_face_redraws_once_a_minute() {
    let (bytes, _) = bindings_for(&[Code::new().system(SystemValue::Now).format_time(CLOCK)]);
    let bindings = Bindings::load(&AppImage::parse(&bytes).unwrap()).unwrap();

    assert_eq!(
        bindings.next_redraw_after(SUNDAY_AFTERNOON),
        Some(1_791_727_560_000)
    );
}

#[test]
fn nothing_redraws_by_time_when_nothing_reads_the_clock() {
    let (bytes, _) = bindings_for(&[Code::new().state(0).format_number(0)]);
    let bindings = Bindings::load(&AppImage::parse(&bytes).unwrap()).unwrap();

    assert_eq!(bindings.next_redraw_after(SUNDAY_AFTERNOON), None);
}

#[test]
fn hundredths_redraw_ten_times_a_second() {
    let (bytes, _) = bindings_for(&[Code::new()
        .system(SystemValue::Now)
        .format_duration(MINUTES_SECONDS_HUNDREDTHS)]);
    let bindings = Bindings::load(&AppImage::parse(&bytes).unwrap()).unwrap();

    assert_eq!(bindings.info(0).unwrap().redraw_period_in_milliseconds, 100);
}

#[test]
fn a_changed_slot_marks_only_the_expressions_that_read_it() {
    let (bytes, _) = bindings_for(&[
        Code::new().state(0),
        Code::new().state(1),
        Code::new().state(0).state(1).add(),
        Code::new().system(SystemValue::BatteryPercent),
    ]);
    let bindings = Bindings::load(&AppImage::parse(&bytes).unwrap()).unwrap();

    let affected: Vec<usize> = bindings.affected_by(Inputs::state_slot(1)).collect();
    let battery: Vec<usize> = bindings
        .affected_by(Inputs::system_value(SystemValue::BatteryPercent))
        .collect();

    assert_eq!(affected, [1, 2]);
    assert_eq!(battery, [3]);
}

#[test]
fn only_expressions_whose_step_passed_are_due() {
    let (bytes, _) = bindings_for(&[
        Code::new()
            .system(SystemValue::Now)
            .format_time(CLOCK_WITH_SECONDS),
        Code::new().system(SystemValue::Now).format_time(CLOCK),
    ]);
    let bindings = Bindings::load(&AppImage::parse(&bytes).unwrap()).unwrap();
    let start_of_minute = 1_791_727_500_000;

    let a_second_later: Vec<usize> = bindings
        .due_between(start_of_minute, start_of_minute + 1_000)
        .collect();
    let a_minute_later: Vec<usize> = bindings
        .due_between(start_of_minute, start_of_minute + 60_000)
        .collect();

    assert_eq!(a_second_later, [0]);
    assert_eq!(a_minute_later, [0, 1]);
}

// ============================================================================
// Format patterns
// ============================================================================

fn format_time_with(pattern: &str, system: &FakeSystem) -> String {
    let mut builder = ImageBuilder::new();
    let pattern = builder.string(pattern);
    builder.expression(Code::new().system(SystemValue::Now).format_time(pattern));
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();
    let mut interpreter = Interpreter::new();
    let value = interpreter
        .evaluate_expression(&image, 0, &AppState::new(&image).unwrap(), system)
        .unwrap();
    interpreter.text(&image, value).unwrap().into()
}

fn format_duration_with(pattern: &str, milliseconds: i64) -> String {
    let mut builder = ImageBuilder::new();
    let pattern = builder.string(pattern);
    builder.expression(Code::new().long(milliseconds).format_duration(pattern));
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();
    let mut interpreter = Interpreter::new();
    let value = interpreter
        .evaluate_expression(&image, 0, &AppState::new(&image).unwrap(), &FakeSystem::default())
        .unwrap();
    interpreter.text(&image, value).unwrap().into()
}

fn pattern_problem(pattern: &str, is_time: bool) -> Option<CodeProblem> {
    let mut builder = ImageBuilder::new();
    let pattern = builder.string(pattern);
    let code = Code::new().system(SystemValue::Now);
    builder.expression(if is_time {
        code.format_time(pattern)
    } else {
        code.format_duration(pattern)
    });
    verify_problem(&builder)
}

#[test]
fn writes_weekday_and_month_names() {
    let system = FakeSystem {
        now: SUNDAY_AFTERNOON,
        ..Default::default()
    };

    assert_eq!(
        format_time_with("dddd D MMMM YYYY", &system),
        "Sunday 11 October 2026"
    );
    assert_eq!(format_time_with("ddd DD MMM", &system), "Sun 11 Oct");
    assert_eq!(format_time_with("D/M", &system), "11/10");
}

#[test]
fn writes_a_12_hour_clock_with_and_without_am_and_pm() {
    let system = FakeSystem {
        now: SUNDAY_AFTERNOON,
        uses_24_hour_clock: true,
        ..Default::default()
    };

    assert_eq!(format_time_with("hh:mm", &system), "02:05");
    assert_eq!(format_time_with("h:mm a", &system), "2:05 pm");
    assert_eq!(format_time_with("h:mm A", &system), "2:05 PM");
    assert_eq!(format_time_with("H:mm:ss", &system), "14:05:09");
}

#[test]
fn copies_bracketed_text_as_it_is() {
    let system = FakeSystem {
        now: SUNDAY_AFTERNOON,
        uses_24_hour_clock: true,
        ..Default::default()
    };

    assert_eq!(format_time_with("[Day] D", &system), "Day 11");
}

#[test]
fn the_largest_duration_unit_shows_the_whole_amount() {
    assert_eq!(format_duration_with("mm:ss", 75 * 60_000 + 3_000), "75:03");
    assert_eq!(format_duration_with("h:mm:ss", 75 * 60_000 + 3_000), "1:15:03");
    assert_eq!(format_duration_with("s.cc", 83_456), "83.45");
    assert_eq!(format_duration_with("m[ min]", 125_000), "2 min");
}

#[test]
fn refuses_a_pattern_with_an_unknown_letter() {
    assert_eq!(pattern_problem("HH:mm x", true), Some(CodeProblem::BadPattern));
    assert_eq!(pattern_problem("mm:ss.c", false), Some(CodeProblem::BadPattern));
    assert_eq!(pattern_problem("[unclosed", true), Some(CodeProblem::BadPattern));
    assert_eq!(pattern_problem("dddd, D MMMM", true), None);
}

#[test]
fn a_pattern_redraws_as_often_as_its_smallest_unit() {
    let step = |pattern: &str| {
        let mut builder = ImageBuilder::new();
        let pattern = builder.string(pattern);
        builder.expression(Code::new().system(SystemValue::Now).format_time(pattern));
        let bytes = builder.build();
        let bindings = Bindings::load(&AppImage::parse(&bytes).unwrap()).unwrap();
        bindings.info(0).unwrap().redraw_period_in_milliseconds
    };

    assert_eq!(step("dddd"), 60_000);
    assert_eq!(step("h:mm:ss a"), 1_000);
}

// ============================================================================
// Widgets
// ============================================================================

use PropertySource::{Constant, Expression, Handler};

fn widget_problem(builder: &ImageBuilder) -> Option<WidgetProblem> {
    let bytes = builder.build();
    match verify(&AppImage::parse(&bytes).unwrap()) {
        Err(ImageError::BadWidget { problem, .. }) => Some(problem),
        Err(other) => panic!("unexpected error {other:?}"),
        Ok(()) => None,
    }
}

/// The stopwatch's screen: a column holding its time label and its start button.
fn stopwatch_screen() -> ImageBuilder {
    let mut builder = ImageBuilder::new();
    let start = builder.string("Start");
    let elapsed = builder.expression(Code::new().small(0).format_duration(MINUTES_SECONDS));
    let toggle = builder.handler(Code::new());
    builder.widget(WidgetKind::Column, 2, &[]);
    builder.widget(
        WidgetKind::Label,
        0,
        &[
            (Property::Text, Expression(elapsed)),
            (Property::Font, Constant(2)),
        ],
    );
    builder.widget(
        WidgetKind::Button,
        0,
        &[
            (Property::Text, Constant(start as i32)),
            (Property::OnClick, Handler(toggle)),
        ],
    );
    builder
}

#[test]
fn a_screen_passes_its_checks() {
    assert_eq!(widget_problem(&stopwatch_screen()), None);
}

#[test]
fn walks_the_tree_and_reads_properties() {
    let bytes = stopwatch_screen().build();
    let image = AppImage::parse(&bytes).unwrap();
    let tree = WidgetTree::new(image);

    let screens: Vec<u16> = tree.screens().collect();
    let children: Vec<u16> = tree.children(0).collect();
    let button = tree.widget(2).unwrap();

    assert_eq!(screens, [0]);
    assert_eq!(children, [1, 2]);
    assert_eq!(button.kind, WidgetKind::Button);
    assert_eq!(button.property(Property::OnClick), Some(Handler(0)));
    assert_eq!(
        tree.widget(1).unwrap().property(Property::Text),
        Some(Expression(0))
    );
}

#[test]
fn finds_a_screen_by_name() {
    let mut builder = ImageBuilder::new();
    let main = builder.string("main");
    let settings = builder.string("settings");
    builder.widget(
        WidgetKind::Column,
        1,
        &[(Property::ScreenName, Constant(main as i32))],
    );
    builder.widget(WidgetKind::Label, 0, &[]);
    builder.widget(
        WidgetKind::Column,
        0,
        &[(Property::ScreenName, Constant(settings as i32))],
    );
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();
    let tree = WidgetTree::new(image);

    let screens: Vec<u16> = tree.screens().collect();

    assert_eq!(widget_problem(&builder), None);
    assert_eq!(screens, [0, 2]);
    assert_eq!(tree.screen_named("settings"), Some(2));
    assert_eq!(tree.screen_named("missing"), None);
}

#[test]
fn refuses_children_inside_a_label() {
    let mut builder = ImageBuilder::new();
    builder.widget(WidgetKind::Label, 1, &[]);
    builder.widget(WidgetKind::Label, 0, &[]);

    assert_eq!(widget_problem(&builder), Some(WidgetProblem::ChildrenNotAllowed));
}

#[test]
fn refuses_a_tree_that_ends_early() {
    let mut builder = ImageBuilder::new();
    builder.widget(WidgetKind::Column, 2, &[]);
    builder.widget(WidgetKind::Label, 0, &[]);

    assert_eq!(widget_problem(&builder), Some(WidgetProblem::Unreadable));
}

#[test]
fn refuses_nesting_deeper_than_the_limit() {
    let mut builder = ImageBuilder::new();
    for _ in 0..17 {
        builder.widget(WidgetKind::Column, 1, &[]);
    }
    builder.widget(WidgetKind::Label, 0, &[]);

    assert_eq!(widget_problem(&builder), Some(WidgetProblem::TooDeep));
}

#[test]
fn refuses_a_property_the_widget_does_not_have() {
    let mut builder = ImageBuilder::new();
    builder.widget(WidgetKind::Label, 0, &[(Property::ShowSeconds, Constant(1))]);

    assert_eq!(widget_problem(&builder), Some(WidgetProblem::PropertyNotOnWidget));
}

#[test]
fn refuses_a_screen_event_below_the_screen_root() {
    let mut builder = ImageBuilder::new();
    let handler = builder.handler(Code::new());
    builder.widget(WidgetKind::Column, 1, &[]);
    builder.widget(WidgetKind::Column, 0, &[(Property::OnSwipe, Handler(handler))]);

    assert_eq!(
        widget_problem(&builder),
        Some(WidgetProblem::ScreenPropertyBelowRoot)
    );
}

#[test]
fn refuses_an_event_without_a_handler_and_a_handler_on_a_value() {
    let mut event_as_constant = ImageBuilder::new();
    event_as_constant.widget(WidgetKind::Button, 0, &[(Property::OnClick, Constant(0))]);
    let mut handler_as_value = ImageBuilder::new();
    handler_as_value.handler(Code::new());
    handler_as_value.widget(WidgetKind::Label, 0, &[(Property::Text, Handler(0))]);

    assert_eq!(
        widget_problem(&event_as_constant),
        Some(WidgetProblem::EventWithoutHandler)
    );
    assert_eq!(
        widget_problem(&handler_as_value),
        Some(WidgetProblem::HandlerOnNonEvent)
    );
}

#[test]
fn refuses_missing_expressions_handlers_and_strings() {
    let mut missing_expression = ImageBuilder::new();
    missing_expression.widget(WidgetKind::Label, 0, &[(Property::Text, Expression(3))]);
    let mut missing_handler = ImageBuilder::new();
    missing_handler.widget(WidgetKind::Button, 0, &[(Property::OnClick, Handler(3))]);
    let mut missing_string = ImageBuilder::new();
    missing_string.widget(WidgetKind::Label, 0, &[(Property::Text, Constant(900))]);

    assert_eq!(
        widget_problem(&missing_expression),
        Some(WidgetProblem::ExpressionOutOfRange)
    );
    assert_eq!(
        widget_problem(&missing_handler),
        Some(WidgetProblem::HandlerOutOfRange)
    );
    assert_eq!(widget_problem(&missing_string), Some(WidgetProblem::BadConstant));
}

#[test]
fn refuses_a_property_given_twice() {
    let mut builder = ImageBuilder::new();
    builder.widget(
        WidgetKind::Label,
        0,
        &[(Property::Font, Constant(1)), (Property::Font, Constant(2))],
    );

    assert_eq!(widget_problem(&builder), Some(WidgetProblem::RepeatedProperty));
}

// ============================================================================
// Arrays and loops
// ============================================================================

/// `for (local 0 = 0; local 0 < count; local 0 += 1) { body }`, with an empty stack around the body.
fn counting_loop(count: i8, body: Code) -> Code {
    let step = Code::new().read_local(0).small(1).add().write_local(0);
    let body = body.then(step);
    // The loop back covers the test, the jump out, the body and itself.
    let test = Code::new().read_local(0).small(count).op(0x22);
    let exit_distance = body.len() + 3;
    let back_distance = 1 + test.len() + 3 + body.len() + 3;
    Code::new()
        .small(0)
        .write_local(0)
        .loop_start()
        .then(test)
        .jump_if_false(exit_distance)
        .then(body)
        .loop_back(back_distance)
}

#[test]
fn a_loop_sums_the_numbers_below_ten() {
    let sum_into_local_1 = Code::new().read_local(1).read_local(0).add().write_local(1);
    let code = counting_loop(10, sum_into_local_1).read_local(1);

    assert_eq!(evaluate(code), Ok(45));
}

#[test]
fn a_loop_that_never_ends_is_stopped() {
    let mut builder = ImageBuilder::new();
    builder.handler(Code::new().loop_start().loop_back(4));
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = state_with(&image, &[]);

    assert_eq!(verify(&image), Ok(()));
    assert_eq!(
        Interpreter::new()
            .run_handler(&image, 0, &mut state, &[], &FakeSystem::default())
            .err(),
        Some(Failure::TooManySteps)
    );
}

#[test]
fn a_handler_writes_every_cell_of_a_board() {
    let mut builder = ImageBuilder::new();
    let board = builder.array("board", 400, 1, 0);
    // board[i] = i % 7, for every cell.
    let set_cell = Code::new()
        .read_local(0)
        .read_local(0)
        .small(7)
        .op(0x14)
        .write_element(board);
    builder.handler(counting_loop(127, set_cell.clone()));
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = state_with(&image, &[]);

    let outcome = Interpreter::new()
        .run_handler(&image, 0, &mut state, &[], &FakeSystem::default())
        .unwrap();

    assert_eq!(state.placement(board).unwrap().stored_bytes(), 400);
    assert_eq!(state.element(board, 10), Some(3));
    assert_eq!(state.element(board, 126), Some(0));
    assert_eq!(state.element(board, 127), Some(0));
    assert_eq!(outcome.changed_variables(), 1 << board);
}

#[test]
fn reads_elements_and_fills_and_copies_arrays() {
    let mut builder = ImageBuilder::new();
    let first = builder.array("first", 16, 2, 0);
    let second = builder.array("second", 16, 4, 0);
    let total = builder.variable("total", INT_TYPE, false, 0);
    builder.handler(
        Code::new()
            .int(-300)
            .fill(first)
            .copy_array(second, first)
            .small(5)
            .element(second)
            .write_state(total),
    );
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = state_with(&image, &[]);

    let outcome = Interpreter::new()
        .run_handler(&image, 0, &mut state, &[], &FakeSystem::default())
        .unwrap();

    assert_eq!(state.element(second, 15), Some(-300));
    assert_eq!(state.get(total), Some(-300));
    assert_eq!(outcome.changed_variables(), 0b111);
}

#[test]
fn an_element_past_the_end_fails_and_changes_nothing() {
    let mut builder = ImageBuilder::new();
    let board = builder.array("board", 4, 1, 9);
    builder.handler(
        Code::new()
            .small(0)
            .small(1)
            .write_element(board)
            .small(4)
            .small(1)
            .write_element(board),
    );
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = state_with(&image, &[]);

    let result = Interpreter::new().run_handler(&image, 0, &mut state, &[], &FakeSystem::default());

    assert_eq!(result.err(), Some(Failure::IndexOutOfRange));
    assert_eq!(state.element(board, 0), Some(9));
}

#[test]
fn a_value_too_big_for_its_element_fails() {
    let mut builder = ImageBuilder::new();
    let board = builder.array("board", 4, 1, 0);
    builder.handler(Code::new().small(0).int(300).write_element(board));
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = state_with(&image, &[]);

    let result = Interpreter::new().run_handler(&image, 0, &mut state, &[], &FakeSystem::default());

    assert_eq!(result.err(), Some(Failure::Overflow));
}

#[test]
fn a_handler_cannot_change_more_than_the_scratch_area_holds() {
    let mut builder = ImageBuilder::new();
    let big = builder.array("big", 1000, 1, 0);
    let other = builder.array("other", 100, 1, 0);
    builder.handler(
        Code::new()
            .small(0)
            .small(1)
            .write_element(big)
            .small(0)
            .small(1)
            .write_element(other),
    );
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = state_with(&image, &[]);

    let result = Interpreter::new().run_handler(&image, 0, &mut state, &[], &FakeSystem::default());

    assert_eq!(result.err(), Some(Failure::ScratchFull));
}

#[test]
fn refuses_state_too_large_for_ram() {
    let mut too_much = ImageBuilder::new();
    too_much.array("first", 1000, 1, 0);
    too_much.array("second", 1000, 1, 0);
    too_much.array("third", 100, 1, 0);
    let mut one_too_big = ImageBuilder::new();
    one_too_big.array("board", 1100, 1, 0);

    assert_eq!(
        AppImage::parse(&too_much.build()).err(),
        Some(ImageError::StateTooLarge)
    );
    assert_eq!(
        AppImage::parse(&one_too_big.build()).err(),
        Some(ImageError::StateTooLarge)
    );
}

#[test]
fn refuses_an_array_default_that_does_not_fit() {
    let mut builder = ImageBuilder::new();
    builder.array("board", 4, 1, 1000);

    assert_eq!(
        AppImage::parse(&builder.build()).err(),
        Some(ImageError::BadDefaultValue(0))
    );
}

#[test]
fn refuses_a_loop_entered_with_values_on_the_stack() {
    let mut builder = ImageBuilder::new();
    builder.handler(Code::new().small(1).loop_start().drop_value());

    assert_eq!(verify_problem(&builder), Some(CodeProblem::LoopWithStack));
}

#[test]
fn refuses_a_loop_back_that_misses_its_start() {
    let mut builder = ImageBuilder::new();
    builder.handler(Code::new().loop_start().small(1).drop_value().loop_back(3));

    assert_eq!(verify_problem(&builder), Some(CodeProblem::BadLoop));
}

#[test]
fn refuses_using_an_array_as_a_value_and_a_value_as_an_array() {
    let mut array_as_value = ImageBuilder::new();
    let board = array_as_value.array("board", 4, 1, 0);
    array_as_value.expression(Code::new().state(board));
    let mut value_as_array = ImageBuilder::new();
    let count = value_as_array.variable("count", INT_TYPE, false, 0);
    value_as_array.expression(Code::new().small(0).element(count));

    assert_eq!(
        verify_problem(&array_as_value),
        Some(CodeProblem::WrongVariableKind)
    );
    assert_eq!(
        verify_problem(&value_as_array),
        Some(CodeProblem::WrongVariableKind)
    );
}

#[test]
fn refuses_copying_arrays_of_different_lengths() {
    let mut builder = ImageBuilder::new();
    let short = builder.array("short", 4, 1, 0);
    let long = builder.array("long", 8, 1, 0);
    builder.handler(Code::new().copy_array(long, short));

    assert_eq!(verify_problem(&builder), Some(CodeProblem::WrongVariableKind));
}

#[test]
fn refuses_array_writes_in_an_expression_and_locals_past_the_limit() {
    let mut write_in_expression = ImageBuilder::new();
    let board = write_in_expression.array("board", 4, 1, 0);
    write_in_expression.expression(Code::new().small(0).small(1).write_element(board).small(1));
    let mut local_too_far = ImageBuilder::new();
    local_too_far.expression(Code::new().read_local(16));

    assert_eq!(
        verify_problem(&write_in_expression),
        Some(CodeProblem::WriteInExpression)
    );
    assert_eq!(verify_problem(&local_too_far), Some(CodeProblem::LocalOutOfRange));
}

#[test]
fn an_expression_reading_an_array_redraws_when_it_changes() {
    let mut builder = ImageBuilder::new();
    builder.variable("score", INT_TYPE, false, 0);
    let board = builder.array("board", 16, 1, 0);
    builder.expression(Code::new().small(3).element(board));
    let bytes = builder.build();
    let bindings = Bindings::load(&AppImage::parse(&bytes).unwrap()).unwrap();

    let affected: Vec<usize> = bindings.affected_by(Inputs::state_slot(board)).collect();

    assert_eq!(affected, [0]);
}

// ============================================================================
// Writing images
// ============================================================================

use crate::events::EventKind;
use crate::instruction::{decode, Instruction, TimePart as Part, TimeUnit};
use crate::writer::{CodeError, CodeWriter, ImageWriter};

#[test]
fn every_instruction_decodes_to_itself() {
    use Instruction::*;
    let instructions = [
        PushSmall(-5),
        PushInt(-70_000),
        PushLong(i64::MIN + 3),
        PushNone,
        PushText(513),
        ReadState(7),
        ReadSystem(SystemValue::WeatherReceivedAt),
        ReadEvent(3),
        ReadEventText(2),
        Duplicate,
        Drop,
        Swap,
        ReadLocal(15),
        WriteLocal(2),
        Add,
        Subtract,
        Multiply,
        Divide,
        Remainder,
        Negate,
        Minimum,
        Maximum,
        Absolute,
        RandomInt,
        BitAnd,
        BitOr,
        BitXor,
        BitNot,
        ShiftLeft,
        ShiftRight,
        Equal,
        NotEqual,
        Less,
        LessOrEqual,
        Greater,
        GreaterOrEqual,
        Not,
        IsNone,
        Jump(300),
        JumpIfFalse(2),
        LoopStart,
        LoopBack(1_000),
        TimePart(Part::Weekday),
        StartOf(TimeUnit::Day),
        TextFromCodes(6),
        ToUpper,
        TextLength,
        SliceText,
        PadStart,
        FormatNumber(3),
        FormatDuration(9),
        FormatTime(258),
        Join,
        WriteState(4),
        Effect(EffectKind::SetHeartRateLed),
        ReadElement(1),
        WriteElement(2),
        CopyArray {
            destination: 3,
            source: 4,
        },
        FillArray(5),
    ];

    for instruction in instructions {
        let encoded = instruction.encode();
        let (decoded, next_offset) = decode(encoded.as_bytes(), 0).unwrap();

        assert_eq!(decoded, instruction);
        assert_eq!(next_offset, encoded.as_bytes().len());
    }
}

#[test]
fn the_writer_builds_a_stopwatch_that_runs() {
    let mut writer = ImageWriter::new("stopwatch", "Stopwatch", AppKind::App, 1);
    let started_at = writer.variable("started_at", ValueType::Instant, true, NONE);
    let stopped_after = writer.variable("stopped_after", ValueType::Duration, true, 0);
    let pattern = writer.string("mm:ss");

    // started_at is none ? stopped_after : now - started_at, as mm:ss.
    let mut label = CodeWriter::new();
    let running = label.label();
    let done = label.label();
    label
        .emit(Instruction::ReadState(started_at))
        .emit(Instruction::IsNone)
        .jump_if_false(running);
    label.emit(Instruction::ReadState(stopped_after)).jump(done);
    label
        .place(running)
        .emit(Instruction::ReadSystem(SystemValue::Now))
        .emit(Instruction::ReadState(started_at));
    label.emit(Instruction::Subtract);
    label.place(done).emit(Instruction::FormatDuration(pattern));
    let elapsed = writer.expression(label.finish().unwrap());

    let mut toggle = CodeWriter::new();
    toggle
        .emit(Instruction::ReadSystem(SystemValue::Now))
        .emit(Instruction::ReadState(stopped_after));
    toggle
        .emit(Instruction::Subtract)
        .emit(Instruction::WriteState(started_at));
    let start = writer.handler(toggle.finish().unwrap());

    let column = writer.widget(WidgetKind::Column, 0, &[]);
    writer.widget(
        WidgetKind::Label,
        0,
        &[(Property::Text, PropertySource::Expression(elapsed))],
    );
    writer.widget(
        WidgetKind::Button,
        0,
        &[(Property::OnClick, PropertySource::Handler(start))],
    );
    writer.set_child_count(column, 2);
    let bytes = writer.build();

    let image = AppImage::parse(&bytes).unwrap();
    assert_eq!(verify(&image), Ok(()));
    assert_eq!(image.string(image.name_string), Some("Stopwatch"));

    let mut state = AppState::new(&image).unwrap();
    let system = FakeSystem {
        now: 90_000,
        ..Default::default()
    };
    let mut interpreter = Interpreter::new();
    interpreter
        .run_handler(&image, start, &mut state, &[], &system)
        .unwrap();
    let later = FakeSystem {
        now: 155_000,
        ..Default::default()
    };
    let value = interpreter
        .evaluate_expression(&image, elapsed, &state, &later)
        .unwrap();

    assert_eq!(interpreter.text(&image, value), Ok("01:05"));
}

#[test]
fn the_code_writer_fills_in_loops_and_refuses_unplaced_labels() {
    let mut summing = CodeWriter::new();
    let done = summing.label();
    summing
        .emit(Instruction::PushSmall(0))
        .emit(Instruction::WriteLocal(0));
    let top = summing.loop_start();
    summing
        .emit(Instruction::ReadLocal(0))
        .emit(Instruction::PushSmall(5))
        .emit(Instruction::Less);
    summing.jump_if_false(done);
    summing
        .emit(Instruction::ReadLocal(1))
        .emit(Instruction::ReadLocal(0))
        .emit(Instruction::Add);
    summing.emit(Instruction::WriteLocal(1));
    summing
        .emit(Instruction::ReadLocal(0))
        .emit(Instruction::PushSmall(1))
        .emit(Instruction::Add);
    summing.emit(Instruction::WriteLocal(0)).loop_back(top);
    summing.place(done).emit(Instruction::ReadLocal(1));

    let mut unplaced = CodeWriter::new();
    let nowhere = unplaced.label();
    unplaced.jump(nowhere);

    assert_eq!(
        evaluate(Code {
            bytes: summing.finish().unwrap()
        }),
        Ok(10)
    );
    assert_eq!(unplaced.finish(), Err(CodeError::LabelNeverPlaced(nowhere)));
}

#[test]
fn event_fields_are_in_their_agreed_places() {
    assert_eq!(EventKind::for_property(Property::OnDrag), Some(EventKind::Drag));
    assert_eq!(EventKind::Drag.field_index("start_y"), Some(3));
    assert_eq!(EventKind::Click.field_index("y"), Some(1));
    assert_eq!(EventKind::Change.field_index("value"), Some(0));
    assert_eq!(EventKind::Change.field_index("x"), None);
    assert_eq!(EventKind::for_property(Property::Text), None);
}

// ============================================================================
// Theming
// ============================================================================

#[test]
fn a_theme_colour_is_looked_up_only_when_drawn() {
    let accent = ColorValue::Theme(ThemeColor::Accent);
    let mut theme = DEFAULT_THEME;
    theme.set_color(ThemeColor::Accent, 0xF800);

    assert_eq!(ColorValue::from_value(accent.to_value()), Some(accent));
    assert_eq!(
        accent.resolve(&DEFAULT_THEME),
        DEFAULT_THEME.color(ThemeColor::Accent)
    );
    assert_eq!(accent.resolve(&theme), 0xF800);
    assert_eq!(ColorValue::Fixed(0x07E0).resolve(&theme), 0x07E0);
    assert_eq!(ThemeColor::from_name("muted"), Some(ThemeColor::Muted));
}

#[test]
fn widgets_and_state_accept_theme_colours_but_not_other_numbers() {
    let accent = ColorValue::Theme(ThemeColor::Accent).to_value();
    let mut themed = ImageBuilder::new();
    themed.variable("highlight", 4, false, accent);
    themed.widget(
        WidgetKind::Label,
        0,
        &[(Property::Color, Constant(accent as i32))],
    );
    let mut not_a_colour = ImageBuilder::new();
    not_a_colour.widget(WidgetKind::Label, 0, &[(Property::Color, Constant(1 << 20))]);

    assert_eq!(widget_problem(&themed), None);
    assert_eq!(widget_problem(&not_a_colour), Some(WidgetProblem::BadConstant));
}

// ============================================================================
// Start of a minute, hour or day
// ============================================================================

fn start_of(unit: TimeUnit, system: &FakeSystem) -> Value {
    evaluate_with(
        Code::new().system(SystemValue::Now).op_byte(0x31, unit as u8),
        system,
    )
}

fn evaluate_with(code: Code, system: &FakeSystem) -> Value {
    let mut builder = ImageBuilder::new();
    builder.expression(code);
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();
    Interpreter::new()
        .evaluate_expression(&image, 0, &AppState::new(&image).unwrap(), system)
        .unwrap()
}

#[test]
fn cuts_a_time_back_to_the_start_of_its_minute_hour_and_day() {
    // 14:05:09 UTC is 19:35:09 in a time zone 5 and a half hours ahead.
    let system = FakeSystem {
        now: SUNDAY_AFTERNOON,
        utc_offset: 19_800_000,
        ..Default::default()
    };

    assert_eq!(start_of(TimeUnit::Minute, &system), SUNDAY_AFTERNOON - 9_000);
    assert_eq!(
        start_of(TimeUnit::Hour, &system),
        SUNDAY_AFTERNOON - (35 * 60_000 + 9_000)
    );
    assert_eq!(
        start_of(TimeUnit::Day, &system),
        SUNDAY_AFTERNOON - (19 * 3_600_000 + 35 * 60_000 + 9_000)
    );
}

#[test]
fn a_clock_cut_to_the_minute_redraws_once_a_minute() {
    let (bytes, _) = bindings_for(&[Code::new()
        .system(SystemValue::Now)
        .op_byte(0x31, TimeUnit::Minute as u8)]);
    let bindings = Bindings::load(&AppImage::parse(&bytes).unwrap()).unwrap();

    assert_eq!(bindings.info(0).unwrap().redraw_period_in_milliseconds, 60_000);
}

// ============================================================================
// Text from character codes
// ============================================================================

#[test]
fn shows_text_kept_as_character_codes() {
    let mut builder = ImageBuilder::new();
    let message = builder.array("message", 96, 4, 0);
    let length = builder.variable("length", INT_TYPE, false, 0);
    builder.expression(Code::new().state(length).op_byte(0x44, message));
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    for (index, character) in "SOS é".chars().enumerate() {
        state
            .set_element(message, index as Value, character as Value)
            .unwrap();
    }
    state.set(length, 5).unwrap();
    let mut interpreter = Interpreter::new();

    let value = interpreter
        .evaluate_expression(&image, 0, &state, &FakeSystem::default())
        .unwrap();

    assert_eq!(verify(&image), Ok(()));
    assert_eq!(interpreter.text(&image, value), Ok("SOS é"));
}

#[test]
fn only_the_open_screens_clocks_wake_the_watch() {
    let (bytes, _) = bindings_for(&[
        Code::new()
            .system(SystemValue::Now)
            .format_time(CLOCK_WITH_SECONDS),
        Code::new().system(SystemValue::Now).format_time(CLOCK),
    ]);
    let bindings = Bindings::load(&AppImage::parse(&bytes).unwrap()).unwrap();
    let start_of_minute = 1_791_727_500_000;

    assert_eq!(
        bindings.next_redraw_after_among(start_of_minute, 0b10),
        Some(start_of_minute + 60_000)
    );
    assert_eq!(bindings.next_redraw_after_among(start_of_minute, 0), None);
    assert_eq!(
        bindings
            .due_between_among(start_of_minute, start_of_minute + 1_000, 0b10)
            .count(),
        0
    );
}

// ============================================================================
// System text
// ============================================================================

#[test]
fn shows_and_joins_text_the_system_keeps() {
    let mut builder = ImageBuilder::new();
    let prefix = builder.string("Now playing: ");
    builder.expression(Code::new().text(prefix).system(SystemValue::MusicTrack).join());
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();
    let mut interpreter = Interpreter::new();

    let value = interpreter
        .evaluate_expression(&image, 0, &AppState::new(&image).unwrap(), &FakeSystem::default())
        .unwrap();

    assert_eq!(interpreter.text(&image, value), Ok("Now playing: Blue Monday"));
}

#[test]
fn every_system_value_has_a_bit_in_the_mask_and_related_values_share_one() {
    let values = (0..SystemValue::COUNT as u8).map(|byte| SystemValue::from_byte(byte).unwrap());

    for value in values {
        assert!(value.bit().trailing_zeros() < 32);
    }
    assert_eq!(SystemValue::MusicArtist.bit(), SystemValue::MusicLength.bit());
    assert_eq!(
        SystemValue::WeatherLocation.bit(),
        SystemValue::WeatherTemperature.bit()
    );
    assert_ne!(
        SystemValue::MusicTrack.bit(),
        SystemValue::NavigationInstruction.bit()
    );
    assert_ne!(SystemValue::MusicTrack.bit(), SystemValue::BatteryPercent.bit());
}

// ============================================================================
// Boards, cell grids and starting values
// ============================================================================

#[test]
fn an_array_can_start_with_different_values() {
    let mut writer = ImageWriter::new("puzzle", "Puzzle", AppKind::App, 1);
    let tiles = writer
        .array_with_values("tiles", ValueType::Int, false, 1, &[1, 2, 3, 0, -4])
        .unwrap();
    let bytes = writer.build();
    let image = AppImage::parse(&bytes).unwrap();
    let state = AppState::new(&image).unwrap();

    assert_eq!(verify(&image), Ok(()));
    assert_eq!(state.element(tiles, 1), Some(2));
    assert_eq!(state.element(tiles, 4), Some(-4));
    assert!(writer
        .array_with_values("too_wide", ValueType::Int, false, 1, &[300])
        .is_none());
}

#[test]
fn refuses_starting_values_that_do_not_fit_their_type() {
    let mut writer = ImageWriter::new("flags", "Flags", AppKind::App, 1);
    writer
        .array_with_values("flags", ValueType::Bool, false, 1, &[0, 1, 2])
        .unwrap();

    assert_eq!(
        AppImage::parse(&writer.build()).err(),
        Some(ImageError::BadInitialValues(0))
    );
}

#[test]
fn a_cell_grid_names_its_arrays_and_takes_touches() {
    let mut writer = ImageWriter::new("life", "Life", AppKind::App, 1);
    let cells = writer.array("cells", ValueType::Int, false, 256, 1, 0);
    let palette = writer
        .array_with_values("palette", ValueType::Color, false, 4, &[0, 0xFFFF])
        .unwrap();
    let toggle = writer.handler(Vec::new());
    writer.widget(WidgetKind::Column, 1, &[(Property::OnDrag, Handler(toggle))]);
    writer.widget(
        WidgetKind::CellGrid,
        0,
        &[
            (Property::Columns, Constant(16)),
            (Property::Rows, Constant(16)),
            (Property::Cells, Constant(cells as i32)),
            (Property::Palette, Constant(palette as i32)),
            (Property::OnClick, Handler(toggle)),
        ],
    );
    let bytes = writer.build();

    assert_eq!(verify(&AppImage::parse(&bytes).unwrap()), Ok(()));
    assert_eq!(EventKind::Click.field_index("value"), Some(2));
}

#[test]
fn an_array_property_must_name_an_array_directly() {
    let mut not_an_array = ImageWriter::new("grid", "Grid", AppKind::App, 1);
    let count = not_an_array.variable("count", ValueType::Int, false, 0);
    not_an_array.widget(
        WidgetKind::CellGrid,
        0,
        &[(Property::Cells, Constant(count as i32))],
    );
    let mut through_expression = ImageWriter::new("grid", "Grid", AppKind::App, 1);
    through_expression.array("cells", ValueType::Int, false, 4, 1, 0);
    let expression = through_expression.expression(Code::new().small(0).bytes);
    through_expression.widget(
        WidgetKind::CellGrid,
        0,
        &[(Property::Cells, Expression(expression))],
    );

    let problem = |writer: &ImageWriter| match verify(&AppImage::parse(&writer.build()).unwrap()) {
        Err(ImageError::BadWidget { problem, .. }) => Some(problem),
        _ => None,
    };
    assert_eq!(problem(&not_an_array), Some(WidgetProblem::BadConstant));
    assert_eq!(
        problem(&through_expression),
        Some(WidgetProblem::ArrayNotConstant)
    );
}

#[test]
fn arcs_and_click_areas_can_hold_widgets() {
    let mut writer = ImageWriter::new("timer", "Timer", AppKind::App, 1);
    let arc = writer.widget(WidgetKind::Arc, 0, &[]);
    writer.widget(WidgetKind::Label, 0, &[]);
    writer.set_child_count(arc, 1);
    let area = writer.widget(WidgetKind::ClickArea, 0, &[]);
    writer.widget(WidgetKind::Label, 0, &[]);
    writer.set_child_count(area, 1);

    assert_eq!(verify(&AppImage::parse(&writer.build()).unwrap()), Ok(()));
}

// ============================================================================
// Text functions
// ============================================================================

fn evaluate_text_with(builder: &mut ImageBuilder, code: Code) -> String {
    builder.expression(code);
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();
    let mut interpreter = Interpreter::new();
    let value = interpreter
        .evaluate_expression(&image, 0, &AppState::new(&image).unwrap(), &FakeSystem::default())
        .unwrap();
    interpreter.text(&image, value).unwrap().into()
}

#[test]
fn puts_text_made_while_running_in_capitals() {
    let mut builder = ImageBuilder::new();
    let pattern = builder.string("ddd");

    let text = evaluate_text_with(
        &mut builder,
        Code::new().long(SUNDAY_AFTERNOON).format_time(pattern).op(0x45),
    );

    assert_eq!(text, "SUN");
}

#[test]
fn counts_characters_not_bytes() {
    let mut builder = ImageBuilder::new();
    let word = builder.string("café");

    assert_eq!(
        evaluate_with_builder(&mut builder, Code::new().text(word).op(0x46)),
        4
    );
}

fn evaluate_with_builder(builder: &mut ImageBuilder, code: Code) -> Value {
    builder.expression(code);
    let bytes = builder.build();
    let image = AppImage::parse(&bytes).unwrap();
    Interpreter::new()
        .evaluate_expression(&image, 0, &AppState::new(&image).unwrap(), &FakeSystem::default())
        .unwrap()
}

#[test]
fn slices_text_as_javascript_does() {
    let mut builder = ImageBuilder::new();
    let word = builder.string("Saturday");
    let first_three = evaluate_text_with(&mut builder, Code::new().text(word).small(0).small(3).op(0x47));
    let mut builder = ImageBuilder::new();
    let word = builder.string("Saturday");
    let last_three = evaluate_text_with(&mut builder, Code::new().text(word).small(-3).small(100).op(0x47));

    assert_eq!(first_three, "Sat");
    assert_eq!(last_three, "day");
}

#[test]
fn pads_formatted_text_in_front() {
    let mut builder = ImageBuilder::new();

    let text = evaluate_text_with(
        &mut builder,
        Code::new()
            .small(7)
            .format_number(0)
            .small(3)
            .small(b' ' as i8)
            .op(0x48),
    );

    assert_eq!(text, "  7");
}

// ============================================================================
// Subscriptions
// ============================================================================

use crate::resources::{ImageFormat, ImageReference, Run};
use crate::subscriptions::{Subscription, SubscriptionKind, SubscriptionProblem, SubscriptionValue};

fn subscription(kind: SubscriptionKind, value: SubscriptionValue) -> Subscription {
    Subscription {
        kind,
        only_while_open: false,
        handler: 0,
        detail: 0,
        days: 0b111_1111,
        value,
    }
}

fn with_subscription(subscription: Subscription) -> (Vec<u8>, Result<(), ImageError>) {
    let mut writer = ImageWriter::new("timer", "Timer", AppKind::App, 1);
    writer.variable("alarm_at", ValueType::Instant, true, NONE);
    writer.handler(Vec::new());
    writer.subscription(subscription);
    let bytes = writer.build();
    let result = verify(&AppImage::parse(&bytes).unwrap());
    (bytes, result)
}

#[test]
fn subscriptions_read_back_as_written() {
    let written = Subscription {
        kind: SubscriptionKind::Daily,
        only_while_open: false,
        handler: 0,
        detail: 0,
        days: 0b001_1111,
        value: SubscriptionValue::Constant(7 * 60 + 30),
    };
    let (bytes, result) = with_subscription(written);
    let image = AppImage::parse(&bytes).unwrap();

    assert_eq!(result, Ok(()));
    assert_eq!(image.subscription(0), Some(written));
    assert_eq!(written.kind.event(), EventKind::Time);
}

#[test]
fn works_out_when_timers_and_alarms_are_next_due() {
    let (bytes, _) = with_subscription(subscription(
        SubscriptionKind::Every,
        SubscriptionValue::Constant(60_000),
    ));
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let every_minute = subscription(SubscriptionKind::Every, SubscriptionValue::Constant(60_000));
    let alarm = subscription(SubscriptionKind::At, SubscriptionValue::Variable(0));
    let next_hour = Subscription {
        detail: 1,
        ..subscription(SubscriptionKind::ClockChange, SubscriptionValue::Constant(0))
    };

    assert_eq!(
        every_minute.next_due(SUNDAY_AFTERNOON, &state, 0),
        Some(1_791_727_560_000)
    );
    assert_eq!(alarm.next_due(SUNDAY_AFTERNOON, &state, 0), None);
    state.set(0, 5_000).unwrap();
    assert_eq!(alarm.next_due(SUNDAY_AFTERNOON, &state, 0), Some(5_000));
    // 14:05 UTC is 19:35 in a zone 5 and a half hours ahead, so the next local hour is 20:00.
    assert_eq!(
        next_hour.next_due(SUNDAY_AFTERNOON, &state, 19_800_000),
        Some(SUNDAY_AFTERNOON + 25 * 60_000 - 9_000)
    );
}

#[test]
fn a_daily_alarm_skips_days_it_is_not_set_for() {
    let (bytes, _) = with_subscription(subscription(
        SubscriptionKind::Every,
        SubscriptionValue::Constant(60_000),
    ));
    let image = AppImage::parse(&bytes).unwrap();
    let state = AppState::new(&image).unwrap();
    // 07:00 on weekdays only; it is Sunday afternoon, so the next one is Monday morning.
    let weekdays_at_seven = Subscription {
        days: 0b001_1111,
        ..subscription(SubscriptionKind::Daily, SubscriptionValue::Constant(7 * 60))
    };
    let monday_seven = 1_791_788_400_000;

    assert_eq!(
        weekdays_at_seven.next_due(SUNDAY_AFTERNOON, &state, 0),
        Some(monday_seven)
    );
}

#[test]
fn refuses_subscriptions_that_would_keep_the_watch_busy_or_name_the_wrong_thing() {
    let fast_background = subscription(SubscriptionKind::Every, SubscriptionValue::Constant(1_000));
    let fast_while_open = Subscription {
        only_while_open: true,
        ..fast_background
    };
    let alarm_on_a_number = subscription(SubscriptionKind::Daily, SubscriptionValue::Variable(0));
    let no_days = Subscription {
        days: 0,
        ..subscription(SubscriptionKind::Daily, SubscriptionValue::Constant(60))
    };
    let missing_handler = Subscription {
        handler: 4,
        ..fast_while_open
    };

    let problem = |subscription| match with_subscription(subscription).1 {
        Err(ImageError::BadSubscription { problem, .. }) => Some(problem),
        _ => None,
    };
    assert_eq!(
        problem(fast_background),
        Some(SubscriptionProblem::MustBeOnlyWhileOpen)
    );
    assert_eq!(problem(fast_while_open), None);
    assert_eq!(problem(alarm_on_a_number), Some(SubscriptionProblem::BadVariable));
    assert_eq!(problem(no_days), Some(SubscriptionProblem::BadDays));
    assert_eq!(
        problem(missing_handler),
        Some(SubscriptionProblem::HandlerOutOfRange)
    );
}

#[test]
fn a_handler_reads_a_notifications_title() {
    let mut writer = ImageWriter::new("notes", "Notes", AppKind::App, 1);
    let title = writer.variable("title_length", ValueType::Int, false, 0);
    let mut code = CodeWriter::new();
    code.emit(Instruction::ReadEventText(2))
        .emit(Instruction::TextLength)
        .emit(Instruction::WriteState(title));
    let handler = writer.handler(code.finish().unwrap());
    let bytes = writer.build();
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();

    Interpreter::new()
        .run_handler_with_text(
            &image,
            handler,
            &mut state,
            &[7, 0, 0, 0, 0],
            &["", "Mail", "Lunch?", "", ""],
            &FakeSystem::default(),
        )
        .unwrap();

    assert_eq!(state.get(title), Some(6));
    assert!(EventKind::Notification.is_text_field(2));
    assert!(!EventKind::Notification.is_text_field(0));
}

// ============================================================================
// Saving state
// ============================================================================

#[test]
fn persistent_state_survives_a_save_and_load() {
    let mut writer = ImageWriter::new("steps", "Steps", AppKind::App, 1);
    let goal = writer.variable("goal", ValueType::Int, true, 8_000);
    let scratch = writer.variable("scratch", ValueType::Int, false, 0);
    let history = writer.array("history", ValueType::Int, true, 7, 2, 0);
    let bytes = writer.build();
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    state.set(goal, 12_000).unwrap();
    state.set(scratch, 99).unwrap();
    state.set_element(history, 6, -1_234).unwrap();
    let mut saved = [0u8; 256];

    let used = state.save_persistent(&image, &mut saved).unwrap();
    let mut reopened = AppState::new(&image).unwrap();
    let report = reopened.load_persistent(&image, &saved[..used]);

    assert_eq!(report.loaded, 2);
    assert_eq!(reopened.get(goal), Some(12_000));
    assert_eq!(reopened.get(scratch), Some(0));
    assert_eq!(reopened.element(history, 6), Some(-1_234));
}

#[test]
fn a_new_version_of_an_app_keeps_what_still_matches() {
    let mut old = ImageWriter::new("steps", "Steps", AppKind::App, 1);
    old.variable("goal", ValueType::Int, true, 8_000);
    old.array("history", ValueType::Int, true, 7, 2, 5);
    let old_bytes = old.build();
    let old_image = AppImage::parse(&old_bytes).unwrap();
    let mut old_state = AppState::new(&old_image).unwrap();
    old_state.set(0, 10_000).unwrap();
    let mut saved = [0u8; 256];
    let used = old_state.save_persistent(&old_image, &mut saved).unwrap();

    let mut new = ImageWriter::new("steps", "Steps", AppKind::App, 1);
    let history = new.array("history", ValueType::Int, true, 30, 2, 0);
    let goal = new.variable("goal", ValueType::Int, true, 8_000);
    let new_bytes = new.build();
    let new_image = AppImage::parse(&new_bytes).unwrap();
    let mut new_state = AppState::new(&new_image).unwrap();

    let report = new_state.load_persistent(&new_image, &saved[..used]);

    assert_eq!((report.loaded, report.skipped), (1, 1));
    assert_eq!(new_state.get(goal), Some(10_000));
    assert_eq!(new_state.element(history, 0), Some(0));
    assert!(new_state.load_persistent(&new_image, b"garbage").unreadable);
}

#[test]
fn saving_into_too_small_a_buffer_fails() {
    let mut writer = ImageWriter::new("steps", "Steps", AppKind::App, 1);
    writer.array("history", ValueType::Int, true, 100, 8, 0);
    let bytes = writer.build();
    let image = AppImage::parse(&bytes).unwrap();

    assert!(AppState::new(&image)
        .unwrap()
        .save_persistent(&image, &mut [0u8; 64])
        .is_err());
}

// ============================================================================
// Images
// ============================================================================

#[test]
fn an_image_is_stored_as_runs_and_read_back() {
    let mut writer = ImageWriter::new("logo", "Logo", AppKind::App, 1);
    let pixels = [0xF800, 0xF800, 0xF800, 0x001F, 0x001F, 0x07E0];
    let logo = writer.image_resource(ImageFormat::Color, 3, 2, &pixels);
    writer.widget(
        WidgetKind::Image,
        0,
        &[(Property::Image, Constant(ImageReference::App(logo).to_value()))],
    );
    let bytes = writer.build();
    let image = AppImage::parse(&bytes).unwrap();
    let resource = image.resource(logo).unwrap();

    let runs: Vec<Run> = resource.runs().collect();

    assert_eq!(verify(&image), Ok(()));
    assert_eq!((resource.width, resource.height), (3, 2));
    assert_eq!(
        runs,
        [
            Run {
                count: 3,
                value: 0xF800
            },
            Run {
                count: 2,
                value: 0x001F
            },
            Run {
                count: 1,
                value: 0x07E0
            }
        ]
    );
}

#[test]
fn refuses_an_image_whose_runs_do_not_fill_it_and_a_missing_image() {
    let mut short = ImageWriter::new("logo", "Logo", AppKind::App, 1);
    short.image_resource(ImageFormat::Mask, 4, 4, &[255, 255]);
    let mut missing = ImageWriter::new("logo", "Logo", AppKind::App, 1);
    missing.widget(
        WidgetKind::Image,
        0,
        &[(Property::Image, Constant(ImageReference::App(3).to_value()))],
    );
    let mut shared_icon = ImageWriter::new("logo", "Logo", AppKind::App, 1);
    shared_icon.widget(
        WidgetKind::Icon,
        0,
        &[(
            Property::Icon,
            Constant(ImageReference::SharedLibrary(42).to_value()),
        )],
    );

    assert_eq!(
        verify(&AppImage::parse(&short.build()).unwrap()),
        Err(ImageError::BadResource(0))
    );
    assert!(matches!(
        verify(&AppImage::parse(&missing.build()).unwrap()),
        Err(ImageError::BadWidget {
            problem: WidgetProblem::BadConstant,
            ..
        })
    ));
    assert_eq!(verify(&AppImage::parse(&shared_icon.build()).unwrap()), Ok(()));
}

#[test]
fn the_shared_library_is_an_image_of_its_own_kind() {
    let mut writer = ImageWriter::new("shared", "Shared library", AppKind::Library, 1);
    writer.image_resource(ImageFormat::Mask, 2, 1, &[0, 255]);
    let bytes = writer.build();
    let image = AppImage::parse(&bytes).unwrap();

    assert_eq!(image.kind, AppKind::Library);
    assert_eq!(image.resource_count(), 1);
}

#[test]
fn an_app_can_name_its_first_image_as_its_icon() {
    let mut with_icon = ImageWriter::new("dice", "Dice", AppKind::App, 1);
    with_icon.image_resource(ImageFormat::Mask, 1, 1, &[255]);
    with_icon.set_has_icon();
    let mut icon_missing = ImageWriter::new("dice", "Dice", AppKind::App, 1);
    icon_missing.set_has_icon();

    assert!(AppImage::parse(&with_icon.build()).unwrap().has_icon);
    assert_eq!(
        AppImage::parse(&icon_missing.build()).err(),
        Some(ImageError::BadResource(0))
    );
}

#[test]
fn a_derive_subscription_rebuilds_a_hidden_array_and_a_scale_takes_a_value() {
    let derive = subscription(SubscriptionKind::Derive, SubscriptionValue::Constant(0));
    let (bytes, result) = with_subscription(derive);
    let mut gauge = ImageWriter::new("gauge", "Gauge", AppKind::App, 1);
    gauge.widget(
        WidgetKind::Scale,
        0,
        &[
            (Property::Value, Constant(40)),
            (Property::MajorColor, Constant(0xFFFF)),
        ],
    );

    assert_eq!(result, Ok(()));
    assert_eq!(
        AppImage::parse(&bytes)
            .unwrap()
            .subscription(0)
            .unwrap()
            .kind
            .event(),
        EventKind::Derive
    );
    assert_eq!(verify(&AppImage::parse(&gauge.build()).unwrap()), Ok(()));
}

// ============================================================================
// Memory
// ============================================================================

#[test]
fn working_memory_stays_inside_the_budget() {
    let interpreter = core::mem::size_of::<Interpreter>();
    let handler_outcome = core::mem::size_of::<HandlerOutcome>();
    let bindings = core::mem::size_of::<Bindings>();
    let state = core::mem::size_of::<AppState>();
    std::println!(
        "interpreter {interpreter} B, handler outcome {handler_outcome} B, bindings {bindings} B, state {state} B"
    );

    assert!(interpreter <= 1_800);
    assert!(handler_outcome <= 300);
    assert!(bindings <= 2_100);
    assert!(state <= 2_600);
    assert!(interpreter + bindings + state <= 6_400);
}

#[test]
fn a_list_system_value_is_read_one_element_at_a_time() {
    let high = SystemValue::WeatherForecastHigh as u8;
    assert_eq!(evaluate(Code::new().small(1).op_byte(0x0F, high)), Ok(23));
    assert_eq!(evaluate(Code::new().small(5).op_byte(0x0F, high)), Ok(NONE));

    let mut whole_list = ImageBuilder::new();
    whole_list.expression(Code::new().op_byte(0x07, high));
    assert_eq!(
        verify_problem(&whole_list),
        Some(CodeProblem::WrongSystemValueKind)
    );

    let mut element_of_single = ImageBuilder::new();
    element_of_single.expression(
        Code::new()
            .small(0)
            .op_byte(0x0F, SystemValue::BatteryPercent as u8),
    );
    assert_eq!(
        verify_problem(&element_of_single),
        Some(CodeProblem::WrongSystemValueKind)
    );
}

#[test]
fn state_and_bindings_load_in_place_as_they_do_when_made() {
    let stopwatch = stopwatch();
    let image = AppImage::parse(&stopwatch.bytes).unwrap();

    let mut state = AppState::EMPTY;
    state.load_layout(&image).unwrap();
    let mut bindings = Bindings::EMPTY;
    bindings.load_into(&image).unwrap();

    let made = AppState::new(&image).unwrap();
    for variable in 0..image.state_count() as u8 {
        assert_eq!(state.placement(variable), made.placement(variable));
        assert_eq!(state.get(variable), made.get(variable));
    }
    let loaded = Bindings::load(&image).unwrap();
    for expression in 0..image.expression_count() as usize {
        assert_eq!(bindings.info(expression), loaded.info(expression));
    }
}

#[test]
fn a_library_carries_fonts_and_an_app_keeps_eight_sections() {
    use crate::writer::ImageWriter;

    let mut library = ImageWriter::new("shared", "Shared library", AppKind::Library, 1);
    library.font(b"NTFN first font");
    library.font(b"NTFN second font");
    let bytes = library.build();
    let image = AppImage::parse(&bytes).unwrap();
    assert_eq!(image.font_count(), 2);
    assert_eq!(image.font(1), Some(&b"NTFN second font"[..]));

    let app = ImageWriter::new("app", "App", AppKind::App, 1).build();
    let sections = u16::from_le_bytes([app[10], app[11]]);
    assert_eq!(sections, 8);
    assert_eq!(AppImage::parse(&app).unwrap().font_count(), 0);
}
