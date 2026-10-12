//! Tests for define_app's subscriptions and lifecycle handlers, read back from the compiled image.

use neotime_runtime::subscriptions::{Subscription, SubscriptionKind, SubscriptionValue};
use neotime_runtime::{AppImage, NONE};

use super::{detail_names, handler_position, lifecycle_kind, subscription_kind};
use crate::compile_source;

// ============================================================================
// Helpers
// ============================================================================

/// An app that declares `declarations` and passes `options` to define_app, with every permission granted.
fn app_with(declarations: &str, options: &str) -> String {
    format!(
        r#"
import {{ computed, signal }} from "@preact/signals";
import {{
  define_app, every, at, daily, clock_change, heart_rate_reading, steps, motion, acceleration, battery,
  phone, notification, settings_change, navigation, message, vibrate, EVERY_DAY, type Instant, type TimeOfDay,
}} from "neotime";
import {{ Label, Screen }} from "neotime/widgets";

const count = signal(0);
function tick() {{ count.value += 1; }}
{declarations}
function Main() {{ return <Screen><Label>{{count.value}}</Label></Screen>; }}

export default define_app({{
  id: "test", name: "Test", screens: {{ main: Main }},
  permissions: ["background", "motion", "heart_rate", "notifications", "phone"],
  {options}
}});
"#
    )
}

/// An app whose only subscriptions are the list given.
fn subscribing(declarations: &str, list: &str) -> String {
    app_with(declarations, &format!("subscriptions: [{list}],"))
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
// Names
// ============================================================================

#[test]
fn each_lifecycle_option_names_its_subscription() {
    assert_eq!(lifecycle_kind("onOpen"), Some(SubscriptionKind::Open));
    assert_eq!(lifecycle_kind("onClose"), Some(SubscriptionKind::Close));
    assert_eq!(lifecycle_kind("onScreenOn"), Some(SubscriptionKind::ScreenOn));
    assert_eq!(lifecycle_kind("onScreenOff"), Some(SubscriptionKind::ScreenOff));
    assert_eq!(lifecycle_kind("onWake"), None);
}

#[test]
fn each_subscription_function_names_its_kind() {
    assert_eq!(
        subscription_kind("settings_change"),
        Some(SubscriptionKind::SettingsChange)
    );
    assert_eq!(
        subscription_kind("navigation"),
        Some(SubscriptionKind::Navigation)
    );
    assert_eq!(subscription_kind("weather"), Some(SubscriptionKind::Weather));
    assert_eq!(subscription_kind("format_time"), None);
}

#[test]
fn only_kinds_with_a_detail_list_detail_names() {
    assert_eq!(
        detail_names(SubscriptionKind::Battery),
        &["low", "starts", "stops", "completes"]
    );
    assert_eq!(
        detail_names(SubscriptionKind::Phone),
        &["connects", "disconnects"]
    );
    assert!(detail_names(SubscriptionKind::Steps).is_empty());
}

#[test]
fn the_handler_follows_the_period_or_kind_argument() {
    assert_eq!(handler_position(SubscriptionKind::Every), 1);
    assert_eq!(handler_position(SubscriptionKind::Battery), 1);
    assert_eq!(handler_position(SubscriptionKind::Steps), 0);
    assert_eq!(handler_position(SubscriptionKind::Notification), 0);
}

// ============================================================================
// Lifecycle handlers
// ============================================================================

#[test]
fn every_lifecycle_handler_is_written_in_order() {
    let source = app_with(
        "",
        "onClose: tick, onScreenOn: tick, onScreenOff: () => { count.value = 0; },",
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
            SubscriptionKind::ScreenOff
        ]
    );
}

// ============================================================================
// The list
// ============================================================================

#[test]
fn subscriptions_written_as_something_other_than_an_array_are_refused() {
    let source = app_with("", "subscriptions: every({ minutes: 1 }, tick),");

    assert!(first_error(&source).starts_with("write `subscriptions` as an array"));
}

#[test]
fn a_subscription_that_is_not_a_call_is_refused() {
    let source = subscribing("", "tick");

    assert!(first_error(&source).starts_with("each subscription is a call, such as"));
}

