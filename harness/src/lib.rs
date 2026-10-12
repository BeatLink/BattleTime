//! A test watch for NeoTime apps, in the spirit of wasp-os's `make check`.
//!
//! `TestWatch` compiles an app or face from its source with `neotime-compiler`, opens the image on
//! the real `AppHost`, and drives it the way the watch would: taps, swipes, the side button, the
//! clock and events from the phone and sensors. A recording `FakeSystem` stands in for the
//! firmware, keeping every effect and failure, and a `Framebuffer` holds the drawn pixels.
//! docs/testing.md shows how to write a test with it.

mod fake_system;
mod framebuffer;
mod sources;
mod watch;

pub use fake_system::{FakeSystem, ForecastDay, InstalledApp, RecordedEffect, START_TIME_IN_MILLISECONDS};
pub use framebuffer::{Framebuffer, SCREEN_SIDE_IN_PIXELS};
pub use sources::{
    compile_package, compile_source_text, every_package, package_source, repository_root, shared_library,
    OpenFailure, Package, PackageKind,
};
pub use watch::{TestWatch, MOST_WAKEUPS_PER_ADVANCE};

pub use neotime_app_host::{AppFailure, BatteryChange, CallKind, EventResult, MotionKind, PushedEvent};
pub use neotime_runtime::events::SwipeDirection as Direction;
pub use neotime_runtime::{EffectKind, SystemValue, Value, WidgetKind};

#[cfg(test)]
mod tests;
