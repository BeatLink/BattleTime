//! Runs a NeoTime app: the loop between an app image, the UI toolkit and the system.
//!
//! `AppHost` opens an image, builds the first screen's widgets and lays them out.
//! Touches go to the handlers the widgets name; a handler's effects go to the embedder's `System`,
//! and only the widgets whose bound values changed are drawn again. The clock redraws only what
//! shows it, and `next_wakeup` says when, so the watch can sleep in between. The same code runs on
//! the watch, in tests and in the desktop simulator. docs/app-runtime.md describes the design.

#![no_std]

extern crate alloc;

mod bitmap_file;
pub mod closed_apps;
mod drawing;
pub mod folder;
pub mod fonts;
pub mod haptics;
mod host;
mod layout;
pub mod motion;
mod nodes;
pub mod phone;
pub mod quick_apps;
pub mod settings;
pub mod side_button;
mod subscriptions;
pub mod swipes;
mod system;
mod text;
mod text_file;
pub mod wake;
pub mod way_back;
mod widgets;

#[cfg(any(test, feature = "demo"))]
pub mod demo;

pub use fonts::FontName;
pub use host::{
    AppHost, OpenError, Transition, LARGEST_SAVED_STATE_BYTES, PAINT_STEP_PIXELS, SCREEN_AREA,
    SCREEN_HISTORY_DEPTH, STRIP_PIXELS, TRANSITION_TIME_IN_MILLISECONDS,
};
pub use neotime_ui::geometry::Rect;
pub use nodes::{is_mapped, Align, Length};
pub use subscriptions::{BatteryChange, CallKind, MotionKind, PushedEvent};
pub use system::{AppEffect, AppFailure, EventResult, System, Touch};
pub use text::WrappedLines;

#[cfg(test)]
mod tests;
