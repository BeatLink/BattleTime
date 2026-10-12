//! What the vibration motor plays for an app's haptic effects, a notification or a chime.
//!
//! Do not disturb silences everything but the `alarm` pattern, which alarms and timers ring with,
//! and a stop, so a ringing alarm can always be silenced. The embedder turns each `Vibration` into
//! motor steps.

use neotime_runtime::{EffectKind, Value};

/// The longest single buzz `vibrate` may ask for.
pub const LONGEST_BUZZ_IN_MILLISECONDS: u16 = 2000;

// ============================================================================
// Patterns
// ============================================================================

/// The named patterns of `vibrate_pattern`, in the order the compiler numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VibrationPattern {
    Tap,
    DoubleTap,
    Notification,
    /// Rings again and again until stopped, for alarms and timers.
    Alarm,
    Success,
    Failure,
}

impl VibrationPattern {
    const ALL: [VibrationPattern; 6] = [
        VibrationPattern::Tap,
        VibrationPattern::DoubleTap,
        VibrationPattern::Notification,
        VibrationPattern::Alarm,
        VibrationPattern::Success,
        VibrationPattern::Failure,
    ];

    /// The pattern `vibrate_pattern` names by this number.
    pub fn from_number(number: Value) -> Option<VibrationPattern> {
        usize::try_from(number)
            .ok()
            .and_then(|index| Self::ALL.get(index))
            .copied()
    }

    pub fn sounds_in_do_not_disturb(self) -> bool {
        self == VibrationPattern::Alarm
    }
}

// ============================================================================
// Vibrations
// ============================================================================

/// What the motor is asked to do; a new vibration cuts the one playing short.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vibration {
    Buzz { milliseconds: u16 },
    Pattern(VibrationPattern),
    Stop,
}

impl Vibration {
    pub fn sounds_in_do_not_disturb(self) -> bool {
        match self {
            Vibration::Buzz { .. } => false,
            Vibration::Pattern(pattern) => pattern.sounds_in_do_not_disturb(),
            Vibration::Stop => true,
        }
    }

    /// This vibration, or `None` when do not disturb silences it.
    fn unless_silenced(self, is_do_not_disturb: bool) -> Option<Vibration> {
        (!is_do_not_disturb || self.sounds_in_do_not_disturb()).then_some(self)
    }
}

/// What an effect asks of the motor, or `None` for an effect that is not haptic or is silenced.
///
/// `notify` buzzes the notification pattern, as a notification from the phone does.
pub fn vibration_for_effect(
    kind: EffectKind,
    first_argument: Value,
    is_do_not_disturb: bool,
) -> Option<Vibration> {
    let vibration = match kind {
        EffectKind::Vibrate => Vibration::Buzz {
            milliseconds: first_argument.clamp(0, LONGEST_BUZZ_IN_MILLISECONDS as Value) as u16,
        },
        EffectKind::VibratePattern => Vibration::Pattern(VibrationPattern::from_number(first_argument)?),
        EffectKind::StopVibrating => Vibration::Stop,
        EffectKind::Notify => Vibration::Pattern(VibrationPattern::Notification),
        _ => return None,
    };
    vibration.unless_silenced(is_do_not_disturb)
}

/// The buzz for a notification from the phone, or `None` in do not disturb.
pub fn notification_vibration(is_do_not_disturb: bool) -> Option<Vibration> {
    Vibration::Pattern(VibrationPattern::Notification).unless_silenced(is_do_not_disturb)
}

/// Two taps on the hour and one on the half hour, or `None` in do not disturb.
pub fn chime_vibration(is_on_the_hour: bool, is_do_not_disturb: bool) -> Option<Vibration> {
    let pattern = match is_on_the_hour {
        true => VibrationPattern::DoubleTap,
        false => VibrationPattern::Tap,
    };
    Vibration::Pattern(pattern).unless_silenced(is_do_not_disturb)
}

#[cfg(test)]
#[path = "haptics_tests.rs"]
mod haptics_tests;
