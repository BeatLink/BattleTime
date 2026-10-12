//! Checks every piece of code in an image before it is installed.
//!
//! Jumps only go forward, so one pass over the code sees every path. The pass follows the stack
//! depth, records the depth each jump expects at its target, and checks that every path agrees.
//! Code that passes cannot underflow or overflow the stack, read outside its state or strings,
//! or run longer than its own length.
//!
//! Functions are checked first, in order. A function may only call functions numbered below it,
//! so no call can recurse, and each call's depth and whether it acts like a handler are known
//! before anything calls it.

use crate::image::{AppImage, CodeBlock, CodeProblem, ImageError, MOST_FUNCTIONS};
use crate::instruction::{decode, Instruction, StackEffect};
use crate::interpreter::{LOCAL_POOL, MOST_CALL_DEPTH, MOST_EVENT_FIELDS, MOST_LOCALS, STACK_DEPTH};
use crate::pattern;
use crate::resources::verify_resources;
use crate::subscriptions::verify_subscriptions;
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

/// Checks every function, expression and handler in the image.
pub fn verify(image: &AppImage) -> Result<(), ImageError> {
    let mut functions = CheckedFunctions::NONE;
    for index in 0..image.function_count() {
        let block = CodeBlock::Function(index);
        let bad_header = ImageError::BadCode {
            block,
            offset: 0,
            problem: CodeProblem::BadFunctionHeader,
        };
        let function = image.function(index).ok_or(bad_header)?;
        if function.result_count > 1
            || function.argument_count > function.local_count
            || function.local_count as usize > MOST_LOCALS
        {
            return Err(bad_header);
        }
        let unit = CodeUnit {
            block,
            expected_results: function.result_count as usize,
            local_count: function.local_count,
            callable_functions: index,
        };
        let summary = verify_code(image, function.code, unit, &functions)?;
        let locals_needed = function.local_count as usize + summary.callee_locals as usize;
        if summary.call_depth as usize >= MOST_CALL_DEPTH || locals_needed > LOCAL_POOL - MOST_LOCALS {
            return Err(ImageError::BadCode {
                block,
                offset: 0,
                problem: CodeProblem::CallsTooDeep,
            });
        }
        functions.record(index, summary, locals_needed as u8);
    }

    for index in 0..image.expression_count() {
        let code = image.expression_code(index).unwrap_or_default();
        let unit = CodeUnit::top_level(image, CodeBlock::Expression(index));
        verify_code(image, code, unit, &functions)?;
    }
    for index in 0..image.handler_count() {
        let code = image.handler_code(index).unwrap_or_default();
        let unit = CodeUnit::top_level(image, CodeBlock::Handler(index));
        verify_code(image, code, unit, &functions)?;
    }
    verify_widgets(image)?;
    verify_subscriptions(image)?;
    verify_resources(image)
}

/// Which piece of code is being checked and what it must end with.
#[derive(Clone, Copy)]
struct CodeUnit {
    block: CodeBlock,
    expected_results: usize,
    local_count: u8,
    /// Calls may name only functions numbered below this.
    callable_functions: u16,
}

impl CodeUnit {
    fn top_level(image: &AppImage, block: CodeBlock) -> CodeUnit {
        CodeUnit {
            block,
            expected_results: matches!(block, CodeBlock::Expression(_)) as usize,
            local_count: MOST_LOCALS as u8,
            callable_functions: image.function_count(),
        }
    }

    fn is_expression(&self) -> bool {
        matches!(self.block, CodeBlock::Expression(_))
    }
}

/// What checking one piece of code found out about it.
#[derive(Clone, Copy, Default)]
struct CodeSummary {
    /// It writes state, reads the event, causes effects or draws random numbers, itself or through calls.
    acts_like_handler: bool,
    /// How many calls deep it goes: 0 when it calls nothing.
    call_depth: u8,
    /// The most locals the functions it calls take at once, with everything they call in turn.
    callee_locals: u8,
    /// The deepest the stack goes, counted from where this code starts, inside calls too.
    stack_needed: u8,
}

/// What the functions checked so far do, by number.
struct CheckedFunctions {
    acts_like_handler: [u64; MOST_FUNCTIONS / 64],
    call_depth: [u8; MOST_FUNCTIONS],
    locals_needed: [u8; MOST_FUNCTIONS],
    stack_needed: [u8; MOST_FUNCTIONS],
}

impl CheckedFunctions {
    const NONE: CheckedFunctions = CheckedFunctions {
        acts_like_handler: [0; MOST_FUNCTIONS / 64],
        call_depth: [0; MOST_FUNCTIONS],
        locals_needed: [0; MOST_FUNCTIONS],
        stack_needed: [0; MOST_FUNCTIONS],
    };

