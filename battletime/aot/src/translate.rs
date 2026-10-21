//! Translating one code unit's bytecode into a native function.
//!
//! The translation first decodes the unit and works out the stack's depth before every instruction,
//! as the verifier does, so each stack slot is a fixed place above the unit's part of the stack. It
//! then cuts the code into blocks that run straight through, each counting its instructions against
//! the step limit as it starts, and writes each instruction it can do natively. Every other way out
//! of a block goes through a small stub that hands the interpreter the instruction's offset, the
//! stack's depth and the steps the block counted but did not run.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use battletime_runtime::instruction::{decode, Instruction};
use battletime_runtime::interpreter::STACK_DEPTH;
use battletime_runtime::text::TextReference;
use battletime_runtime::NONE;

use crate::thumb::{Assembler, Condition, DataOperation, Label, Register, R0, R1, R2, R3, R4, R5, R6, R7};
use crate::{Exit, DEPTH_AT, LOCALS_AT, RESUME_OFFSET_AT, STACK_AT, STACK_ROOM_AT, STEPS_LEFT_AT};

// ============================================================================
// Why a unit was not translated
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Untranslatable {
    BadInstruction,
    BadJump,
    /// Two paths reach an instruction with different stack depths, or the stack goes below empty or above full.
    BadStack,
    TooLarge,
}

// ============================================================================
// Registers and sizes
// ============================================================================

const CONTEXT: Register = R4;
const LOCALS: Register = R5;
const STACK: Register = R6;
const SCRATCH: Register = R7;

const VALUE_BYTES: u32 = 8;

/// A block counts its steps with one 8-bit compare.
const MOST_BLOCK_STEPS: usize = 255;

// ============================================================================
// Decoding and stack depths
// ============================================================================

struct Decoded {
    offset: usize,
    instruction: Instruction,
    /// The stack's depth before the instruction, or none where no path reaches it.
    depth: Option<usize>,
    /// Where a jump goes.
    target: Option<usize>,
}

/// Every instruction with its depth, and the depth at the end of the code.
fn decode_unit(code: &[u8]) -> Result<(Vec<Decoded>, Option<usize>), Untranslatable> {
    let mut decoded = Vec::new();
    let mut offset = 0;
    while offset < code.len() {
        let (instruction, next) = decode(code, offset).map_err(|_| Untranslatable::BadInstruction)?;
        let target = match instruction {
            Instruction::Jump(distance) | Instruction::JumpIfFalse(distance) => {
                Some(next + distance as usize)
            }
            Instruction::LoopBack(distance) => Some(
                next.checked_sub(distance as usize)
                    .ok_or(Untranslatable::BadJump)?,
            ),
            _ => None,
        };
        decoded.push(Decoded {
            offset,
            instruction,
            depth: None,
            target,
        });
        offset = next;
    }

    let starts = |at: usize| at == code.len() || decoded.iter().any(|entry| entry.offset == at);
    if decoded
        .iter()
        .filter_map(|entry| entry.target)
        .any(|target| !starts(target))
    {
        return Err(Untranslatable::BadJump);
    }

    let mut depth_at_target: BTreeMap<usize, usize> = BTreeMap::new();
    let mut depth = Some(0);
    for index in 0..decoded.len() {
        let offset = decoded[index].offset;
        depth = merge(depth, depth_at_target.get(&offset).copied())?;
        decoded[index].depth = depth;
        let Some(before) = depth else {
            continue;
        };
        let instruction = decoded[index].instruction;
        let effect = instruction.stack_effect();
        let after = before.checked_sub(effect.taken).ok_or(Untranslatable::BadStack)? + effect.left;
        if after > STACK_DEPTH {
            return Err(Untranslatable::BadStack);
        }
        depth = Some(after);
        match instruction {
            Instruction::LoopStart if before != 0 => return Err(Untranslatable::BadStack),
            Instruction::Jump(_) | Instruction::JumpIfFalse(_) => {
                let target = decoded[index].target.unwrap_or_default();
                if target <= offset {
                    return Err(Untranslatable::BadJump);
                }
                let known = depth_at_target.get(&target).copied();
                depth_at_target.insert(target, merge(Some(after), known)?.unwrap_or_default());
                if matches!(instruction, Instruction::Jump(_)) {
                    depth = None;
                }
            }
            Instruction::LoopBack(_) => {
                let target = decoded[index].target.unwrap_or_default();
                let loop_start = decoded.iter().find(|entry| entry.offset == target);
                if after != 0 || !loop_start.is_some_and(|entry| entry.instruction == Instruction::LoopStart)
                {
                    return Err(Untranslatable::BadJump);
                }
                depth = None;
            }
            _ => {}
        }
    }
    let end_depth = merge(depth, depth_at_target.get(&code.len()).copied())?;
    Ok((decoded, end_depth))
}

