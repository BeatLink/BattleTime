//! Tests for the array questions and work the interpreter does natively.

extern crate std;

use std::vec::Vec;

use super::*;
use crate::image::{CodeBlock, CodeProblem};
use crate::instruction::{decode, ArrayQuestion as Question, ArrayWork as Work, Instruction};
use crate::writer::{CodeWriter, ImageWriter};

// ============================================================================
// Helpers
// ============================================================================

struct System;

impl SystemValues for System {
    fn read(&self, _value: SystemValue) -> Value {
        0
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

fn pushes(values: &[i8]) -> Vec<Instruction> {
    values
        .iter()
        .map(|&value| Instruction::PushSmall(value))
        .collect()
}

/// An app with one array holding `values`, whose only expression pushes `arguments` and asks `question`.
fn answer(values: &[i64], question: Question, arguments: &[i8]) -> Result<Value, Failure> {
    let mut writer = ImageWriter::new("arrays", "Arrays", AppKind::App, 1);
    let array = writer
        .array_with_values("board", ValueType::Int, false, 1, values)
        .unwrap();
    let mut instructions = pushes(arguments);
    instructions.push(Instruction::AskArray { question, array });
    writer.expression(code(&instructions));
    let bytes = writer.build();
    let image = AppImage::parse(&bytes).unwrap();
    assert_eq!(verify(&image), Ok(()));
    let state = AppState::new(&image).unwrap();
    Interpreter::new().evaluate_expression(&image, 0, &state, &System)
}

/// The array after a handler pushes `arguments` and does `work` from the array into itself.
fn worked(values: &[i64], work: Work, arguments: &[i8]) -> Result<Vec<Value>, Failure> {
    let mut writer = ImageWriter::new("arrays", "Arrays", AppKind::App, 1);
    let array = writer
        .array_with_values("board", ValueType::Int, false, 1, values)
        .unwrap();
    let mut instructions = pushes(arguments);
    instructions.push(Instruction::WorkArray {
        work,
        destination: array,
        source: array,
    });
    writer.handler(code(&instructions));
    let bytes = writer.build();
    let image = AppImage::parse(&bytes).unwrap();
    assert_eq!(verify(&image), Ok(()));
    let mut state = AppState::new(&image).unwrap();
    Interpreter::new().run_handler(&image, 0, &mut state, &[], &System)?;
    Ok((0..values.len())
        .map(|index| state.element(array, index as Value).unwrap())
        .collect())
}

// ============================================================================
// Questions
// ============================================================================

#[test]
fn count_of_counts_the_equal_elements() {
    assert_eq!(answer(&[0, 2, 0, 1, 0], Question::CountOf, &[0]), Ok(3));
    assert_eq!(answer(&[0, 2, 0, 1, 0], Question::CountOf, &[5]), Ok(0));
}

#[test]
fn nth_index_of_finds_each_equal_element_in_turn() {
    let board = [1, 0, 1, 0, 0];

    assert_eq!(answer(&board, Question::NthIndexOf, &[0, 0]), Ok(1));
    assert_eq!(answer(&board, Question::NthIndexOf, &[0, 2]), Ok(4));
    assert_eq!(answer(&board, Question::NthIndexOf, &[0, 3]), Ok(-1));
    assert_eq!(answer(&board, Question::NthIndexOf, &[7, 0]), Ok(-1));
}

/// A 4 by 3 grid with a diagonal of 1s from the bottom left, a row of 2s, and a column of 2s.
const LINES: [i64; 12] = [
    2, 2, 2, 1, //
    2, 0, 1, 0, //
    2, 1, 0, 0, //
];

#[test]
fn line_through_follows_rows_columns_and_both_diagonals() {
    assert_eq!(answer(&LINES, Question::LineThrough, &[4, 1, 2]), Ok(3));
    assert_eq!(answer(&LINES, Question::LineThrough, &[4, 4, 2]), Ok(3));
    assert_eq!(answer(&LINES, Question::LineThrough, &[4, 6, 1]), Ok(3));
    // The empty cell counts as holding the value, so it completes the 2s on the diagonal through it.
    assert_eq!(answer(&LINES, Question::LineThrough, &[4, 5, 2]), Ok(3));
    assert_eq!(answer(&LINES, Question::LineThrough, &[4, 5, 1]), Ok(2));
    assert_eq!(answer(&LINES, Question::LineThrough, &[4, 10, 1]), Ok(2));
}

#[test]
fn lines_stop_at_the_edges_instead_of_wrapping() {
    let wraps_if_allowed = [0, 0, 0, 1, 1, 1, 0, 0];

    assert_eq!(
        answer(&wraps_if_allowed, Question::LineThrough, &[4, 4, 1]),
        Ok(2)
    );
}

#[test]
fn line_through_refuses_a_cell_off_the_grid_or_a_ragged_width() {
    assert_eq!(
        answer(&LINES, Question::LineThrough, &[4, 12, 1]),
        Err(Failure::IndexOutOfRange)
    );
    assert_eq!(
        answer(&LINES, Question::LineThrough, &[5, 0, 1]),
        Err(Failure::IndexOutOfRange)
    );
    assert_eq!(
        answer(&LINES, Question::LineThrough, &[0, 0, 1]),
        Err(Failure::IndexOutOfRange)
    );
}

// ============================================================================
// Work
// ============================================================================

#[test]
fn a_push_moves_the_history_along_in_place() {
    assert_eq!(worked(&[1, 2, 3], Work::PushLast, &[9]), Ok(std::vec![2, 3, 9]));
    assert_eq!(worked(&[1, 2, 3], Work::PushFirst, &[9]), Ok(std::vec![9, 1, 2]));
    assert_eq!(worked(&[1], Work::PushLast, &[9]), Ok(std::vec![9]));
}

#[test]
fn inversions_count_the_pairs_out_of_order() {
    assert_eq!(answer(&[1, 2, 3], Question::Inversions, &[]), Ok(0));
    assert_eq!(answer(&[3, 1, 2], Question::Inversions, &[]), Ok(2));
    assert_eq!(
        answer(&[0; 65], Question::Inversions, &[]),
        Err(Failure::IndexOutOfRange)
    );
}

#[test]
fn a_shuffle_keeps_the_elements_and_leaves_the_rest_in_place() {
    let shuffled = worked(&[1, 2, 3, 4, 5, 6], Work::Shuffle, &[1, 4]).unwrap();
    let mut middle = shuffled[1..5].to_vec();
    middle.sort();

    assert_eq!((shuffled[0], shuffled[5]), (1, 6));
    assert_eq!(middle, [2, 3, 4, 5]);
    assert_ne!(shuffled, [1, 2, 3, 4, 5, 6]);
    assert_eq!(
        worked(&[1, 2, 3], Work::Shuffle, &[2, 2]),
        Err(Failure::IndexOutOfRange)
    );
}

#[test]
fn equal_compares_two_arrays() {
    let mut writer = ImageWriter::new("arrays", "Arrays", AppKind::App, 1);
    let first = writer
        .array_with_values("first", ValueType::Int, false, 1, &[1, 2, 3])
        .unwrap();
    let same = writer
        .array_with_values("same", ValueType::Int, false, 1, &[1, 2, 3])
        .unwrap();
    let other = writer
        .array_with_values("other", ValueType::Int, false, 1, &[1, 2, 4])
        .unwrap();
    for second in [same, other] {
        writer.expression(code(&[Instruction::WorkArray {
            work: Work::Equal,
            destination: first,
            source: second,
        }]));
    }
    let bytes = writer.build();
    let image = AppImage::parse(&bytes).unwrap();
    let state = AppState::new(&image).unwrap();
    let mut interpreter = Interpreter::new();

    assert_eq!(verify(&image), Ok(()));
    assert_eq!(interpreter.evaluate_expression(&image, 0, &state, &System), Ok(1));
    assert_eq!(interpreter.evaluate_expression(&image, 1, &state, &System), Ok(0));
}

// ============================================================================
// Checks and encoding
// ============================================================================

#[test]
fn an_expression_may_compare_arrays_but_not_rewrite_them() {
    let mut writer = ImageWriter::new("arrays", "Arrays", AppKind::App, 1);
    let board = writer.array("board", ValueType::Int, false, 4, 1, 0);
    writer.expression(code(&[
        Instruction::PushSmall(2),
        Instruction::WorkArray {
            work: Work::PushLast,
            destination: board,
            source: board,
        },
        Instruction::PushSmall(0),
    ]));
    let bytes = writer.build();

    let error = verify(&AppImage::parse(&bytes).unwrap()).unwrap_err();

    assert!(matches!(
        error,
        ImageError::BadCode {
            block: CodeBlock::Expression(0),
            problem: CodeProblem::WriteInExpression,
            ..
        }
    ));
}

#[test]
fn array_work_names_arrays_of_one_length() {
    let mut writer = ImageWriter::new("arrays", "Arrays", AppKind::App, 1);
    let long = writer.array("long", ValueType::Int, false, 4, 1, 0);
    let short = writer.array("short", ValueType::Int, false, 3, 1, 0);
    let single = writer.variable("single", ValueType::Int, false, 0);
    writer.expression(code(&[Instruction::WorkArray {
        work: Work::Equal,
        destination: long,
        source: short,
    }]));
    writer.expression(code(&[
        Instruction::PushSmall(0),
        Instruction::AskArray {
            question: Question::CountOf,
            array: single,
        },
    ]));
    let bytes = writer.build();

    let error = verify(&AppImage::parse(&bytes).unwrap()).unwrap_err();

    assert!(matches!(
        error,
        ImageError::BadCode {
            problem: CodeProblem::WrongVariableKind,
            ..
        }
    ));
}

#[test]
fn the_array_instructions_encode_and_decode() {
    let ask = Instruction::AskArray {
        question: Question::Inversions,
        array: 3,
    };
    let work = Instruction::WorkArray {
        work: Work::PushLast,
        destination: 4,
        source: 5,
    };

    assert_eq!(ask.encode().as_bytes(), &[0x64, 5, 3]);
    assert_eq!(work.encode().as_bytes(), &[0x65, 3, 4, 5]);
    assert_eq!(decode(ask.encode().as_bytes(), 0), Ok((ask, 3)));
    assert_eq!(decode(work.encode().as_bytes(), 0), Ok((work, 4)));
    assert!(decode(&[0x64, 9, 0], 0).is_err());
    assert!(decode(&[0x65, 9, 0, 0], 0).is_err());
}

#[test]
fn the_unused_array_codes_do_not_decode() {
    for question in [3, 4] {
        assert!(decode(&[0x64, question, 0], 0).is_err());
    }
    for work in [1, 2] {
        assert!(decode(&[0x65, work, 0, 0], 0).is_err());
    }
}

#[test]
fn bindings_see_the_arrays_a_question_reads() {
    let mut writer = ImageWriter::new("arrays", "Arrays", AppKind::App, 1);
    writer.variable("first", ValueType::Int, false, 0);
    let board = writer.array("board", ValueType::Int, false, 4, 1, 0);
    let other = writer.array("other", ValueType::Int, false, 4, 1, 0);
    writer.expression(code(&[
        Instruction::PushSmall(0),
        Instruction::AskArray {
            question: Question::CountOf,
            array: board,
        },
        Instruction::WorkArray {
            work: Work::Equal,
            destination: board,
            source: other,
        },
        Instruction::Add,
    ]));
    let bytes = writer.build();
    let image = AppImage::parse(&bytes).unwrap();

    let info = analyse_expression(&image, image.expression_code(0).unwrap()).unwrap();

    assert_eq!(info.inputs.state_slots, (1 << board) | (1 << other));
}

// ============================================================================
// File bytes
// ============================================================================

/// A system holding one file, `/remote/buttons`.
struct SystemWithFile {
    bytes: Vec<u8>,
}

impl SystemValues for SystemWithFile {
    fn read(&self, _value: SystemValue) -> Value {
        0
    }

    fn random_number(&self) -> u32 {
        7
    }

    fn read_file(&self, path: &str, offset: u32, out: &mut [u8]) -> Option<usize> {
        if path != "/remote/buttons" {
            return None;
        }
        let rest = self.bytes.get(offset as usize..).unwrap_or(&[]);
        let length = rest.len().min(out.len());
        out[..length].copy_from_slice(&rest[..length]);
        Some(length)
    }
}

/// The array of `length` elements after a handler reads the file at `path` into it.
fn file_bytes(path: &str, length: u16, bytes: &[u8]) -> Result<Vec<Value>, Failure> {
    file_bytes_from(path, None, length, bytes)
}

/// The array of `length` elements after a handler reads the file at `path` from `offset` into it.
fn file_bytes_from(
    path: &str,
    offset: Option<i32>,
    length: u16,
    bytes: &[u8],
) -> Result<Vec<Value>, Failure> {
    let mut writer = ImageWriter::new("arrays", "Arrays", AppKind::App, 1);
    let array = writer.array("buttons", ValueType::Int, false, length, 2, 7);
    let path = writer.string(path);
    let mut instructions = std::vec![Instruction::PushText(path)];
    if let Some(offset) = offset {
        instructions.push(Instruction::PushInt(offset));
    }
    instructions.push(Instruction::WorkArray {
        work: match offset {
            Some(_) => Work::FileBytesFrom,
            None => Work::FileBytes,
        },
        destination: array,
        source: array,
    });
    writer.handler(code(&instructions));
    let bytes_of_image = writer.build();
    let image = AppImage::parse(&bytes_of_image).unwrap();
    assert_eq!(verify(&image), Ok(()));
    let mut state = AppState::new(&image).unwrap();
    let system = SystemWithFile {
        bytes: bytes.to_vec(),
    };
    Interpreter::new().run_handler(&image, 0, &mut state, &[], &system)?;
    Ok((0..length)
        .map(|index| state.element(array, index as Value).unwrap())
        .collect())
}

#[test]
fn file_bytes_reads_the_file_and_fills_past_its_end_with_zeros() {
    assert_eq!(
        file_bytes("/remote/buttons", 6, &[1, 3, 255, 0, 9]),
        Ok(std::vec![1, 3, 255, 0, 9, 0])
    );
}

#[test]
fn file_bytes_reads_a_long_array_in_several_pieces() {
    let bytes: Vec<u8> = (0..100).collect();

    let read = file_bytes("/remote/buttons", 70, &bytes).unwrap();

    assert_eq!(read, (0..70).collect::<Vec<Value>>());
}

#[test]
fn file_bytes_gives_zeros_for_a_missing_file() {
    assert_eq!(
        file_bytes("/remote/missing", 3, &[1, 2, 3]),
        Ok(std::vec![0, 0, 0])
    );
}

#[test]
fn file_bytes_needs_a_path_from_the_image() {
    let mut writer = ImageWriter::new("arrays", "Arrays", AppKind::App, 1);
    let array = writer.array("buttons", ValueType::Int, false, 2, 2, 0);
    writer.handler(code(&[
        Instruction::PushSmall(4),
        Instruction::FormatNumber(0),
        Instruction::WorkArray {
            work: Work::FileBytes,
            destination: array,
            source: array,
        },
    ]));
    let bytes = writer.build();
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let system = SystemWithFile { bytes: std::vec![1] };

    let result = Interpreter::new().run_handler(&image, 0, &mut state, &[], &system);

    assert_eq!(result.err(), Some(Failure::BadText));
}

#[test]
fn an_expression_may_not_read_file_bytes() {
    let mut writer = ImageWriter::new("arrays", "Arrays", AppKind::App, 1);
    let array = writer.array("buttons", ValueType::Int, false, 2, 2, 0);
    let path = writer.string("/remote/buttons");
    writer.expression(code(&[
        Instruction::PushText(path),
        Instruction::WorkArray {
            work: Work::FileBytes,
            destination: array,
            source: array,
        },
        Instruction::PushSmall(0),
    ]));
    let bytes = writer.build();

    let error = verify(&AppImage::parse(&bytes).unwrap()).unwrap_err();

    assert!(matches!(
        error,
        ImageError::BadCode {
            problem: CodeProblem::WriteInExpression,
            ..
        }
    ));
}

#[test]
fn file_bytes_encodes_and_decodes() {
    let work = Instruction::WorkArray {
        work: Work::FileBytes,
        destination: 2,
        source: 2,
    };

    assert_eq!(work.encode().as_bytes(), &[0x65, 6, 2, 2]);
    assert_eq!(decode(work.encode().as_bytes(), 0), Ok((work, 4)));
}

#[test]
fn file_bytes_from_an_offset_pages_through_the_file() {
    let bytes: Vec<u8> = (0..100).collect();

    let read = file_bytes_from("/remote/buttons", Some(60), 50, &bytes).unwrap();

    let mut expected: Vec<Value> = (60..100).collect();
    expected.resize(50, 0);
    assert_eq!(read, expected);
}

#[test]
fn file_bytes_from_a_negative_offset_fails() {
    assert_eq!(
        file_bytes_from("/remote/buttons", Some(-1), 2, &[1, 2]),
        Err(Failure::IndexOutOfRange)
    );
}

#[test]
fn file_bytes_from_encodes_and_decodes() {
    let work = Instruction::WorkArray {
        work: Work::FileBytesFrom,
        destination: 2,
        source: 2,
    };

    assert_eq!(work.encode().as_bytes(), &[0x65, 7, 2, 2]);
    assert_eq!(decode(work.encode().as_bytes(), 0), Ok((work, 4)));
    assert_eq!(work.stack_effect().taken, 2);
}
