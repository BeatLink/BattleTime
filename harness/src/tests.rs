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
