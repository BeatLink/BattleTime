extern crate std;

use std::vec::Vec;

use super::*;
use crate::image::{crc32, AppKind};
use crate::writer::ImageWriter;

// ============================================================================
// Helpers
// ============================================================================

const ALARM_VARIABLE: u8 = 0;
const WAKE_MINUTE_VARIABLE: u8 = 1;
const LAPS_VARIABLE: u8 = 2;
const MISSING_VARIABLE: u8 = 9;

const MILLISECONDS_PER_HOUR: Value = 60 * MILLISECONDS_PER_MINUTE;

/// Midnight UTC at the start of Monday 5 January 1970.
const MONDAY_MIDNIGHT: Value = 4 * MILLISECONDS_PER_DAY;

fn subscription(kind: SubscriptionKind, value: SubscriptionValue) -> Subscription {
    Subscription {
        kind,
        only_while_open: false,
        handler: 0,
        detail: 0,
        days: ALL_DAYS,
        value,
    }
}

/// An image with an alarm instant, a wake-up minute, a laps array and one handler.
fn image_with(subscriptions: &[Subscription]) -> Vec<u8> {
    let mut writer = ImageWriter::new("alarm", "Alarm", AppKind::App, 1);
    writer.variable("alarm_at", ValueType::Instant, true, NONE);
    writer.variable("wake_minute", ValueType::Int, true, NONE);
    writer.array("laps", ValueType::Int, false, 3, 4, 0);
    writer.handler(Vec::new());
    for subscription in subscriptions {
        writer.subscription(*subscription);
    }
    writer.build()
}

fn problem_with(subscription: Subscription) -> Option<SubscriptionProblem> {
    let bytes = image_with(&[subscription]);
    let image = AppImage::parse(&bytes).unwrap();
    match verify_subscriptions(&image) {
        Ok(()) => None,
        Err(ImageError::BadSubscription { problem, .. }) => Some(problem),
        Err(other) => panic!("unexpected error {other:?}"),
    }
}

fn empty_state(bytes: &[u8]) -> AppState {
    AppState::new(&AppImage::parse(bytes).unwrap()).unwrap()
}

// ============================================================================
// Kinds
// ============================================================================

#[test]
fn each_kind_byte_reads_back_as_its_kind_and_none_past_the_last() {
    for byte in 0..SubscriptionKind::ALL.len() as u8 {
        assert_eq!(
            SubscriptionKind::from_byte(byte).map(|kind| kind as u8),
            Some(byte)
        );
    }

    assert_eq!(
        SubscriptionKind::from_byte(SubscriptionKind::ALL.len() as u8),
        None
    );
    assert_eq!(SubscriptionKind::from_byte(u8::MAX), None);
}

#[test]
fn each_kind_sends_its_handler_the_matching_event() {
    use SubscriptionKind::*;
    let expected = [
        (Open, EventKind::Open),
        (Close, EventKind::Close),
        (ScreenOn, EventKind::ScreenOn),
        (ScreenOff, EventKind::ScreenOff),
        (Every, EventKind::Time),
        (At, EventKind::Time),
        (Daily, EventKind::Time),
        (ClockChange, EventKind::Time),
        (HeartRate, EventKind::HeartRate),
        (Steps, EventKind::Steps),
        (StepGoal, EventKind::Steps),
        (Motion, EventKind::Motion),
        (Acceleration, EventKind::Acceleration),
        (Battery, EventKind::Battery),
        (Phone, EventKind::Phone),
        (Notification, EventKind::Notification),
        (Call, EventKind::Call),
        (Music, EventKind::PhoneData),
        (Weather, EventKind::PhoneData),
        (Navigation, EventKind::PhoneData),
        (SettingsChange, EventKind::SettingsChange),
        (Derive, EventKind::Derive),
    ];

    for (kind, event) in expected {
        assert_eq!(kind.event(), event, "{kind:?}");
    }
}

// ============================================================================
// Reading and writing bytes
// ============================================================================

