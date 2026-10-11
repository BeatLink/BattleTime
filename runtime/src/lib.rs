//! The core of NeoTime's app runtime: app images, the checks run when one is installed, the bytecode
//! interpreter, and the bindings that decide which widgets to redraw and when.
//!
//! Nothing here touches hardware, allocates or panics on bad input, so the same code runs on the
//! watch, in tests and in the simulator. docs/app-runtime.md describes the design.

#![no_std]

pub mod bindings;
pub mod image;
pub mod instruction;
pub mod interpreter;
pub mod text;
pub mod verifier;

pub use bindings::{analyse_expression, Bindings, ExpressionInfo, Inputs};
pub use image::{AppImage, AppKind, ImageError, StateVariable, ValueType};
pub use instruction::{EffectKind, SystemValue};
pub use interpreter::{Effect, Failure, HandlerOutcome, Interpreter, StateWrite, SystemValues, Value, NONE};
pub use verifier::verify;

#[cfg(test)]
mod tests;
