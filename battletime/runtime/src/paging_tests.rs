//! Tests for paged images: the resident image, and running its code through a `CodeCache`.

extern crate std;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::Cell;

use super::*;
use crate::instruction::{EffectKind, Instruction};
use crate::interpreter::Interpreter;
use crate::state::AppState;
use crate::writer::{CodeWriter, ImageWriter};
use crate::{AppKind, SystemValue, Value, ValueType};

// ============================================================================
// Helpers
// ============================================================================

const PATH: &str = "/apps/paged/app.ntapp";

/// A system whose only file is the whole image at `PATH`, counting the reads of it.
struct ImageFile {
    bytes: Vec<u8>,
    reads: Cell<u32>,
}

impl SystemValues for ImageFile {
    fn read(&self, _value: SystemValue) -> Value {
        0
    }

    fn random_number(&self) -> u32 {
        7
    }

    fn read_file(&self, path: &str, offset: u32, out: &mut [u8]) -> Option<usize> {
        if path != PATH {
            return None;
        }
        self.reads.set(self.reads.get() + 1);
        let rest = self.bytes.get(offset as usize..)?;
        let length = rest.len().min(out.len());
        out[..length].copy_from_slice(&rest[..length]);
        Some(length)
    }
}

fn code(instructions: &[Instruction]) -> Vec<u8> {
    let mut writer = CodeWriter::new();
    for instruction in instructions {
        writer.emit(*instruction);
    }
    writer.finish().unwrap()
}

/// An app whose handler adds `tens(units(3))` and then `tens(4)` to `total` and buzzes,
/// and whose expression reads `tens(total)`.
struct Sample {
    bytes: Vec<u8>,
    handler: u16,
    expression: u16,
}

fn sample() -> Sample {
    use Instruction::*;
    let mut writer = ImageWriter::new("paged", "Paged", AppKind::App, 1);
    let total = writer.variable("total", ValueType::Int, false, 1);
    let tens = writer.function(1, 1, 1, code(&[ReadLocal(0), PushSmall(10), Multiply]));
    let units = writer.function(1, 1, 1, code(&[ReadLocal(0), PushSmall(1), Add, Call(tens)]));
    let handler = writer.handler(code(&[
        ReadState(total),
        PushSmall(3),
        Call(units),
        Add,
        PushSmall(4),
        Call(tens),
        Add,
        WriteState(total),
        PushInt(200),
        Effect(EffectKind::Vibrate),
    ]));
    let expression = writer.expression(code(&[ReadState(total), Call(tens)]));
    Sample {
        bytes: writer.build(),
        handler,
        expression,
    }
}

fn whole_file(bytes: &[u8]) -> ImageFile {
    ImageFile {
        bytes: bytes.to_vec(),
        reads: Cell::new(0),
    }
}

/// Runs the sample's handler and gives back `total` and the effects' kinds.
fn run_handler(
    image: &AppImage,
    interpreter: &mut Interpreter,
    system: &ImageFile,
) -> (Value, Vec<EffectKind>) {
    let sample = sample();
    let mut state = AppState::new(image).unwrap();
    let outcome = interpreter
        .run_handler(image, sample.handler, &mut state, &[], system)
        .unwrap();
    let effects = outcome.effects().iter().map(|effect| effect.kind).collect();
    (state.get(0).unwrap(), effects)
}

fn paged_interpreter(image: &AppImage, capacity: usize) -> Interpreter {
    let mut interpreter = Interpreter::for_image(image);
    interpreter.set_code_cache(Some(Box::new(
        CodeCache::new(PATH, &sample().bytes, capacity).unwrap(),
    )));
    interpreter
}

// ============================================================================
// The resident image
// ============================================================================

#[test]
fn the_resident_image_keeps_all_but_the_code_and_says_where_the_code_lies() {
    let sample = sample();
    let whole = AppImage::parse(&sample.bytes).unwrap();
    let resident_bytes = resident_image(&sample.bytes).unwrap();
    let resident = AppImage::parse(&resident_bytes).unwrap();

    assert!(resident.is_paged());
    assert!(resident_bytes.len() <= super::resident_bytes(&whole));
    assert_eq!(
        resident_bytes.len(),
        sample.bytes.len() - whole.code_bytes() + 12 + 2 * 12
    );
    assert_eq!(resident.string_count(), whole.string_count());
    assert_eq!(
        resident.expression_code(sample.expression),
        whole.expression_code(sample.expression)
    );
    assert_eq!(resident.handler_code(sample.handler), None);
    let place = resident.handler_place(sample.handler).unwrap();
    let in_file = &sample.bytes[place.at as usize..(place.at + place.length) as usize];
    assert_eq!(Some(in_file), whole.handler_code(sample.handler));
    for function in 0..whole.function_count() {
        let place = resident.function_place(function).unwrap();
        let in_file = &sample.bytes[place.at as usize..(place.at + place.length) as usize];
        assert_eq!(in_file[3..], *whole.function(function).unwrap().code);
    }
    assert_eq!(whole.handler_place(0), None);
}

#[test]
fn a_paged_image_is_neither_verified_nor_paged_again() {
    let resident = resident_image(&sample().bytes).unwrap();
    let image = AppImage::parse(&resident).unwrap();

    assert_eq!(crate::verify(&image), Err(ImageError::Paged));
    assert!(matches!(resident_image(&resident), Err(ImageError::Paged)));
    assert!(!pages_code(&image));
}

