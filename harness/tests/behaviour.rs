//! What some apps and faces do, checked through the test watch the way a wearer would use them.

use std::time::Duration;

use neotime_app_testing::{Direction, EffectKind, EventResult, SystemValue, TestWatch};

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
// Going back
// ============================================================================

#[test]
fn the_button_returns_from_a_settings_screen_to_the_list_and_then_leaves() {
    let mut watch = TestWatch::open_app("settings").unwrap();
    watch.tap_text("Brightness");
    assert_eq!(watch.screen_name(), Some("brightness"));

    assert_eq!(watch.press_button(), EventResult::Handled);
    assert_eq!(watch.screen_name(), Some("main"));

    assert_eq!(watch.press_button(), EventResult::NotHandled);
}

#[test]
fn a_right_swipe_returns_to_the_previous_screen() {
    let mut watch = TestWatch::open_app("settings").unwrap();
    watch.tap_text("Brightness");

    assert_eq!(watch.swipe(Direction::Right), EventResult::Handled);

    assert_eq!(watch.screen_name(), Some("main"));
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
// System info
// ============================================================================

#[test]
fn system_info_shows_a_new_firmware_version_and_a_dirty_tree() {
    let mut watch = TestWatch::open_app("system-info").unwrap();
    assert!(watch.shows_text("0.1.0+1b19aed"), "{:?}", watch.visible_texts());

    watch.set_system_text(SystemValue::FirmwareVersion, "0.2.0+0a1b2c3*");

    assert!(watch.shows_text("0.2.0+0a1b2c3*"), "{:?}", watch.visible_texts());
    assert!(!watch.shows_text("0.1.0+1b19aed"));
    assert_eq!(watch.pixels_left_stale(), 0);
    assert!(watch.failures().is_empty());
}

#[test]
fn system_info_says_the_last_run_crashed() {
    let mut watch = TestWatch::open_app("system-info").unwrap();
    assert!(watch.shows_text("watchdog"), "{:?}", watch.visible_texts());

    watch.set_system_text(SystemValue::ResetReason, "Fault at 0x0001a2b4");

    assert!(
        watch.shows_text("Fault at 0x0001a2b4"),
        "{:?}",
        watch.visible_texts()
    );
    assert!(watch.failures().is_empty());
}

#[test]
fn system_info_counts_uptime_from_a_start_the_phone_moved() {
    let mut watch = TestWatch::open_app("system-info").unwrap();
    let now = watch.now();

    watch.set_system_value(SystemValue::StartedAt, now - 2 * 24 * 3_600_000 - 61_000);

    assert!(watch.shows_text("2d 00:01:01"), "{:?}", watch.visible_texts());
}

// ============================================================================
// Faces
// ============================================================================

#[test]
fn the_digital_face_shows_the_date() {
    let watch = TestWatch::open_face("digital").unwrap();

    assert!(watch.shows_text("Mon 5 Jan 2026"), "{:?}", watch.visible_texts());
}

// ============================================================================
// Notifications
// ============================================================================

const NOTIFYING_APP: &str = r#"
import { define_app, notify, unread_notifications } from "neotime";
import { Label, Screen } from "neotime/widgets";
function Main() {
  return <Screen onClick={() => notify("Done", "Your timer ended")}><Label>{`${unread_notifications.value} unread`}</Label></Screen>;
}
export default define_app({ id: "notifier", name: "Notifier", permissions: ["notifications"], screens: { main: Main } });
"#;

#[test]
fn an_apps_notification_is_kept_under_its_name_and_counted_unread() {
    let mut watch = TestWatch::open_source(NOTIFYING_APP).unwrap();
    assert!(watch.shows_text("0 unread"));

    watch.tap(120, 120);

    assert!(watch.shows_text("1 unread"), "{:?}", watch.visible_texts());
    let kept = watch.system().notifications.get(0).unwrap();
    assert_eq!(kept.source, "Notifier");
    assert_eq!(kept.title, "Done");
    assert_eq!(kept.body, "Your timer ended");
}

#[test]
fn opening_the_notifications_app_marks_every_notification_read() {
    let mut watch = TestWatch::open_app("notifications").unwrap();
    watch.screen_off();
    watch.deliver(neotime_app_testing::PushedEvent::Notification {
        id: 5,
        app: "Messages",
        title: "Sam",
        body: "On my way",
        time: watch.now(),
    });
    assert_eq!(watch.system().notifications.unread_count(), 1);

    watch.screen_on();

    assert_eq!(watch.system().notifications.unread_count(), 0);
    assert!(watch.shows_text("Sam"));
    assert!(watch.failures().is_empty());
}

// ============================================================================
// Notes
// ============================================================================

mod notes {
    use super::*;
    use neotime_app_testing::{compile_package, FakeSystem, PackageKind, STARTING_NOTE_PATH};

    fn open_with_note(note: &str, font: Option<&str>) -> TestWatch {
        let mut system = FakeSystem::new();
        system.write_file(STARTING_NOTE_PATH, note.as_bytes());
        if let Some(font) = font {
            system.write_file("/notes/font", font.as_bytes());
        }
        let image = compile_package(PackageKind::App, "notes").unwrap();
        TestWatch::open_image_on(image, system).unwrap()
    }

    #[test]
    fn the_notes_app_shows_the_note_the_phone_sent() {
        let watch = TestWatch::open_app("notes").unwrap();

        assert!(watch.shows_text("Packing list"), "{:?}", watch.visible_texts());
        assert!(watch.shows_text("- Passport"));
        assert!(watch.failures().is_empty());
    }

    #[test]
    fn the_notes_app_scrolls_a_long_note_to_its_end() {
        let note: String = (1..=300).map(|line| format!("Line {line}\n")).collect();
        let mut watch = open_with_note(&note, Some("small"));
        assert!(watch.shows_text("Line 1"));
        assert!(!watch.shows_text("Line 300"));

        for _ in 0..120 {
            watch.drag((120, 230), (120, 10));
        }

        assert!(watch.shows_text("Line 300"), "{:?}", watch.visible_texts());
        assert!(watch.failures().is_empty());
    }

    #[test]
    fn the_notes_app_shows_fewer_lines_in_a_larger_font() {
        let note: String = (1..=40).map(|line| format!("Line {line}\n")).collect();
        let small = open_with_note(&note, Some("small")).visible_texts().len();
        let large = open_with_note(&note, Some("large")).visible_texts().len();

        assert!(large < small, "large {large}, small {small}");
    }

    #[test]
    fn the_notes_app_asks_for_a_note_while_there_is_none() {
        let mut system = FakeSystem::new();
        system.files.clear();
        let image = compile_package(PackageKind::App, "notes").unwrap();
        let watch = TestWatch::open_image_on(image, system).unwrap();

        assert!(watch.shows_text("No note yet."), "{:?}", watch.visible_texts());
    }
}
