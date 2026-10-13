//! Tests for `send_to_phone` and `library_icon`.

use neotime_runtime::instruction::EffectKind;
use neotime_runtime::{AppImage, AppState, Interpreter, SystemValue, SystemValues, Value};

use crate::compile_source;
use crate::types::{icon_number, ICON_NAMES};

struct System;

impl SystemValues for System {
    fn read(&self, _value: SystemValue) -> Value {
        0
    }

    fn random_number(&self) -> u32 {
        7
    }
}

/// An app whose screen calls `tap` on a swipe, with `body` before the screen and `label` shown.
fn source(body: &str, label: &str, permissions: &str) -> String {
    format!(
        r#"
import {{ signal }} from "@preact/signals";
import {{ define_app, send_to_phone, type SwipeEvent }} from "neotime";
import {{ icons, library_icon }} from "neotime/icons";
import {{ Icon, Label, Screen }} from "neotime/widgets";

const count = signal(3);
{body}

function Main() {{
  return <Screen onSwipe={{tap}}>{label}</Screen>;
}}

export default define_app({{ id: "test", name: "Test", screens: {{ main: Main }}, permissions: [{permissions}] }});
"#
    )
}

fn compiled(body: &str, label: &str) -> Vec<u8> {
    let source = source(body, label, "\"phone_messages\"");
    let output = compile_source(&source);
    let errors: Vec<String> = output
        .errors()
        .map(|error| error.render("test.tsx", &source))
        .collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    output.image.expect("an image")
}

fn first_error(body: &str, permissions: &str) -> String {
    let output = compile_source(&source(body, "<Label>Hi</Label>", permissions));
    let message = output.errors().next().expect("an error").message.clone();
    message
}

/// The channel and data texts of each message the swipe handler sends.
fn messages_sent(body: &str) -> Vec<(String, String)> {
    let bytes = compiled(body, "<Label>Hi</Label>");
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let mut interpreter = Interpreter::new();
    let outcome = interpreter
        .run_handler(&image, 0, &mut state, &[0], &System)
        .unwrap();
    outcome
        .effects()
        .iter()
        .filter(|effect| effect.kind == EffectKind::SendToPhone)
        .map(|effect| {
            let text = |value| interpreter.text(&image, value).unwrap().to_string();
            (text(effect.arguments[0]), text(effect.arguments[1]))
        })
        .collect()
}

/// The value the first expression gives.
fn shown_value(label: &str) -> Value {
    let bytes = compiled("function tap(event: SwipeEvent) {}", label);
    let image = AppImage::parse(&bytes).unwrap();
    let state = AppState::new(&image).unwrap();
    Interpreter::new()
        .evaluate_expression(&image, 0, &state, &System)
        .unwrap()
}

// ============================================================================
// send_to_phone
// ============================================================================

#[test]
fn send_to_phone_sends_the_channel_and_its_numbers_as_json_text() {
    let sent = messages_sent(
        r#"function tap(event: SwipeEvent) { send_to_phone("remote", { button: count.value - 1, "x-y": -40, z_3: 7, last: 0 }); }"#,
    );

    assert_eq!(
        sent,
        [(
            "remote".to_string(),
            r#"{"button":2,"x-y":-40,"z_3":7,"last":0}"#.to_string()
        )]
    );
}

#[test]
fn send_to_phone_with_no_fields_sends_an_empty_object() {
    let sent = messages_sent(r#"function tap(event: SwipeEvent) { send_to_phone("ping", {}); }"#);

    assert_eq!(sent, [("ping".to_string(), "{}".to_string())]);
}

#[test]
fn send_to_phone_checks_its_channel_fields_and_permission() {
    let call = |arguments: &str| {
        first_error(
            &format!("function tap(event: SwipeEvent) {{ send_to_phone({arguments}); }}"),
            "\"phone_messages\"",
        )
    };

    assert!(call(r#""has space", { a: 1 }"#).contains("channel must be written out"));
    assert!(call(r#""remote", count.value"#).contains("object written out"));
    assert!(call(r#""remote", { a: 1, b: 2, c: 3, d: 4, e: 5 }"#).contains("at most 4 fields"));
    assert!(call(r#""remote", { "a b": 1 }"#).contains("field's name"));
    assert!(call(r#""remote", { ["a"]: 1 }"#).contains("field's name"));
    assert!(call(r#""remote", { ...{ a: 1 } }"#).contains("without `...`"));
    assert!(call(r#""remote", { a: "text" }"#).contains("number"));
    assert!(call(r#""remote""#).contains("takes a channel"));
    let error = first_error(
        r#"function tap(event: SwipeEvent) { send_to_phone("remote", { a: 1 }); }"#,
        "",
    );
    assert!(error.contains("phone_messages"), "{error}");
}

#[test]
fn send_to_phone_belongs_on_its_own_line_in_a_handler() {
    let error = first_error(
        r#"function tap(event: SwipeEvent) { const sent = send_to_phone("remote", { a: 1 }); }"#,
        "\"phone_messages\"",
    );
    assert!(error.contains("its own line in a handler"), "{error}");
}

// ============================================================================
// library_icon
// ============================================================================

#[test]
fn library_icon_numbers_icons_in_library_order_and_gives_none_outside_it() {
    let label = |index: &str| format!("<Icon icon={{library_icon(count.value * 0 + {index})}} />");
    let none = icon_number("none").unwrap();

    assert_eq!(shown_value(&label("0")), icon_number(ICON_NAMES[0]).unwrap());
    assert_eq!(shown_value(&label("3")), 32768 + 3);
    assert_eq!(shown_value(&label("62")), icon_number("info").unwrap());
    assert_eq!(shown_value(&label(&ICON_NAMES.len().to_string())), none);
    assert_eq!(shown_value(&label("-1")), none);
}
