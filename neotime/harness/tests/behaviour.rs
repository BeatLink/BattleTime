//! What some apps and faces do, checked through the test watch the way a wearer would use them.

use std::time::Duration;

use neotime_app_testing::{
    Direction, EffectKind, EventResult, FakeTask, SystemValue, TestWatch, WidgetKind, NONE,
};
use neotime_runtime::SystemValues;

// ============================================================================
// Named numbers
// ============================================================================

/// `vibrate_pattern("alarm")`, as the compiler numbers the patterns.
const ALARM_PATTERN: i64 = 3;

/// The middle of the stopwatch's start button, at the bottom right.
const STOPWATCH_START_X: i32 = 178;
const STOPWATCH_START_Y: i32 = 200;

// ============================================================================
// Stopwatch
// ============================================================================

#[test]
fn the_stopwatch_counts_up_after_starting() {
    let mut watch = TestWatch::open_app("stopwatch").unwrap();
    assert!(watch.shows_text("00:00"));

    watch.tap(STOPWATCH_START_X, STOPWATCH_START_Y);
    watch.advance(Duration::from_secs(3));

    assert!(watch.shows_text("00:03"), "{:?}", watch.visible_texts());
    let keep_screen_on = watch.effects_of(EffectKind::KeepScreenOn);
    assert_eq!(keep_screen_on.last().unwrap().number(0), 1);
    assert!(watch.failures().is_empty());
}

