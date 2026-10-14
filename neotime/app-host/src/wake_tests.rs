use neotime_settings::{shake_threshold, Setting, Settings};

use super::*;

const LIT: ScreenState = ScreenState {
    is_lit: true,
    is_held_on: false,
};

const DARK: ScreenState = ScreenState {
    is_lit: false,
    is_held_on: false,
};

fn with_every_gesture_on() -> Settings {
    let mut settings = Settings::default();
    for setting in [Setting::RaiseToWake, Setting::LowerToSleep, Setting::ShakeToWake] {
        settings.set(setting, 1).unwrap();
    }
    settings
}

// ============================================================================
// Wrist gestures
// ============================================================================

#[test]
fn nothing_is_watched_for_until_a_gesture_is_turned_on() {
    let settings = Settings::default();

    assert_eq!(
        gestures_to_watch(&settings, DARK),
        WakeGestures {
            shake_threshold: shake_threshold::DEFAULT,
            ..WakeGestures::default()
        }
    );
}

#[test]
fn a_dark_screen_watches_for_a_raise_and_a_shake() {
    let watched = gestures_to_watch(&with_every_gesture_on(), DARK);

    assert!(watched.raise);
    assert!(watched.shake);
    assert!(!watched.lower);
}

#[test]
fn a_lit_screen_watches_only_for_a_lowered_arm() {
    let watched = gestures_to_watch(&with_every_gesture_on(), LIT);

    assert!(watched.lower);
    assert!(!watched.raise);
    assert!(!watched.shake);
}

#[test]
fn a_screen_held_on_is_never_lowered_to_sleep() {
    let held_on = ScreenState {
        is_lit: true,
        is_held_on: true,
    };

    assert!(!gestures_to_watch(&with_every_gesture_on(), held_on).lower);
}

#[test]
fn sleep_mode_watches_for_nothing() {
    let mut settings = with_every_gesture_on();
    settings.set(Setting::SleepMode, 1).unwrap();

    assert_eq!(gestures_to_watch(&settings, DARK), WakeGestures::default());
    assert_eq!(gestures_to_watch(&settings, LIT), WakeGestures::default());
}

#[test]
fn the_shake_threshold_comes_from_the_settings() {
    let mut settings = with_every_gesture_on();
    settings.set(Setting::ShakeThreshold, 900).unwrap();

    assert_eq!(gestures_to_watch(&settings, DARK).shake_threshold, 900);
}

// ============================================================================
// The face's double tap
// ============================================================================

fn click(x: i32, y: i32) -> Touch {
    Touch::Click { x, y }
}

#[test]
fn two_taps_close_together_make_a_double_tap() {
    let mut finder = DoubleTapFinder::default();

    assert!(!finder.touch(&click(100, 100), false, 0));
    assert!(finder.touch(&click(110, 95), false, 300));
}

#[test]
fn taps_too_far_apart_in_time_or_place_do_not() {
    let mut finder = DoubleTapFinder::default();

    finder.touch(&click(100, 100), false, 0);
    assert!(!finder.touch(&click(100, 100), false, 501));
    assert!(!finder.touch(&click(200, 100), false, 600));
    assert!(!finder.touch(&click(200, 200), false, 700));
}

#[test]
fn a_double_tap_starts_afresh_after_it_is_found() {
    let mut finder = DoubleTapFinder::default();

    finder.touch(&click(100, 100), false, 0);
    assert!(finder.touch(&click(100, 100), false, 200));
    assert!(!finder.touch(&click(100, 100), false, 400));
}

#[test]
fn presses_and_lifts_between_the_taps_are_ignored() {
    let mut finder = DoubleTapFinder::default();

    finder.touch(&click(100, 100), false, 0);
    finder.touch(&Touch::Release { x: 100, y: 100 }, false, 50);
    finder.touch(&Touch::Press { x: 100, y: 100 }, false, 200);

    assert!(finder.touch(&click(100, 100), false, 250));
}

#[test]
fn a_swipe_or_a_tap_the_face_took_breaks_a_double_tap() {
    let mut finder = DoubleTapFinder::default();

    finder.touch(&click(100, 100), false, 0);
    finder.touch(
        &Touch::Swipe(neotime_runtime::events::SwipeDirection::Up),
        false,
        100,
    );
    assert!(!finder.touch(&click(100, 100), false, 200));

    finder.touch(&click(100, 100), true, 1000);
    assert!(!finder.touch(&click(100, 100), false, 1100));
}

#[test]
fn a_double_tap_from_the_panel_counts_unless_the_face_took_it() {
    let mut finder = DoubleTapFinder::default();
    let double_tap = Touch::DoubleClick { x: 120, y: 120 };

    assert!(finder.touch(&double_tap, false, 0));
    assert!(!finder.touch(&double_tap, true, 100));
}
