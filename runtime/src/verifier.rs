//! Checks every piece of code in an image before it is installed.
//!
//! Jumps only go forward, so one pass over the code sees every path. The pass follows the stack
//! depth, records the depth each jump expects at its target, and checks that every path agrees.
//! Code that passes cannot underflow or overflow the stack, read outside its state or strings,
//! or run longer than its own length.

use crate::image::{AppImage, CodeBlock, CodeProblem, ImageError};
use crate::instruction::{decode, Instruction};
use crate::interpreter::{MOST_EVENT_FIELDS, MOST_LOCALS, STACK_DEPTH};
use crate::pattern;
use crate::widgets::verify_widgets;

// ============================================================================
// Limits
// ============================================================================

/// How many forward jumps may be waiting for their target at once.
const MOST_OPEN_JUMPS: usize = 16;

/// How many loops one piece of code may have.
const MOST_LOOPS: usize = 16;

// ============================================================================
// Verifying an image
// ============================================================================

/// Checks every expression and handler in the image.
pub fn verify(image: &AppImage) -> Result<(), ImageError> {
    for index in 0..image.expression_count() {
        let code = image.expression_code(index).unwrap_or_default();
        verify_code(image, code, CodeBlock::Expression(index))?;
    }
    for index in 0..image.handler_count() {
        let code = image.handler_code(index).unwrap_or_default();
        verify_code(image, code, CodeBlock::Handler(index))?;
    }
    verify_widgets(image)
}

#[derive(Clone, Copy)]
struct OpenJump {
    target: usize,
    depth: usize,
}

fn verify_code(image: &AppImage, code: &[u8], block: CodeBlock) -> Result<(), ImageError> {
    let is_expression = matches!(block, CodeBlock::Expression(_));
    let mut open_jumps = [OpenJump { target: 0, depth: 0 }; MOST_OPEN_JUMPS];
    let mut open_jump_count = 0;
    let mut loop_starts = [0usize; MOST_LOOPS];
    let mut loop_start_count = 0;

    // None means no path reaches this point.
    let mut depth: Option<usize> = Some(0);
    let mut offset = 0;

    let fail = |offset: usize, problem: CodeProblem| ImageError::BadCode {
        block,
        offset,
        problem,
    };

    loop {
        // Join the paths that jump here.
        let mut index = 0;
        while index < open_jump_count {
            let jump = open_jumps[index];
            if jump.target == offset {
                match depth {
                    None => depth = Some(jump.depth),
                    Some(current) if current != jump.depth => {
                        return Err(fail(offset, CodeProblem::MismatchedStackAtJump));
                    }
                    Some(_) => {}
                }
                open_jump_count -= 1;
                open_jumps[index] = open_jumps[open_jump_count];
            } else {
                index += 1;
            }
        }

        if offset >= code.len() {
            break;
        }

        let (instruction, next_offset) =
            decode(code, offset).map_err(|_| fail(offset, CodeProblem::Undecodable))?;

        if open_jumps[..open_jump_count]
            .iter()
            .any(|jump| jump.target > offset && jump.target < next_offset)
        {
            return Err(fail(offset, CodeProblem::JumpIntoInstruction));
        }
        check_operands(image, instruction, is_expression).map_err(|problem| fail(offset, problem))?;

        match instruction {
            Instruction::LoopStart if depth.is_some() => {
                if depth != Some(0) {
                    return Err(fail(offset, CodeProblem::LoopWithStack));
                }
                if loop_start_count == MOST_LOOPS {
                    return Err(fail(offset, CodeProblem::TooManyLoops));
                }
                loop_starts[loop_start_count] = offset;
                loop_start_count += 1;
            }
            Instruction::LoopBack(distance) if depth.is_some() => {
                if depth != Some(0) {
                    return Err(fail(offset, CodeProblem::LoopWithStack));
                }
                let target = next_offset.checked_sub(distance as usize);
                if !target.is_some_and(|target| loop_starts[..loop_start_count].contains(&target)) {
                    return Err(fail(offset, CodeProblem::BadLoop));
                }
                depth = None;
            }
            _ => {}
        }

        if let Some(current) = depth {
            let effect = instruction.stack_effect();
            let after_taking = current
                .checked_sub(effect.taken)
                .ok_or(fail(offset, CodeProblem::StackUnderflow))?;
            let after = after_taking + effect.left;
            if after > STACK_DEPTH {
                return Err(fail(offset, CodeProblem::StackTooDeep));
            }
            depth = Some(after);

            let jump_distance = match instruction {
                Instruction::Jump(distance) | Instruction::JumpIfFalse(distance) => Some(distance),
                _ => None,
            };
            if let Some(distance) = jump_distance {
                let target = next_offset + distance as usize;
                if target > code.len() {
                    return Err(fail(offset, CodeProblem::JumpPastEnd));
                }
                if open_jump_count == MOST_OPEN_JUMPS {
                    return Err(fail(offset, CodeProblem::TooManyOpenJumps));
                }
                open_jumps[open_jump_count] = OpenJump { target, depth: after };
                open_jump_count += 1;
                if matches!(instruction, Instruction::Jump(_)) {
                    depth = None;
                }
            }
        }
        offset = next_offset;
    }

    let expected_results = if is_expression { 1 } else { 0 };
    match depth {
        Some(results) if results == expected_results => Ok(()),
        // No path reaches the end, so the call can only stop by failing, such as at the step limit.
        None => Ok(()),
        _ => Err(fail(code.len(), CodeProblem::WrongResultCount)),
    }
}

