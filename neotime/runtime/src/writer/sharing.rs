//! Shares code that repeats with only its constants changed.
//!
//! A compiler that inlines everything writes the same code many times over, such as one
//! expression per cell of a board that differ only in the cell's number. This pass finds
//! expressions, and handlers, that match instruction for instruction apart from the numbers and
//! texts they push. Each such group becomes one function that takes the differing constants as
//! arguments, and each member becomes those pushes and a `Call`. Only groups that make the image
//! smaller are shared.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::ImageWriter;
use crate::image::MOST_FUNCTIONS;
use crate::instruction::{decode, Instruction};
use crate::interpreter::{LOCAL_POOL, MOST_CALL_DEPTH, MOST_LOCALS, NONE};

// ============================================================================
// Costs
// ============================================================================

/// A function's header and its entry in the functions table.
const FUNCTION_OVERHEAD_BYTES: usize = 3 + 4;

// ============================================================================
// Decoded code
// ============================================================================

/// A constant pushed by code, which a shared function can take as an argument instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Constant {
    Number(i64),
    Text(u16),
}

impl Constant {
    fn push(self) -> Instruction {
        match self {
            Constant::Number(NONE) => Instruction::PushNone,
            Constant::Number(value) => push_number(value),
            Constant::Text(index) => Instruction::PushText(index),
        }
    }
}

/// The smallest instruction that pushes a number.
fn push_number(value: i64) -> Instruction {
    if let Ok(small) = i8::try_from(value) {
        Instruction::PushSmall(small)
    } else if let Ok(int) = i32::try_from(value) {
        Instruction::PushInt(int)
    } else {
        Instruction::PushLong(value)
    }
}

/// Code taken apart into instructions, with each jump's target as an instruction number.
struct DecodedCode {
    instructions: Vec<Instruction>,
    /// For each jump or loop back, the number of the instruction it goes to; the end is the count.
    targets: Vec<Option<usize>>,
}

impl DecodedCode {
    fn decode(code: &[u8]) -> Option<DecodedCode> {
        let mut instructions = Vec::new();
        let mut offsets = Vec::new();
        let mut offset = 0;
        while offset < code.len() {
            let (instruction, next_offset) = decode(code, offset).ok()?;
            offsets.push(offset);
            instructions.push(instruction);
            offset = next_offset;
        }
        offsets.push(code.len());

        let mut targets = Vec::with_capacity(instructions.len());
        for (index, instruction) in instructions.iter().enumerate() {
            let next_offset = offsets[index + 1];
            let target_offset = match *instruction {
                Instruction::Jump(distance) | Instruction::JumpIfFalse(distance) => {
                    Some(next_offset + distance as usize)
                }
                Instruction::LoopBack(distance) => Some(next_offset.checked_sub(distance as usize)?),
                _ => None,
            };
            targets.push(match target_offset {
                Some(target) => Some(offsets.iter().position(|&placed| placed == target)?),
                None => None,
            });
        }
        Some(DecodedCode {
            instructions,
            targets,
        })
    }

    fn constants(&self) -> Vec<Constant> {
        self.instructions
            .iter()
            .filter_map(|instruction| constant_of(*instruction))
            .collect()
    }

    /// The code with its constants left out, so code that differs only in them matches.
    fn shape(&self) -> Vec<u8> {
        let mut shape = Vec::new();
        for (instruction, target) in self.instructions.iter().zip(&self.targets) {
            match (constant_of(*instruction), target) {
                (Some(Constant::Number(_)), _) => shape.push(0xF0),
                (Some(Constant::Text(_)), _) => shape.push(0xF1),
                (None, Some(target)) => {
                    shape.push(instruction.encode().as_bytes()[0]);
                    shape.extend((*target as u32).to_le_bytes());
                }
                (None, None) => shape.extend(instruction.encode().as_bytes()),
            }
        }
        shape
    }

    /// How many locals the code uses: one more than the highest it names.
    fn local_count(&self) -> usize {
        self.instructions
            .iter()
            .filter_map(|instruction| match instruction {
                Instruction::ReadLocal(local) | Instruction::WriteLocal(local) => Some(*local as usize + 1),
                _ => None,
            })
            .max()
            .unwrap_or(0)
    }
}

fn constant_of(instruction: Instruction) -> Option<Constant> {
    match instruction {
        Instruction::PushSmall(value) => Some(Constant::Number(value as i64)),
        Instruction::PushInt(value) => Some(Constant::Number(value as i64)),
        Instruction::PushLong(value) => Some(Constant::Number(value)),
        Instruction::PushNone => Some(Constant::Number(NONE)),
        Instruction::PushText(index) => Some(Constant::Text(index)),
        _ => None,
    }
}

