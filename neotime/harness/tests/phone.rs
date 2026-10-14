//! Companion lines from the phone, read as the watch reads them and passed to apps and faces.

use neotime_app_testing::{EffectKind, SystemValue, TestWatch, Value};
use neotime_runtime::SystemValues;

/// Counts the calls of each kind, as the `call` subscription reports them.
const CALL_LISTENER_APP: &str = r#"
// Counts the phone's calls by kind.
import { define_app, on, signal, type CallEvent } from "neotime";
import { Label, Screen } from "neotime/widgets";

const incoming = signal(0);
const answered = signal(0);
const ended = signal(0);

function count(event: CallEvent) {
  if (event.kind === "incoming") incoming.value = incoming.value + 1;
  else if (event.kind === "answered") answered.value = answered.value + 1;
  else ended.value = ended.value + 1;
}

function Calls() {
  return (
    <Screen>
      <Label>{`Incoming ${incoming.value}`}</Label>
      <Label>{`Answered ${answered.value}`}</Label>
      <Label>{`Ended ${ended.value}`}</Label>
    </Screen>
  );
}

on("call", count, { while_open: true });

export default define_app({
  id: "calls",
  name: "Calls",
  screens: { main: Calls },
});
"#;

#[test]
fn calls_from_the_phone_reach_the_call_subscription() {
    let mut watch = TestWatch::open_source(CALL_LISTENER_APP).unwrap();

    watch
        .receive_from_phone(r#"GB({"t":"call","cmd":"incoming","name":"Sam","number":"+15550100"})"#)
        .unwrap();
    watch
        .receive_from_phone(r#"GB({"t":"call","cmd":"accept"})"#)
        .unwrap();
    watch
        .receive_from_phone(r#"GB({"t":"call","cmd":"end"})"#)
        .unwrap();

    assert!(watch.shows_text("Incoming 1"), "{:?}", watch.visible_texts());
    assert!(watch.shows_text("Answered 1"));
    assert!(watch.shows_text("Ended 1"));
    assert!(watch.failures().is_empty(), "{:?}", watch.failures());
}

#[test]
fn the_call_app_sends_its_answers_to_the_phone_and_leaves_when_the_call_ends() {
    let mut watch = TestWatch::open_app("call").unwrap();
    watch
        .receive_from_phone(r#"GB({"t":"call","cmd":"incoming","number":"+15550100"})"#)
        .unwrap();
    assert!(watch.shows_text("+15550100"), "{:?}", watch.visible_texts());

    watch.tap_text("Mute");
    watch.tap_text("Accept");
    assert_eq!(
        watch.phone_lines(),
        [
            "{\"t\":\"call\",\"n\":\"IGNORE\"}\n",
            "{\"t\":\"call\",\"n\":\"ACCEPT\"}\n"
        ]
    );
    assert!(watch.effects_of(EffectKind::Back).is_empty());

    watch
        .receive_from_phone(r#"GB({"t":"call","cmd":"end","number":"+15550100"})"#)
        .unwrap();
    assert_eq!(watch.effects_of(EffectKind::Back).len(), 1);
    assert!(watch.shows_text("No call"));
    assert!(watch.failures().is_empty(), "{:?}", watch.failures());
}

#[test]
fn rejecting_a_call_tells_the_phone_and_leaves() {
    let mut watch = TestWatch::open_app("call").unwrap();
    watch
        .receive_from_phone(r#"GB({"t":"call","cmd":"incoming","name":"Sam"})"#)
        .unwrap();
    watch.tap_text("Reject");
    assert_eq!(watch.phone_lines(), ["{\"t\":\"call\",\"n\":\"REJECT\"}\n"]);
    assert_eq!(watch.effects_of(EffectKind::Back).len(), 1);
}

#[test]
fn the_phones_weather_replaces_the_forecast_and_is_stamped_with_its_arrival() {
    let mut watch = TestWatch::open_app("weather").unwrap();

    watch
        .receive_from_phone(r#"{"t":"weather","temp":271,"code":601,"loc":"North Pole"}"#)
        .unwrap();

    let system = watch.system();
    assert_eq!(system.read(SystemValue::WeatherTemperature), -2);
    assert_eq!(system.read(SystemValue::WeatherHigh), Value::MIN);
    assert_eq!(system.read(SystemValue::WeatherReceivedAt), watch.now());
    assert_eq!(system.read(SystemValue::WeatherForecastLength), 0);
    assert_eq!(system.read_text(SystemValue::WeatherLocation), "North Pole");
    assert!(watch.shows_text("North Pole"), "{:?}", watch.visible_texts());
    assert!(watch.shows_text("Snow"));
}

#[test]
fn a_face_shows_the_phones_temperature() {
    let mut watch = TestWatch::open_face("digital").unwrap();
    assert!(!watch.visible_texts().iter().any(|text| text.contains("-7")));

    watch
        .receive_from_phone(r#"{"t":"weather","temp":266,"code":800}"#)
        .unwrap();

    assert!(
        watch.visible_texts().iter().any(|text| text.contains("-7")),
        "{:?}",
        watch.visible_texts()
    );
    assert!(watch.failures().is_empty());
}

#[test]
fn a_line_the_watch_cannot_read_or_does_not_pass_on_is_refused() {
    let mut watch = TestWatch::open_app("music").unwrap();

    let unreadable = watch.receive_from_phone("GB(oops)").unwrap_err();
    let not_for_apps = watch.receive_from_phone(r#"{"t":"settings"}"#).unwrap_err();

    assert!(unreadable.contains("cannot read"), "{unreadable}");
    assert!(not_for_apps.contains("is not a call"), "{not_for_apps}");
}
