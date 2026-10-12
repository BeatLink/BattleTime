//! What some apps and faces do, checked through the test watch the way a wearer would use them.

use std::time::Duration;

use neotime_app_testing::{Direction, EffectKind, EventResult, TestWatch};

// ============================================================================
// Named numbers
// ============================================================================

/// `vibrate_pattern("alarm")`, as the compiler numbers the patterns.
const ALARM_PATTERN: i64 = 3;

// ============================================================================
// Stopwatch
// ============================================================================

#[test]
fn the_stopwatch_counts_up_after_starting() {
    let mut watch = TestWatch::open_app("stopwatch").unwrap();
    assert!(watch.shows_text("00:00"));

    watch.press_button();
    watch.advance(Duration::from_secs(3));

    assert!(watch.shows_text("00:03"), "{:?}", watch.visible_texts());
    let keep_screen_on = watch.effects_of(EffectKind::KeepScreenOn);
    assert_eq!(keep_screen_on.last().unwrap().number(0), 1);
    assert!(watch.failures().is_empty());
}

#[test]
fn the_stopwatch_holds_its_time_while_paused() {
    let mut watch = TestWatch::open_app("stopwatch").unwrap();
    watch.press_button();
    watch.advance(Duration::from_secs(7));

    watch.press_button();
    watch.advance(Duration::from_secs(60));

    assert!(watch.shows_text("00:07"), "{:?}", watch.visible_texts());
}

// ============================================================================
// Timer
// ============================================================================

#[test]
fn the_timer_rings_when_it_finishes_and_a_dismiss_silences_it() {
    let mut watch = TestWatch::open_app("timer").unwrap();
    let start_button = *watch.tappable_widgets().last().unwrap();
    assert_eq!(watch.tap_widget(start_button), EventResult::Handled);

    watch.advance(Duration::from_secs(10 * 60));

    let alarms = watch.effects_of(EffectKind::VibratePattern);
    assert_eq!(alarms.len(), 1);
    assert_eq!(alarms[0].number(0), ALARM_PATTERN);
    assert_eq!(watch.effects_of(EffectKind::WakeScreen).len(), 1);
    assert!(watch.shows_text("Time's up"));

    watch.tap_text("Dismiss");

    assert!(!watch.effects_of(EffectKind::StopVibrating).is_empty());
    assert!(!watch.shows_text("Time's up"));
    assert!(watch.failures().is_empty());
}

// ============================================================================
// Launcher
// ============================================================================

#[test]
fn the_launcher_lists_the_installed_apps_and_opens_the_one_tapped() {
    let mut watch = TestWatch::open_app("launcher").unwrap();
    watch.install_apps(&["stopwatch", "timer"]).unwrap();
    assert!(watch.shows_text("Stopwatch"));

    watch.tap_text("Timer");

    let opened = watch.effects_of(EffectKind::OpenApp);
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].text(0), Some("timer"));
}

// ============================================================================
// Template
// ============================================================================

#[test]
fn the_template_shows_the_last_event_it_received() {
    let mut watch = TestWatch::open_app("template").unwrap();

    watch.tap(100, 150);
    assert!(watch.shows_text("(100, 150)"), "{:?}", watch.visible_texts());

    watch.swipe(Direction::Up);
    assert!(watch.shows_text("Swipe up"));

    watch.press_button();
    assert!(watch.shows_text("Button"));
}

#[test]
fn the_template_leaves_on_a_long_press_of_the_button() {
    let mut watch = TestWatch::open_app("template").unwrap();

    watch.long_press_button();

    assert_eq!(watch.effects_of(EffectKind::Back).len(), 1);
}

// ============================================================================
// Faces
// ============================================================================

#[test]
fn the_digital_face_shows_the_date() {
    let watch = TestWatch::open_face("digital").unwrap();

    assert!(watch.shows_text("Mon 5 Jan 2026"), "{:?}", watch.visible_texts());
}
