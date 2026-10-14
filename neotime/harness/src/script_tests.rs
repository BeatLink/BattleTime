use std::path::PathBuf;
use std::time::Duration;

use neotime_runtime::events::SwipeDirection;
use neotime_runtime::{EffectKind, SystemValue};

use super::*;

// ============================================================================
// Helpers
// ============================================================================

/// A test app that counts each kind of touch and every notification; its Fail button divides by zero.
const SCRIPTED_APP: &str = r#"
// A test app for scripts: it counts each kind of touch and every notification, and can fail.
import { battery_percent, define_app, on, signal, vibrate } from "neotime";
import { Button, Label, Screen } from "neotime/widgets";

const taps = signal(0);
const holds = signal(0);
const double_taps = signal(0);
const drags = signal(0);
const notifications = signal(0);
const divisor = signal(1);

function count_tap() {
  taps.value = taps.value + 1;
  vibrate(30);
}

function fail() {
  divisor.value = 0;
  taps.value = Math.trunc(10 / divisor.value);
}

function Scripted() {
  return (
    <Screen gap={4}>
      <Label>{`Taps ${taps.value} Holds ${holds.value} Doubles ${double_taps.value}`}</Label>
      <Label>{`Drags ${drags.value} Notes ${notifications.value} Battery ${battery_percent.value}`}</Label>
      <Button
        onClick={count_tap}
        onLongPress={() => { holds.value = holds.value + 1; }}
        onDoubleClick={() => { double_taps.value = double_taps.value + 1; }}
        onDrag={() => { drags.value = drags.value + 1; }}
      >
        Count
      </Button>
      <Button onClick={fail}>Fail</Button>
    </Screen>
  );
}

on("notification", () => { notifications.value = notifications.value + 1; }, { while_open: true });

export default define_app({
  id: "scripted",
  name: "Scripted",
  screens: { main: Scripted },
});
"#;

fn parse_one(line: &str) -> ScriptCommand {
    parse_script(line).unwrap().steps.remove(0).command
}

fn parse_error(text: &str) -> ScriptError {
    parse_script(text).unwrap_err()
}

fn run_on_scripted_app(text: &str) -> ScriptRun {
    let watch = TestWatch::open_source(SCRIPTED_APP).unwrap();
    run_script_on(watch, &parse_script(text).unwrap(), &std::env::temp_dir(), |_| {})
}

fn point(x: i32, y: i32) -> ScreenPoint {
    ScreenPoint { x, y }
}

// ============================================================================
// Parsing commands
// ============================================================================

#[test]
fn every_command_parses_into_its_step() {
    let script = parse_script(
        r#"
open app stopwatch
open face digital
install stopwatch timer
tap 120 138
tap "Start"
long-press "Count"
long-press 10 20
double-tap 30 40
drag 1 2 3 4
swipe left
button
wait 250ms
set battery_percent 15
notify "Hello" "From the phone"
shake
acceleration 10 -20 1000
screen off
screen on
expect text "00:03"
expect no text "Time's up"
expect screen main
expect effect open-app timer
expect no effect vibrate
expect no failures
screenshot out/frame.png
"#,
    )
    .unwrap();

    let commands: Vec<ScriptCommand> = script.steps.into_iter().map(|step| step.command).collect();
    assert_eq!(
        commands,
        [
            ScriptCommand::OpenApp("stopwatch".to_string()),
            ScriptCommand::OpenFace("digital".to_string()),
            ScriptCommand::InstallApps(vec!["stopwatch".to_string(), "timer".to_string()]),
            ScriptCommand::Touch {
                kind: TouchKind::Tap,
                target: TouchTarget::Point(point(120, 138)),
            },
            ScriptCommand::Touch {
                kind: TouchKind::Tap,
                target: TouchTarget::Text("Start".to_string()),
            },
            ScriptCommand::Touch {
                kind: TouchKind::LongPress,
                target: TouchTarget::Text("Count".to_string()),
            },
            ScriptCommand::Touch {
                kind: TouchKind::LongPress,
                target: TouchTarget::Point(point(10, 20)),
            },
            ScriptCommand::Touch {
                kind: TouchKind::DoubleTap,
                target: TouchTarget::Point(point(30, 40)),
            },
            ScriptCommand::Drag {
                from: point(1, 2),
                to: point(3, 4),
            },
            ScriptCommand::Swipe(SwipeDirection::Left),
            ScriptCommand::PressButton,
            ScriptCommand::Wait(Duration::from_millis(250)),
            ScriptCommand::SetSystemValue {
                value: SystemValue::BatteryPercent,
                number: 15,
            },
            ScriptCommand::Notify {
                app: String::new(),
                title: "Hello".to_string(),
                body: "From the phone".to_string(),
            },
            ScriptCommand::Shake,
            ScriptCommand::Acceleration {
                x: 10,
                y: -20,
                z: 1000,
            },
            ScriptCommand::ScreenOff,
            ScriptCommand::ScreenOn,
            ScriptCommand::ExpectText("00:03".to_string()),
            ScriptCommand::ExpectNoText("Time's up".to_string()),
            ScriptCommand::ExpectScreen("main".to_string()),
            ScriptCommand::ExpectEffect(EffectCheck {
                kind: EffectKind::OpenApp,
                arguments: vec![ExpectedArgument::Text("timer".to_string())],
            }),
            ScriptCommand::ExpectNoEffect(EffectCheck {
                kind: EffectKind::Vibrate,
                arguments: Vec::new(),
            }),
            ScriptCommand::ExpectNoFailures,
            ScriptCommand::Screenshot(PathBuf::from("out/frame.png")),
        ]
    );
}

