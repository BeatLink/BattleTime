//! The core of BattleTime's app runtime: app images, the checks run when one is installed, the bytecode
//! interpreter, and the bindings that decide which widgets to redraw and when.
//!
//! Nothing here touches hardware or panics on bad input, so the same code runs on the watch, in
//! tests and in the simulator. The open app's state, bindings and scratch room are allocated at the
//! size its image asks for; `writer` allocates freely and is only built for the compiler and tests.
//! docs/app-runtime.md describes the design.

#![no_std]

extern crate alloc;

pub mod bindings;
pub mod events;
pub mod image;
pub mod instruction;
pub mod interpreter;
pub mod memory_need;
pub mod pattern;
pub mod resources;
pub mod state;
pub mod subscriptions;
pub mod text;
pub mod theme;
pub mod verifier;
pub mod widgets;
#[cfg(any(test, feature = "writer"))]
pub mod writer;

pub use bindings::{
    analyse_expression, analyse_expression_in, for_each_reached_instruction, for_each_reached_instruction_in,
    Bindings, ExpressionInfo, Inputs,
};
pub use image::{AppImage, AppKind, ImageError, StateVariable, ValueType};
pub use instruction::{EffectKind, SystemValue};
pub use interpreter::{Effect, Failure, HandlerOutcome, Interpreter, SystemValues, Value, NONE};
pub use memory_need::{estimate_memory_need, MemoryNeed, APP_HEAP_BUDGET_BYTES};
pub use state::AppState;
pub use theme::{
    preset_matching, preset_named, ColorValue, PresetTheme, Theme, ThemeColor, DEFAULT_THEME, PRESET_THEMES,
};
pub use verifier::{verify, verify_with, CheckedFunctions};
pub use widgets::{Property, PropertySource, Widget, WidgetKind, WidgetTree};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod array_work_tests;
#[cfg(test)]
mod function_tests;
#[cfg(test)]
mod memory_need_tests;
