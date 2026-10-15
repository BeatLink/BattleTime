//! Draws the main screen of every app and face, and a few screens after some taps, and compares each
//! with its reference picture in `harness/snapshots/screens/`, so a change to how anything looks is seen.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;

use battletime_app_testing::{
    check_screen_snapshot, every_package, Direction, Package, PackageKind, TestWatch, WidgetKind,
};

// ============================================================================
// Taking a screenshot
// ============================================================================

/// The name the reference picture of a package goes by, such as `app-stopwatch`.
fn snapshot_name(package: &Package) -> String {
    let kind = match package.kind {
        PackageKind::App => "app",
        PackageKind::Shell => "shell",
        PackageKind::Face => "face",
    };
    format!("{kind}-{}", package.id)
}

/// Checks the watch's screen against a reference picture, catching a panic so one broken app does not hide the rest.
fn check_screen(name: &str, show: impl FnOnce() -> TestWatch) -> Result<(), String> {
    let mut watch = catch_unwind(AssertUnwindSafe(show))
        .map_err(|_| format!("{name}: panicked; its message is printed above"))?;
    check_screen_snapshot(name, watch.frame())
}

#[track_caller]
fn assert_all_match(results: Vec<Result<(), String>>) {
    let report: Vec<String> = results.into_iter().filter_map(Result::err).collect();
    assert!(
        report.is_empty(),
        "\n{}\nRun with BATTLETIME_UPDATE_SNAPSHOTS=1 to accept the change.",
        report.join("\n")
    );
}

// ============================================================================
// Main screens
// ============================================================================

#[test]
fn every_main_screen_matches_its_reference_picture() {
    let packages = every_package();
    assert!(!packages.is_empty(), "no apps or faces found");

    let results = packages
        .iter()
        .map(|package| {
            check_screen(&snapshot_name(package), || {
                TestWatch::open_package(package.kind, &package.id).unwrap()
            })
        })
        .collect();

    assert_all_match(results);
}

// ============================================================================
// After some taps
// ============================================================================

#[test]
fn the_stopwatch_after_three_seconds_matches_its_reference_picture() {
    let result = check_screen("app-stopwatch-running", || {
        let mut watch = TestWatch::open_app("stopwatch").unwrap();
        // The start button, at the bottom right.
        watch.tap(178, 200);
        watch.advance(Duration::from_secs(3));
        watch
    });

    assert_all_match(vec![result]);
}

#[test]
fn the_battery_page_matches_its_reference_picture() {
    let result = check_screen("app-system-battery", || {
        let mut watch = TestWatch::open_app("system").unwrap();
        watch.swipe(Direction::Left);
        watch.advance(Duration::from_secs(1));
        watch
    });

    assert_all_match(vec![result]);
}

#[test]
fn the_steps_day_page_matches_its_reference_picture() {
    let result = check_screen("app-steps-day", || {
        let mut watch = TestWatch::open_app("steps").unwrap();
        watch.swipe(Direction::Left);
        watch.swipe(Direction::Left);
        watch.advance(Duration::from_secs(1));
        watch
    });

    assert_all_match(vec![result]);
}

/// One key of a keypad, by its row, its place in the row and how many keys the row has.
struct KeypadKey {
    row: i32,
    column: i32,
    keys_in_row: i32,
}

/// The calculator's keypad has four rows; this taps the middle of one key.
fn tap_calculator_key(watch: &mut TestWatch, key: KeypadKey) {
    const CALCULATOR_KEYPAD_ROWS: i32 = 4;
    let keypad = watch
        .tappable_widgets()
        .into_iter()
        .find(|&widget_index| watch.widget_kind(widget_index) == Some(WidgetKind::ButtonMatrix))
        .expect("the calculator shows its keypad");
    let area = watch.visible_area(keypad).unwrap();
    let x = area.x + area.width * (2 * key.column + 1) / (2 * key.keys_in_row);
    let y = area.y + area.height * (2 * key.row + 1) / (2 * CALCULATOR_KEYPAD_ROWS);
    watch.tap(x, y);
}

#[test]
fn the_calculator_after_a_sum_matches_its_reference_picture() {
    let result = check_screen("app-calculator-sum", || {
        let mut watch = TestWatch::open_app("calculator").unwrap();
        let seven = KeypadKey {
            row: 0,
            column: 0,
            keys_in_row: 5,
        };
        let plus = KeypadKey {
            row: 1,
            column: 3,
            keys_in_row: 5,
        };
        let five = KeypadKey {
            row: 1,
            column: 1,
            keys_in_row: 5,
        };
        let equals = KeypadKey {
            row: 3,
            column: 3,
            keys_in_row: 4,
        };
        for key in [seven, plus, five, equals] {
            tap_calculator_key(&mut watch, key);
        }
        assert!(watch.shows_text("12"), "{:?}", watch.visible_texts());
        watch
    });

    assert_all_match(vec![result]);
}

#[test]
fn the_launcher_with_installed_apps_matches_its_reference_picture() {
    let result = check_screen("app-launcher-installed", || {
        let mut watch = TestWatch::open_app("launcher").unwrap();
        watch
            .install_apps(&["stopwatch", "timer", "calculator", "alarm"])
            .unwrap();
        watch
    });

    assert_all_match(vec![result]);
}

#[test]
fn the_launcher_in_each_style_and_view_matches_its_reference_pictures() {
    const GROUPS: [(&str, &str, &[&str]); 3] = [
        ("Games", "game", &["snake", "twos", "dice", "puzzle15"]),
        ("Time", "timer", &["alarm", "stopwatch", "timer"]),
        ("Tools", "calculator", &["calculator", "flashlight"]),
    ];
    let grouped = |style: &'static str, view: &'static str| {
        move || {
            let mut watch = TestWatch::open_app("launcher").unwrap();
            watch.group_launcher(style, view, &GROUPS).unwrap();
            watch
        }
    };
    let results = vec![
        check_screen("app-launcher-folders", grouped("folders", "list")),
        check_screen("app-launcher-folder-open", || {
            let mut watch = grouped("folders", "grid")();
            watch.tap_text("Games");
            watch
        }),
        check_screen("app-launcher-sections", grouped("sections", "list")),
        check_screen("app-launcher-sections-grid", grouped("sections", "grid")),
        check_screen("app-launcher-pages", grouped("pages", "list")),
        check_screen("app-launcher-grid", grouped("list", "grid")),
    ];

    assert_all_match(results);
}

#[test]
fn the_remote_with_one_four_and_nine_buttons_matches_its_reference_pictures() {
    use battletime_app_testing::{compile_package, FakeSystem};

    // Icons in library order: phone, play, next, previous, music, volume up, volume down, flashlight and bell.
    let icons = [31, 32, 28, 34, 26, 56, 55, 16, 3];
    let results = [1, 4, 9]
        .into_iter()
        .map(|count| {
            check_screen(&format!("app-remote-{count}"), || {
                let mut file = vec![1, count as u8];
                file.extend(&icons[..count]);
                let mut system = FakeSystem::new();
                system.write_file("/remote/buttons", &file);
                let image = compile_package(PackageKind::App, "remote").unwrap();
                TestWatch::open_image_on(image, system).unwrap()
            })
        })
        .collect();

    assert_all_match(results);
}

#[test]
fn the_software_app_refusing_an_app_matches_its_reference_picture() {
    let result = check_screen("app-software-refused", || {
        let mut watch = TestWatch::open_app("software").unwrap();
        watch.tap_text("Atlas");
        assert_eq!(watch.screen_name(), Some("refused"));
        watch
    });

    assert_all_match(vec![result]);
}
