//! Messages from the phone for one watch app, `{"t":"app","ch":"<app id>","d":{...}}`, heard by its
//! `message` handler whether the app is open or closed.

use std::time::Duration;

use battletime_app_testing::{EffectKind, TestWatch};

/// Shows the level of the latest note and counts the notes heard, toasts its label, and wakes the screen for an urgent one.
const NOTES_APP: &str = r#"
import { define_app, on, persisted, show_toast, wake_screen } from "battletime";
import { Label, Screen } from "battletime/widgets";

type Note = { level: number; label: string; urgent: boolean };

const level = persisted("level", 0);
const heard = persisted("heard", 0);

function hear(note: Note) {
  level.value = note.level;
  show_toast(note.label);
  heard.value = heard.value + 1;
  if (note.urgent) wake_screen();
}

function Notes() {
  return (
    <Screen>
      <Label>{`Level ${level.value}`}</Label>
      <Label>{`Heard ${heard.value}`}</Label>
    </Screen>
  );
}

on("message", hear);

export default define_app({ id: "notes", name: "Notes", screens: { main: Notes } });
"#;

fn note_line(data: &str) -> String {
    format!(r#"{{"t":"app","ch":"notes","d":{data}}}"#)
}

/// The text of the latest toast the app asked for.
fn last_toast(watch: &TestWatch) -> Option<String> {
    let toasts = watch.effects_of(EffectKind::ShowToast);
    toasts.last().and_then(|toast| toast.text(0)).map(String::from)
}

// ============================================================================
// The open app
// ============================================================================

#[test]
fn the_open_app_hears_the_numbers_and_text_under_its_keys() {
    let mut watch = TestWatch::open_source(NOTES_APP).unwrap();

    watch
        .receive_from_phone(&note_line(r#"{"level":7,"label":"Café \"one\""}"#))
        .unwrap();

    assert!(watch.shows_text("Level 7"), "{:?}", watch.visible_texts());
    assert!(watch.shows_text("Heard 1"));
    assert_eq!(last_toast(&watch).as_deref(), Some("Café \"one\""));
    assert!(watch.failures().is_empty(), "{:?}", watch.failures());
}

#[test]
fn keys_the_message_lacks_read_as_zero_and_empty_text() {
    let mut watch = TestWatch::open_source(NOTES_APP).unwrap();
    watch
        .receive_from_phone(&note_line(r#"{"level":7,"label":"first"}"#))
        .unwrap();

    watch
        .receive_from_phone(&format!("GB({})", note_line(r#"{"other":3}"#)))
        .unwrap();

    assert!(watch.shows_text("Level 0"), "{:?}", watch.visible_texts());
    assert!(watch.shows_text("Heard 2"));
    assert_eq!(last_toast(&watch).as_deref(), Some(""));
}

#[test]
fn a_message_for_another_app_is_not_heard() {
    let mut watch = TestWatch::open_source(NOTES_APP).unwrap();

    watch
        .receive_from_phone(r#"{"t":"app","ch":"elsewhere","d":{"level":7}}"#)
        .unwrap();

    assert!(watch.shows_text("Heard 0"), "{:?}", watch.visible_texts());
}

#[test]
fn malformed_message_lines_are_refused() {
    let mut watch = TestWatch::open_source(NOTES_APP).unwrap();

    for data in [
        r#"{"a":1,"b":2,"c":3,"d":4,"e":5}"#,
        r#"{"a":"one","b":"two"}"#,
        r#"{"level":null}"#,
        "[1]",
    ] {
        let line = note_line(data);
        assert!(watch.receive_from_phone(&line).is_err(), "{line}");
    }
    assert!(watch
        .receive_from_phone(r#"{"t":"app","d":{"level":1}}"#)
        .is_err());
    assert!(watch.shows_text("Heard 0"), "{:?}", watch.visible_texts());
}

// ============================================================================
// A closed app
// ============================================================================

#[test]
fn a_closed_app_runs_its_handler_in_the_background_and_saves_its_state() {
    let mut watch = TestWatch::open_source(NOTES_APP).unwrap();
    watch.switch_to_app("stopwatch").unwrap();

    watch
        .receive_from_phone(&note_line(r#"{"level":4,"label":"away"}"#))
        .unwrap();
    assert_eq!(watch.id(), "stopwatch");

    watch.switch_to_app("notes").unwrap();
    assert!(watch.shows_text("Level 4"), "{:?}", watch.visible_texts());
    assert!(watch.shows_text("Heard 1"), "{:?}", watch.visible_texts());
    assert!(watch.failures().is_empty(), "{:?}", watch.failures());
}

#[test]
fn a_closed_app_that_wakes_the_screen_opens_in_front() {
    let mut watch = TestWatch::open_source(NOTES_APP).unwrap();
    watch.switch_to_app("stopwatch").unwrap();
    watch.screen_off();

    watch
        .receive_from_phone(&note_line(r#"{"level":9,"urgent":true}"#))
        .unwrap();

    assert_eq!(watch.id(), "notes");
    assert!(watch.is_screen_on());
    assert!(watch.shows_text("Level 9"), "{:?}", watch.visible_texts());
}

#[test]
fn a_handler_only_while_open_is_not_run_for_a_closed_app() {
    let source = NOTES_APP.replace(
        r#"on("message", hear);"#,
        r#"on("message", hear, { while_open: true });"#,
    );
    let mut watch = TestWatch::open_source(&source).unwrap();
    watch.switch_to_app("stopwatch").unwrap();

    watch.receive_from_phone(&note_line(r#"{"level":4}"#)).unwrap();
    watch.switch_to_app("notes").unwrap();

    assert!(watch.shows_text("Heard 0"), "{:?}", watch.visible_texts());
}

// ============================================================================
// The SOS app
// ============================================================================

/// Holds the SOS button long enough to send the SOS.
fn start_sos(watch: &mut TestWatch) {
    watch.press(120, 120);
    watch.advance(Duration::from_millis(5200));
    watch.release(120, 120);
    assert!(watch.shows_text("SOS sent"), "{:?}", watch.visible_texts());
}

#[test]
fn the_phone_stops_the_open_sos_without_being_told_back() {
    let mut watch = TestWatch::open_app("sos").unwrap();
    start_sos(&mut watch);
    let lines_before = watch.phone_lines().len();

    watch
        .receive_from_phone(r#"{"t":"app","ch":"sos","d":{"on":1}}"#)
        .unwrap();
    assert!(watch.shows_text("SOS sent"), "{:?}", watch.visible_texts());

    watch
        .receive_from_phone(r#"{"t":"app","ch":"sos","d":{"on":0}}"#)
        .unwrap();
    assert!(
        watch.shows_text("Hold for 5 seconds"),
        "{:?}",
        watch.visible_texts()
    );
    assert_eq!(watch.persisted_value("is_active", 0), Some(0));
    assert_eq!(watch.persisted_value("reached_phone", 0), Some(0));
    assert_eq!(
        watch.phone_lines().len(),
        lines_before,
        "{:?}",
        watch.phone_lines()
    );
    let led_off = watch
        .effects_of(EffectKind::SetHeartRateLed)
        .last()
        .map(|effect| effect.number(0));
    assert_eq!(led_off, Some(0));
    assert!(watch.failures().is_empty(), "{:?}", watch.failures());
}

#[test]
fn the_phone_stops_a_closed_sos_in_the_background() {
    let mut watch = TestWatch::open_app("sos").unwrap();
    start_sos(&mut watch);
    watch.switch_to_app("stopwatch").unwrap();
    let lines_before = watch.phone_lines().len();

    let line = r#"{"t":"app","ch":"sos","d":{"on":0}}"#;
    watch.receive_from_phone(line).unwrap();
    assert_eq!(watch.id(), "stopwatch");

    watch.switch_to_app("sos").unwrap();
    assert!(
        watch.shows_text("Hold for 5 seconds"),
        "{:?}",
        watch.visible_texts()
    );
    assert_eq!(
        watch.phone_lines().len(),
        lines_before,
        "{:?}",
        watch.phone_lines()
    );
    assert!(watch.failures().is_empty(), "{:?}", watch.failures());
}