#[test]
fn the_stopwatch_holds_its_time_while_paused() {
    let mut watch = TestWatch::open_app("stopwatch").unwrap();
    watch.tap(STOPWATCH_START_X, STOPWATCH_START_Y);
    watch.advance(Duration::from_secs(7));

    watch.tap(STOPWATCH_START_X, STOPWATCH_START_Y);
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
// Closed apps
// ============================================================================

#[test]
fn a_timer_left_running_rings_from_the_face_and_opens_itself_on_a_dark_screen() {
    let mut watch = TestWatch::open_app("timer").unwrap();
    let start_button = *watch.tappable_widgets().last().unwrap();
    watch.tap_widget(start_button);
    watch.switch_to_face("digital").unwrap();
    watch.screen_off();
    assert_eq!(watch.closed_app_ids(), vec!["timer"]);

    watch.advance(Duration::from_secs(10 * 60 - 1));
    assert_eq!(watch.id(), "digital");

    watch.advance(Duration::from_secs(1));

    assert_eq!(watch.id(), "timer");
    assert!(watch.is_screen_on());
    assert!(watch.shows_text("Time's up"), "{:?}", watch.visible_texts());
    let alarms = watch.effects_of(EffectKind::VibratePattern);
    assert_eq!(alarms.len(), 1);
    assert_eq!(alarms[0].number(0), ALARM_PATTERN);
    assert_eq!(watch.closed_app_ids(), Vec::<String>::new());
    assert!(watch.failures().is_empty());
}

#[test]
fn a_timer_installed_with_a_run_saved_rings_once_its_time_comes() {
    let mut watch = TestWatch::open_app("timer").unwrap();
    let start_button = *watch.tappable_widgets().last().unwrap();
    watch.tap_widget(start_button);
    watch.switch_to_app("stopwatch").unwrap();

    watch.advance(Duration::from_secs(10 * 60));

    assert_eq!(watch.id(), "timer");
    assert!(watch.shows_text("Time's up"));
    watch.tap_text("Dismiss");
    watch.switch_to_app("stopwatch").unwrap();
    assert!(watch.closed_app_ids().iter().all(|id| id != "timer"));
}

/// Sets a one-off alarm for 08:00, the alarm app's default, which is tomorrow at the fake system's 09:00.
fn set_an_alarm_for_eight() -> TestWatch {
    let mut watch = TestWatch::open_app("alarm").unwrap();
    let new_alarm = *watch.tappable_widgets().last().unwrap();
    watch.tap_widget(new_alarm);
    let next = *watch.tappable_widgets().last().unwrap();
    watch.tap_widget(next);
    let save = *watch.tappable_widgets().last().unwrap();
    watch.tap_widget(save);
    assert!(watch.shows_text("in 23:00"), "{:?}", watch.visible_texts());
    watch
}

fn shown_icons(watch: &TestWatch) -> usize {
    watch
        .visible_widgets()
        .into_iter()
        .filter(|&widget| watch.widget_kind(widget) == Some(WidgetKind::Icon))
        .count()
}

#[test]
fn a_set_alarm_shows_the_bell_on_the_face_and_rings_with_the_alarm_app_on_its_ringing_screen() {
    let mut watch = set_an_alarm_for_eight();
    let tomorrow_at_eight = watch.now() + 23 * 60 * 60 * 1000;
    watch.switch_to_face("digital").unwrap();
    assert_eq!(watch.system().read(SystemValue::NextAlarm), tomorrow_at_eight);
    let icons_with_bell = shown_icons(&watch);

    watch.screen_off();
    watch.advance(Duration::from_secs(23 * 60 * 60));

    assert_eq!(watch.id(), "alarm");
    assert_eq!(watch.screen_name(), Some("ring"));
    assert!(watch.is_screen_on());
    assert_eq!(
        watch
            .effects_of(EffectKind::VibratePattern)
            .last()
            .unwrap()
            .number(0),
        ALARM_PATTERN
    );

    watch.tap_text("Snooze");
    watch.switch_to_face("digital").unwrap();
    assert_eq!(
        watch.system().read(SystemValue::NextAlarm),
        watch.now() + 10 * 60 * 1000
    );
    assert_eq!(shown_icons(&watch), icons_with_bell);
    assert!(watch.failures().is_empty());
}

#[test]
fn the_face_shows_no_bell_once_the_alarm_is_switched_off() {
    let mut watch = set_an_alarm_for_eight();
    watch.switch_to_face("digital").unwrap();
    let icons_with_bell = shown_icons(&watch);

    watch.switch_to_app("alarm").unwrap();
    let switch = watch
        .tappable_widgets()
        .into_iter()
        .find(|&widget| watch.widget_kind(widget) == Some(WidgetKind::Switch))
        .unwrap();
    watch.tap_widget(switch);
    watch.switch_to_face("digital").unwrap();

    assert_eq!(watch.system().read(SystemValue::NextAlarm), NONE);
    assert_eq!(shown_icons(&watch), icons_with_bell - 1);
    assert!(watch.closed_app_ids().iter().all(|id| id != "alarm"));
}

#[test]
fn installed_apps_with_nothing_set_stay_unloaded() {
    let mut watch = TestWatch::open_face("digital").unwrap();

    watch
        .install_apps(&["alarm", "timer", "pomodoro", "stopwatch"])
        .unwrap();

    assert_eq!(watch.closed_app_ids(), Vec::<String>::new());
    assert_eq!(watch.system().read(SystemValue::NextAlarm), NONE);
}

/// Keeps the latest heart rate a measurement in the background gave, once a minute, even while closed.
const HEART_LOG_APP: &str = r#"
// Keeps the latest heart rate once a minute.
import { define_app, on, persisted, type HeartRateEvent, MINUTE } from "neotime";
import { Label, Screen } from "neotime/widgets";

const latest = persisted("latest", 0);

function keep(event: HeartRateEvent) {
  latest.value = event.beats_per_minute;
}

function Log() {
  return (
    <Screen>
      <Label>{`Latest ${latest.value}`}</Label>
    </Screen>
  );
}

on("heart_rate", keep, { every: MINUTE });

export default define_app({
  id: "heart-log",
  name: "Heart log",
  screens: { main: Log },
});
"#;

#[test]
fn a_closed_app_hears_the_heart_rate_measured_in_the_background() {
    let mut watch = TestWatch::open_source(HEART_LOG_APP).unwrap();
    assert!(watch.shows_text("Latest 0"));
    watch.switch_to_face("digital").unwrap();
    assert_eq!(watch.closed_app_ids(), vec!["heart-log"]);
    watch.set_system_value(SystemValue::HeartRateStatus, 3);
    watch.deliver(neotime_app_testing::PushedEvent::HeartRate {
        beats_per_minute: 72,
        confidence: 90,
    });

    watch.advance(Duration::from_secs(60));
    watch.switch_to_app("heart-log").unwrap();

    assert!(watch.shows_text("Latest 72"), "{:?}", watch.visible_texts());
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

/// Games and Time, each with two apps, as the companion's launcher file would group them.
const TWO_GROUPS: [(&str, &str, &[&str]); 2] = [
    ("Games", "game", &["snake", "twos"]),
    ("Time", "timer", &["stopwatch", "timer"]),
];

fn opened_app(watch: &TestWatch) -> Option<String> {
    let opened = watch.effects_of(EffectKind::OpenApp);
    opened.last().and_then(|effect| effect.text(0)).map(String::from)
}

#[test]
fn the_launcher_in_folders_lists_the_groups_and_a_tap_opens_one() {
    let mut watch = TestWatch::open_app("launcher").unwrap();
    watch.group_launcher("folders", "list", &TWO_GROUPS).unwrap();
    assert!(watch.shows_text("Games") && watch.shows_text("Time"));
    assert!(!watch.shows_text("Snake"));

    watch.tap_text("Time");

    assert_eq!(watch.screen_name(), Some("group"));
    assert!(watch.shows_text("Stopwatch") && watch.shows_text("Timer"));
    assert!(!watch.shows_text("Snake"));
    watch.tap_text("Timer");
    assert_eq!(opened_app(&watch).as_deref(), Some("timer"));
}

#[test]
fn an_open_folder_goes_back_to_the_groups_with_the_button() {
    let mut watch = TestWatch::open_app("launcher").unwrap();
    watch.group_launcher("folders", "list", &TWO_GROUPS).unwrap();
    watch.tap_text("Games");

    assert_eq!(watch.press_button(), EventResult::Handled);

    assert_eq!(watch.screen_name(), Some("main"));
    assert!(watch.shows_text("Games"));
}

#[test]
fn the_launcher_in_pages_shows_one_group_and_swipes_to_the_next() {
    let mut watch = TestWatch::open_app("launcher").unwrap();
    watch.group_launcher("pages", "list", &TWO_GROUPS).unwrap();
    assert!(watch.shows_text("Snake") && !watch.shows_text("Games"));
    assert!(!watch.shows_text("Stopwatch"));

    watch.swipe(Direction::Left);
    assert!(watch.shows_text("Stopwatch"));
    assert!(!watch.shows_text("Snake"));

    watch.swipe(Direction::Left);
    assert!(watch.shows_text("Stopwatch"));
    watch.swipe(Direction::Right);
    assert!(watch.shows_text("Snake"));
}

#[test]
fn the_launcher_in_sections_heads_each_group_in_one_list() {
    let mut watch = TestWatch::open_app("launcher").unwrap();
    watch.group_launcher("sections", "list", &TWO_GROUPS).unwrap();

    assert!(watch.shows_text("Games") && watch.shows_text("Snake") && watch.shows_text("Time"));
    watch.tap_text("Snake");
    assert_eq!(opened_app(&watch).as_deref(), Some("snake"));
}

#[test]
fn the_launcher_in_a_grid_still_opens_the_app_tapped() {
    let mut watch = TestWatch::open_app("launcher").unwrap();
    watch.group_launcher("list", "grid", &TWO_GROUPS).unwrap();

    watch.tap_text("2048");

    assert_eq!(opened_app(&watch).as_deref(), Some("twos"));
}

// ============================================================================
// Software
// ============================================================================

#[test]
fn the_software_memory_note_scrolls_away_with_the_list() {
    let mut watch = TestWatch::open_app("software").unwrap();
    assert!(watch.shows_text("Each app may use"));

    watch.swipe(Direction::Up);
    watch.swipe(Direction::Up);

    assert!(watch.shows_text("Timer"), "{:?}", watch.visible_texts());
    assert!(!watch.shows_text("Each app may use"));
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
fn a_right_swipe_stays_on_the_screen() {
    let mut watch = TestWatch::open_app("settings").unwrap();
    watch.tap_text("Brightness");

    assert_eq!(watch.swipe(Direction::Right), EventResult::NotHandled);

    assert_eq!(watch.screen_name(), Some("brightness"));
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

    watch.swipe(Direction::Right);
    assert!(watch.shows_text("Swipe right"));
}

// ============================================================================
// System
// ============================================================================

/// Opens the system app on its system page, the first.
fn open_system_page() -> TestWatch {
    let watch = TestWatch::open_app("system").unwrap();
    assert_eq!(watch.screen_name(), Some("main"));
    watch
}

#[test]
fn the_system_app_opens_on_the_system_page_and_pages_left_to_the_battery() {
    let mut watch = TestWatch::open_app("system").unwrap();
    assert_eq!(watch.screen_name(), Some("main"));
    assert!(watch.shows_text("Last restart"), "{:?}", watch.visible_texts());

    watch.swipe(Direction::Left);
    assert_eq!(watch.screen_name(), Some("main"));
    assert!(watch.shows_text("Discharging"), "{:?}", watch.visible_texts());
    assert!(!watch.shows_text("Last restart"));

    watch.swipe(Direction::Right);
    assert!(watch.shows_text("Last restart"), "{:?}", watch.visible_texts());
    watch.swipe(Direction::Right);
    assert!(
        watch.effects_of(EffectKind::Back).is_empty(),
        "the button goes back, not a swipe"
    );
    assert!(watch.failures().is_empty());
}

#[test]
fn the_hardware_page_writes_the_chip_numbers_in_hex_and_names_the_chips() {
    let mut watch = open_system_page();
    watch.set_system_value(SystemValue::BluetoothAddress, 0x0A0B_0C0D_0E0F);
    watch.set_system_value(SystemValue::FlashId, 0xC8_4016);
    watch.set_system_value(SystemValue::TouchChipId, 0xB6);
    watch.set_system_value(SystemValue::AccelerometerChipId, 0);
    for _ in 0..4 {
        watch.swipe(Direction::Left);
    }

    for text in ["0A:0B:0C:0D:0E:0F", "C84016", "CST816D v2", "No answer"] {
        assert!(watch.shows_text(text), "{text}: {:?}", watch.visible_texts());
    }
    assert!(!watch.shows_text("XT25F32B"));
}

#[test]
fn the_bootloader_page_shows_the_crc_of_a_bootloader_without_a_version_record() {
    let mut watch = open_system_page();
    watch.set_system_text(SystemValue::BootloaderVersion, "");
    watch.set_system_text(SystemValue::BootloaderBuildDate, "");
    watch.set_system_value(SystemValue::BootloaderCrc, 0x1A2B_3C4D);
    for _ in 0..3 {
        watch.swipe(Direction::Left);
    }

    assert!(watch.shows_text("CRC 1a2b3c4"), "{:?}", watch.visible_texts());
    assert!(!watch.shows_text("Built"));
    watch.set_system_value(SystemValue::BootloaderCrc, NONE);
    assert!(watch.shows_text("Unreadable"), "{:?}", watch.visible_texts());
}

#[test]
fn the_tasks_page_counts_runs_in_thousands_past_a_hundred_thousand_and_shares_the_time() {
    let mut watch = open_system_page();
    watch.set_tasks(vec![
        FakeTask {
            name: "motion".to_string(),
            runs: 2_400_000,
            busy_milliseconds: 1_230_000,
        },
        FakeTask {
            name: "storage".to_string(),
            runs: 999,
            busy_milliseconds: 0,
        },
    ]);
    for _ in 0..5 {
        watch.swipe(Direction::Left);
    }

    assert!(watch.failures().is_empty(), "{:?}", watch.failures());
    assert!(watch.shows_text("2400k"), "{:?}", watch.visible_texts());
    assert!(watch.shows_text("10.0%"), "{:?}", watch.visible_texts());
    assert!(
        watch.shows_text("storage      999 0.0%"),
        "{:?}",
        watch.visible_texts()
    );
    assert!(!watch.shows_text("bluetooth"));
    assert!(watch.failures().is_empty(), "{:?}", watch.failures());
}

#[test]
fn the_system_page_shows_a_new_firmware_version_and_a_dirty_tree() {
    let mut watch = open_system_page();
    assert!(watch.shows_text("0.1.0+1b19aed"), "{:?}", watch.visible_texts());

    watch.set_system_text(SystemValue::FirmwareVersion, "0.2.0+0a1b2c3*");

    assert!(watch.shows_text("0.2.0+0a1b2c3*"), "{:?}", watch.visible_texts());
    assert!(!watch.shows_text("0.1.0+1b19aed"));
    assert_eq!(watch.pixels_left_stale(), 0);
    assert!(watch.failures().is_empty());
}

#[test]
fn the_system_page_says_the_last_run_crashed() {
    let mut watch = open_system_page();
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
fn the_system_page_counts_uptime_from_a_start_the_phone_moved() {
    let mut watch = open_system_page();
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
export default define_app({ id: "notifier", name: "Notifier", screens: { main: Main } });
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

// ============================================================================
// Remote
// ============================================================================

mod remote {
    use super::*;
    use neotime_app_testing::{compile_package, FakeSystem, PackageKind};

    const BUTTONS_PATH: &str = "/remote/buttons";
    const SCREEN_PADDING: i32 = 8;
    const GAP: i32 = 8;

    /// The remote opened on a watch whose button file holds `bytes`.
    fn open_with_buttons(bytes: &[u8]) -> TestWatch {
        let mut system = FakeSystem::new();
        system.write_file(BUTTONS_PATH, bytes);
        let image = compile_package(PackageKind::App, "remote").unwrap();
        TestWatch::open_image_on(image, system).unwrap()
    }

    /// A file of `count` buttons, each with the same icon.
    fn file_of(count: usize) -> Vec<u8> {
        let mut file = std::vec![1, count as u8];
        file.extend(std::iter::repeat_n(18, count));
        file
    }

    /// The middle of each place a grid of `columns` by `rows` buttons has, across each row from the top left.
    fn button_middles(columns: i32, rows: i32) -> Vec<(i32, i32)> {
        let inside = 240 - 2 * SCREEN_PADDING;
        let width = (inside - (columns - 1) * GAP) / columns;
        let height = (inside - (rows - 1) * GAP) / rows;
        (0..rows)
            .flat_map(|row| (0..columns).map(move |column| (column, row)))
            .map(|(column, row)| {
                (
                    SCREEN_PADDING + column * (width + GAP) + width / 2,
                    SCREEN_PADDING + row * (height + GAP) + height / 2,
                )
            })
            .collect()
    }

    /// The button number each tap at these points sends the phone, or none for a tap that sends nothing.
    fn buttons_tapped(watch: &mut TestWatch, points: &[(i32, i32)]) -> Vec<Option<String>> {
        points
            .iter()
            .map(|&(x, y)| {
                let before = watch.phone_lines().len();
                watch.tap(x, y);
                watch.phone_lines().get(before).map(|line| {
                    let start = line.find("\"button\":").unwrap() + "\"button\":".len();
                    line[start..line.find('}').unwrap()].to_string()
                })
            })
            .collect()
    }

    #[test]
    fn the_remote_asks_for_buttons_while_the_phone_has_set_none() {
        for bytes in [&[][..], &[1, 0], &[2, 3, 1, 2, 3]] {
            let mut watch = open_with_buttons(bytes);

            assert!(
                watch.shows_text("Choose buttons"),
                "{bytes:?}: {:?}",
                watch.visible_texts()
            );
            watch.tap(120, 120);
            assert!(watch.phone_lines().is_empty(), "{bytes:?}");
        }
    }

    #[test]
    fn the_remote_lays_its_buttons_out_by_how_many_there_are() {
        let layouts = [
            (1, 1, 1),
            (2, 2, 1),
            (3, 2, 2),
            (4, 2, 2),
            (5, 3, 2),
            (6, 3, 2),
            (7, 3, 3),
            (9, 3, 3),
        ];
        for (count, columns, rows) in layouts {
            let mut watch = open_with_buttons(&file_of(count));

            let tapped = buttons_tapped(&mut watch, &button_middles(columns, rows));

            let expected: Vec<Option<String>> = (0..(columns * rows) as usize)
                .map(|place| (place < count).then(|| place.to_string()))
                .collect();
            assert_eq!(tapped, expected, "{count} buttons");
            assert!(!watch.shows_text("Choose buttons"));
            assert!(watch.failures().is_empty());
        }
    }

    #[test]
    fn tapping_a_button_buzzes_and_tells_the_phone_its_number() {
        let mut watch = open_with_buttons(&[1, 4, 26, 32, 30, 28]);
        let (x, y) = button_middles(2, 2)[2];

        watch.tap(x, y);

        assert_eq!(watch.effects_of(EffectKind::VibratePattern)[0].number(0), 0);
        assert_eq!(
            watch.phone_lines(),
            ["{\"t\":\"app\",\"ch\":\"remote\",\"d\":{\"button\":2}}\n"]
        );
        assert!(watch.failures().is_empty());
    }

    #[test]
    fn the_remote_shows_the_buttons_the_phone_sends_while_it_was_away() {
        let mut watch = open_with_buttons(&file_of(1));
        watch.write_file(BUTTONS_PATH, &file_of(3));

        watch.reopen();

        let third = button_middles(2, 2)[2];
        assert_eq!(buttons_tapped(&mut watch, &[third]), [Some("2".to_string())]);
        watch.write_file(BUTTONS_PATH, &file_of(2));
        watch.screen_off();
        watch.screen_on();
        let second = button_middles(2, 1)[1];
        assert_eq!(buttons_tapped(&mut watch, &[second]), [Some("1".to_string())]);
        assert!(watch.failures().is_empty());
    }

    #[test]
    fn the_remote_draws_an_icon_beyond_the_library_as_none() {
        let mut watch = open_with_buttons(&[1, 2, 200, 32]);

        assert_eq!(watch.pixels_left_stale(), 0);
        assert!(watch.failures().is_empty());
    }
}

// ============================================================================
// Software
// ============================================================================

/// Taps a package's row in the Software app, scrolling the list down to it first when it is out of view.
fn tap_package(watch: &mut TestWatch, name: &str) {
    if !watch.shows_text(name) {
        watch.drag((120, 220), (120, 40));
    }
    watch.tap_text(name);
}

#[test]
fn the_software_app_lists_every_package_with_its_memory() {
    let watch = TestWatch::open_app("software").unwrap();

    assert!(
        watch.shows_text("Each app may use 27 KB"),
        "{:?}",
        watch.visible_texts()
    );
    assert!(watch.shows_text("Atlas"));
    assert!(watch.shows_text("41 KB"));
    assert!(watch.shows_text("Dice"));
    assert!(watch.shows_text("Face, 13 KB"));
}

#[test]
fn the_software_app_turns_an_app_on_when_the_watch_allows_it() {
    let mut watch = TestWatch::open_app("software").unwrap();

    tap_package(&mut watch, "Dice");

    let asked = watch.effects_of(EffectKind::SetAppEnabled);
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].text(0), Some("dice"));
    assert_eq!(asked[0].number(1), 1);
    assert!(watch.system().packages[1].enabled);
    assert_eq!(watch.screen_name(), Some("main"));
}

#[test]
fn the_software_app_turns_an_app_off_too() {
    let mut watch = TestWatch::open_app("software").unwrap();

    tap_package(&mut watch, "Timer");

    assert_eq!(watch.effects_of(EffectKind::SetAppEnabled)[0].number(1), 0);
    assert!(!watch.system().packages[5].enabled);
}

#[test]
fn the_software_app_says_why_a_switch_is_refused_and_changes_nothing() {
    let cases = [
        ("Atlas", "Needs more memory than an app can have"),
        ("Digital", "The last watch face stays on"),
        ("Apps", "The launcher stays on"),
        ("Software", "It is open now"),
    ];
    for (name, reason) in cases {
        let mut watch = TestWatch::open_app("software").unwrap();

        tap_package(&mut watch, name);

        assert_eq!(watch.screen_name(), Some("refused"), "{name}");
        assert!(watch.shows_text(reason), "{:?}", watch.visible_texts());
        assert!(watch.effects_of(EffectKind::SetAppEnabled).is_empty());
        watch.tap_text("OK");
        assert_eq!(watch.screen_name(), Some("main"));
    }
}

#[test]
fn the_software_app_shows_the_need_against_the_budget_for_an_app_too_large() {
    let mut watch = TestWatch::open_app("software").unwrap();

    tap_package(&mut watch, "Atlas");

    assert!(
        watch.shows_text("Needs 41 KB of 27 KB"),
        "{:?}",
        watch.visible_texts()
    );
}

// ============================================================================
// Four in a Row
// ============================================================================

/// The middle of the right column's top place, where the aimed counter hangs, and of its bottom place.
const RIGHT_COLUMN_X: i32 = 222;
const AIMED_COUNTER_Y: i32 = 18;
const BOTTOM_PLACE_Y: i32 = 222;

#[test]
fn four_in_a_row_drops_a_counter_into_the_right_column_on_screen() {
    let mut watch = TestWatch::open_app("four-in-a-row").unwrap();
    watch.swipe(Direction::Down);
    watch.tap(RIGHT_COLUMN_X, 120);
    let red = watch
        .fresh_frame()
        .pixel(RIGHT_COLUMN_X as usize, AIMED_COUNTER_Y as usize);

    watch.swipe(Direction::Down);
    let frame = watch.fresh_frame();

    assert_eq!(frame.pixel(RIGHT_COLUMN_X as usize, BOTTOM_PLACE_Y as usize), red);
    assert_ne!(
        frame.pixel(239, BOTTOM_PLACE_Y as usize),
        red,
        "the board ends inside the screen"
    );
}

// ============================================================================
// Swipes a screen ignores
// ============================================================================

#[test]
fn a_swipe_past_the_steps_pages_goes_on_to_the_shell() {
    let mut watch = TestWatch::open_app("steps").unwrap();

    // Before the first page and past the last, and up and down, the shell has the swipe.
    assert_eq!(watch.swipe(Direction::Right), EventResult::NotHandled);
    assert_eq!(watch.swipe(Direction::Up), EventResult::NotHandled);
    assert_eq!(watch.swipe(Direction::Left), EventResult::Handled);
    assert!(watch.shows_text("Step history"), "{:?}", watch.visible_texts());
    assert_eq!(watch.swipe(Direction::Left), EventResult::Handled);
    assert_eq!(watch.swipe(Direction::Left), EventResult::NotHandled);
    assert!(watch.effects_of(EffectKind::Back).is_empty());
}

#[test]
fn a_swipe_past_the_system_apps_pages_goes_on_to_the_shell() {
    let mut watch = TestWatch::open_app("system").unwrap();

    // Before the first page and past the seventh, the last, and up and down, the shell has the swipe.
    assert_eq!(watch.swipe(Direction::Right), EventResult::NotHandled);
    assert_eq!(watch.swipe(Direction::Up), EventResult::NotHandled);
    for _ in 0..6 {
        assert_eq!(watch.swipe(Direction::Left), EventResult::Handled);
    }
    assert!(watch.shows_text("Licence"), "{:?}", watch.visible_texts());
    assert_eq!(watch.swipe(Direction::Left), EventResult::NotHandled);
    assert_eq!(watch.swipe(Direction::Down), EventResult::NotHandled);
    assert!(watch.effects_of(EffectKind::Back).is_empty());
}

// ============================================================================
// Highlighted keys
// ============================================================================

/// A point on one key of the open screen's ButtonMatrix, a quarter of the way across it.
fn key_point(watch: &TestWatch, row: i32, column: i32, keys_in_row: i32, row_count: i32) -> (i32, i32) {
    let keypad = watch
        .tappable_widgets()
        .into_iter()
        .find(|&widget_index| watch.widget_kind(widget_index) == Some(WidgetKind::ButtonMatrix))
        .expect("a keypad");
    let area = watch.visible_area(keypad).unwrap();
    let key_width = area.width / keys_in_row;
    let key_height = area.height / row_count;
    let x = area.x + key_width * column + key_width / 4;
    let y = area.y + key_height * row + key_height / 2;
    (x, y)
}

/// The fill of one key of the open screen's ButtonMatrix, sampled beside its label.
fn key_fill(watch: &mut TestWatch, row: i32, column: i32, keys_in_row: i32, row_count: i32) -> String {
    let (x, y) = key_point(watch, row, column, keys_in_row, row_count);
    format!("{:?}", watch.frame().pixel(x as usize, y as usize))
}

/// Taps one key of the open screen's ButtonMatrix.
fn tap_key(watch: &mut TestWatch, row: i32, column: i32, keys_in_row: i32, row_count: i32) {
    let (x, y) = key_point(watch, row, column, keys_in_row, row_count);
    watch.tap(x, y);
}

#[test]
fn the_calculator_lights_only_the_operation_chosen() {
    let mut watch = TestWatch::open_app("calculator").unwrap();
    let plain = key_fill(&mut watch, 0, 1, 5, 4);

    assert_eq!(
        key_fill(&mut watch, 0, 0, 5, 4),
        plain,
        "the 7 key is unlit at first"
    );
    assert_eq!(
        key_fill(&mut watch, 1, 3, 5, 4),
        plain,
        "the + key is unlit at first"
    );
    tap_key(&mut watch, 0, 0, 5, 4);
    tap_key(&mut watch, 1, 3, 5, 4);
    assert_ne!(key_fill(&mut watch, 1, 3, 5, 4), plain, "the + key is lit");
    assert_eq!(key_fill(&mut watch, 0, 0, 5, 4), plain, "the 7 key stays unlit");
    assert_eq!(key_fill(&mut watch, 2, 3, 5, 4), plain, "the × key stays unlit");
}

#[test]
fn the_pomodoro_takes_a_long_break_after_every_fourth_work_session() {
    let mut watch = TestWatch::open_app("pomodoro").unwrap();
    let buzz = Duration::from_secs(10);
    watch.tap_text("Start");

    for work_session in 1..=4 {
        assert!(watch.shows_text("Work"), "{:?}", watch.visible_texts());
        assert!(
            watch.shows_text(&format!("{work_session} of 4")),
            "{:?}",
            watch.visible_texts()
        );
        watch.advance(Duration::from_secs(25 * 60));
        watch.advance(buzz);
        if work_session < 4 {
            assert!(watch.shows_text("Short break"), "{:?}", watch.visible_texts());
            watch.advance(Duration::from_secs(5 * 60));
            watch.advance(buzz);
        }
    }

    assert!(watch.shows_text("Long break"), "{:?}", watch.visible_texts());
    assert!(watch.shows_text("15:00"), "{:?}", watch.visible_texts());
    watch.advance(Duration::from_secs(15 * 60));
    watch.advance(buzz);
    assert!(watch.shows_text("1 of 4"), "{:?}", watch.visible_texts());
    assert!(watch.failures().is_empty(), "{:?}", watch.failures());
}

// ============================================================================
// Prompt
// ============================================================================

mod prompt {
    use super::*;
    use neotime_app_testing::{compile_package, EffectKind, FakeSystem, PackageKind};

    /// The prompt opened, as the phone runs it, with `question` in its data file.
    fn open_with_question(question: &str) -> TestWatch {
        let mut system = FakeSystem::new();
        system.write_file("/temp/prompt/text", question.as_bytes());
        let image = compile_package(PackageKind::App, "prompt").unwrap();
        TestWatch::open_image_on(image, system).unwrap()
    }

    #[test]
    fn the_prompt_shows_the_phones_question_and_sends_yes_then_leaves() {
        let mut watch = open_with_question("Leave now?");
        assert!(watch.shows_text("Leave now?"), "{:?}", watch.visible_texts());

        watch.tap_text("Yes");

        assert_eq!(
            watch.phone_lines(),
            ["{\"t\":\"app\",\"ch\":\"prompt\",\"d\":{\"answer\":1}}\n"]
        );
        assert_eq!(watch.effects_of(EffectKind::Back).len(), 1);
        assert!(watch.failures().is_empty(), "{:?}", watch.failures());
    }

    #[test]
    fn the_prompt_sends_no_when_no_is_tapped() {
        let mut watch = open_with_question("Leave now?");

        watch.tap_text("No");

        assert_eq!(
            watch.phone_lines(),
            ["{\"t\":\"app\",\"ch\":\"prompt\",\"d\":{\"answer\":0}}\n"]
        );
    }

    #[test]
    fn the_prompt_says_so_when_the_phone_sent_no_question() {
        let watch = open_with_question("");

        assert!(
            watch.shows_text("No question from"),
            "{:?}",
            watch.visible_texts()
        );
    }
}

// ============================================================================
// Paging through a file
// ============================================================================

const PAGING_APP: &str = r#"
import { signal } from "@preact/signals";
import { define_app, file_bytes, prefetch_file } from "neotime";
import { Label, Screen } from "neotime/widgets";
const page = signal(0);
const bytes = signal([0, 0, 0, 0]);
function turn() {
  bytes.value = file_bytes("/apps/pager/data", 4, page.value * 4);
  page.value = page.value + 1;
  prefetch_file("/apps/pager/data", page.value * 4, 4);
}
function Main() {
  return <Screen onClick={turn}><Label>{`${bytes.value[0]} ${bytes.value[3]}`}</Label></Screen>;
}
export default define_app({ id: "pager", name: "Pager", screens: { main: Main } });
"#;

#[test]
fn an_app_pages_through_a_file_and_reads_the_next_page_ahead() {
    let mut system = neotime_app_testing::FakeSystem::new();
    system.write_file("/apps/pager/data", &(10..30).collect::<Vec<u8>>());
    let image = neotime_app_testing::compile_source_text(PAGING_APP).unwrap();
    let mut watch = TestWatch::open_image_on(image, system).unwrap();

    watch.tap(120, 120);
    watch.tap(120, 120);

    assert!(watch.shows_text("14 17"), "{:?}", watch.visible_texts());
    let prefetches = watch.effects_of(EffectKind::PrefetchFile);
    assert_eq!(prefetches.len(), 2);
    assert_eq!(prefetches[1].text(0), Some("/apps/pager/data"));
    assert_eq!((prefetches[1].number(1), prefetches[1].number(2)), (8, 4));
}
