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

pub mod native_file;
#[cfg(any(test, feature = "simulator"))]
pub mod simulator;
pub mod thumb;
pub mod translate;

#[cfg(test)]
mod tests;

pub use translate::{translate, Untranslatable};

pub use battletime_runtime::native::{Context, Exit};