#[test]
fn a_subscription_on_a_variable_survives_writing_and_reading() {
    let written = Subscription {
        only_while_open: true,
        handler: 0x0102,
        detail: 2,
        ..subscription(SubscriptionKind::At, SubscriptionValue::Variable(ALARM_VARIABLE))
    };

    let bytes = written.to_bytes();

    assert_eq!(bytes[4], 1);
    assert_eq!(Subscription::parse(&bytes), Some(written));
}

#[test]
fn only_timed_subscriptions_that_outlive_the_open_app_run_while_closed() {
    use SubscriptionKind::*;
    let timed = [Every, At, Daily, ClockChange, HeartRate];
    for kind in SubscriptionKind::ALL {
        let mut each = subscription(kind, SubscriptionValue::Constant(MILLISECONDS_PER_MINUTE));
        assert_eq!(each.runs_while_closed(), timed.contains(&kind), "{kind:?}");
        each.only_while_open = true;
        assert!(!each.runs_while_closed(), "{kind:?} only while open");
    }
}

#[test]
fn bytes_of_the_wrong_length_do_not_parse() {
    let bytes = subscription(SubscriptionKind::Open, SubscriptionValue::Constant(0)).to_bytes();

    assert_eq!(Subscription::parse(&bytes[..SUBSCRIPTION_BYTES - 1]), None);
    assert_eq!(Subscription::parse(&[0; SUBSCRIPTION_BYTES + 1]), None);
}

#[test]
fn an_unknown_value_source_or_kind_does_not_parse() {
    let good = subscription(SubscriptionKind::Every, SubscriptionValue::Constant(60_000)).to_bytes();
    let mut unknown_source = good;
    unknown_source[4] = 2;
    let mut unknown_kind = good;
    unknown_kind[0] = SubscriptionKind::ALL.len() as u8;

    assert_eq!(Subscription::parse(&unknown_source), None);
    assert_eq!(Subscription::parse(&unknown_kind), None);
}

#[test]
fn a_variable_number_above_255_does_not_parse() {
    let mut bytes = subscription(SubscriptionKind::At, SubscriptionValue::Variable(0)).to_bytes();
    bytes[8..16].copy_from_slice(&256i64.to_le_bytes());

    assert_eq!(Subscription::parse(&bytes), None);
}

// ============================================================================
// When timers are next due
// ============================================================================

#[test]
fn a_period_shorter_than_the_shortest_allowed_is_stretched_to_it() {
    let bytes = image_with(&[]);
    let state = empty_state(&bytes);
    let too_fast = subscription(SubscriptionKind::Every, SubscriptionValue::Constant(5));

    let due = too_fast.next_due(1_000, &state, 0);

    assert_eq!(due, Some(1_000 + SHORTEST_PERIOD_IN_MILLISECONDS));
}

#[test]
fn heart_rate_and_acceleration_readings_fall_on_multiples_of_their_period() {
    let bytes = image_with(&[]);
    let state = empty_state(&bytes);
    let heart_rate = subscription(SubscriptionKind::HeartRate, SubscriptionValue::Constant(5_000));
    let acceleration = subscription(SubscriptionKind::Acceleration, SubscriptionValue::Constant(100));

    assert_eq!(heart_rate.next_due(12_345, &state, 0), Some(15_000));
    assert_eq!(heart_rate.next_due(15_000, &state, 0), Some(20_000));
    assert_eq!(acceleration.next_due(250, &state, 0), Some(300));
}

#[test]
fn an_alarm_at_a_constant_time_is_due_even_after_it_has_passed() {
    let bytes = image_with(&[]);
    let state = empty_state(&bytes);
    let alarm = subscription(SubscriptionKind::At, SubscriptionValue::Constant(5_000));

    assert_eq!(alarm.next_due(9_000, &state, 0), Some(5_000));
}

#[test]
fn a_daily_alarm_later_today_rings_today() {
    let bytes = image_with(&[]);
    let state = empty_state(&bytes);
    let at_seven = subscription(SubscriptionKind::Daily, SubscriptionValue::Constant(7 * 60));

    let due = at_seven.next_due(MONDAY_MIDNIGHT + 6 * MILLISECONDS_PER_HOUR, &state, 0);

    assert_eq!(due, Some(MONDAY_MIDNIGHT + 7 * MILLISECONDS_PER_HOUR));
}

