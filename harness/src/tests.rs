use std::time::Duration;

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;

use super::*;

// ============================================================================
// A small app to drive
// ============================================================================

/// Counts seconds and taps, shows the step count, and fails on purpose when asked.
const COUNTER_APP: &str = r#"
// A test app: it counts seconds and taps, and its Fail button divides by zero.
import { signal } from "@preact/signals";
import { define_app, every, show_toast, steps_today, vibrate } from "neotime";
import { Button, Label, Screen } from "neotime/widgets";

const seconds = signal(0);
const taps = signal(0);
const divisor = signal(1);

function count_second() {
  seconds.value = seconds.value + 1;
}

function count_tap() {
  taps.value = taps.value + 1;
  show_toast(`Tapped ${taps.value}`);
  vibrate(30);
}

function fail() {
  divisor.value = 0;
  taps.value = Math.trunc(10 / divisor.value);
}

function Counter() {
  return (
    <Screen gap={4}>
      <Label>{`Seconds ${seconds.value}`}</Label>
      <Label>{`Taps ${taps.value}`}</Label>
      <Label>{`Steps ${steps_today.value}`}</Label>
      <Button onClick={count_tap}>Count</Button>
      <Button onClick={fail}>Fail</Button>
    </Screen>
  );
}

export default define_app({
  id: "counter",
  name: "Counter",
  screens: { main: Counter },
  subscriptions: [every({ seconds: 1 }, count_second, { only_while_open: true })],
});
"#;

fn counter() -> TestWatch {
    TestWatch::open_source(COUNTER_APP).unwrap()
}

// ============================================================================
// Opening
// ============================================================================

#[test]
fn an_app_opens_showing_its_first_screen() {
    let watch = counter();

    assert_eq!(watch.id(), "counter");
    assert_eq!(watch.name(), "Counter");
    assert_eq!(watch.now(), START_TIME_IN_MILLISECONDS);
    assert!(watch.is_screen_on());
    assert_eq!(
        watch.visible_texts(),
        ["Seconds 0", "Taps 0", "Steps 4321", "Count", "Fail"]
    );
}

#[test]
fn source_that_does_not_compile_fails_with_the_compilers_diagnostics() {
    let failure = TestWatch::open_source("const broken = ;").err().unwrap();

    let message = failure.to_string();
    assert!(message.starts_with("does not compile"), "{message}");
    assert!(message.contains("app.tsx"), "{message}");
}

#[test]
fn every_package_is_found_with_apps_first() {
    let packages = every_package();

    assert!(packages.iter().any(|package| package.label() == "apps/stopwatch"));
    assert!(packages.iter().any(|package| package.label() == "faces/digital"));
    let first_face = packages
        .iter()
        .position(|package| package.kind == PackageKind::Face)
        .unwrap();
    assert!(packages[first_face..]
        .iter()
        .all(|package| package.kind == PackageKind::Face));
}

#[test]
fn an_open_failure_reads_the_same_when_debug_printed() {
    let failure = OpenFailure::CannotRead("missing".into());

    assert_eq!(format!("{failure:?}"), format!("{failure}"));
    assert_eq!(failure.to_string(), "cannot read the source: missing");
}

// ============================================================================
// Driving
// ============================================================================

#[test]
fn tapping_a_button_by_its_text_runs_its_handler_and_records_its_effects() {
    let mut watch = counter();

    let result = watch.tap_text("Count");

    assert_eq!(result, EventResult::Handled);
    assert!(watch.shows_text("Taps 1"));
    let effects = watch.take_effects();
    assert_eq!(effects.len(), 2);
    assert_eq!(effects[0].kind, EffectKind::ShowToast);
    assert_eq!(effects[0].text(0), Some("Tapped 1"));
    assert_eq!(effects[1].kind, EffectKind::Vibrate);
    assert_eq!(effects[1].number(0), 30);
    assert_eq!(effects[1].text(0), None);
    assert!(watch.effects().is_empty());
}

#[test]
#[should_panic(expected = r#"the screen shows ["Seconds 0", "Taps 0", "Steps 4321", "Count", "Fail"]"#)]
fn tapping_text_that_is_not_shown_fails_listing_what_is() {
    counter().tap_text("Start");
}

#[test]
fn a_failing_handler_is_recorded_and_changes_nothing() {
    let mut watch = counter();

    let result = watch.tap_text("Fail");

    assert_eq!(result, EventResult::Failed);
    assert!(matches!(
        watch.failures(),
        [AppFailure::Handler {
            failure: neotime_runtime::Failure::DivideByZero,
            ..
        }]
    ));
    assert!(watch.shows_text("Taps 0"));
}