fn merge(current: Option<usize>, at_target: Option<usize>) -> Result<Option<usize>, Untranslatable> {
    match (current, at_target) {
        (Some(current), Some(at_target)) if current != at_target => Err(Untranslatable::BadStack),
        (Some(current), _) => Ok(Some(current)),
        (None, at_target) => Ok(at_target),
    }
}

/// Whether native code does the instruction itself, rather than handing it to the interpreter.
fn is_native(instruction: Instruction, local_count: usize) -> bool {
    use Instruction::*;
    match instruction {
        ReadLocal(local) | WriteLocal(local) => (local as usize) < local_count,
        PushSmall(_) | PushInt(_) | PushLong(_) | PushNone | PushText(_) | Duplicate | Drop | Swap | Add
        | Subtract | Negate | Minimum | Maximum | Absolute | BitAnd | BitOr | BitXor | BitNot | Equal
        | NotEqual | Less | LessOrEqual | Greater | GreaterOrEqual | Not | IsNone | Jump(_)
        | JumpIfFalse(_) | LoopStart | LoopBack(_) => true,
        _ => false,
    }
}

/// Whether a block must start at the instruction after this one.
fn ends_block(instruction: Instruction, local_count: usize) -> bool {
    matches!(
        instruction,
        Instruction::Jump(_) | Instruction::JumpIfFalse(_) | Instruction::LoopBack(_)
    ) || !is_native(instruction, local_count)
}

// ============================================================================
// Handing over to the interpreter
// ============================================================================

/// One way to hand over: where the interpreter carries on, the depth there, and the steps to give back.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Handover {
    offset: usize,
    depth: usize,
    refund: usize,
}

struct Writer {
    assembler: Assembler,
    handovers: BTreeMap<Handover, Label>,
    labels: BTreeMap<usize, Label>,
}

impl Writer {
    fn handover(&mut self, handover: Handover) -> Label {
        if let Some(label) = self.handovers.get(&handover) {
            return *label;
        }
        let label = self.assembler.label();
        self.handovers.insert(handover, label);
        label
    }

    fn label_at(&mut self, offset: usize) -> Label {
        if let Some(label) = self.labels.get(&offset) {
            return *label;
        }
        let label = self.assembler.label();
        self.labels.insert(offset, label);
        label
    }
}

fn slot(index: usize) -> u32 {
    index as u32 * VALUE_BYTES
}

// ============================================================================
// Translating
// ============================================================================

