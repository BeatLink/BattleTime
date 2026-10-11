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
pub mod pattern;
pub mod state;
pub mod text;
pub mod verifier;
pub mod widgets;

pub use bindings::{analyse_expression, Bindings, ExpressionInfo, Inputs};
pub use image::{AppImage, AppKind, ImageError, StateVariable, ValueType};
pub use instruction::{EffectKind, SystemValue};
pub use interpreter::{Effect, Failure, HandlerOutcome, Interpreter, SystemValues, Value, NONE};
pub use state::AppState;
pub use verifier::verify;
pub use widgets::{Property, PropertySource, Widget, WidgetKind, WidgetTree};

#[cfg(test)]
mod tests;