#[test]
fn a_daily_alarm_that_has_rung_today_rings_next_tomorrow() {
    let bytes = image_with(&[]);
    let state = empty_state(&bytes);
    let at_seven = subscription(SubscriptionKind::Daily, SubscriptionValue::Constant(7 * 60));

    let due = at_seven.next_due(MONDAY_MIDNIGHT + 7 * MILLISECONDS_PER_HOUR, &state, 0);

    assert_eq!(
        due,
        Some(MONDAY_MIDNIGHT + MILLISECONDS_PER_DAY + 7 * MILLISECONDS_PER_HOUR)
    );
}

#[test]
fn a_daily_alarm_rings_at_the_local_time_of_the_zone() {
    let bytes = image_with(&[]);
    let state = empty_state(&bytes);
    let at_seven = subscription(SubscriptionKind::Daily, SubscriptionValue::Constant(7 * 60));
    let two_hours_ahead = 2 * MILLISECONDS_PER_HOUR;

    let due = at_seven.next_due(MONDAY_MIDNIGHT, &state, two_hours_ahead);

    assert_eq!(due, Some(MONDAY_MIDNIGHT + 5 * MILLISECONDS_PER_HOUR));
}

#[test]
fn a_daily_alarm_reads_its_time_from_its_variable_and_is_off_while_that_is_none() {
    let bytes = image_with(&[]);
    let mut state = empty_state(&bytes);
    let wake_up = subscription(
        SubscriptionKind::Daily,
        SubscriptionValue::Variable(WAKE_MINUTE_VARIABLE),
    );

    let while_off = wake_up.next_due(MONDAY_MIDNIGHT, &state, 0);
    state.set(WAKE_MINUTE_VARIABLE, 8 * 60).unwrap();
    let once_set = wake_up.next_due(MONDAY_MIDNIGHT, &state, 0);

    assert_eq!(while_off, None);
    assert_eq!(once_set, Some(MONDAY_MIDNIGHT + 8 * MILLISECONDS_PER_HOUR));
}

#[test]
fn a_daily_alarm_set_for_no_days_never_rings() {
    let bytes = image_with(&[]);
    let state = empty_state(&bytes);
    let no_days = Subscription {
        days: 0,
        ..subscription(SubscriptionKind::Daily, SubscriptionValue::Constant(7 * 60))
    };

    assert_eq!(no_days.next_due(MONDAY_MIDNIGHT, &state, 0), None);
}

#[test]
fn a_saturday_alarm_waits_until_saturday() {
    let bytes = image_with(&[]);
    let state = empty_state(&bytes);
    let saturday_bit = 1 << 5;
    let saturday_at_nine = Subscription {
        days: saturday_bit,
        ..subscription(SubscriptionKind::Daily, SubscriptionValue::Constant(9 * 60))
    };

    let due = saturday_at_nine.next_due(MONDAY_MIDNIGHT, &state, 0);

    assert_eq!(
        due,
        Some(MONDAY_MIDNIGHT + 5 * MILLISECONDS_PER_DAY + 9 * MILLISECONDS_PER_HOUR)
    );
}

#[test]
fn a_change_of_minute_is_due_at_the_next_whole_minute() {
    let bytes = image_with(&[]);
    let state = empty_state(&bytes);
    let each_minute = subscription(SubscriptionKind::ClockChange, SubscriptionValue::Constant(0));

    assert_eq!(each_minute.next_due(90_500, &state, 0), Some(120_000));
}

#[test]
fn a_change_of_day_is_due_at_local_midnight() {
    let bytes = image_with(&[]);
    let state = empty_state(&bytes);
    let each_day = Subscription {
        detail: 2,
        ..subscription(SubscriptionKind::ClockChange, SubscriptionValue::Constant(0))
    };
    let one_hour_behind = -MILLISECONDS_PER_HOUR;

    let due = each_day.next_due(
        MONDAY_MIDNIGHT + 12 * MILLISECONDS_PER_HOUR,
        &state,
        one_hour_behind,
    );

    assert_eq!(
        due,
        Some(MONDAY_MIDNIGHT + MILLISECONDS_PER_DAY + MILLISECONDS_PER_HOUR)
    );
}

