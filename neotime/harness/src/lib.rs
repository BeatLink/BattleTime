//! A test watch for NeoTime apps, in the spirit of wasp-os's `make check`.
//!
//! `TestWatch` compiles an app or face from its source with `neotime-compiler`, opens the image on
//! the real `AppHost`, and drives it the way the watch would: taps, swipes, the side button, the
//! clock and events from the phone and sensors. A recording `FakeSystem` stands in for the
//! firmware, keeping every effect and failure, and a `Framebuffer` holds the drawn pixels.
//! `snapshots` keeps what the watch shows in committed files, for the parity and screenshot tests.
//! Scripts in a small line-based format drive the same watch end to end, through `run_script`.
//! docs/testing.md shows how to write a test with it.

mod fake_system;
mod framebuffer;
mod script;
mod snapshots;
mod sources;
mod watch;

pub use fake_system::{
    starting_packages, starting_step_hours, FakeLauncherGroup, FakePackage, FakeSystem, ForecastDay,
    InstalledApp, RecordedEffect, HOURS_PER_DAY, STARTING_APP_MEMORY_BUDGET, STARTING_DAY_STEPS,
    STARTING_NOTE, STARTING_NOTE_PATH, STARTING_QUESTION, STARTING_QUESTION_PATH, STARTING_TRIP_STEPS,
    START_TIME_IN_MILLISECONDS, STEP_HISTORY_DAYS,
};
pub use framebuffer::{Framebuffer, SCREEN_SIDE_IN_PIXELS};
pub use script::{
    load_script, parse_script, run_script, run_script_on, EffectCheck, ExpectedArgument, ScreenPoint, Script,
    ScriptCommand, ScriptError, ScriptRun, ScriptStep, StepFailure, StepOutcome, TouchKind, TouchTarget,
};
pub use snapshots::{
    check_screen_snapshot, check_text_snapshot, decode_png, difference_picture, encode_png,
    failure_directory, frame_to_rgb_bytes, is_updating_snapshots, line_difference, screen_texts_json,
    ScreenTexts, SnapshotFolders, UPDATE_SNAPSHOTS_VARIABLE,
};
pub use sources::{
    compile_package, compile_source_text, docs_root, every_package, package_source, repository_root,
    shared_library, OpenFailure, Package, PackageKind,
};
pub use watch::{TestWatch, MOST_WAKEUPS_PER_ADVANCE};

pub use neotime_app_host::{
    AppFailure, BatteryChange, CallKind, EventResult, MotionKind, PushedEvent, Touch, Transition,
};
pub use neotime_runtime::events::SwipeDirection as Direction;
pub use neotime_runtime::{EffectKind, SystemValue, Value, WidgetKind};

#[cfg(test)]
mod tests;