#[test]
fn advancing_the_clock_fires_each_timer_on_the_way() {
    let mut watch = counter();

    watch.advance(Duration::from_secs(5));

    assert_eq!(watch.now(), START_TIME_IN_MILLISECONDS + 5_000);
    assert!(watch.shows_text("Seconds 5"), "{:?}", watch.visible_texts());
}

#[test]
fn delivering_steps_updates_the_system_value_the_app_shows() {
    let mut watch = counter();

    watch.deliver(PushedEvent::Steps { steps_today: 5000 });

    assert!(watch.shows_text("Steps 5000"));
}

#[test]
fn setting_a_system_value_redraws_what_shows_it() {
    let mut watch = counter();

    watch.set_system_value(SystemValue::StepsToday, 12);

    assert!(watch.shows_text("Steps 12"));
}

/// Counts the phone connecting and going away through `phone` subscriptions.
const PHONE_LISTENER_APP: &str = r#"
// A test app: it counts how often the phone connects and goes away.
import { signal } from "@preact/signals";
import { define_app, phone } from "neotime";
import { Label, Screen } from "neotime/widgets";

const connects = signal(0);
const disconnects = signal(0);

function count_connect() {
  connects.value = connects.value + 1;
}

function count_disconnect() {
  disconnects.value = disconnects.value + 1;
}

function Listener() {
  return (
    <Screen>
      <Label>{`Connects ${connects.value}`}</Label>
      <Label>{`Disconnects ${disconnects.value}`}</Label>
    </Screen>
  );
}

export default define_app({
  id: "listener",
  name: "Listener",
  screens: { main: Listener },
  subscriptions: [
    phone("connects", count_connect, { only_while_open: true }),
    phone("disconnects", count_disconnect, { only_while_open: true }),
  ],
});
"#;

#[test]
fn setting_is_phone_connected_pushes_the_phone_event() {
    let mut watch = TestWatch::open_source(PHONE_LISTENER_APP).unwrap();

    watch.set_system_value(SystemValue::IsPhoneConnected, 0);
    assert!(watch.shows_text("Disconnects 1"), "{:?}", watch.visible_texts());
    assert!(watch.shows_text("Connects 0"));

    watch.set_system_value(SystemValue::IsPhoneConnected, 1);
    assert!(watch.shows_text("Connects 1"), "{:?}", watch.visible_texts());
    assert!(watch.failures().is_empty());
}

#[test]
fn gestures_nobody_answers_are_left_to_the_system() {
    let mut watch = counter();

    assert_eq!(watch.swipe(Direction::Right), EventResult::NotHandled);
    assert_eq!(watch.press_button(), EventResult::NotHandled);
    assert_eq!(watch.long_press_button(), EventResult::NotHandled);
    assert_eq!(watch.long_press(5, 5), EventResult::NotHandled);
    assert_eq!(watch.double_tap(5, 5), EventResult::NotHandled);
    assert_eq!(watch.drag((5, 5), (100, 5)), EventResult::NotHandled);
    assert!(watch.failures().is_empty());
}

#[test]
fn the_screen_turns_off_and_on() {
    let mut watch = counter();

    watch.screen_off();
    assert!(!watch.is_screen_on());
    watch.screen_on();

    assert!(watch.is_screen_on());
}

#[test]
fn tappable_widgets_are_the_ones_with_click_handlers() {
    let watch = counter();

    let tappable = watch.tappable_widgets();

    assert_eq!(tappable.len(), 2);
    assert!(tappable
        .iter()
        .all(|&widget_index| watch.widget_kind(widget_index) == Some(WidgetKind::Button)));
}

// ============================================================================
// Drawing
// ============================================================================

#[test]
fn a_frame_holds_the_drawn_screen_and_matches_a_fresh_one() {
    let mut watch = counter();
    let background = watch.frame().pixel(0, 0);

    watch.tap_text("Count");
    watch.advance(Duration::from_secs(2));
    let frame = watch.frame().clone();

    assert!(frame.count_of(background) < frame.pixels().len());
    assert_eq!(frame.pixels_drawn_off_screen(), 0);
    assert_eq!(watch.pixels_left_stale(), 0);
}

