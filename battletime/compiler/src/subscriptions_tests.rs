//! Tests for the events an app connects to with `on`, read back from the compiled image.

use battletime_runtime::subscriptions::{Subscription, SubscriptionKind, SubscriptionValue};
use battletime_runtime::{AppImage, AppState, Interpreter, SystemValue, SystemValues, Value, NONE};

use super::EVENTS;
use crate::compile_source;
use crate::permissions::Permission;

// ============================================================================
// Helpers
// ============================================================================

/// An app that declares `declarations` and then connects the `on` calls in `listeners`.
fn app_with(declarations: &str, listeners: &str) -> String {
    format!(
        r#"
import {{
  computed, define_app, on, signal, vibrate, EVERY_DAY, MINUTE, SECOND, type Instant, type TimeOfDay,
}} from "battletime";
import {{ Label, Screen }} from "battletime/widgets";

const count = signal(0);
function tick() {{ count.value += 1; }}
{declarations}
function Main() {{ return <Screen><Label>{{count.value}}</Label></Screen>; }}

{listeners}

export default define_app({{ id: "test", name: "Test", screens: {{ main: Main }} }});
"#
    )
}

/// An app whose only listener is the one `on(...)` call given.
fn listening(declarations: &str, call: &str) -> String {
    app_with(declarations, &format!("{call};"))
}

/// Compiles an app, failing the test on any error, and returns the subscriptions its image lists.
fn subscriptions_of(source: &str) -> Vec<Subscription> {
    let output = compile_source(source);
    let errors: Vec<String> = output.errors().map(|error| error.message.clone()).collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    let bytes = output.image.expect("an image");
    let image = AppImage::parse(&bytes).unwrap();
    (0..image.subscription_count())
        .map(|index| image.subscription(index).unwrap())
        .filter(|subscription| subscription.kind != SubscriptionKind::Derive)
        .collect()
}

/// The one subscription an app lists, apart from any the compiler adds for derived values.
fn only_subscription(source: &str) -> Subscription {
    let subscriptions = subscriptions_of(source);
    assert_eq!(subscriptions.len(), 1, "{subscriptions:?}");
    subscriptions[0]
}

/// The first error from compiling an app.
fn first_error(source: &str) -> String {
    let output = compile_source(source);
    let message = output.errors().next().expect("an error").message.clone();
    message
}

// ============================================================================
// Event names
// ============================================================================

#[test]
fn every_event_name_is_written_once() {
    for (place, (name, _, _)) in EVENTS.iter().enumerate() {
        assert!(
            EVENTS[place + 1..].iter().all(|(other, _, _)| other != name),
            "`{name}` is listed twice"
        );
    }
}

#[test]
fn each_detail_event_becomes_its_kind_and_position() {
    let source = app_with(
        "",
        r#"on("day", tick); on("charging_completes", tick); on("phone_disconnects", tick); on("moving", tick); on("wrist_tap", tick);"#,
    );

    let details: Vec<(SubscriptionKind, u8)> = subscriptions_of(&source)
        .iter()
        .map(|subscription| (subscription.kind, subscription.detail))
        .collect();

    assert_eq!(
        details,
        [
            (SubscriptionKind::ClockChange, 2),
            (SubscriptionKind::Battery, 3),
            (SubscriptionKind::Phone, 1),
            (SubscriptionKind::Motion, 5),
            (SubscriptionKind::Motion, 2),
        ]
    );
}

