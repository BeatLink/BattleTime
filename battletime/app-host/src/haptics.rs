//! What the vibration motor plays for an app's haptic effects, a notification, a chime or the phone.
//!
//! Do not disturb silences everything but the `alarm` pattern, which alarms and timers ring with,
//! and a stop, so a ringing alarm can always be silenced. The wearer can have it silence alarms too.
//! The embedder turns each `Vibration` into motor steps.

use battletime_runtime::{EffectKind, Value};

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
    Buzz {
        milliseconds: u16,
    },
    Pattern(VibrationPattern),
    Buzzes {
        count: u8,
    },
    /// Rings for an incoming call until it is stopped or two minutes pass.
    CallRing,
    Stop,
}

impl Vibration {
    pub fn sounds_in_do_not_disturb(self) -> bool {
        match self {
            Vibration::Buzz { .. } | Vibration::Buzzes { .. } | Vibration::CallRing => false,
            Vibration::Pattern(pattern) => pattern.sounds_in_do_not_disturb(),
            Vibration::Stop => true,
        }
    }

    /// This vibration, or `None` when do not disturb silences it.
    fn unless_silenced(self, is_do_not_disturb: bool) -> Option<Vibration> {
        (!is_do_not_disturb || self.sounds_in_do_not_disturb()).then_some(self)
    }
}

/// How much of what apps ask the motor for do not disturb silences.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Silence {
    Nothing,
    AllButAlarms,
    /// Everything but a stop.
    Everything,
}

impl Silence {
    pub fn from_settings(is_do_not_disturb: bool, alarms_ring_in_do_not_disturb: bool) -> Silence {
        match (is_do_not_disturb, alarms_ring_in_do_not_disturb) {
            (false, _) => Silence::Nothing,
            (true, true) => Silence::AllButAlarms,
            (true, false) => Silence::Everything,
        }
    }
}

/// What an effect asks of the motor, or `None` for an effect that is not haptic or is silenced.
///
/// `notify` buzzes the notification pattern, as a notification from the phone does.
pub fn vibration_for_effect(kind: EffectKind, first_argument: Value, silence: Silence) -> Option<Vibration> {
    let vibration = match kind {
        EffectKind::Vibrate => Vibration::Buzz {
            milliseconds: first_argument.clamp(0, LONGEST_BUZZ_IN_MILLISECONDS as Value) as u16,
        },
        EffectKind::VibratePattern => Vibration::Pattern(VibrationPattern::from_number(first_argument)?),
        EffectKind::StopVibrating => Vibration::Stop,
        EffectKind::Notify => Vibration::Pattern(VibrationPattern::Notification),
        _ => return None,
    };
    match silence {
        Silence::Nothing => Some(vibration),
        Silence::AllButAlarms => vibration.unless_silenced(true),
        Silence::Everything => (vibration == Vibration::Stop).then_some(vibration),
    }
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

// ============================================================================
// The phone
// ============================================================================

/// The ring that helps the wearer find the watch, or a stop; it sounds even in do not disturb.
pub fn find_vibration(is_on: bool) -> Vibration {
    match is_on {
        true => Vibration::Pattern(VibrationPattern::Alarm),
        false => Vibration::Stop,
    }
}

/// The buzzes the phone's `vibrate` asks for, or `None` in do not disturb.
pub fn phone_buzzes_vibration(count: u8, is_do_not_disturb: bool) -> Option<Vibration> {
    Vibration::Buzzes { count }.unless_silenced(is_do_not_disturb)
}

/// The ring for an incoming call, or `None` in do not disturb; an answered or ended call stops the ring if it still plays.
pub fn call_vibration(
    kind: battletime_companion::CallKind,
    is_do_not_disturb: bool,
    is_ringing: bool,
) -> Option<Vibration> {
    match kind {
        battletime_companion::CallKind::Incoming => Vibration::CallRing.unless_silenced(is_do_not_disturb),
        battletime_companion::CallKind::Answered | battletime_companion::CallKind::Ended => {
            is_ringing.then_some(Vibration::Stop)
        }
    }
}

#[cfg(test)]
#[path = "haptics_tests.rs"]
mod haptics_tests;
