use super::*;

// ============================================================================
// Effects
// ============================================================================

#[test]
fn vibrate_buzzes_for_the_time_asked_within_two_seconds() {
    assert_eq!(
        vibration_for_effect(EffectKind::Vibrate, 30, false),
        Some(Vibration::Buzz { milliseconds: 30 })
    );
    assert_eq!(
        vibration_for_effect(EffectKind::Vibrate, 10_000, false),
        Some(Vibration::Buzz { milliseconds: 2000 })
    );
    assert_eq!(
        vibration_for_effect(EffectKind::Vibrate, -5, false),
        Some(Vibration::Buzz { milliseconds: 0 })
    );
}

#[test]
fn vibrate_pattern_plays_the_pattern_of_its_number() {
    assert_eq!(
        vibration_for_effect(EffectKind::VibratePattern, 0, false),
        Some(Vibration::Pattern(VibrationPattern::Tap))
    );
    assert_eq!(
        vibration_for_effect(EffectKind::VibratePattern, 3, false),
        Some(Vibration::Pattern(VibrationPattern::Alarm))
    );
    assert_eq!(
        vibration_for_effect(EffectKind::VibratePattern, 5, false),
        Some(Vibration::Pattern(VibrationPattern::Failure))
    );
    assert_eq!(vibration_for_effect(EffectKind::VibratePattern, 6, false), None);
    assert_eq!(vibration_for_effect(EffectKind::VibratePattern, -1, false), None);
}

#[test]
fn notify_and_stop_vibrating_reach_the_motor_and_other_effects_do_not() {
    assert_eq!(
        vibration_for_effect(EffectKind::Notify, 0, false),
        Some(Vibration::Pattern(VibrationPattern::Notification))
    );
    assert_eq!(
        vibration_for_effect(EffectKind::StopVibrating, 0, false),
        Some(Vibration::Stop)
    );
    assert_eq!(vibration_for_effect(EffectKind::KeepScreenOn, 1, false), None);
}

// ============================================================================
// Do not disturb
// ============================================================================

#[test]
fn do_not_disturb_silences_everything_but_the_alarm_and_a_stop() {
    assert_eq!(vibration_for_effect(EffectKind::Vibrate, 30, true), None);
    assert_eq!(vibration_for_effect(EffectKind::VibratePattern, 2, true), None);
    assert_eq!(vibration_for_effect(EffectKind::Notify, 0, true), None);
    assert_eq!(
        vibration_for_effect(EffectKind::VibratePattern, 3, true),
        Some(Vibration::Pattern(VibrationPattern::Alarm))
    );
    assert_eq!(
        vibration_for_effect(EffectKind::StopVibrating, 0, true),
        Some(Vibration::Stop)
    );
}

#[test]
fn a_phone_notification_buzzes_unless_do_not_disturb_is_on() {
    assert_eq!(
        notification_vibration(false),
        Some(Vibration::Pattern(VibrationPattern::Notification))
    );
    assert_eq!(notification_vibration(true), None);
}

#[test]
fn chimes_tap_twice_on_the_hour_once_on_the_half_hour_and_never_in_do_not_disturb() {
    assert_eq!(
        chime_vibration(true, false),
        Some(Vibration::Pattern(VibrationPattern::DoubleTap))
    );
    assert_eq!(
        chime_vibration(false, false),
        Some(Vibration::Pattern(VibrationPattern::Tap))
    );
    assert_eq!(chime_vibration(true, true), None);
}