#[test]
fn a_clock_change_at_the_end_of_time_has_no_next_time() {
    let bytes = image_with(&[]);
    let state = empty_state(&bytes);
    let each_minute = subscription(SubscriptionKind::ClockChange, SubscriptionValue::Constant(0));
    let at_seven = subscription(SubscriptionKind::Daily, SubscriptionValue::Constant(7 * 60));

    assert_eq!(each_minute.next_due(Value::MAX, &state, 1), None);
    assert_eq!(at_seven.next_due(Value::MAX, &state, 1), None);
}

#[test]
fn subscriptions_that_wait_on_events_are_never_due() {
    let bytes = image_with(&[]);
    let state = empty_state(&bytes);

    for kind in [
        SubscriptionKind::Open,
        SubscriptionKind::Steps,
        SubscriptionKind::Notification,
        SubscriptionKind::Derive,
    ] {
        let waiting = subscription(kind, SubscriptionValue::Constant(60_000));

        assert_eq!(waiting.next_due(MONDAY_MIDNIGHT, &state, 0), None, "{kind:?}");
    }
}

#[test]
fn a_subscription_on_a_variable_the_app_lacks_is_never_due() {
    let bytes = image_with(&[]);
    let state = empty_state(&bytes);
    let alarm = subscription(
        SubscriptionKind::At,
        SubscriptionValue::Variable(MISSING_VARIABLE),
    );

    assert_eq!(alarm.current_value(&state), None);
    assert_eq!(alarm.next_due(MONDAY_MIDNIGHT, &state, 0), None);
}

// ============================================================================
// Checking
// ============================================================================

#[test]
fn well_formed_subscriptions_pass() {
    let bytes = image_with(&[
        subscription(SubscriptionKind::Every, SubscriptionValue::Constant(60_000)),
        subscription(SubscriptionKind::At, SubscriptionValue::Variable(ALARM_VARIABLE)),
        subscription(
            SubscriptionKind::Daily,
            SubscriptionValue::Variable(WAKE_MINUTE_VARIABLE),
        ),
        subscription(SubscriptionKind::Daily, SubscriptionValue::Constant(NONE)),
        Subscription {
            detail: 5,
            ..subscription(SubscriptionKind::Motion, SubscriptionValue::Constant(0))
        },
    ]);

    assert_eq!(verify_subscriptions(&AppImage::parse(&bytes).unwrap()), Ok(()));
}

#[test]
fn the_error_names_the_subscription_at_fault() {
    let bytes = image_with(&[
        subscription(SubscriptionKind::Open, SubscriptionValue::Constant(0)),
        subscription(SubscriptionKind::Every, SubscriptionValue::Constant(1)),
    ]);

    let result = verify_subscriptions(&AppImage::parse(&bytes).unwrap());

    assert_eq!(
        result,
        Err(ImageError::BadSubscription {
            subscription: 1,
            problem: SubscriptionProblem::PeriodTooShort,
        })
    );
}

#[test]
fn a_detail_past_what_the_kind_offers_is_refused() {
    let tap_kinds = |kind, detail| Subscription {
        detail,
        ..subscription(kind, SubscriptionValue::Constant(0))
    };

    assert_eq!(
        problem_with(tap_kinds(SubscriptionKind::Motion, 6)),
        Some(SubscriptionProblem::BadDetail)
    );
    assert_eq!(
        problem_with(tap_kinds(SubscriptionKind::Battery, 4)),
        Some(SubscriptionProblem::BadDetail)
    );
    assert_eq!(
        problem_with(tap_kinds(SubscriptionKind::Phone, 2)),
        Some(SubscriptionProblem::BadDetail)
    );
    assert_eq!(
        problem_with(tap_kinds(SubscriptionKind::ClockChange, 3)),
        Some(SubscriptionProblem::BadDetail)
    );
    assert_eq!(
        problem_with(tap_kinds(SubscriptionKind::Steps, 1)),
        Some(SubscriptionProblem::BadDetail)
    );
    assert_eq!(problem_with(tap_kinds(SubscriptionKind::Battery, 3)), None);
}

