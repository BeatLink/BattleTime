//! What the host needs from the firmware or the simulator around it, and what it hands back.

use neotime_runtime::{
    events::{EventKind, SwipeDirection},
    instruction::MOST_EFFECT_ARGUMENTS,
    AppImage, EffectKind, Failure, Interpreter, SystemValue, SystemValues, Value,
};

// ============================================================================
// The system
// ============================================================================

/// The embedder: it keeps the system values current and carries out what apps ask for.
pub trait System: SystemValues {
    /// Carries out an effect a handler asked for, such as a vibration; `go` never arrives here.
    fn carry_out(&mut self, effect: &AppEffect);

    /// An app's code failed; the app stays open and its state is as it was before the call.
    fn app_failed(&mut self, failure: AppFailure);
}

/// One effect, with its arguments readable as numbers or text.
pub struct AppEffect<'call> {
    pub kind: EffectKind,
    pub arguments: [Value; MOST_EFFECT_ARGUMENTS],
    pub(crate) image: &'call AppImage<'call>,
    pub(crate) interpreter: &'call Interpreter,
}

impl AppEffect<'_> {
    pub fn number(&self, position: usize) -> Value {
        self.arguments.get(position).copied().unwrap_or(0)
    }

    /// An argument that holds text, such as a toast's message.
    pub fn text(&self, position: usize) -> Option<&str> {
        self.interpreter.text(self.image, self.number(position)).ok()
    }
}

/// Which piece of app code failed, and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppFailure {
    Handler {
        handler: u16,
        event: EventKind,
        failure: Failure,
    },
    Expression {
        expression: u16,
        failure: Failure,
    },
}

// ============================================================================
// Input
// ============================================================================

/// A touch, in screen pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Touch {
    Click {
        x: i32,
        y: i32,
    },
    LongPress {
        x: i32,
        y: i32,
    },
    DoubleClick {
        x: i32,
        y: i32,
    },
    Press {
        x: i32,
        y: i32,
    },
    Release {
        x: i32,
        y: i32,
    },
    Drag {
        x: i32,
        y: i32,
        start_x: i32,
        start_y: i32,
    },
    Swipe(SwipeDirection),
    /// How fast the finger was moving, in pixels a second, as it lifted from a drag; sent just before its `Release`.
    Fling {
        x_speed: i32,
        y_speed: i32,
    },
}

/// What became of an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventResult {
    /// A handler ran to the end.
    Handled,
    /// No handler takes it, so the system's own meaning applies, such as a swipe up on the face opening the launcher.
    NotHandled,
    /// The handler failed and changed nothing; the system has been told.
    Failed,
}

pub(crate) fn swipe_value(direction: SwipeDirection) -> Value {
    match direction {
        SwipeDirection::Up => 0,
        SwipeDirection::Down => 1,
        SwipeDirection::Left => 2,
        SwipeDirection::Right => 3,
    }
}

// ============================================================================
// The clock seen by app code
// ============================================================================

/// The system's values, with `now` fixed to the time the host was given for this call.
pub(crate) struct Clocked<'system, Values: SystemValues + ?Sized> {
    pub values: &'system Values,
    pub now: Value,
}

impl<Values: SystemValues + ?Sized> SystemValues for Clocked<'_, Values> {
    fn read(&self, value: SystemValue) -> Value {
        match value {
            SystemValue::Now => self.now,
            other => self.values.read(other),
        }
    }

    fn read_text(&self, value: SystemValue) -> &str {
        self.values.read_text(value)
    }

    fn read_element(&self, value: SystemValue, index: Value) -> Value {
        self.values.read_element(value, index)
    }

    fn read_element_text(&self, value: SystemValue, index: Value) -> &str {
        self.values.read_element_text(value, index)
    }

    fn random_number(&self) -> u32 {
        self.values.random_number()
    }

    fn read_file(&self, path: &str, offset: u32, out: &mut [u8]) -> Option<usize> {
        self.values.read_file(path, offset, out)
    }
}

#[cfg(test)]
#[path = "system_tests.rs"]
mod system_tests;
