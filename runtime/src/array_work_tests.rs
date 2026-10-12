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

const UP: i8 = 0;
const DOWN: i8 = 1;
const LEFT: i8 = 2;
const RIGHT: i8 = 3;
const ANY: i8 = 4;

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

/// A 2 by 2 grid of levels whose top row can merge and whose columns cannot move up.
const TILES: [i64; 4] = [
    1, 1, //
    2, 3, //
];

#[test]
fn slide_changes_says_whether_a_slide_moves_anything() {
    assert_eq!(answer(&TILES, Question::SlideChanges, &[2, LEFT]), Ok(1));
    assert_eq!(answer(&TILES, Question::SlideChanges, &[2, UP]), Ok(0));
    assert_eq!(answer(&TILES, Question::SlideChanges, &[2, ANY]), Ok(1));
    assert_eq!(answer(&[1, 2, 3, 1], Question::SlideChanges, &[2, ANY]), Ok(0));
}

#[test]
fn slide_points_score_each_merge_at_its_new_value() {
    assert_eq!(answer(&TILES, Question::SlidePoints, &[2, RIGHT]), Ok(4));
    assert_eq!(answer(&[1, 1, 1, 1], Question::SlidePoints, &[4, LEFT]), Ok(8));
    assert_eq!(
        answer(&TILES, Question::SlidePoints, &[2, ANY]),
        Err(Failure::IndexOutOfRange)
    );
}

// ============================================================================
// Work
// ============================================================================

#[test]
fn a_slide_merges_each_pair_once_towards_the_edge() {
    assert_eq!(
        worked(&[1, 1, 1, 1], Work::Slide, &[4, LEFT]),
        Ok(std::vec![2, 2, 0, 0])
    );
    assert_eq!(
        worked(&[1, 0, 1, 2], Work::Slide, &[4, RIGHT]),
        Ok(std::vec![0, 0, 2, 2])
    );
    assert_eq!(
        worked(&[2, 1, 1, 0], Work::Slide, &[4, LEFT]),
        Ok(std::vec![2, 2, 0, 0])
    );
}

#[test]
fn a_slide_moves_columns_up_and_down() {
    let board = [
        1, 0, //
        0, 2, //
        1, 2, //
    ];

    assert_eq!(
        worked(&board, Work::Slide, &[2, UP]),
        Ok(std::vec![2, 3, 0, 0, 0, 0])
    );
    assert_eq!(
        worked(&board, Work::Slide, &[2, DOWN]),
        Ok(std::vec![0, 0, 0, 0, 2, 3])
    );
}

#[test]
fn a_blinker_turns_on_a_wrapping_grid() {
    let mut board = [0; 25];
    board[11] = 1;
    board[12] = 1;
    board[13] = 1;
    let mut turned = std::vec![0; 25];
    turned[7] = 5;
    turned[12] = 5;
    turned[17] = 5;

    assert_eq!(worked(&board, Work::LifeStep, &[5, 5]), Ok(turned));
}

#[test]
fn life_wraps_neighbours_round_the_edges() {
    let mut board = [0; 16];
    // A blinker along the top row's wrapped edge: columns 3, 0 and 1.
    board[3] = 1;
    board[0] = 1;
    board[1] = 1;

    let next = worked(&board, Work::LifeStep, &[4, 1]).unwrap();

    assert_eq!(next.iter().filter(|&&cell| cell != 0).count(), 3);
    assert_eq!((next[0], next[4], next[12]), (1, 1, 1));
}

#[test]
fn life_refuses_a_grid_wider_than_its_row_buffers() {
    assert_eq!(
        worked(&[0; 34], Work::LifeStep, &[17, 1]),
        Err(Failure::IndexOutOfRange)
    );
}

#[test]
fn a_push_moves_the_history_along_in_place() {
    assert_eq!(worked(&[1, 2, 3], Work::PushLast, &[9]), Ok(std::vec![2, 3, 9]));
    assert_eq!(worked(&[1, 2, 3], Work::PushFirst, &[9]), Ok(std::vec![9, 1, 2]));
    assert_eq!(worked(&[1], Work::PushLast, &[9]), Ok(std::vec![9]));
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
        Instruction::PushSmall(LEFT),
        Instruction::WorkArray {
            work: Work::Slide,
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
        question: Question::SlidePoints,
        array: 3,
    };
    let work = Instruction::WorkArray {
        work: Work::LifeStep,
        destination: 4,
        source: 5,
    };

    assert_eq!(ask.encode().as_bytes(), &[0x64, 4, 3]);
    assert_eq!(work.encode().as_bytes(), &[0x65, 1, 4, 5]);
    assert_eq!(decode(ask.encode().as_bytes(), 0), Ok((ask, 3)));
    assert_eq!(decode(work.encode().as_bytes(), 0), Ok((work, 4)));
    assert!(decode(&[0x64, 9, 0], 0).is_err());
    assert!(decode(&[0x65, 9, 0, 0], 0).is_err());
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