#[test]
fn the_framebuffer_counts_differences_and_pixels_off_screen() {
    let mut framebuffer = Framebuffer::new();
    let other = Framebuffer::default();

    let Ok(()) = framebuffer.draw_iter([
        Pixel(Point::new(1, 2), Rgb565::RED),
        Pixel(Point::new(-1, 0), Rgb565::RED),
        Pixel(Point::new(0, SCREEN_SIDE_IN_PIXELS as i32), Rgb565::RED),
    ]);

    assert_eq!(framebuffer.pixel(1, 2), Rgb565::RED);
    assert_eq!(framebuffer.count_of(Rgb565::RED), 1);
    assert_eq!(framebuffer.differing_pixels(&other), 1);
    assert_eq!(framebuffer.pixels_drawn_off_screen(), 2);
}

// ============================================================================
// The fake system
// ============================================================================

#[test]
fn random_numbers_are_the_same_on_every_run() {
    use neotime_runtime::SystemValues;
    let first = FakeSystem::new();
    let second = FakeSystem::default();

    let first_numbers: Vec<u32> = (0..4).map(|_| first.random_number()).collect();
    let second_numbers: Vec<u32> = (0..4).map(|_| second.random_number()).collect();

    assert_eq!(first_numbers, second_numbers);
    assert_ne!(first_numbers[0], first_numbers[1]);
}

#[test]
fn the_fake_system_answers_lists_and_text() {
    use neotime_runtime::{SystemValues, NONE};
    let mut system = FakeSystem::new();
    system.installed_apps.push(InstalledApp {
        name: "Timer".into(),
        id: "timer".into(),
    });
    system.set_text(SystemValue::MusicTrack, "Song");

    assert_eq!(system.read(SystemValue::InstalledAppCount), 1);
    assert_eq!(
        system.read_element_text(SystemValue::InstalledAppName, 0),
        "Timer"
    );
    assert_eq!(system.read_element_text(SystemValue::InstalledAppId, 0), "timer");
    assert_eq!(system.read_element_text(SystemValue::InstalledAppId, 1), "");
    assert_eq!(system.read_element_text(SystemValue::MusicTrack, 0), "");
    assert_ne!(system.read_element(SystemValue::InstalledAppIcon, 0), NONE);
    assert_eq!(system.read_element(SystemValue::InstalledAppIcon, 1), NONE);
    assert_eq!(system.read_element(SystemValue::InstalledAppIcon, -1), NONE);
    assert_eq!(system.read(SystemValue::WeatherForecastLength), 3);
    assert_eq!(system.read_element(SystemValue::WeatherForecastHigh, 0), 17);
    assert_eq!(system.read_element(SystemValue::WeatherForecastLow, 1), 7);
    assert_eq!(system.read_element(SystemValue::WeatherForecastCondition, 2), 0);
    assert_eq!(
        system.read_element(SystemValue::WeatherForecastCondition, 3),
        NONE
    );
    assert_eq!(system.read_element(SystemValue::BatteryPercent, 0), NONE);
    assert_eq!(system.read_text(SystemValue::MusicTrack), "Song");
}

#[test]
fn installed_apps_are_compiled_and_listed_by_name_and_id() {
    let mut watch = TestWatch::open_app("launcher").unwrap();

    watch.install_apps(&["timer"]).unwrap();

    assert_eq!(
        watch.system().installed_apps,
        [InstalledApp {
            name: "Timer".into(),
            id: "timer".into(),
        }]
    );
    assert!(watch.shows_text("Timer"));
}

// ============================================================================
// Snapshots
// ============================================================================

/// Snapshot folders of a test's own under target/, so a check never touches the committed files.
fn scratch_snapshot_folders(test_name: &str, is_updating: bool) -> SnapshotFolders {
    let root = repository_root()
        .join("target/harness-snapshot-tests")
        .join(test_name);
    let _ = std::fs::remove_dir_all(&root);
    SnapshotFolders {
        texts: root.join("texts"),
        screens: root.join("screens"),
        failures: root.join("failures"),
        is_updating,
    }
}

#[test]
fn screen_texts_are_written_sorted_one_to_a_line_with_json_escapes() {
    let screens = [
        ScreenTexts {
            name: "main".into(),
            reached_by: None,
            texts: vec!["b".into(), "a \"quoted\"\\\n\t\u{1}".into()],
        },
        ScreenTexts {
            name: "empty".into(),
            reached_by: Some("swipe up".into()),
            texts: Vec::new(),
        },
    ];

    let json = screen_texts_json(&screens);

    assert_eq!(
        json,
        "{\n  \"screens\": [\n    {\n      \"name\": \"main\",\n      \"reached_by\": null,\n      \"texts\": [\n        \"a \\\"quoted\\\"\\\\\\n\\t\\u0001\",\n        \"b\"\n      ]\n    },\n    {\n      \"name\": \"empty\",\n      \"reached_by\": \"swipe up\",\n      \"texts\": []\n    }\n  ]\n}\n"
    );
    assert_eq!(screen_texts_json(&[]), "{\n  \"screens\": []\n}\n");
}

