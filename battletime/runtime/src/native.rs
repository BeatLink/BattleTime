//! Running code units as native code, translated from their bytecode by `battletime-aot`.
//!
//! The interpreter hands each app code unit that has native code to it, with the unit's part of
//! the operand stack and its locals where they lie in the interpreter. Native code runs on them
//! directly and calls back for each instruction it leaves to the interpreter, which runs that one
//! instruction itself. When native code stops before the unit's end, at a call or at anything it
//! does not handle, the interpreter carries on from that instruction; after a call returns, the
//! unit runs natively again from the instruction after it.

use crate::paging::CodeUnit;
use crate::Value;

// ============================================================================
// What native code is given
// ============================================================================

/// What a native unit is given in r0, laid out as the translated code reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Context {
    /// The unit's first local, each a 64-bit value.
    pub locals: u32,
    /// The bottom of the unit's part of the operand stack.
    pub stack: u32,
    /// How many stack slots the unit may use above `stack`.
    pub stack_room: u32,
    /// How many more instructions the run may take before it fails with too many steps.
    pub steps_left: u32,
    /// Where the unit starts, and where the interpreter carries on when it stops.
    pub resume_offset: u32,
    /// The stack's depth above `stack`: at a helper call, and when the unit stops.
    pub depth: u32,
    /// The table of helpers, whose first runs one instruction in the interpreter.
    pub helpers: u32,
}

pub const LOCALS_AT: u32 = 0;
pub const STACK_AT: u32 = 4;
pub const STACK_ROOM_AT: u32 = 8;
pub const STEPS_LEFT_AT: u32 = 12;
pub const RESUME_OFFSET_AT: u32 = 16;
pub const DEPTH_AT: u32 = 20;
pub const HELPERS_AT: u32 = 24;

const _: () = {
    assert!(core::mem::offset_of!(Context, locals) as u32 == LOCALS_AT);
    assert!(core::mem::offset_of!(Context, stack) as u32 == STACK_AT);
    assert!(core::mem::offset_of!(Context, stack_room) as u32 == STACK_ROOM_AT);
    assert!(core::mem::offset_of!(Context, steps_left) as u32 == STEPS_LEFT_AT);
    assert!(core::mem::offset_of!(Context, resume_offset) as u32 == RESUME_OFFSET_AT);
    assert!(core::mem::offset_of!(Context, depth) as u32 == DEPTH_AT);
    assert!(core::mem::offset_of!(Context, helpers) as u32 == HELPERS_AT);
};

/// The helper that runs the instruction at an offset of the unit in the interpreter, as the first of the helpers.
pub const EXECUTE_HELPER: u32 = 0;

/// What a native unit gives back in r0, and a helper too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    /// The unit ran to its end, or a helper's instruction ran.
    Finished = 0,
    /// The interpreter carries on at `resume_offset`, with the steps left as the context says.
    Interpret = 1,
    /// An instruction a helper ran failed, and the run fails with it.
    Failed = 2,
}

impl Exit {
    pub fn from_word(word: u32) -> Option<Exit> {
        match word {
            0 => Some(Exit::Finished),
            1 => Some(Exit::Interpret),
            2 => Some(Exit::Failed),
            _ => None,
        }
    }
}

// ============================================================================
// What runs it
// ============================================================================

/// The interpreter's state a native unit works on, for whatever runs native code.
pub struct NativeFrame<'run> {
    /// The unit's part of the operand stack, `stack_room` slots of it.
    pub stack: *mut Value,
    pub stack_room: usize,
    /// The unit's locals, with the rest of the pool after them, `locals_room` values in all.
    pub locals: *mut Value,
    pub locals_room: usize,
    pub steps_left: u32,
    pub resume_offset: u32,
    pub depth: u32,
    /// Runs the instruction at an offset with the stack at a depth, false when it fails.
    pub execute: &'run mut dyn FnMut(u32, u32) -> bool,
}

/// Native code for an image's units, run through whatever the platform offers: the processor on
/// the watch, a simulator on the computer.
pub trait NativeCode {
    /// Runs `unit` natively from `frame.resume_offset`, or gives none when it has no native code.
    fn run(&mut self, unit: CodeUnit, frame: &mut NativeFrame<'_>) -> Option<Exit>;
}