#[test]
fn an_unknown_event_lists_the_choices() {
    let message = first_error(&listening("", r#"on("wake", tick)"#));

    assert!(
        message.starts_with("there is no event `wake`; choose from open, close"),
        "{message}"
    );
}

#[test]
fn an_event_name_that_is_not_text_is_refused() {
    let message = first_error(&listening("", "on(3, tick)"));

    assert!(
        message.starts_with("name the event with text written out"),
        "{message}"
    );
}

#[test]
fn the_message_event_is_not_in_the_image_format_yet() {
    let message = first_error(&listening("", r#"on("message", tick)"#));

    assert!(
        message.starts_with("the `message` event is not in the image format yet"),
        "{message}"
    );
}

#[test]
fn an_event_without_detail_or_period_writes_only_its_kind_and_handler() {
    let source = app_with("", r#"on("notification", tick); on("settings_change", tick);"#);

    let subscriptions = subscriptions_of(&source);

    let kinds: Vec<SubscriptionKind> = subscriptions
        .iter()
        .map(|subscription| subscription.kind)
        .collect();
    assert_eq!(
        kinds,
        [SubscriptionKind::Notification, SubscriptionKind::SettingsChange]
    );
    assert!(subscriptions.iter().all(
        |subscription| subscription.value == SubscriptionValue::Constant(0) && subscription.detail == 0
    ));
}

// ============================================================================
// Lifecycle events
// ============================================================================

#[test]
fn lifecycle_events_are_written_in_the_order_connected() {
    let source = app_with(
        "",
        r#"on("close", tick); on("screen_on", tick); on("screen_off", () => { count.value = 0; }); on("open", tick);"#,
    );

    let kinds: Vec<SubscriptionKind> = subscriptions_of(&source)
        .iter()
        .map(|subscription| subscription.kind)
        .collect();

    assert_eq!(
        kinds,
        [
            SubscriptionKind::Close,
            SubscriptionKind::ScreenOn,
            SubscriptionKind::ScreenOff,
            SubscriptionKind::Open
        ]
    );
}

#[test]
fn a_lifecycle_event_takes_no_while_open() {
    let message = first_error(&listening("", r#"on("open", tick, { while_open: true })"#));

    assert!(message.contains("only comes while the app is open"), "{message}");
}

#[test]
fn a_lifecycle_event_needs_no_permission() {
    let output = compile_source(&listening("", r#"on("open", tick)"#));

    assert!(output.permissions.is_empty(), "{:?}", output.permissions);
}

// ============================================================================
// The call
// ============================================================================

#[test]
fn on_inside_a_handler_is_refused() {
    let source = app_with(
        "function start() { on(\"steps\", tick); }",
        r#"on("open", start);"#,
    );

    let message = first_error(&source);

    assert!(
        message.contains("connects events only at the top of the file"),
        "{message}"
    );
}

#[test]
fn a_call_to_the_apps_own_function_at_the_top_is_refused() {
    let message = first_error(&listening("function make() { return 1; }", "make()"));

    assert!(
        message.starts_with("only declarations and `on(...)` may sit at the top"),
        "{message}"
    );
}

#[test]
fn a_statement_that_is_not_a_call_at_the_top_is_refused() {
    let message = first_error(&listening("", "count.value + 1"));

    assert!(
        message.starts_with("only declarations and `on(...)` may sit at the top"),
        "{message}"
    );
}

#[test]
fn on_after_define_app_is_refused() {
    let source = format!("{}\non(\"steps\", tick);\n", listening("", r#"on("open", tick)"#));

    assert_eq!(
        first_error(&source),
        "connect events with `on` before `export default define_app`"
    );
}

#[test]
fn spread_arguments_in_on_are_refused() {
    let message = first_error(&listening("const extra = [tick];", r#"on("steps", ...extra)"#));

    assert_eq!(message, "`...` in `on` is not supported");
}

#[test]
fn on_without_a_handler_is_refused() {
    let message = first_error(&listening("", r#"on("steps")"#));

    assert!(message.starts_with("write `on(\"event\", handler)`"), "{message}");
}

#[test]
fn on_with_too_many_arguments_is_refused() {
    let message = first_error(&listening("", r#"on("steps", tick, {}, 3)"#));

    assert_eq!(message, "`on` takes an event, a handler and its options");
}

#[test]
fn a_handler_that_is_not_a_function_is_refused() {
    let message = first_error(&listening(
        "const nothing = undefined;",
        r#"on("steps", nothing)"#,
    ));

    assert_eq!(
        message,
        "give the subscription a handler, such as a function the app defines"
    );
}

#[test]
fn an_effect_with_no_arguments_may_be_the_handler() {
    let source = r#"
import { define_app, on, back } from "battletime";
import { Screen } from "battletime/widgets";
function Main() { return <Screen></Screen>; }
on("shake", back, { while_open: true });
export default define_app({ id: "t", name: "T", screens: { main: Main } });
"#;

    assert_eq!(only_subscription(source).kind, SubscriptionKind::Motion);
}

// ============================================================================
// define_app's old options
// ============================================================================

#[test]
fn define_app_refuses_subscriptions_lifecycle_options_and_permissions() {
    let cases = [
        ("subscriptions: [],", "connect events with `on` above define_app"),
        ("onOpen: tick,", "connect `onOpen` with `on` above define_app"),
        ("onScreenOff: tick,", "connect `onScreenOff` with `on`"),
        (r#"permissions: ["motion"],"#, "remove `permissions`"),
    ];
    for (option, advice) in cases {
        let source = format!(
            r#"
import {{ define_app, signal }} from "battletime";
import {{ Screen }} from "battletime/widgets";
const count = signal(0);
function tick() {{ count.value += 1; }}
function Main() {{ return <Screen></Screen>; }}
export default define_app({{ id: "t", name: "T", screens: {{ main: Main }}, {option} }});
"#
        );
        let message = first_error(&source);
        assert!(message.starts_with(advice), "{option}: {message}");
    }
}

// ============================================================================
// Options
// ============================================================================

#[test]
fn while_open_is_written_into_the_subscription() {
    assert!(only_subscription(&listening("", r#"on("steps", tick, { while_open: true })"#)).only_while_open);
}

#[test]
fn while_open_false_keeps_coming_in_the_background() {
    assert!(
        !only_subscription(&listening("", r#"on("steps", tick, { while_open: false })"#)).only_while_open
    );
}

#[test]
fn options_that_are_not_an_object_are_refused() {
    let message = first_error(&listening("", r#"on("steps", tick, true)"#));

    assert!(
        message.starts_with("write `on`'s options as an object"),
        "{message}"
    );
}

#[test]
fn options_with_a_spread_or_a_computed_name_are_refused() {
    let spread = first_error(&listening(
        "const more = {};",
        r#"on("steps", tick, { ...more })"#,
    ));
    let computed_name = first_error(&listening(
        r#"const field = "while_open";"#,
        r#"on("steps", tick, { [field]: true })"#,
    ));

    assert!(
        spread.starts_with("write `on`'s options out one by one"),
        "{spread}"
    );
    assert!(
        computed_name.starts_with("write `on`'s options with plain names"),
        "{computed_name}"
    );
}

#[test]
fn while_open_that_is_not_written_out_is_refused() {
    let message = first_error(&listening(
        "const yes = true;",
        r#"on("steps", tick, { while_open: yes })"#,
    ));

    assert_eq!(message, "`while_open` must be written out as true or false");
}

#[test]
fn an_unknown_option_is_refused() {
    let message = first_error(&listening("", r#"on("steps", tick, { repeat: true })"#));

    assert!(message.starts_with("`on` has no option `repeat`"), "{message}");
}

#[test]
fn an_option_the_event_does_not_take_is_refused() {
    let cases = [
        (r#"on("steps", tick, { every: MINUTE })"#, "takes no `every`"),
        (
            r#"on("tick", tick, { every: MINUTE, at: 5000 })"#,
            "takes no `at`",
        ),
        (
            r#"on("time", tick, { at: 5000, days: EVERY_DAY })"#,
            "takes no `days`",
        ),
    ];
    for (call, advice) in cases {
        let message = first_error(&listening("", call));
        assert!(message.contains(advice), "{call}: {message}");
    }
}

// ============================================================================
// Periods
// ============================================================================

#[test]
fn every_takes_milliseconds_or_a_period_that_adds_up_every_unit() {
    let in_milliseconds = only_subscription(&listening("", r#"on("tick", tick, { every: 2 * MINUTE })"#));
    let as_period = only_subscription(&listening(
        "",
        r#"on("tick", tick, { every: { hours: 1, minutes: 2, seconds: 3, milliseconds: 4 } })"#,
    ));

    assert_eq!(in_milliseconds.kind, SubscriptionKind::Every);
    assert_eq!(in_milliseconds.value, SubscriptionValue::Constant(120_000));
    assert_eq!(as_period.value, SubscriptionValue::Constant(3_723_004));
}

#[test]
fn a_heart_rate_period_is_kept_like_tick() {
    let subscription = only_subscription(&listening("", r#"on("heart_rate", tick, { every: 5 * MINUTE })"#));

    assert_eq!(subscription.kind, SubscriptionKind::HeartRate);
    assert_eq!(subscription.value, SubscriptionValue::Constant(300_000));
}

#[test]
fn tick_without_every_is_refused() {
    let message = first_error(&listening("", r#"on("tick", tick)"#));

    assert!(message.starts_with("the `tick` event needs `every`"), "{message}");
}

#[test]
fn a_period_not_known_when_compiling_is_refused() {
    let message = first_error(&listening("", r#"on("tick", tick, { every: count.value })"#));

    assert!(
        message.starts_with("a length of time must be known when compiling"),
        "{message}"
    );
}

#[test]
fn a_period_of_nothing_is_refused() {
    let message = first_error(&listening("", r#"on("tick", tick, { every: 0 })"#));

    assert!(
        message.starts_with("a length of time must be known when compiling and above zero"),
        "{message}"
    );
}

#[test]
fn a_period_in_an_unknown_unit_is_refused() {
    let message = first_error(&listening("", r#"on("tick", tick, { every: { days: 1 } })"#));

    assert_eq!(
        message,
        "a period has no `days`; use milliseconds, seconds, minutes or hours"
    );
}

#[test]
fn a_period_that_is_not_a_number_is_refused() {
    let message = first_error(&listening(
        "",
        r#"on("tick", tick, { every: { minutes: "five" } })"#,
    ));

    assert_eq!(message, "`minutes` must be a whole number");
}

#[test]
fn a_period_that_overflows_is_refused() {
    let message = first_error(&listening(
        "",
        r#"on("tick", tick, { every: { hours: 9000000000000000 } })"#,
    ));

    assert_eq!(message, "this period is too long");
}

#[test]
fn a_period_shorter_than_the_runtime_allows_is_refused() {
    let message = first_error(&listening(
        "",
        r#"on("tick", tick, { every: 5, while_open: true })"#,
    ));

    assert_eq!(
        message,
        "`tick` runs at most every 20 milliseconds, and this period is 5"
    );
}

#[test]
fn a_period_under_a_minute_must_come_only_while_open() {
    let message = first_error(&listening("", r#"on("tick", tick, { every: SECOND })"#));

    assert!(
        message.contains("add `while_open: true` to the `tick` event"),
        "{message}"
    );
}

#[test]
fn acceleration_must_come_only_while_the_app_is_open() {
    let message = first_error(&listening("", r#"on("acceleration", tick, { every: 100 })"#));

    assert!(
        message.starts_with("`acceleration` comes only while the app is open"),
        "{message}"
    );
}

#[test]
fn acceleration_may_use_a_period_under_a_minute_while_open() {
    let subscription = only_subscription(&listening(
        "",
        r#"on("acceleration", tick, { every: 100, while_open: true })"#,
    ));

    assert_eq!(subscription.kind, SubscriptionKind::Acceleration);
    assert_eq!(subscription.value, SubscriptionValue::Constant(100));
}

// ============================================================================
// Time
// ============================================================================

#[test]
fn time_at_a_constant_instant_keeps_the_instant() {
    let subscription = only_subscription(&listening("", r#"on("time", tick, { at: 5000 })"#));

    assert_eq!(subscription.kind, SubscriptionKind::At);
    assert_eq!(subscription.value, SubscriptionValue::Constant(5000));
}

#[test]
fn time_at_null_is_switched_off() {
    let subscription = only_subscription(&listening("", r#"on("time", tick, { at: null })"#));

    assert_eq!(subscription.value, SubscriptionValue::Constant(NONE));
}

#[test]
fn time_without_at_is_refused() {
    let message = first_error(&listening("", r#"on("time", tick)"#));

    assert!(message.starts_with("the `time` event needs `at`"), "{message}");
}

#[test]
fn time_at_text_is_refused() {
    let message = first_error(&listening("", r#"on("time", tick, { at: "noon" })"#));

    assert_eq!(message, "`at` needs an Instant, such as a signal holding one");
}

#[test]
fn time_at_a_signal_holding_an_instant_reads_that_signal() {
    let subscription = only_subscription(&listening(
        "const ring_at = signal<Instant | null>(null);",
        r#"on("time", tick, { at: ring_at })"#,
    ));

    assert!(
        matches!(subscription.value, SubscriptionValue::Variable(_)),
        "{subscription:?}"
    );
}

#[test]
fn time_at_a_computed_time_is_kept_in_a_hidden_variable() {
    let subscriptions = subscriptions_of(&listening(
        "const start = signal<Instant>(0); const ring_at = computed(() => start.value + 60000);",
        r#"on("time", tick, { at: ring_at })"#,
    ));

    let at = subscriptions
        .iter()
        .find(|subscription| subscription.kind == SubscriptionKind::At)
        .expect("a time subscription");
    assert!(matches!(at.value, SubscriptionValue::Variable(_)), "{at:?}");
}

// ============================================================================
// Daily
// ============================================================================

#[test]
fn daily_at_a_constant_time_keeps_the_minute_of_the_day_and_every_day() {
    let subscription = only_subscription(&listening(
        "",
        r#"on("daily", tick, { at: { hour: 7, minute: 30 } })"#,
    ));

    assert_eq!(subscription.value, SubscriptionValue::Constant(450));
    assert_eq!(subscription.days, 0b111_1111);
}

#[test]
fn daily_keeps_the_days_given() {
    let subscription = only_subscription(&listening(
        "",
        r#"on("daily", tick, { at: { hour: 0, minute: 0 }, days: 0b0011111 })"#,
    ));

    assert_eq!(subscription.days, 0b001_1111);
}

#[test]
fn daily_with_every_day_named_keeps_all_seven() {
    let subscription = only_subscription(&listening(
        "",
        r#"on("daily", tick, { at: { hour: 23, minute: 59 }, days: EVERY_DAY, while_open: true })"#,
    ));

    assert_eq!(subscription.days, 0b111_1111);
    assert_eq!(subscription.value, SubscriptionValue::Constant(1439));
    assert!(subscription.only_while_open);
}

#[test]
fn daily_without_at_is_refused() {
    let message = first_error(&listening("", r#"on("daily", tick)"#));

    assert!(message.starts_with("the `daily` event needs `at`"), "{message}");
}

#[test]
fn daily_with_no_days_is_refused() {
    let message = first_error(&listening(
        "",
        r#"on("daily", tick, { at: { hour: 6, minute: 0 }, days: 0 })"#,
    ));

    assert!(
        message.starts_with("the days are a mask from 1 to 127"),
        "{message}"
    );
}

#[test]
fn daily_with_days_from_a_signal_is_refused() {
    let message = first_error(&listening(
        "",
        r#"on("daily", tick, { at: { hour: 6, minute: 0 }, days: count.value })"#,
    ));

    assert!(
        message.starts_with("the days must be known when compiling"),
        "{message}"
    );
}

#[test]
fn daily_at_null_is_switched_off() {
    let subscription = only_subscription(&listening("", r#"on("daily", tick, { at: null })"#));

    assert_eq!(subscription.value, SubscriptionValue::Constant(NONE));
}

#[test]
fn daily_at_a_time_without_a_minute_is_refused() {
    let message = first_error(&listening("", r#"on("daily", tick, { at: { hour: 7 } })"#));

    assert!(
        message.starts_with("a time of day is `{ hour, minute }`"),
        "{message}"
    );
}

#[test]
fn daily_at_an_hour_or_minute_out_of_range_is_refused() {
    for time in ["{ hour: 24, minute: 0 }", "{ hour: 1, minute: 60 }"] {
        let message = first_error(&listening("", &format!(r#"on("daily", tick, {{ at: {time} }})"#)));
        assert_eq!(message, "the hour must be 0 to 23 and the minute 0 to 59");
    }
}

#[test]
fn daily_at_a_signal_holding_a_time_of_day_is_kept_in_a_variable() {
    let subscriptions = subscriptions_of(&listening(
        "const wake = signal<TimeOfDay>({ hour: 8, minute: 0 });",
        r#"on("daily", tick, { at: wake })"#,
    ));

    let daily = subscriptions
        .iter()
        .find(|subscription| subscription.kind == SubscriptionKind::Daily)
        .expect("a daily subscription");
    assert!(matches!(daily.value, SubscriptionValue::Variable(_)), "{daily:?}");
}

#[test]
fn daily_at_a_signal_holding_the_minute_of_the_day_reads_that_signal() {
    let subscription = only_subscription(&listening(
        "const minute_of_day = signal(480);",
        r#"on("daily", tick, { at: minute_of_day })"#,
    ));

    assert!(
        matches!(subscription.value, SubscriptionValue::Variable(_)),
        "{subscription:?}"
    );
}

#[test]
fn daily_at_text_held_in_a_signal_is_refused() {
    let message = first_error(&listening(
        r#"const label = signal("seven");"#,
        r#"on("daily", tick, { at: label })"#,
    ));

    assert!(
        message.starts_with("`daily` needs a time of day known when compiling"),
        "{message}"
    );
}

// ============================================================================
// At most every
// ============================================================================

struct Clock {
    now: Value,
}

impl SystemValues for Clock {
    fn read(&self, value: SystemValue) -> Value {
        match value {
            SystemValue::Now => self.now,
            _ => 0,
        }
    }

    fn read_text(&self, _value: SystemValue) -> &str {
        ""
    }

    fn random_number(&self) -> u32 {
        0
    }
}

#[test]
fn at_most_every_skips_the_handler_until_the_pause_has_passed() {
    let output = compile_source(&listening(
        "",
        r#"on("shake", tick, { while_open: true, at_most_every: SECOND })"#,
    ));
    let bytes = output.image.expect("an image");
    let image = AppImage::parse(&bytes).unwrap();
    let mut state = AppState::new(&image).unwrap();
    let handler = image.subscription(0).unwrap().handler;
    let count_slot = (0..image.state_count())
        .find(|slot| image.string(image.state_variable(*slot).unwrap().key_string) == Some("count"))
        .unwrap();

    let mut shakes_at = |milliseconds: Value| {
        let clock = Clock {
            now: 1_000_000 + milliseconds,
        };
        Interpreter::new()
            .run_handler(&image, handler, &mut state, &[0; 8], &clock)
            .unwrap();
    };
    shakes_at(0);
    shakes_at(400);
    shakes_at(999);
    shakes_at(1000);
    shakes_at(1500);
    shakes_at(2100);

    assert_eq!(state.get(count_slot as u8), Some(3));
}

#[test]
fn at_most_every_keeps_its_last_run_in_a_hidden_variable() {
    let output = compile_source(&listening("", r#"on("shake", tick, { at_most_every: SECOND })"#));
    let image = AppImage::parse(output.image.as_ref().expect("an image")).unwrap();

    let keys: Vec<&str> = (0..image.state_count())
        .map(|slot| {
            image
                .string(image.state_variable(slot).unwrap().key_string)
                .unwrap()
        })
        .collect();

    assert!(keys.iter().any(|key| key.starts_with("~last_run")), "{keys:?}");
}

#[test]
fn at_most_every_needs_a_function_the_app_defines() {
    let source = r#"
import { define_app, on, back, SECOND } from "battletime";
import { Screen } from "battletime/widgets";
function Main() { return <Screen></Screen>; }
on("shake", back, { while_open: true, at_most_every: SECOND });
export default define_app({ id: "t", name: "T", screens: { main: Main } });
"#;

    assert_eq!(
        first_error(source),
        "`at_most_every` needs a function the app defines as the handler"
    );
}

#[test]
fn at_most_every_takes_a_period_too() {
    let source = listening("", r#"on("shake", tick, { at_most_every: { seconds: 2 } })"#);

    assert_eq!(only_subscription(&source).kind, SubscriptionKind::Motion);
}

// ============================================================================
// Permissions
// ============================================================================

#[test]
fn the_permissions_an_app_needs_are_worked_out_from_its_events() {
    let source = app_with(
        "",
        r#"on("shake", tick, { while_open: true }); on("heart_rate", tick, { every: 5 * MINUTE }); on("notification", tick);"#,
    );

    let output = compile_source(&source);

    assert_eq!(
        output.permissions,
        [
            Permission::HeartRate,
            Permission::Motion,
            Permission::Notifications,
            Permission::Background
        ]
    );
}

#[test]
fn an_event_only_while_open_needs_no_background_permission() {
    let output = compile_source(&listening("", r#"on("steps", tick, { while_open: true })"#));

    assert!(output.permissions.is_empty(), "{:?}", output.permissions);
}