#[test]
fn a_call_to_the_apps_own_function_is_not_a_subscription() {
    let source = subscribing("function make() { return 1; }", "make()");

    assert!(first_error(&source).starts_with("each subscription is a call to a `neotime` subscription"));
}

#[test]
fn the_message_subscription_is_not_in_the_image_format_yet() {
    let source = subscribing("", r#"message("chat", tick)"#);

    assert!(first_error(&source).starts_with("the `message` subscription is not in the image format yet"));
}

#[test]
fn a_neotime_function_that_is_not_a_subscription_is_refused() {
    let source = subscribing("", "vibrate(100)");

    assert_eq!(first_error(&source), "`vibrate` is not a subscription");
}

#[test]
fn spread_arguments_in_a_subscription_are_refused() {
    let source = subscribing("const extra = [tick];", "steps(...extra)");

    assert_eq!(first_error(&source), "`...` in a subscription is not supported");
}

#[test]
fn a_subscription_missing_its_handler_is_refused() {
    let source = subscribing("", "every({ minutes: 1 })");

    assert_eq!(first_error(&source), "`every` needs more arguments");
}

#[test]
fn a_subscription_with_too_many_arguments_is_refused() {
    let source = subscribing("", "steps(tick, {}, 3)");

    assert_eq!(first_error(&source), "`steps` takes at most 2 arguments");
}

#[test]
fn a_handler_that_is_not_a_function_is_refused() {
    let source = subscribing("const nothing = undefined;", "steps(nothing)");

    assert_eq!(
        first_error(&source),
        "give the subscription a handler, such as a function the app defines"
    );
}

#[test]
fn a_subscription_without_detail_or_period_writes_only_its_kind_and_handler() {
    let source = subscribing("", "notification(tick), settings_change(tick)");

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

#[test]
fn a_subscription_that_needs_a_permission_reports_the_missing_one() {
    let source = r#"
import { define_app, notification } from "neotime";
import { Screen } from "neotime/widgets";
function Main() { return <Screen></Screen>; }
export default define_app({ id: "t", name: "T", screens: { main: Main }, permissions: ["background"],
  subscriptions: [notification(() => {})] });
"#;

    let message = first_error(source);

    assert!(
        message.starts_with("the `notification` subscription needs the `notifications` permission"),
        "{message}"
    );
}

// ============================================================================
// Options
// ============================================================================

#[test]
fn only_while_open_is_written_into_the_subscription() {
    let source = subscribing("", "steps(tick, { only_while_open: true })");

    assert!(only_subscription(&source).only_while_open);
}

#[test]
fn only_while_open_false_keeps_running_in_the_background() {
    let source = subscribing("", "steps(tick, { only_while_open: false })");

    assert!(!only_subscription(&source).only_while_open);
}

#[test]
fn options_that_are_not_an_object_are_refused() {
    let source = subscribing("", "steps(tick, true)");

    assert_eq!(
        first_error(&source),
        "write the options as `{ only_while_open: true }`"
    );
}

#[test]
fn only_while_open_that_is_not_a_boolean_is_refused() {
    let source = subscribing("", "steps(tick, { only_while_open: 1 })");

    assert_eq!(first_error(&source), "`only_while_open` must be true or false");
}

#[test]
fn an_unknown_subscription_option_is_refused() {
    let source = subscribing("", "steps(tick, { repeat: true })");

    assert_eq!(first_error(&source), "a subscription has no option `repeat`");
}

// ============================================================================
// Periods
// ============================================================================

#[test]
fn a_period_adds_up_every_unit_in_milliseconds() {
    let source = subscribing(
        "",
        "every({ hours: 1, minutes: 2, seconds: 3, milliseconds: 4 }, tick)",
    );

    let subscription = only_subscription(&source);

    assert_eq!(subscription.value, SubscriptionValue::Constant(3_723_004));
}

#[test]
fn a_heart_rate_period_is_kept_like_every() {
    let source = subscribing("", "heart_rate_reading({ minutes: 5 }, tick)");

    let subscription = only_subscription(&source);

    assert_eq!(subscription.kind, SubscriptionKind::HeartRate);
    assert_eq!(subscription.value, SubscriptionValue::Constant(300_000));
}

#[test]
fn a_period_not_known_when_compiling_is_refused() {
    let source = subscribing("", "every(count.value, tick)");

    assert!(first_error(&source).starts_with("a period must be known when compiling"));
}

#[test]
fn a_period_in_an_unknown_unit_is_refused() {
    let source = subscribing("", "every({ days: 1 }, tick)");

    assert_eq!(
        first_error(&source),
        "a period has no `days`; use milliseconds, seconds, minutes or hours"
    );
}

#[test]
fn a_period_that_is_not_a_number_is_refused() {
    let source = subscribing("", r#"every({ minutes: "five" }, tick)"#);

    assert_eq!(first_error(&source), "`minutes` must be a whole number");
}

#[test]
fn a_period_that_overflows_is_refused() {
    let source = subscribing("", "every({ hours: 9000000000000000 }, tick)");

    assert_eq!(first_error(&source), "this period is too long");
}

#[test]
fn a_period_shorter_than_the_runtime_allows_is_refused() {
    let source = subscribing("", "every({ milliseconds: 5 }, tick, { only_while_open: true })");

    assert_eq!(
        first_error(&source),
        "`every` runs at most every 20 milliseconds, and this period is 5"
    );
}

#[test]
fn acceleration_must_run_only_while_the_app_is_open() {
    let source = subscribing("", "acceleration({ milliseconds: 100 }, tick)");

    assert!(first_error(&source).starts_with("`acceleration` runs only while the app is open"));
}

#[test]
fn acceleration_may_use_a_period_under_a_minute_while_open() {
    let source = subscribing(
        "",
        "acceleration({ milliseconds: 100 }, tick, { only_while_open: true })",
    );

    let subscription = only_subscription(&source);

    assert_eq!(subscription.kind, SubscriptionKind::Acceleration);
    assert_eq!(subscription.value, SubscriptionValue::Constant(100));
}

// ============================================================================
// At
// ============================================================================

#[test]
fn at_a_constant_instant_keeps_the_instant() {
    let source = subscribing("", "at(5000, tick)");

    assert_eq!(
        only_subscription(&source).value,
        SubscriptionValue::Constant(5000)
    );
}

#[test]
fn at_null_is_switched_off() {
    let source = subscribing("", "at(null, tick)");

    assert_eq!(
        only_subscription(&source).value,
        SubscriptionValue::Constant(NONE)
    );
}

#[test]
fn at_text_is_refused() {
    let source = subscribing("", r#"at("noon", tick)"#);

    assert_eq!(
        first_error(&source),
        "`at` needs an Instant, such as a signal holding one"
    );
}

#[test]
fn at_a_signal_holding_an_instant_reads_that_signal() {
    let source = subscribing(
        "const ring_at = signal<Instant | null>(null);",
        "at(ring_at, tick)",
    );

    let subscription = only_subscription(&source);

    assert!(
        matches!(subscription.value, SubscriptionValue::Variable(_)),
        "{subscription:?}"
    );
}

#[test]
fn at_a_computed_time_is_kept_in_a_hidden_variable() {
    let source = subscribing(
        "const start = signal<Instant>(0); const ring_at = computed(() => start.value + 60000);",
        "at(ring_at, tick)",
    );

    let subscriptions = subscriptions_of(&source);

    let at = subscriptions
        .iter()
        .find(|subscription| subscription.kind == SubscriptionKind::At)
        .expect("an at subscription");
    assert!(matches!(at.value, SubscriptionValue::Variable(_)), "{at:?}");
}

// ============================================================================
// Daily
// ============================================================================

#[test]
fn daily_at_a_constant_time_keeps_the_minute_of_the_day_and_every_day() {
    let source = subscribing("", "daily({ hour: 7, minute: 30 }, tick)");

    let subscription = only_subscription(&source);

    assert_eq!(subscription.value, SubscriptionValue::Constant(450));
    assert_eq!(subscription.days, 0b111_1111);
}

#[test]
fn daily_keeps_the_days_given() {
    let source = subscribing("", "daily({ hour: 0, minute: 0 }, tick, 0b0011111)");

    assert_eq!(only_subscription(&source).days, 0b001_1111);
}

#[test]
fn daily_with_every_day_named_keeps_all_seven() {
    let source = subscribing("", "daily({ hour: 23, minute: 59 }, tick, EVERY_DAY)");

    let subscription = only_subscription(&source);

    assert_eq!(subscription.days, 0b111_1111);
    assert_eq!(subscription.value, SubscriptionValue::Constant(1439));
}

#[test]
fn daily_with_undefined_days_rings_every_day() {
    let source = subscribing(
        "",
        "daily({ hour: 6, minute: 0 }, tick, undefined, { only_while_open: true })",
    );

    let subscription = only_subscription(&source);

    assert_eq!(subscription.days, 0b111_1111);
    assert!(subscription.only_while_open);
}

#[test]
fn daily_with_no_days_is_refused() {
    let source = subscribing("", "daily({ hour: 6, minute: 0 }, tick, 0)");

    assert!(first_error(&source).starts_with("the days are a mask from 1 to 127"));
}

#[test]
fn daily_with_days_from_a_signal_is_refused() {
    let source = subscribing("", "daily({ hour: 6, minute: 0 }, tick, count.value)");

    assert!(first_error(&source).starts_with("the days must be known when compiling"));
}

#[test]
fn daily_at_null_is_switched_off() {
    let source = subscribing("", "daily(null, tick)");

    assert_eq!(
        only_subscription(&source).value,
        SubscriptionValue::Constant(NONE)
    );
}

#[test]
fn daily_at_a_time_without_a_minute_is_refused() {
    let source = subscribing("", "daily({ hour: 7 }, tick)");

    assert!(first_error(&source).starts_with("a time of day is `{ hour, minute }`"));
}

#[test]
fn daily_at_an_hour_past_the_day_is_refused() {
    let source = subscribing("", "daily({ hour: 24, minute: 0 }, tick)");

    assert_eq!(
        first_error(&source),
        "the hour must be 0 to 23 and the minute 0 to 59"
    );
}

#[test]
fn daily_at_a_minute_past_the_hour_is_refused() {
    let source = subscribing("", "daily({ hour: 1, minute: 60 }, tick)");

    assert_eq!(
        first_error(&source),
        "the hour must be 0 to 23 and the minute 0 to 59"
    );
}

#[test]
fn daily_at_a_signal_holding_a_time_of_day_is_kept_in_a_variable() {
    let source = subscribing(
        "const wake = signal<TimeOfDay>({ hour: 8, minute: 0 });",
        "daily(wake, tick)",
    );

    let subscriptions = subscriptions_of(&source);

    let daily = subscriptions
        .iter()
        .find(|subscription| subscription.kind == SubscriptionKind::Daily)
        .expect("a daily subscription");
    assert!(matches!(daily.value, SubscriptionValue::Variable(_)), "{daily:?}");
}

#[test]
fn daily_at_a_signal_holding_the_minute_of_the_day_reads_that_signal() {
    let source = subscribing("const minute_of_day = signal(480);", "daily(minute_of_day, tick)");

    let subscription = only_subscription(&source);

    assert!(
        matches!(subscription.value, SubscriptionValue::Variable(_)),
        "{subscription:?}"
    );
}

#[test]
fn daily_at_text_held_in_a_signal_is_refused() {
    let source = subscribing(r#"const label = signal("seven");"#, "daily(label, tick)");

    assert!(first_error(&source).starts_with("`daily` needs a time of day known when compiling"));
}

// ============================================================================
// Details
// ============================================================================

#[test]
fn each_detail_name_becomes_its_position() {
    let source = subscribing(
        "",
        r#"clock_change("day", tick), battery("completes", tick), phone("disconnects", tick), motion("moving", tick)"#,
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
        ]
    );
}

#[test]
fn an_unknown_detail_name_lists_the_choices() {
    let source = subscribing("", r#"battery("empty", tick)"#);

    assert_eq!(
        first_error(&source),
        "`battery` takes one of: low, starts, stops, completes"
    );
}

#[test]
fn a_detail_that_is_not_text_lists_the_choices() {
    let source = subscribing("", "phone(1, tick)");

    assert_eq!(
        first_error(&source),
        "`phone` takes one of: connects, disconnects"
    );
}