fn check_operands(
    image: &AppImage,
    instruction: Instruction,
    is_expression: bool,
) -> Result<(), CodeProblem> {
    let is_array = |variable: u8| {
        image
            .state_variable(variable as u16)
            .map(|variable| variable.is_array())
    };
    let array_length = |variable: u8| {
        image
            .state_variable(variable as u16)
            .filter(|variable| variable.is_array())
            .map(|variable| variable.length)
    };
    let writes_state = matches!(
        instruction,
        Instruction::WriteState(_)
            | Instruction::WriteElement(_)
            | Instruction::FillArray(_)
            | Instruction::CopyArray { .. }
    );

    match instruction {
        _ if writes_state && is_expression => Err(CodeProblem::WriteInExpression),
        Instruction::ReadState(variable) | Instruction::WriteState(variable) => match is_array(variable) {
            None => Err(CodeProblem::StateSlotOutOfRange),
            Some(true) => Err(CodeProblem::WrongVariableKind),
            Some(false) => Ok(()),
        },
        Instruction::ReadElement(variable)
        | Instruction::WriteElement(variable)
        | Instruction::FillArray(variable)
        | Instruction::TextFromCodes(variable) => match is_array(variable) {
            None => Err(CodeProblem::StateSlotOutOfRange),
            Some(false) => Err(CodeProblem::WrongVariableKind),
            Some(true) => Ok(()),
        },
        Instruction::CopyArray { destination, source } => {
            match (array_length(destination), array_length(source)) {
                (Some(destination_length), Some(source_length)) if destination_length == source_length => {
                    Ok(())
                }
                _ => Err(CodeProblem::WrongVariableKind),
            }
        }
        Instruction::ReadLocal(local) | Instruction::WriteLocal(local) if local as usize >= MOST_LOCALS => {
            Err(CodeProblem::LocalOutOfRange)
        }
        Instruction::PushText(index) if index >= image.string_count() => Err(CodeProblem::StringOutOfRange),
        Instruction::FormatTime(index) => match image.string(index) {
            None => Err(CodeProblem::StringOutOfRange),
            Some(pattern) if !pattern::is_valid_time_pattern(pattern) => Err(CodeProblem::BadPattern),
            Some(_) => Ok(()),
        },
        Instruction::FormatDuration(index) => match image.string(index) {
            None => Err(CodeProblem::StringOutOfRange),
            Some(pattern) if !pattern::is_valid_duration_pattern(pattern) => Err(CodeProblem::BadPattern),
            Some(_) => Ok(()),
        },
        Instruction::ReadEvent(_) if is_expression => Err(CodeProblem::EventInExpression),
        Instruction::ReadEvent(field) if field as usize >= MOST_EVENT_FIELDS => {
            Err(CodeProblem::EventFieldOutOfRange)
        }
        Instruction::Effect(_) if is_expression => Err(CodeProblem::EffectInExpression),
        Instruction::RandomInt if is_expression => Err(CodeProblem::RandomInExpression),
        _ => Ok(()),
    }
}