#[test]
fn a_daily_alarm_on_days_past_sunday_is_refused() {
    let eighth_day = Subscription {
        days: ALL_DAYS + 1,
        ..subscription(SubscriptionKind::Daily, SubscriptionValue::Constant(60))
    };

    assert_eq!(problem_with(eighth_day), Some(SubscriptionProblem::BadDays));
}

#[test]
fn only_alarms_and_daily_times_may_read_a_variable() {
    let timer_on_a_variable = subscription(
        SubscriptionKind::Every,
        SubscriptionValue::Variable(ALARM_VARIABLE),
    );

    assert_eq!(
        problem_with(timer_on_a_variable),
        Some(SubscriptionProblem::BadVariable)
    );
}

#[test]
fn an_alarm_on_an_array_a_missing_variable_or_the_wrong_type_is_refused() {
    let on_an_array = subscription(SubscriptionKind::At, SubscriptionValue::Variable(LAPS_VARIABLE));
    let on_a_missing_variable = subscription(
        SubscriptionKind::At,
        SubscriptionValue::Variable(MISSING_VARIABLE),
    );
    let on_a_number = subscription(
        SubscriptionKind::At,
        SubscriptionValue::Variable(WAKE_MINUTE_VARIABLE),
    );

    assert_eq!(problem_with(on_an_array), Some(SubscriptionProblem::BadVariable));
    assert_eq!(
        problem_with(on_a_missing_variable),
        Some(SubscriptionProblem::BadVariable)
    );
    assert_eq!(problem_with(on_a_number), Some(SubscriptionProblem::BadVariable));
}

#[test]
fn a_period_under_the_shortest_is_refused_even_while_open() {
    let racing_heart_rate = Subscription {
        only_while_open: true,
        ..subscription(SubscriptionKind::HeartRate, SubscriptionValue::Constant(10))
    };

    assert_eq!(
        problem_with(racing_heart_rate),
        Some(SubscriptionProblem::PeriodTooShort)
    );
}

#[test]
fn a_daily_time_outside_the_day_is_refused() {
    let at_midnight_tomorrow = subscription(SubscriptionKind::Daily, SubscriptionValue::Constant(24 * 60));
    let before_midnight = subscription(SubscriptionKind::Daily, SubscriptionValue::Constant(-1));
    let last_minute = subscription(SubscriptionKind::Daily, SubscriptionValue::Constant(24 * 60 - 1));

    assert_eq!(
        problem_with(at_midnight_tomorrow),
        Some(SubscriptionProblem::BadTimeOfDay)
    );
    assert_eq!(
        problem_with(before_midnight),
        Some(SubscriptionProblem::BadTimeOfDay)
    );
    assert_eq!(problem_with(last_minute), None);
}

#[test]
fn raw_acceleration_must_stop_when_the_app_closes_whatever_its_period() {
    let slow_background = subscription(
        SubscriptionKind::Acceleration,
        SubscriptionValue::Constant(120_000),
    );
    let slow_while_open = Subscription {
        only_while_open: true,
        ..slow_background
    };

    assert_eq!(
        problem_with(slow_background),
        Some(SubscriptionProblem::MustBeOnlyWhileOpen)
    );
    assert_eq!(problem_with(slow_while_open), None);
}

#[test]
fn a_heart_rate_reading_once_a_minute_may_run_in_the_background() {
    let each_minute = subscription(SubscriptionKind::HeartRate, SubscriptionValue::Constant(60_000));

    assert_eq!(problem_with(each_minute), None);
}

#[test]
fn a_subscription_whose_bytes_do_not_parse_is_unreadable() {
    let good = subscription(SubscriptionKind::Open, SubscriptionValue::Constant(0));
    let mut bytes = image_with(&[good]);
    let stored = good.to_bytes();
    let start = bytes
        .windows(SUBSCRIPTION_BYTES)
        .position(|window| window == stored)
        .unwrap();
    bytes[start] = u8::MAX;
    let crc = crc32(&bytes[16..]);
    bytes[12..16].copy_from_slice(&crc.to_le_bytes());

    let result = verify_subscriptions(&AppImage::parse(&bytes).unwrap());

    assert_eq!(
        result,
        Err(ImageError::BadSubscription {
            subscription: 0,
            problem: SubscriptionProblem::Unreadable,
        })
    );
}