    fn record(&mut self, index: u16, summary: CodeSummary, locals_needed: u8) {
        self.locals_needed[index as usize] = locals_needed;
        self.stack_needed[index as usize] = summary.stack_needed;
        if summary.acts_like_handler {
            self.acts_like_handler[index as usize / 64] |= 1 << (index % 64);
        }
        self.call_depth[index as usize] = summary.call_depth;
    }

    fn acts_like_handler(&self, index: u8) -> bool {
        self.acts_like_handler[index as usize / 64] & (1 << (index % 64)) != 0
    }
}

#[derive(Clone, Copy)]
struct OpenJump {
    target: usize,
    depth: usize,
}

fn verify_code(
    image: &AppImage,
    code: &[u8],
    unit: CodeUnit,
    functions: &CheckedFunctions,
) -> Result<CodeSummary, ImageError> {
    let block = unit.block;
    let is_expression = unit.is_expression();
    let mut summary = CodeSummary::default();
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
        if let Instruction::ReadLocal(local) | Instruction::WriteLocal(local) = instruction {
            if local >= unit.local_count {
                return Err(fail(offset, CodeProblem::LocalOutOfRange));
            }
        }
        summary.acts_like_handler |= is_handler_only(instruction);
        let mut stack_effect = instruction.stack_effect();
        if let Instruction::Call(callee) = instruction {
            let function = image
                .function(callee as u16)
                .filter(|_| (callee as u16) < unit.callable_functions)
                .ok_or(fail(offset, CodeProblem::BadCall))?;
            let callee_acts_like_handler = functions.acts_like_handler(callee);
            if is_expression && callee_acts_like_handler {
                return Err(fail(offset, CodeProblem::HandlerFunctionInExpression));
            }
            summary.acts_like_handler |= callee_acts_like_handler;
            summary.call_depth = summary.call_depth.max(functions.call_depth[callee as usize] + 1);
            summary.callee_locals = summary
                .callee_locals
                .max(functions.locals_needed[callee as usize]);
            stack_effect = StackEffect {
                taken: function.argument_count as usize,
                left: function.result_count as usize,
            };
            if let Some(current) = depth {
                let under_call = current.saturating_sub(stack_effect.taken);
                let deepest_in_call = under_call + functions.stack_needed[callee as usize] as usize;
                if deepest_in_call > STACK_DEPTH {
                    return Err(fail(offset, CodeProblem::StackTooDeep));
                }
                summary.stack_needed = summary.stack_needed.max(deepest_in_call as u8);
            }
        }

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
            let effect = stack_effect;
            let after_taking = current
                .checked_sub(effect.taken)
                .ok_or(fail(offset, CodeProblem::StackUnderflow))?;
            let after = after_taking + effect.left;
            if after > STACK_DEPTH {
                return Err(fail(offset, CodeProblem::StackTooDeep));
            }
            summary.stack_needed = summary.stack_needed.max(after as u8);
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

    match depth {
        Some(results) if results == unit.expected_results => Ok(summary),
        // No path reaches the end, so the call can only stop by failing, such as at the step limit.
        None => Ok(summary),
        _ => Err(fail(code.len(), CodeProblem::WrongResultCount)),
    }
}

/// Whether an instruction may only run in a handler.
fn is_handler_only(instruction: Instruction) -> bool {
    matches!(
        instruction,
        Instruction::WriteState(_)
            | Instruction::WriteElement(_)
            | Instruction::FillArray(_)
            | Instruction::CopyArray { .. }
            | Instruction::ReadEvent(_)
            | Instruction::ReadEventText(_)
            | Instruction::Effect(_)
            | Instruction::RandomInt
    )
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
        Instruction::ReadEvent(_) | Instruction::ReadEventText(_) if is_expression => {
            Err(CodeProblem::EventInExpression)
        }
        Instruction::ReadEventText(field) if field as usize >= MOST_EVENT_FIELDS => {
            Err(CodeProblem::EventFieldOutOfRange)
        }
        Instruction::ReadEvent(field) if field as usize >= MOST_EVENT_FIELDS => {
            Err(CodeProblem::EventFieldOutOfRange)
        }
        Instruction::ReadSystem(value) if value.is_list() => Err(CodeProblem::WrongSystemValueKind),
        Instruction::ReadSystemElement(value) if !value.is_list() => Err(CodeProblem::WrongSystemValueKind),
        Instruction::Effect(_) if is_expression => Err(CodeProblem::EffectInExpression),
        Instruction::RandomInt if is_expression => Err(CodeProblem::RandomInExpression),
        _ => Ok(()),
    }
}