/// The native function for a code unit with `local_count` locals, or why it has none.
///
/// The function takes a `Context` in r0 and gives back an `Exit` in r0. The code is an app's, never
/// the shared library's, whose text instructions copy their text out.
pub fn translate(code: &[u8], local_count: usize) -> Result<Vec<u8>, Untranslatable> {
    let (decoded, end_depth) = decode_unit(code)?;
    let highest_depth = decoded
        .iter()
        .filter_map(|entry| {
            let effect = entry.instruction.stack_effect();
            let before = entry.depth?;
            Some(before.max(before - effect.taken + effect.left))
        })
        .max()
        .unwrap_or(0);

    let targeted: Vec<bool> = decoded
        .iter()
        .map(|entry| decoded.iter().any(|other| other.target == Some(entry.offset)))
        .collect();
    let mut block_starts = vec![false; decoded.len()];
    for (index, entry) in decoded.iter().enumerate() {
        let after_end = index > 0 && ends_block(decoded[index - 1].instruction, local_count);
        block_starts[index] =
            index == 0 || targeted[index] || after_end || entry.instruction == Instruction::LoopStart;
    }
    let mut run = 0;
    for start in block_starts.iter_mut() {
        if *start || run == MOST_BLOCK_STEPS {
            *start = true;
            run = 0;
        }
        run += 1;
    }

    let mut writer = Writer {
        assembler: Assembler::new(),
        handovers: BTreeMap::new(),
        labels: BTreeMap::new(),
    };
    let exit = writer.assembler.label();
    let finish = writer.label_at(code.len());

    // The stack pushed is a multiple of 8 bytes, as calls out to Rust will need.
    writer.assembler.push(&[R3, R4, R5, R6, R7, crate::thumb::LINK]);
    writer.assembler.move_register(CONTEXT, R0);
    writer.assembler.load_word(LOCALS, CONTEXT, LOCALS_AT);
    writer.assembler.load_word(STACK, CONTEXT, STACK_AT);
    if highest_depth > 0 {
        let no_room = writer.handover(Handover {
            offset: 0,
            depth: 0,
            refund: 0,
        });
        writer.assembler.load_word(R0, CONTEXT, STACK_ROOM_AT);
        writer.assembler.compare_small(R0, highest_depth as u32);
        writer.assembler.branch_if(Condition::CarryClear, no_room);
    }

    let mut block_length = 0;
    let mut place_in_block = 0;
    // Code after a handover or an unconditional jump runs natively only when something jumps to it.
    let mut reachable = true;
    for (index, entry) in decoded.iter().enumerate() {
        reachable |= targeted[index];
        let Some(depth) = entry.depth.filter(|_| reachable) else {
            continue;
        };
        if block_starts[index] {
            let label = writer.label_at(entry.offset);
            writer.assembler.place(label);
            block_length = (index..decoded.len())
                .take_while(|&later| later == index || !block_starts[later])
                .count();
            place_in_block = 0;
            count_steps(&mut writer, entry.offset, depth, block_length);
        }
        let handover = Handover {
            offset: entry.offset,
            depth,
            refund: block_length - place_in_block,
        };
        write_instruction(&mut writer, entry, depth, handover, local_count);
        place_in_block += 1;
        reachable = !matches!(entry.instruction, Instruction::Jump(_) | Instruction::LoopBack(_))
            && is_native(entry.instruction, local_count);
    }

    writer.assembler.place(finish);
    writer.assembler.move_small(R0, end_depth.unwrap_or(0) as u32);
    writer.assembler.store_word(R0, CONTEXT, DEPTH_AT);
    writer.assembler.move_small(R0, Exit::Finished as u32);
    writer.assembler.place(exit);
    writer
        .assembler
        .pop(&[R3, R4, R5, R6, R7, crate::thumb::PROGRAM_COUNTER]);

    let handovers: Vec<(Handover, Label)> = writer
        .handovers
        .iter()
        .map(|(key, label)| (*key, *label))
        .collect();
    for (handover, label) in handovers {
        let assembler = &mut writer.assembler;
        assembler.place(label);
        if handover.refund > 0 {
            assembler.load_word(R0, CONTEXT, STEPS_LEFT_AT);
            assembler.add_small(R0, handover.refund as u32);
            assembler.store_word(R0, CONTEXT, STEPS_LEFT_AT);
        }
        assembler.load_constant(R0, handover.offset as u32);
        assembler.store_word(R0, CONTEXT, RESUME_OFFSET_AT);
        assembler.move_small(R0, handover.depth as u32);
        assembler.store_word(R0, CONTEXT, DEPTH_AT);
        assembler.move_small(R0, Exit::Interpret as u32);
        assembler.branch(exit);
    }

    writer.assembler.finish().map_err(|_| Untranslatable::TooLarge)
}

/// Takes a block's steps as it starts, or hands over before it when too few are left.
fn count_steps(writer: &mut Writer, offset: usize, depth: usize, block_length: usize) {
    let too_few = writer.handover(Handover {
        offset,
        depth,
        refund: 0,
    });
    let assembler = &mut writer.assembler;
    assembler.load_word(R0, CONTEXT, STEPS_LEFT_AT);
    assembler.compare_small(R0, block_length as u32);
    assembler.branch_if(Condition::CarryClear, too_few);
    assembler.subtract_small(R0, block_length as u32);
    assembler.store_word(R0, CONTEXT, STEPS_LEFT_AT);
}