#[test]
fn a_line_difference_marks_lines_gone_and_lines_new() {
    let difference = line_difference("one\ntwo\nthree\n", "one\n2\nthree\nfour\n");

    assert_eq!(difference, "- two\n+ 2\n+ four\n");
    assert_eq!(line_difference("same\n", "same\n"), "");
}

#[test]
fn a_frame_comes_back_unchanged_from_a_png() {
    let mut frame = Framebuffer::new();
    Pixel(Point::new(3, 4), Rgb565::new(31, 63, 31))
        .draw(&mut frame)
        .unwrap();
    Pixel(Point::new(5, 6), Rgb565::new(1, 2, 3))
        .draw(&mut frame)
        .unwrap();
    let pixels = frame_to_rgb_bytes(&frame);

    let decoded = decode_png(&encode_png(&pixels).unwrap()).unwrap();

    assert_eq!(decoded, pixels);
    assert_eq!(&pixels[(4 * 240 + 3) * 3..][..3], &[255, 255, 255]);
    assert!(decode_png(b"not a png").is_err());
}

#[test]
fn a_difference_picture_paints_and_counts_only_the_changed_pixels() {
    let expected = [200, 100, 0, 10, 20, 30];
    let actual = [200, 100, 0, 11, 20, 30];

    let (picture, changed_pixels) = difference_picture(&expected, &actual);

    assert_eq!(changed_pixels, 1);
    assert_eq!(picture, [50, 25, 0, 255, 0, 255]);
}

#[test]
fn a_text_snapshot_is_written_when_updating_and_then_matches() {
    let updating = scratch_snapshot_folders("text-update", true);
    updating.check_text("app-example", "{}\n").unwrap();
    let checking = SnapshotFolders {
        is_updating: false,
        ..updating
    };

    assert_eq!(checking.check_text("app-example", "{}\n"), Ok(()));
    let problem = checking.check_text("app-example", "{ }\n").unwrap_err();
    assert!(problem.contains("- {}\n+ { }"), "{problem}");
}

#[test]
fn a_changed_screen_leaves_the_actual_and_difference_pictures_behind() {
    let updating = scratch_snapshot_folders("screen-change", true);
    let mut frame = Framebuffer::new();
    updating.check_screen("app-example", &frame).unwrap();
    let checking = SnapshotFolders {
        is_updating: false,
        ..updating
    };
    assert_eq!(checking.check_screen("app-example", &frame), Ok(()));

    Pixel(Point::new(0, 0), Rgb565::RED).draw(&mut frame).unwrap();
    let problem = checking.check_screen("app-example", &frame).unwrap_err();

    assert!(problem.contains("1 pixels differ"), "{problem}");
    let difference = std::fs::read(checking.failures.join("app-example.difference.png")).unwrap();
    assert_eq!(&decode_png(&difference).unwrap()[..3], &[255, 0, 255]);
    let actual = std::fs::read(checking.failures.join("app-example.actual.png")).unwrap();
    assert_eq!(&decode_png(&actual).unwrap()[..3], &[255, 0, 0]);
}

#[test]
fn a_screen_that_matches_again_clears_the_pictures_of_its_last_failure() {
    let updating = scratch_snapshot_folders("screen-recovers", true);
    let frame = Framebuffer::new();
    updating.check_screen("app-example", &frame).unwrap();
    let checking = SnapshotFolders {
        is_updating: false,
        ..updating
    };
    std::fs::create_dir_all(&checking.failures).unwrap();
    std::fs::write(checking.failures.join("app-example.actual.png"), b"old").unwrap();

    checking.check_screen("app-example", &frame).unwrap();

    assert!(!checking.failures.join("app-example.actual.png").exists());
}

#[test]
fn a_missing_reference_picture_fails_and_leaves_the_drawn_screen() {
    let checking = scratch_snapshot_folders("screen-missing", false);

    let problem = checking
        .check_screen("app-missing", &Framebuffer::new())
        .unwrap_err();

    assert!(problem.contains("cannot read"), "{problem}");
    assert!(checking.failures.join("app-missing.actual.png").is_file());
}
