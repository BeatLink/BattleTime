use super::*;

// ============================================================================
// Effects
// ============================================================================

#[test]
fn vibrate_buzzes_for_the_time_asked_within_two_seconds() {
    assert_eq!(
        vibration_for_effect(EffectKind::Vibrate, 30, Silence::Nothing),
        Some(Vibration::Buzz { milliseconds: 30 })
    );
    assert_eq!(
        vibration_for_effect(EffectKind::Vibrate, 10_000, Silence::Nothing),
        Some(Vibration::Buzz { milliseconds: 2000 })
    );
    assert_eq!(
        vibration_for_effect(EffectKind::Vibrate, -5, Silence::Nothing),
        Some(Vibration::Buzz { milliseconds: 0 })
    );
}

#[test]
fn vibrate_pattern_plays_the_pattern_of_its_number() {
    assert_eq!(
        vibration_for_effect(EffectKind::VibratePattern, 0, Silence::Nothing),
        Some(Vibration::Pattern(VibrationPattern::Tap))
    );
    assert_eq!(
        vibration_for_effect(EffectKind::VibratePattern, 3, Silence::Nothing),
        Some(Vibration::Pattern(VibrationPattern::Alarm))
    );
    assert_eq!(
        vibration_for_effect(EffectKind::VibratePattern, 5, Silence::Nothing),
        Some(Vibration::Pattern(VibrationPattern::Failure))
    );
    assert_eq!(
        vibration_for_effect(EffectKind::VibratePattern, 6, Silence::Nothing),
        None
    );
    assert_eq!(
        vibration_for_effect(EffectKind::VibratePattern, -1, Silence::Nothing),
        None
    );
}

#[test]
fn notify_and_stop_vibrating_reach_the_motor_and_other_effects_do_not() {
    assert_eq!(
        vibration_for_effect(EffectKind::Notify, 0, Silence::Nothing),
        Some(Vibration::Pattern(VibrationPattern::Notification))
    );
    assert_eq!(
        vibration_for_effect(EffectKind::StopVibrating, 0, Silence::Nothing),
        Some(Vibration::Stop)
    );
    assert_eq!(
        vibration_for_effect(EffectKind::KeepScreenOn, 1, Silence::Nothing),
        None
    );
}

// ============================================================================
// Do not disturb
// ============================================================================

#[test]
fn do_not_disturb_silences_everything_but_the_alarm_and_a_stop() {
    assert_eq!(
        vibration_for_effect(EffectKind::Vibrate, 30, Silence::AllButAlarms),
        None
    );
    assert_eq!(
        vibration_for_effect(EffectKind::VibratePattern, 2, Silence::AllButAlarms),
        None
    );
    assert_eq!(
        vibration_for_effect(EffectKind::Notify, 0, Silence::AllButAlarms),
        None
    );
    assert_eq!(
        vibration_for_effect(EffectKind::VibratePattern, 3, Silence::AllButAlarms),
        Some(Vibration::Pattern(VibrationPattern::Alarm))
    );
    assert_eq!(
        vibration_for_effect(EffectKind::StopVibrating, 0, Silence::AllButAlarms),
        Some(Vibration::Stop)
    );
}

#[test]
fn do_not_disturb_can_silence_alarms_too_but_never_a_stop() {
    assert_eq!(
        vibration_for_effect(EffectKind::VibratePattern, 3, Silence::Everything),
        None
    );
    assert_eq!(
        vibration_for_effect(EffectKind::Vibrate, 30, Silence::Everything),
        None
    );
    assert_eq!(
        vibration_for_effect(EffectKind::StopVibrating, 0, Silence::Everything),
        Some(Vibration::Stop)
    );
}

#[test]
fn the_settings_choose_how_much_is_silenced() {
    assert_eq!(Silence::from_settings(false, false), Silence::Nothing);
    assert_eq!(Silence::from_settings(false, true), Silence::Nothing);
    assert_eq!(Silence::from_settings(true, true), Silence::AllButAlarms);
    assert_eq!(Silence::from_settings(true, false), Silence::Everything);
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

// ============================================================================
// The phone
// ============================================================================

#[test]
fn finding_the_watch_rings_even_in_do_not_disturb_and_stops_on_request() {
    let ring = find_vibration(true);
    assert_eq!(ring, Vibration::Pattern(VibrationPattern::Alarm));
    assert!(ring.sounds_in_do_not_disturb());
    assert_eq!(find_vibration(false), Vibration::Stop);
}

#[test]
fn the_phones_buzzes_are_silenced_by_do_not_disturb() {
    assert_eq!(
        phone_buzzes_vibration(3, false),
        Some(Vibration::Buzzes { count: 3 })
    );
    assert_eq!(phone_buzzes_vibration(3, true), None);
}

#[test]
fn only_an_incoming_call_buzzes() {
    use neotime_companion::CallKind;
    assert_eq!(
        call_vibration(CallKind::Incoming, false),
        Some(Vibration::Pattern(VibrationPattern::Notification))
    );
    assert_eq!(call_vibration(CallKind::Incoming, true), None);
    assert_eq!(call_vibration(CallKind::Answered, false), None);
    assert_eq!(call_vibration(CallKind::Ended, false), None);
}