/// Encodes instructions, working each jump's distance out again from its target's new place.
fn encode(instructions: &[Instruction], targets: &[Option<usize>]) -> Vec<u8> {
    let mut offsets = Vec::with_capacity(instructions.len() + 1);
    let mut offset = 0;
    for instruction in instructions {
        offsets.push(offset);
        offset += instruction.encode().as_bytes().len();
    }
    offsets.push(offset);

    let mut code = Vec::with_capacity(offset);
    for (index, (instruction, target)) in instructions.iter().zip(targets).enumerate() {
        let next_offset = offsets[index + 1];
        let placed = match (*instruction, target) {
            (Instruction::Jump(_), Some(target)) => {
                Instruction::Jump((offsets[*target] - next_offset) as u16)
            }
            (Instruction::JumpIfFalse(_), Some(target)) => {
                Instruction::JumpIfFalse((offsets[*target] - next_offset) as u16)
            }
            (Instruction::LoopBack(_), Some(target)) => {
                Instruction::LoopBack((next_offset - offsets[*target]) as u16)
            }
            (other, _) => other,
        };
        code.extend(placed.encode().as_bytes());
    }
    code
}

fn encoded_length(instructions: &[Instruction]) -> usize {
    instructions
        .iter()
        .map(|instruction| instruction.encode().as_bytes().len())
        .sum()
}

// ============================================================================
// Sharing
// ============================================================================

/// Which table a piece of code is in.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Table {
    Expressions,
    Handlers,
}

/// Code that repeats, ready to become one function.
struct SharedCode {
    table: Table,
    members: Vec<usize>,
    /// The constants that differ between members, by their place among the code's constants.
    arguments: Vec<usize>,
    bytes_saved: usize,
}

impl ImageWriter {
    /// Replaces expressions and handlers that differ only in their constants with calls to one
    /// function each, where that makes the image smaller.
    pub fn share_repeated_code(&mut self) {
        let mut groups: BTreeMap<(Table, Vec<u8>), Vec<usize>> = BTreeMap::new();
        let mut decoded: [Vec<Option<DecodedCode>>; 2] = [Vec::new(), Vec::new()];
        for (table, codes) in [
            (Table::Expressions, &self.expressions),
            (Table::Handlers, &self.handlers),
        ] {
            for (index, code) in codes.iter().enumerate() {
                let code = DecodedCode::decode(code);
                if let Some(code) = &code {
                    groups.entry((table, code.shape())).or_default().push(index);
                }
                decoded[table as usize].push(code);
            }
        }

        let callees = self.callee_needs();
        let mut shared: Vec<SharedCode> = groups
            .into_iter()
            .filter(|(_, members)| members.len() > 1)
            .filter_map(|((table, _), members)| {
                plan_sharing(table, members, &decoded[table as usize], &callees)
            })
            .collect();
        shared.sort_by_key(|plan| core::cmp::Reverse(plan.bytes_saved));

        for plan in shared {
            if self.functions.len() == MOST_FUNCTIONS {
                break;
            }
            let codes = &decoded[plan.table as usize];
            self.add_shared_function(&plan, codes);
        }
    }

    /// For each function already added, the locals it needs with its callees, and its call depth.
    fn callee_needs(&self) -> Vec<CalleeNeeds> {
        let mut needs: Vec<CalleeNeeds> = Vec::with_capacity(self.functions.len());
        for bytes in &self.functions {
            let local_count = bytes.get(2).copied().unwrap_or(0) as usize;
            let deepest_callee = DecodedCode::decode(bytes.get(3..).unwrap_or_default())
                .map(|code| deepest_callee(&code, &needs))
                .unwrap_or(CalleeNeeds::TOO_MUCH);
            needs.push(CalleeNeeds {
                locals: local_count + deepest_callee.locals,
                depth: deepest_callee.depth + 1,
            });
        }
        needs
    }

