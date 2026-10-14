//! The wrist gestures and the face's double tap that wake the screen or put it to sleep.
//!
//! While the screen is dark the accelerometer watches for a raised wrist and a shake, and while it
//! is lit for a lowered arm, each only when its setting is on, and none in sleep mode. A lit screen
//! held on by the open app or a status page is never lowered to sleep. A double tap on the face
//! that the face does not take for itself puts the watch to sleep; the panel reports no double taps
//! while the screen is lit, so two taps close together in time and place count as one.

use neotime_runtime::Value;
use neotime_settings::{Setting, Settings};

use crate::Touch;

// ============================================================================
// Wrist gestures
// ============================================================================

/// The wrist gestures to watch for now, and the shake speed a shake must pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WakeGestures {
    pub raise: bool,
    pub lower: bool,
    pub shake: bool,
    pub shake_threshold: i32,
}

/// What the screen is doing, as far as the wrist gestures care.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenState {
    pub is_lit: bool,
    /// The open app or a status page keeps the screen on.
    pub is_held_on: bool,
}

/// The gestures that would change the screen now: waking it while dark, or sleeping it while lit.
pub fn gestures_to_watch(settings: &Settings, screen: ScreenState) -> WakeGestures {
    if settings.is_sleep_mode() {
        return WakeGestures::default();
    }
    let is_dark = !screen.is_lit;
    WakeGestures {
        raise: is_dark && settings.is_on(Setting::RaiseToWake),
        lower: screen.is_lit && !screen.is_held_on && settings.is_on(Setting::LowerToSleep),
        shake: is_dark && settings.is_on(Setting::ShakeToWake),
        shake_threshold: settings.get(Setting::ShakeThreshold),
    }
}

// ============================================================================
// The face's double tap
// ============================================================================

/// The second tap of a double tap comes within this long of the first.
pub const DOUBLE_TAP_GAP_IN_MILLISECONDS: Value = 500;

/// The second tap of a double tap lands within this many pixels of the first, either way.
pub const DOUBLE_TAP_REACH_IN_PIXELS: i32 = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Tap {
    x: i32,
    y: i32,
    time_in_milliseconds: Value,
}

/// Finds a double tap in the touches the face did not take.
#[derive(Default)]
pub struct DoubleTapFinder {
    first_tap: Option<Tap>,
}

impl DoubleTapFinder {
    /// Takes a touch on the face and whether the face took it; true when it completes a double tap.
    pub fn touch(&mut self, touch: &Touch, taken_by_face: bool, now: Value) -> bool {
        match *touch {
            Touch::Press { .. } | Touch::Release { .. } => false,
            Touch::DoubleClick { .. } if !taken_by_face => {
                self.forget();
                true
            }
            Touch::Click { x, y } if !taken_by_face => self.tap(Tap {
                x,
                y,
                time_in_milliseconds: now,
            }),
            _ => {
                self.forget();
                false
            }
        }
    }

    pub fn forget(&mut self) {
        self.first_tap = None;
    }

    fn tap(&mut self, tap: Tap) -> bool {
        let completes = self.first_tap.is_some_and(|first| {
            tap.time_in_milliseconds - first.time_in_milliseconds <= DOUBLE_TAP_GAP_IN_MILLISECONDS
                && (tap.x - first.x).abs() <= DOUBLE_TAP_REACH_IN_PIXELS
                && (tap.y - first.y).abs() <= DOUBLE_TAP_REACH_IN_PIXELS
        });
        self.first_tap = if completes { None } else { Some(tap) };
        completes
    }
}

#[cfg(test)]
#[path = "wake_tests.rs"]
mod wake_tests;
