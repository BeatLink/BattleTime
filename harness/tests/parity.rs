//! Records the texts every app and face shows on the test watch, screen by screen, in
//! `harness/snapshots/texts/`, and fails when they change. The browser preview's tests compare the
//! same files with what the preview shows, so the two cannot drift apart unnoticed.

use std::panic::{catch_unwind, AssertUnwindSafe};

use neotime_app_testing::{
    check_text_snapshot, compile_package, every_package, screen_texts_json, Direction, Package, PackageKind,
    ScreenTexts, TestWatch,
};

// ============================================================================
// Reading the screens
// ============================================================================

/// The name the snapshot of a package goes by, such as `app-stopwatch`.
fn snapshot_name(package: &Package) -> String {
    let kind = match package.kind {
        PackageKind::App => "app",
        PackageKind::Shell => "shell",
        PackageKind::Face => "face",
    };
    format!("{kind}-{}", package.id)
}

fn screen_name_of(watch: &TestWatch) -> String {
    watch.screen_name().unwrap_or("main").to_string()
}

const SCREEN_MIDDLE: i32 = 120;

/// A gesture on the main screen that the browser preview's parity tests can make too.
#[derive(Clone)]
enum Gesture {
    TapText(String),
    TapMiddle,
    LongPressMiddle,
    Swipe(Direction),
}

impl Gesture {
    /// How the snapshot names it, which the preview's tests read back.
    fn description(&self) -> String {
        match self {
            Gesture::TapText(text) => format!("tap {text:?}"),
            Gesture::TapMiddle => "tap middle".to_string(),
            Gesture::LongPressMiddle => "long press middle".to_string(),
            Gesture::Swipe(direction) => format!("swipe {}", format!("{direction:?}").to_lowercase()),
        }
    }

    fn make(&self, watch: &mut TestWatch) {
        match self {
            Gesture::TapText(text) => {
                watch.tap_text(text);
            }
            Gesture::TapMiddle => {
                watch.tap(SCREEN_MIDDLE, SCREEN_MIDDLE);
            }
            Gesture::LongPressMiddle => {
                watch.long_press(SCREEN_MIDDLE, SCREEN_MIDDLE);
            }
            Gesture::Swipe(direction) => {
                watch.swipe(*direction);
            }
        }
    }
}

/// Taps on each text the main screen shows once, then a tap and a long press in the middle and the four swipes.
fn gestures_to_try(main_texts: &[String]) -> Vec<Gesture> {
    let texts_shown_once = main_texts
        .iter()
        .filter(|text| main_texts.iter().filter(|other| other == text).count() == 1)
        .map(|text| Gesture::TapText(text.clone()));
    let swipes = [Direction::Up, Direction::Down, Direction::Left, Direction::Right].map(Gesture::Swipe);
    texts_shown_once
        .chain([Gesture::TapMiddle, Gesture::LongPressMiddle])
        .chain(swipes)
        .collect()
}

/// The main screen, then every other screen one gesture on it opens, each in the order first reached.
fn reachable_screens(package: &Package) -> Result<Vec<ScreenTexts>, String> {
    let image = compile_package(package.kind, &package.id).map_err(|failure| failure.to_string())?;
    let open = || TestWatch::open_image(image.clone()).map_err(|failure| failure.to_string());

    let watch = open()?;
    let main_name = screen_name_of(&watch);
    let main_texts = watch.visible_texts();
    let mut screens = vec![ScreenTexts {
        name: main_name.clone(),
        reached_by: None,
        texts: main_texts.clone(),
    }];

    for gesture in gestures_to_try(&main_texts) {
        let mut watch = open()?;
        gesture.make(&mut watch);
        let name = screen_name_of(&watch);
        if name != main_name && screens.iter().all(|screen| screen.name != name) {
            screens.push(ScreenTexts {
                name,
                reached_by: Some(gesture.description()),
                texts: watch.visible_texts(),
            });
        }
    }
    Ok(screens)
}

// ============================================================================
// Every package
// ============================================================================

#[test]
fn every_screen_shows_the_texts_in_its_snapshot() {
    let packages = every_package();
    assert!(!packages.is_empty(), "no apps or faces found");

    let mut report = Vec::new();
    for package in &packages {
        let name = snapshot_name(package);
        let screens = catch_unwind(AssertUnwindSafe(|| reachable_screens(package)))
            .unwrap_or_else(|_| Err("panicked; its message is printed above".to_string()));
        let checked = screens.and_then(|screens| check_text_snapshot(&name, &screen_texts_json(&screens)));
        if let Err(problem) = checked {
            report.push(problem);
        }
    }
    assert!(
        report.is_empty(),
        "\n{}\nRun with NEOTIME_UPDATE_SNAPSHOTS=1 to accept the change.",
        report.join("\n")
    );
}