    fn add_shared_function(&mut self, plan: &SharedCode, codes: &[Option<DecodedCode>]) {
        let Some(first) = codes[plan.members[0]].as_ref() else {
            return;
        };
        let result_count = (plan.table == Table::Expressions) as u8;
        let local_count = (plan.arguments.len() + first.local_count()) as u8;
        let template = template_of(first, &plan.arguments);
        let function = self.function(
            plan.arguments.len() as u8,
            result_count,
            local_count,
            encode(&template, &first.targets),
        );

        for &member in &plan.members {
            let Some(code) = codes[member].as_ref() else {
                continue;
            };
            let call = call_of(&code.constants(), &plan.arguments, function);
            let bytes = encode(&call, &alloc::vec![None; call.len()]);
            match plan.table {
                Table::Expressions => self.expressions[member] = bytes,
                Table::Handlers => self.handlers[member] = bytes,
            }
        }
    }
}

/// The shared function's code: the first member's, reading the differing constants from its
/// first locals and its own locals from the ones after them.
fn template_of(first: &DecodedCode, arguments: &[usize]) -> Vec<Instruction> {
    let argument_count = arguments.len() as u8;
    let mut constant_place = 0;
    let mut template = Vec::with_capacity(first.instructions.len());
    for instruction in &first.instructions {
        template.push(match (*instruction, constant_of(*instruction)) {
            (_, Some(constant)) => {
                let argument = arguments.iter().position(|&place| place == constant_place);
                constant_place += 1;
                match argument {
                    Some(argument) => Instruction::ReadLocal(argument as u8),
                    None => constant.push(),
                }
            }
            (Instruction::ReadLocal(local), None) => Instruction::ReadLocal(local + argument_count),
            (Instruction::WriteLocal(local), None) => Instruction::WriteLocal(local + argument_count),
            (other, None) => other,
        });
    }
    template
}

/// What replaces one member: pushes of its differing constants, then the call.
fn call_of(constants: &[Constant], arguments: &[usize], function: u8) -> Vec<Instruction> {
    let mut call: Vec<Instruction> = arguments.iter().map(|&place| constants[place].push()).collect();
    call.push(Instruction::Call(function));
    call
}

/// What calling a function takes: the locals of it and its callees, and how deep the calls go.
#[derive(Clone, Copy, Debug, Default)]
struct CalleeNeeds {
    locals: usize,
    depth: usize,
}

impl CalleeNeeds {
    const TOO_MUCH: CalleeNeeds = CalleeNeeds {
        locals: LOCAL_POOL,
        depth: MOST_CALL_DEPTH,
    };
}

/// The most that any function this code calls needs.
fn deepest_callee(code: &DecodedCode, needs: &[CalleeNeeds]) -> CalleeNeeds {
    let mut deepest = CalleeNeeds::default();
    for instruction in &code.instructions {
        if let Instruction::Call(function) = instruction {
            let callee = needs
                .get(*function as usize)
                .copied()
                .unwrap_or(CalleeNeeds::TOO_MUCH);
            deepest.locals = deepest.locals.max(callee.locals);
            deepest.depth = deepest.depth.max(callee.depth);
        }
    }
    deepest
}

/// Works out which constants a group's members differ in, and whether sharing them saves bytes.
fn plan_sharing(
    table: Table,
    members: Vec<usize>,
    codes: &[Option<DecodedCode>],
    callees: &[CalleeNeeds],
) -> Option<SharedCode> {
    let first = codes[members[0]].as_ref()?;
    let deepest = deepest_callee(first, callees);
    let constants: Vec<Vec<Constant>> = members
        .iter()
        .map(|&member| codes[member].as_ref().map(DecodedCode::constants))
        .collect::<Option<_>>()?;
    let arguments: Vec<usize> = (0..constants[0].len())
        .filter(|&place| {
            constants
                .iter()
                .any(|member| member[place] != constants[0][place])
        })
        .collect();
    let local_count = arguments.len() + first.local_count();
    let too_deep =
        deepest.depth + 1 >= MOST_CALL_DEPTH || local_count + deepest.locals > LOCAL_POOL - MOST_LOCALS;
    if local_count > MOST_LOCALS || too_deep {
        return None;
    }

    let template_bytes = encoded_length(&template_of(first, &arguments));
    let calls_bytes: usize = constants
        .iter()
        .map(|member| encoded_length(&call_of(member, &arguments, 0)))
        .sum();
    let members_bytes: usize = members
        .iter()
        .filter_map(|&member| codes[member].as_ref())
        .map(|code| encoded_length(&code.instructions))
        .sum();
    let shared_bytes = template_bytes + FUNCTION_OVERHEAD_BYTES + calls_bytes;
    let bytes_saved = members_bytes
        .checked_sub(shared_bytes)
        .filter(|&saved| saved > 0)?;
    Some(SharedCode {
        table,
        members,
        arguments,
        bytes_saved,
    })
}
