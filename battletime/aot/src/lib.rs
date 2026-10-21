//! BattleTime's translator from verified app bytecode to Thumb-2 machine code.
//!
//! A translated code unit runs in place of its bytecode, on the interpreter's own operand stack and
//! locals, so the interpreter can take over at the start of any instruction. Native code does only
//! what it can do exactly as the interpreter would; at anything else, such as an overflow, an
//! instruction it does not translate or a block that would pass the step limit, it hands the
//! interpreter the instruction's offset and the stack's depth, and the interpreter carries on from
//! there. docs/aot-compiler.md explains the design.

#![no_std]

extern crate alloc;

#[cfg(any(test, feature = "simulator"))]
pub mod simulator;
pub mod thumb;
pub mod translate;

#[cfg(test)]
mod tests;

pub use translate::{translate, Untranslatable};

// ============================================================================
// The context a native unit runs with
// ============================================================================

/// What a native unit is given in r0, laid out as the code reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Context {
    /// The unit's first local in the interpreter's pool, each a 64-bit value.
    pub locals: u32,
    /// The bottom of the unit's part of the operand stack.
    pub stack: u32,
    /// How many stack slots the unit may use above `stack`.
    pub stack_room: u32,
    /// How many more instructions the run may take before it fails with too many steps.
    pub steps_left: u32,
    /// Where the interpreter carries on, when the unit hands over.
    pub resume_offset: u32,
    /// The stack's depth above `stack` when the unit finished or handed over.
    pub depth: u32,
}

pub const LOCALS_AT: u32 = 0;
pub const STACK_AT: u32 = 4;
pub const STACK_ROOM_AT: u32 = 8;
pub const STEPS_LEFT_AT: u32 = 12;
pub const RESUME_OFFSET_AT: u32 = 16;
pub const DEPTH_AT: u32 = 20;

const _: () = {
    assert!(core::mem::offset_of!(Context, locals) as u32 == LOCALS_AT);
    assert!(core::mem::offset_of!(Context, stack) as u32 == STACK_AT);
    assert!(core::mem::offset_of!(Context, stack_room) as u32 == STACK_ROOM_AT);
    assert!(core::mem::offset_of!(Context, steps_left) as u32 == STEPS_LEFT_AT);
    assert!(core::mem::offset_of!(Context, resume_offset) as u32 == RESUME_OFFSET_AT);
    assert!(core::mem::offset_of!(Context, depth) as u32 == DEPTH_AT);
};

/// What a native unit gives back in r0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    /// It ran to the end of its code.
    Finished = 0,
    /// The interpreter carries on at `resume_offset`.
    Interpret = 1,
}

impl Exit {
    pub fn from_word(word: u32) -> Option<Exit> {
        match word {
            0 => Some(Exit::Finished),
            1 => Some(Exit::Interpret),
            _ => None,
        }
    }
}
