//! Opens every app and face on the test watch, pokes at everything it shows, and checks that none
//! of its code fails, as wasp-os's `make check` does for its apps.

use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;

use battletime_app_testing::{
    every_package, BatteryChange, CallKind, Direction, MotionKind, Package, PushedEvent, TestWatch,
    WidgetKind,
};

// ============================================================================
// Poking at one package
// ============================================================================

/// The apps the launcher lists while it is tested.
const LAUNCHER_LISTS: [&str; 2] = ["stopwatch", "timer"];

/// How many times the tappable widgets are tapped again, to reach the screens a tap opens.
const TAP_ROUNDS: usize = 3;

/// How many points across a keypad or grid of cells is tapped at, so most of its keys are tried.
const GRID_POINTS_ACROSS: i32 = 5;

const SCREEN_MIDDLE: i32 = 120;

/// How many points across the whole screen is tapped at, a few times over, to reach whatever answers taps.
const SCREEN_POINTS_ACROSS: i32 = 6;

const TEN_MINUTES: Duration = Duration::from_secs(10 * 60);

const ONE_MINUTE: Duration = Duration::from_secs(60);

/// One of every event the phone and sensors push, besides the notification.
fn every_other_event() -> Vec<PushedEvent<'static>> {
    vec![
        PushedEvent::HeartRate {
            beats_per_minute: 72,
            confidence: 90,
        },
        PushedEvent::Steps { steps_today: 5000 },
        PushedEvent::StepGoal { steps_today: 10_000 },
        PushedEvent::Motion(MotionKind::Shake),
        PushedEvent::Motion(MotionKind::WristRaise),
        PushedEvent::Acceleration {
            x: 100,
            y: -200,
            z: 980,
        },
        PushedEvent::Battery {
            change: BatteryChange::Low,
            percent: 9,
        },
        PushedEvent::Phone { is_connected: false },
        PushedEvent::Phone { is_connected: true },
        PushedEvent::Call {
            kind: CallKind::Incoming,
            caller: "A Friend",
        },
        PushedEvent::Music,
        PushedEvent::Weather,
        PushedEvent::Navigation,
        PushedEvent::SettingsChange {
            key: "uses_24_hour_clock",
        },
    ]
}

/// What poking at one package did, and everything wrong with it.
struct Outcome {
    taps: usize,
    effects: usize,
    problems: Vec<String>,
}

fn poke_at(package: &Package) -> Outcome {
    let failed_with = |problem: String| Outcome {
        taps: 0,
        effects: 0,
        problems: vec![problem],
    };
    let mut watch = match TestWatch::open_package(package.kind, &package.id) {
        Ok(watch) => watch,
        Err(failure) => return failed_with(failure.to_string()),
    };
    if package.id == "launcher" {
        if let Err(failure) = watch.install_apps(&LAUNCHER_LISTS) {
            return failed_with(format!("cannot install apps for it: {failure}"));
        }
    }
    let mut problems = Vec::new();
    let mut check_frame = |watch: &mut TestWatch, step: &str| {
        let stale = watch.pixels_left_stale();
        if stale > 0 {
            problems.push(format!(
                "after {step}, {stale} pixels differ from drawing the screen afresh"
            ));
        }
    };
    check_frame(&mut watch, "opening");

    let mut tapped = HashSet::new();
    for _ in 0..TAP_ROUNDS {
        for widget_index in watch.tappable_widgets() {
            let screen = watch.host().screen();
            if tapped.insert((screen, widget_index)) {
                match watch.widget_kind(widget_index) {
                    Some(WidgetKind::ButtonMatrix | WidgetKind::CellGrid) => {
                        watch.tap_all_over(widget_index, GRID_POINTS_ACROSS)
                    }
                    _ => {
                        watch.tap_widget(widget_index);
                    }
                }
                check_frame(&mut watch, &format!("tapping widget {widget_index}"));
            }
        }
    }
    watch.long_press(SCREEN_MIDDLE, SCREEN_MIDDLE);
    check_frame(&mut watch, "a long press");
    watch.double_tap(SCREEN_MIDDLE, SCREEN_MIDDLE);
    check_frame(&mut watch, "a double tap");
    for direction in [Direction::Up, Direction::Down, Direction::Left, Direction::Right] {
        watch.swipe(direction);
        check_frame(&mut watch, &format!("swiping {direction:?}"));
    }
    watch.press_button();
    check_frame(&mut watch, "pressing the button");
    watch.advance(TEN_MINUTES);
    check_frame(&mut watch, "ten minutes");
    watch.deliver(PushedEvent::Notification {
        id: 1,
        app: "Messages",
        title: "Hello",
        body: "Are you there?",
        time: watch.now(),
    });
    check_frame(&mut watch, "a notification");
    for event in every_other_event() {
        watch.deliver(event);
    }
    check_frame(&mut watch, "every kind of event");
    watch.screen_off();
    watch.advance(ONE_MINUTE);
    watch.screen_on();
    check_frame(&mut watch, "a minute with the screen off");
    for _ in 0..TAP_ROUNDS {
        watch.tap_all_over(watch.host().screen(), SCREEN_POINTS_ACROSS);
    }
    check_frame(&mut watch, "tapping all over the screen");

    problems.extend(watch.failures().iter().map(|failure| format!("{failure:?}")));
    Outcome {
        taps: tapped.len(),
        effects: watch.effects().len(),
        problems,
    }
}

// ============================================================================
// Every package
// ============================================================================

#[test]
fn every_app_and_face_runs_without_failing() {
    let packages = every_package();
    assert!(!packages.is_empty(), "no apps or faces found");

    let mut report = Vec::new();
    for package in &packages {
        let started = std::time::Instant::now();
        let outcome = catch_unwind(AssertUnwindSafe(|| poke_at(package))).unwrap_or_else(|_| Outcome {
            taps: 0,
            effects: 0,
            problems: vec!["panicked; its message is printed above".to_string()],
        });
        println!(
            "{:<24} {} widgets tapped, {} effects, {} problems, {:?}",
            package.label(),
            outcome.taps,
            outcome.effects,
            outcome.problems.len(),
            started.elapsed()
        );
        if !outcome.problems.is_empty() {
            report.push(format!(
                "{}:\n  {}",
                package.label(),
                outcome.problems.join("\n  ")
            ));
        }
    }
    println!(
        "{} apps and faces exercised, {} with problems",
        packages.len(),
        report.len()
    );
    assert!(report.is_empty(), "\n{}", report.join("\n"));
}
