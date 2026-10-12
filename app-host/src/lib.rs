//! Runs a NeoTime app: the loop between an app image, the UI toolkit and the system.
//!
//! `AppHost` opens an image, builds the first screen's widgets as rlvgl widgets and lays them out.
//! Touches go to the handlers the widgets name; a handler's effects go to the embedder's `System`,
//! and only the widgets whose bound values changed are drawn again. The clock redraws only what
//! shows it, and `next_wakeup` says when, so the watch can sleep in between. The same code runs on
//! the watch, in tests and in the desktop simulator. docs/app-runtime.md describes the design.

#![no_std]

extern crate alloc;

mod drawing;
pub mod fonts;
pub mod haptics;
mod host;
mod layout;
pub mod motion;
mod nodes;
pub mod phone;
pub mod settings;
mod subscriptions;
mod system;
mod text;

#[cfg(any(test, feature = "demo"))]
pub mod demo;

pub use fonts::FontName;
pub use host::{AppHost, OpenError, LARGEST_SAVED_STATE_BYTES, SCREEN_AREA, SCREEN_HISTORY_DEPTH};
pub use nodes::{is_mapped, Align, Length};
pub use rlvgl_core::widget::Rect;
pub use subscriptions::{BatteryChange, CallKind, MotionKind, PushedEvent};
pub use system::{AppEffect, AppFailure, EventResult, System, Touch};
pub use text::WrappedLines;

#[cfg(test)]
mod tests;