#[test]
fn a_notification_may_name_its_app() {
    assert_eq!(
        parse_one(r#"notify "SMS" "Sam" "On my way""#),
        ScriptCommand::Notify {
            app: "SMS".to_string(),
            title: "Sam".to_string(),
            body: "On my way".to_string(),
        }
    );
    assert!(parse_error(r#"notify "only a title""#)
        .to_string()
        .contains("notify"));
}

#[test]
fn blank_lines_and_comments_are_skipped_and_steps_keep_their_line_numbers() {
    let script = parse_script("# a comment\n\nbutton   # press it\n  wait 1s\n").unwrap();

    let lines: Vec<(usize, &str)> = script
        .steps
        .iter()
        .map(|step| (step.line_number, step.text.as_str()))
        .collect();
    assert_eq!(lines, [(3, "button"), (4, "wait 1s")]);
}

#[test]
fn a_hash_inside_quotes_is_text_not_a_comment() {
    assert_eq!(
        parse_one(r##"expect text "#1""##),
        ScriptCommand::ExpectText("#1".to_string())
    );
}

#[test]
fn quoted_text_may_hold_escaped_quotes_and_backslashes() {
    assert_eq!(
        parse_one(r#"expect text "say \"hi\" \\ \n""#),
        ScriptCommand::ExpectText(r#"say "hi" \ \n"#.to_string())
    );
}

#[test]
fn durations_take_milliseconds_seconds_minutes_and_hours() {
    let durations = ["250ms", "3s", "10min", "2h"].map(|text| parse_one(&format!("wait {text}")));

    assert_eq!(
        durations,
        [
            ScriptCommand::Wait(Duration::from_millis(250)),
            ScriptCommand::Wait(Duration::from_secs(3)),
            ScriptCommand::Wait(Duration::from_secs(600)),
            ScriptCommand::Wait(Duration::from_secs(7200)),
        ]
    );
}

#[test]
fn effect_and_system_value_names_take_hyphens_or_underscores() {
    assert_eq!(
        parse_one("expect effect keep-screen-on"),
        parse_one("expect effect keep_screen_on")
    );
    assert_eq!(
        parse_one("set battery-percent 5"),
        parse_one("set battery_percent 5")
    );
    assert_eq!(
        parse_one("set settings.step_goal 8000"),
        ScriptCommand::SetSystemValue {
            value: SystemValue::StepGoal,
            number: 8000,
        }
    );
}

#[test]
fn effect_arguments_are_read_by_their_type() {
    let check = |line: &str| match parse_one(line) {
        ScriptCommand::ExpectEffect(check) => check.arguments,
        other => panic!("{other:?}"),
    };

    assert_eq!(
        check("expect effect vibrate-pattern alarm"),
        [ExpectedArgument::Number(3)]
    );
    assert_eq!(
        check("expect effect keep-screen-on true"),
        [ExpectedArgument::Number(1)]
    );
    assert_eq!(check("expect effect vibrate 30"), [ExpectedArgument::Number(30)]);
    assert_eq!(
        check(r#"expect effect notify "Title" "Body""#),
        [
            ExpectedArgument::Text("Title".to_string()),
            ExpectedArgument::Text("Body".to_string())
        ]
    );
}

// ============================================================================
// Parse errors
// ============================================================================

#[test]
fn an_unknown_command_names_its_line() {
    let error = parse_error("open app stopwatch\n\ntapp 1 2\n");

    assert_eq!(error.line_number, 3);
    assert!(error.message.contains("unknown command `tapp`"), "{error}");
    assert!(error.to_string().starts_with("line 3: "), "{error}");
}

#[test]
fn a_missing_closing_quote_names_its_line() {
    let error = parse_error("button\ntap \"Start\n");

    assert_eq!(error.line_number, 2);
    assert!(error.message.contains("never closed"), "{error}");
}

#[test]
fn a_bad_duration_names_its_line() {
    for duration in ["3", "3sec", "s", "-1s", "1.5s", "99999999999999999999h"] {
        let error = parse_error(&format!("button\nwait {duration}\n"));

        assert_eq!(error.line_number, 2, "{duration}");
        assert!(error.message.contains("is not a duration"), "{duration}: {error}");
    }
}

#[test]
fn commands_with_the_wrong_arguments_are_refused() {
    let lines = [
        "open window stopwatch",
        "install",
        "tap 1",
        "tap Start",
        "tap x 2",
        "drag 1 2 3",
        "swipe sideways",
        "swipe",
        "button now",
        "wait",
        "set battery_percent",
        "set battery_percent lots",
        "notify Hello",
        "shake hard",
        "acceleration 1 2",
        "acceleration x 2 3",
        "screen dim",
        "expect text 00:03",
        "expect sound",
        "expect effect",
        "expect effect vibrate 1 2",
        "expect effect vibrate-pattern siren",
        "expect effect vibrate many",
        "screenshot",
        "snapshot",
        "snapshot Running",
        "snapshot -running",
        "snapshot two words",
        "heart",
        "heart fast",
        "navigation now",
        "write /notes/note.txt text",
        "write \"/notes/note.txt\"",
        "reopen now",
        "\"tap\" 1 2",
        "phone",
        "phone settings",
        "phone \"{}\" \"{}\"",
    ];

    for line in lines {
        assert!(parse_script(line).is_err(), "{line} should be refused");
    }
}

#[test]
fn unknown_names_and_values_a_script_cannot_set_are_refused() {
    assert!(parse_error("expect effect explode")
        .message
        .contains("not an effect"));
    assert!(parse_error("set happiness 3")
        .message
        .contains("not a system value"));
    assert!(parse_error("set now 0").message.contains("wait"));
    assert!(parse_error("set current_music.track 1")
        .message
        .contains("is text"));
}

// ============================================================================
// Running
// ============================================================================

#[test]
fn a_passing_script_reports_every_step_in_order() {
    let script = parse_script("open app stopwatch\ntap 178 200\nwait 3s\nexpect text \"00:03\"\n").unwrap();
    let mut printed = Vec::new();

    let run = run_script(&script, &std::env::temp_dir(), |outcome| {
        printed.push(outcome.to_string())
    });

    assert!(run.passed());
    assert_eq!(run.passed_steps, script.steps);
    assert_eq!(printed.len(), 4);
    assert_eq!(printed[3], "ok    line 4: expect text \"00:03\"");
}

#[test]
fn a_failed_text_check_reports_its_line_what_it_wanted_and_what_was_shown() {
    let script = parse_script("open app stopwatch\n\nexpect text \"12:34\"  # wrong\nbutton\n").unwrap();
    let mut printed = Vec::new();

    let run = run_script(&script, &std::env::temp_dir(), |outcome| {
        printed.push(outcome.to_string())
    });

    let failure = run.failure.unwrap();
    assert_eq!(failure.line_number, 3);
    assert_eq!(failure.command, "expect text \"12:34\"");
    assert_eq!(failure.expected, "text \"12:34\" shown");
    assert!(failure.found.contains("\"00:00\""), "{}", failure.found);
    assert_eq!(run.passed_steps.len(), 1);
    assert!(
        printed.last().unwrap().starts_with("FAIL  line 3: expect text"),
        "{printed:?}"
    );
}

#[test]
fn driving_before_opening_fails_at_that_line() {
    let run = run_script(
        &parse_script("# nothing open\nbutton\n").unwrap(),
        &std::env::temp_dir(),
        |_| {},
    );

    let failure = run.failure.unwrap();
    assert_eq!(failure.line_number, 2);
    assert_eq!(failure.found, "nothing open");
}

#[test]
fn opening_a_missing_app_fails_naming_it() {
    let run = run_script(
        &parse_script("open app no-such-app\n").unwrap(),
        &std::env::temp_dir(),
        |_| {},
    );

    assert_eq!(run.failure.unwrap().expected, "the app no-such-app to open");
}

#[test]
fn installing_a_missing_app_fails() {
    let run = run_on_scripted_app("install no-such-app\n");

    assert_eq!(run.failure.unwrap().expected, "the apps to install");
}

#[test]
fn touches_by_text_reach_the_widget_reading_it() {
    let run = run_on_scripted_app(
        r#"
tap "Count"
long-press "Count"
double-tap "Count"
drag 60 138 180 138
expect text "Taps 1 Holds 1 Doubles 1"
expect no text "Drags 0"
"#,
    );

    assert_eq!(run.failure, None);
}

#[test]
fn a_touch_on_text_that_is_not_shown_fails_listing_the_shown_texts() {
    let run = run_on_scripted_app("tap \"Start\"\n");

    let failure = run.failure.unwrap();
    assert_eq!(failure.expected, "a shown widget reading exactly \"Start\"");
    assert!(failure.found.contains("\"Count\""), "{}", failure.found);
}

#[test]
fn notifications_and_system_values_reach_the_app() {
    let run = run_on_scripted_app(
        r#"
notify "Hello" "From the phone"
set battery_percent 15
expect text "Notes 1 Battery 15"
"#,
    );

    assert_eq!(run.failure, None);
}

#[test]
fn an_effect_check_sees_only_effects_since_the_steps_after_the_previous_check() {
    let run = run_on_scripted_app(
        r#"
expect no effect vibrate
tap "Count"
expect effect vibrate 30
expect no effect vibrate 31
swipe up
expect no effect vibrate
"#,
    );

    assert_eq!(run.failure, None);
}

#[test]
fn a_failed_effect_check_lists_the_effects_it_saw() {
    let run = run_on_scripted_app("tap \"Count\"\nexpect effect vibrate 99\n");

    let failure = run.failure.unwrap();
    assert_eq!(failure.expected, "effect Vibrate(99, ...)");
    assert_eq!(failure.found, "the effects Vibrate(30)");
}

#[test]
fn a_failed_check_for_no_effect_names_the_effect() {
    let run = run_on_scripted_app("tap \"Count\"\nexpect no effect vibrate\n");

    assert_eq!(run.failure.unwrap().expected, "no effect Vibrate");
}

#[test]
fn a_check_for_no_failures_reports_the_failure() {
    let run = run_on_scripted_app("expect no failures\ntap \"Fail\"\nexpect no failures\n");

    let failure = run.failure.unwrap();
    assert_eq!(failure.line_number, 3);
    assert_ne!(failure.found, "[]");
}

#[test]
fn screen_checks_compare_the_screen_name() {
    let run = run_on_scripted_app("screen off\nscreen on\nexpect screen main\nexpect screen settings\n");

    let failure = run.failure.unwrap();
    assert_eq!(failure.line_number, 4);
    assert_eq!(failure.found, "screen main");
}

#[test]
fn text_that_should_be_gone_fails_while_it_shows() {
    let run = run_on_scripted_app("expect no text \"Count\"\n");

    assert_eq!(run.failure.unwrap().expected, "no text \"Count\" shown");
}

#[test]
fn a_panic_message_is_read_from_its_text() {
    assert_eq!(panic_message(&"broken"), "a panic: broken");
    assert_eq!(panic_message(&String::from("owned")), "a panic: owned");
    assert_eq!(panic_message(&5), "a panic: no message");
}

#[test]
fn a_screenshot_saves_the_screen_as_a_png() {
    let folder = std::env::temp_dir().join(format!("neotime-script-test-{}", std::process::id()));

    let run = run_on_scripted_app(&format!("screenshot {}/shots/frame.png\n", folder.display()));

    assert_eq!(run.failure, None);
    let decoder = png::Decoder::new(std::io::BufReader::new(
        File::open(folder.join("shots/frame.png")).unwrap(),
    ));
    let reader = decoder.read_info().unwrap();
    assert_eq!(reader.info().width, SCREEN_SIDE_IN_PIXELS as u32);
    assert_eq!(reader.info().height, SCREEN_SIDE_IN_PIXELS as u32);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_screenshot_that_cannot_be_saved_fails() {
    let run = run_on_scripted_app("screenshot /proc/neotime/frame.png\n");

    assert!(run
        .failure
        .unwrap()
        .expected
        .starts_with("to save /proc/neotime/frame.png"));
}

#[test]
fn a_snapshot_names_its_picture_after_the_open_app() {
    let script = parse_script("snapshot after-start-2\n").unwrap();
    assert_eq!(
        script.steps[0].command,
        ScriptCommand::Snapshot("after-start-2".to_string())
    );

    let run = run_script(
        &parse_script("open app stopwatch\ntap 178 200\nwait 3s\nsnapshot running\n").unwrap(),
        &std::env::temp_dir(),
        |_| {},
    );

    assert_eq!(run.failure, None);
}

#[test]
fn heart_navigation_write_and_reopen_reach_the_app() {
    let script = parse_script(
        "open app heart\nset heart_rate_status 3\nheart 74\nexpect text \"074\"\n\
         open app navigation\nnavigation\nexpect text \"Main Street\"\n\
         open app notes\nwrite \"/notes/note.txt\" \"\"\nreopen\nexpect text \"No note yet\"\n",
    )
    .unwrap();
    assert_eq!(script.steps[2].command, ScriptCommand::HeartRate(74));
    assert_eq!(script.steps[5].command, ScriptCommand::Navigation);
    assert_eq!(
        script.steps[8].command,
        ScriptCommand::WriteFile {
            path: "/notes/note.txt".to_string(),
            text: String::new(),
        }
    );
    assert_eq!(script.steps[9].command, ScriptCommand::Reopen);

    let run = run_script(&script, &std::env::temp_dir(), |_| {});

    assert_eq!(run.failure, None);
}

#[test]
fn start_on_opens_a_screen_and_fails_on_one_the_app_lacks() {
    let script = parse_script(
        "open app notifications\nnotify \"Sam\" \"Hello\"\nstart on preview\nexpect screen preview\n",
    )
    .unwrap();
    assert_eq!(
        script.steps[2].command,
        ScriptCommand::StartOn("preview".to_string())
    );
    assert_eq!(run_script(&script, &std::env::temp_dir(), |_| {}).failure, None);

    let missing = parse_script("open app notifications\nstart on nowhere\n").unwrap();
    assert!(run_script(&missing, &std::env::temp_dir(), |_| {})
        .failure
        .is_some());
    assert!(parse_script("open app notifications\nstart preview\n").is_err());
}

#[test]
fn a_snapshot_that_differs_from_its_picture_fails() {
    if crate::snapshots::is_updating_snapshots() {
        return;
    }

    let run = run_script(
        &parse_script("open app stopwatch\nsnapshot running\n").unwrap(),
        &std::env::temp_dir(),
        |_| {},
    );

    let failure = run.failure.unwrap();
    assert_eq!(failure.line_number, 2);
    assert_eq!(failure.expected, "the screen to match app-stopwatch-running.png");
}

#[test]
fn a_snapshot_needs_the_script_to_open_the_app() {
    let run = run_on_scripted_app("snapshot main\n");

    assert!(run
        .failure
        .unwrap()
        .expected
        .starts_with("an app or face opened by the script"));
}

#[test]
fn a_phone_line_reaches_the_app_and_a_line_the_watch_refuses_fails() {
    let script = parse_script("phone \"{\\\"t\\\":\\\"find\\\",\\\"n\\\":true}\"\n").unwrap();
    assert_eq!(
        script.steps[0].command,
        ScriptCommand::Phone(r#"{"t":"find","n":true}"#.to_string())
    );

    assert_eq!(
        run_on_scripted_app("phone \"{\\\"t\\\":\\\"vibrate\\\"}\"\n").failure,
        None
    );
    let failure = run_on_scripted_app("phone \"GB(oops)\"\n").failure.unwrap();
    assert_eq!(failure.expected, "a line the watch takes");
    assert!(failure.found.contains("cannot read"), "{}", failure.found);
}