/// An app of `handlers` handlers of `pushes` pushes each, on a screen of a column and ten labels.
fn app_with_code(handlers: usize, pushes: usize) -> Vec<u8> {
    use crate::widgets::WidgetKind;
    let mut writer = ImageWriter::new("big", "Big", AppKind::App, 1);
    for _ in 0..handlers {
        writer.handler(code(&alloc::vec![Instruction::PushInt(1); pushes]));
    }
    writer.widget(WidgetKind::Column, 10, &[]);
    for _ in 0..10 {
        writer.widget(WidgetKind::Label, 0, &[]);
    }
    writer.build()
}

#[test]
fn an_app_is_paged_only_when_that_needs_less_of_the_heap() {
    let small_bytes = sample().bytes;
    let small = AppImage::parse(&small_bytes).unwrap();
    assert!(!pages_code(&small));
    assert_eq!(cache_bytes(&small), LEAST_CACHE_BYTES);

    let many_handlers = app_with_code(20, 100);
    assert!(pages_code(&AppImage::parse(&many_handlers).unwrap()));

    // One long handler needs a cache as large as itself, so paging it gains nothing.
    let one_long = app_with_code(1, 400);
    let image = AppImage::parse(&one_long).unwrap();
    assert!(image.longest_code_bytes() > LEAST_CACHE_BYTES);
    assert_eq!(cache_bytes(&image), image.longest_code_bytes());
    assert!(!pages_code(&image));
}

// ============================================================================
// Running paged code
// ============================================================================

#[test]
fn a_paged_handler_and_the_functions_it_calls_run_as_the_whole_image_does() {
    let sample = sample();
    let whole = AppImage::parse(&sample.bytes).unwrap();
    let resident_bytes = resident_image(&sample.bytes).unwrap();
    let resident = AppImage::parse(&resident_bytes).unwrap();
    let file = whole_file(&sample.bytes);

    let expected = run_handler(&whole, &mut Interpreter::for_image(&whole), &file);
    let mut interpreter = paged_interpreter(&resident, LEAST_CACHE_BYTES);
    let paged = run_handler(&resident, &mut interpreter, &file);

    // 1 + (3 + 1) * 10 + 4 * 10
    assert_eq!(expected, (81, alloc::vec![EffectKind::Vibrate]));
    assert_eq!(paged, expected);
    // The handler and both functions, each read once with the file's CRC beside it.
    assert_eq!(interpreter.code_cache().unwrap().reads, 3);
    assert_eq!(file.reads.get(), 6);

    run_handler(&resident, &mut interpreter, &file);
    assert_eq!(file.reads.get(), 6);
}

#[test]
fn an_expression_held_in_ram_calls_a_paged_function() {
    let sample = sample();
    let resident_bytes = resident_image(&sample.bytes).unwrap();
    let resident = AppImage::parse(&resident_bytes).unwrap();
    let state = AppState::new(&resident).unwrap();
    let file = whole_file(&sample.bytes);

    let value = paged_interpreter(&resident, LEAST_CACHE_BYTES)
        .evaluate_expression(&resident, sample.expression, &state, &file)
        .unwrap();

    assert_eq!(value, 10);
}

#[test]
fn a_cache_that_holds_one_piece_reads_callers_again_and_gets_the_same_result() {
    let sample = sample();
    let resident_bytes = resident_image(&sample.bytes).unwrap();
    let resident = AppImage::parse(&resident_bytes).unwrap();
    let file = whole_file(&sample.bytes);
    let mut interpreter = paged_interpreter(&resident, resident.longest_code_bytes());

    let result = run_handler(&resident, &mut interpreter, &file);

    assert_eq!(result.0, 81);
    // The handler is read again after each call returns, as is anything else dropped meanwhile.
    let reads = interpreter.code_cache().unwrap().reads;
    assert!(reads >= 5);
    assert_eq!(file.reads.get(), 2 * reads);
}

#[test]
fn paged_code_that_cannot_be_read_fails_the_run() {
    use crate::interpreter::Failure;

    let sample = sample();
    let resident_bytes = resident_image(&sample.bytes).unwrap();
    let resident = AppImage::parse(&resident_bytes).unwrap();
    let mut state = AppState::new(&resident).unwrap();
    let handler_at = resident.handler_place(sample.handler).unwrap().at as usize;
    let cut_short = whole_file(&sample.bytes[..handler_at + 1]);

    let mut interpreter = paged_interpreter(&resident, LEAST_CACHE_BYTES);
    let missing = interpreter.run_handler(&resident, sample.handler, &mut state, &[], &cut_short);
    assert_eq!(missing.unwrap_err(), Failure::CodeUnreadable);

    // Another image put at the same path, here the same one with a changed CRC.
    let mut replaced = sample.bytes.clone();
    replaced[12] ^= 1;
    let mut interpreter = paged_interpreter(&resident, LEAST_CACHE_BYTES);
    let other = interpreter.run_handler(&resident, sample.handler, &mut state, &[], &whole_file(&replaced));
    assert_eq!(other.unwrap_err(), Failure::CodeUnreadable);

    let without_cache = Interpreter::for_image(&resident).run_handler(
        &resident,
        sample.handler,
        &mut state,
        &[],
        &whole_file(&sample.bytes),
    );
    assert_eq!(without_cache.unwrap_err(), Failure::NoSuchCode);
}