fn write_instruction(
    writer: &mut Writer,
    entry: &Decoded,
    depth: usize,
    handover: Handover,
    local_count: usize,
) {
    use Instruction::*;

    if !is_native(entry.instruction, local_count) {
        let label = writer.handover(handover);
        writer.assembler.branch(label);
        return;
    }
    let top = depth.wrapping_sub(1);
    let below = depth.wrapping_sub(2);
    match entry.instruction {
        PushSmall(value) => push_constant(writer, depth, value as i64),
        PushInt(value) => push_constant(writer, depth, value as i64),
        PushLong(value) => push_constant(writer, depth, value),
        PushNone => push_constant(writer, depth, NONE),
        PushText(index) => push_constant(writer, depth, TextReference::ImageString(index).to_value()),
        ReadLocal(local) => {
            writer.assembler.load_pair(R0, R1, LOCALS, slot(local as usize));
            writer.assembler.store_pair(R0, R1, STACK, slot(depth));
        }
        WriteLocal(local) => {
            writer.assembler.load_pair(R0, R1, STACK, slot(top));
            writer.assembler.store_pair(R0, R1, LOCALS, slot(local as usize));
        }
        Duplicate => {
            writer.assembler.load_pair(R0, R1, STACK, slot(top));
            writer.assembler.store_pair(R0, R1, STACK, slot(depth));
        }
        Drop | LoopStart => {}
        Swap => {
            let assembler = &mut writer.assembler;
            assembler.load_pair(R0, R1, STACK, slot(top));
            assembler.load_pair(R2, R3, STACK, slot(below));
            assembler.store_pair(R0, R1, STACK, slot(below));
            assembler.store_pair(R2, R3, STACK, slot(top));
        }
        Add | Subtract => {
            let overflowed = writer.handover(handover);
            let assembler = &mut writer.assembler;
            load_two(assembler, below);
            match entry.instruction {
                Add => {
                    assembler.add(R0, R0, R2);
                    assembler.data(DataOperation::AddWithCarry, R1, R3);
                }
                _ => {
                    assembler.subtract(R0, R0, R2);
                    assembler.data(DataOperation::SubtractWithCarry, R1, R3);
                }
            }
            assembler.branch_if(Condition::Overflow, overflowed);
            assembler.store_pair(R0, R1, STACK, slot(below));
        }
        BitAnd | BitOr | BitXor => {
            let operation = match entry.instruction {
                BitAnd => DataOperation::And,
                BitOr => DataOperation::Or,
                _ => DataOperation::ExclusiveOr,
            };
            let assembler = &mut writer.assembler;
            load_two(assembler, below);
            assembler.data(operation, R0, R2);
            assembler.data(operation, R1, R3);
            assembler.store_pair(R0, R1, STACK, slot(below));
        }
        BitNot => {
            let assembler = &mut writer.assembler;
            assembler.load_pair(R0, R1, STACK, slot(top));
            assembler.data(DataOperation::Not, R0, R0);
            assembler.data(DataOperation::Not, R1, R1);
            assembler.store_pair(R0, R1, STACK, slot(top));
        }
        Negate | Absolute => {
            let overflowed = writer.handover(handover);
            let assembler = &mut writer.assembler;
            let done = assembler.label();
            assembler.load_pair(R0, R1, STACK, slot(top));
            if entry.instruction == Absolute {
                assembler.compare_small(R1, 0);
                assembler.branch_short_if(Condition::GreaterOrEqual, done);
            }
            assembler.negate(R0, R0);
            assembler.move_small(R2, 0);
            assembler.data(DataOperation::SubtractWithCarry, R2, R1);
            assembler.branch_if(Condition::Overflow, overflowed);
            assembler.move_register(R1, R2);
            assembler.place(done);
            assembler.store_pair(R0, R1, STACK, slot(top));
        }
        Minimum | Maximum => {
            let assembler = &mut writer.assembler;
            let keep_left = assembler.label();
            load_two(assembler, below);
            assembler.move_register(SCRATCH, R1);
            assembler.compare(R0, R2);
            assembler.data(DataOperation::SubtractWithCarry, SCRATCH, R3);
            let condition = match entry.instruction {
                Minimum => Condition::Less,
                _ => Condition::GreaterOrEqual,
            };
            assembler.branch_short_if(condition, keep_left);
            assembler.move_register(R0, R2);
            assembler.move_register(R1, R3);
            assembler.place(keep_left);
            assembler.store_pair(R0, R1, STACK, slot(below));
        }
        Less | GreaterOrEqual | Greater | LessOrEqual => {
            let assembler = &mut writer.assembler;
            load_two(assembler, below);
            // Greater and less-or-equal compare the other way round, which leaves the signed less and greater-or-equal tests.
            let (low, high, other_low, other_high) = match entry.instruction {
                Less | GreaterOrEqual => (R0, R1, R2, R3),
                _ => (R2, R3, R0, R1),
            };
            assembler.compare(low, other_low);
            assembler.data(DataOperation::SubtractWithCarry, high, other_high);
            let condition = match entry.instruction {
                Less | Greater => Condition::Less,
                _ => Condition::GreaterOrEqual,
            };
            store_truth(assembler, condition, below);
        }
        Equal | NotEqual => {
            let assembler = &mut writer.assembler;
            load_two(assembler, below);
            assembler.data(DataOperation::ExclusiveOr, R0, R2);
            assembler.data(DataOperation::ExclusiveOr, R1, R3);
            assembler.data(DataOperation::Or, R0, R1);
            let condition = match entry.instruction {
                Equal => Condition::Equal,
                _ => Condition::NotEqual,
            };
            store_truth(assembler, condition, below);
        }
        Not => {
            let assembler = &mut writer.assembler;
            assembler.load_pair(R0, R1, STACK, slot(top));
            assembler.data(DataOperation::Or, R0, R1);
            store_truth(assembler, Condition::Equal, top);
        }
        IsNone => {
            let assembler = &mut writer.assembler;
            assembler.load_pair(R0, R1, STACK, slot(top));
            assembler.move_small(R2, 1);
            assembler.shift_left(R2, R2, 31);
            assembler.data(DataOperation::ExclusiveOr, R1, R2);
            assembler.data(DataOperation::Or, R0, R1);
            store_truth(assembler, Condition::Equal, top);
        }
        Jump(_) | LoopBack(_) => {
            let target = writer.label_at(entry.target.unwrap_or_default());
            writer.assembler.branch(target);
        }
        JumpIfFalse(_) => {
            let target = writer.label_at(entry.target.unwrap_or_default());
            let assembler = &mut writer.assembler;
            assembler.load_pair(R0, R1, STACK, slot(top));
            assembler.data(DataOperation::Or, R0, R1);
            assembler.branch_if(Condition::Equal, target);
        }
        _ => {}
    }
}

fn push_constant(writer: &mut Writer, depth: usize, value: i64) {
    let assembler = &mut writer.assembler;
    assembler.load_constant(R0, value as u32);
    assembler.load_constant(R1, (value >> 32) as u32);
    assembler.store_pair(R0, R1, STACK, slot(depth));
}

/// Loads the value at `slot_index` into r0 and r1 and the one above it into r2 and r3.
fn load_two(assembler: &mut Assembler, slot_index: usize) {
    assembler.load_pair(R0, R1, STACK, slot(slot_index));
    assembler.load_pair(R2, R3, STACK, slot(slot_index + 1));
}

/// Stores 1 at `slot_index` if the flags meet `condition`, otherwise 0.
fn store_truth(assembler: &mut Assembler, condition: Condition, slot_index: usize) {
    let is_false = assembler.label();
    let done = assembler.label();
    assembler.branch_short_if(condition.opposite(), is_false);
    assembler.move_small(R0, 1);
    assembler.branch_short(done);
    assembler.place(is_false);
    assembler.move_small(R0, 0);
    assembler.place(done);
    assembler.move_small(R1, 0);
    assembler.store_pair(R0, R1, STACK, slot(slot_index));
}
