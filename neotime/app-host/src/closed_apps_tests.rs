extern crate std;

use std::{string::String, vec, vec::Vec};

use neotime_runtime::{
    instruction::Instruction,
    subscriptions::{Subscription, SubscriptionKind, SubscriptionValue},
    widgets::Property,
    writer::{CodeWriter, ImageWriter},
    AppImage, AppKind, AppState, PropertySource, ValueType, WidgetKind, DEFAULT_THEME,
};

use super::*;
use neotime_runtime::{SystemValue, SystemValues};

// ============================================================================
// A ringer app and a system that records what it asks
// ============================================================================

const START_TIME: Value = 1_000_020_000;
const MINUTE: Value = 60_000;

/// Variable numbers of the ringer app.
const RING_AT: u8 = 0;
const RINGS: u8 = 1;
const HEART_RATE: u8 = 2;

#[derive(Default)]
struct RecordingSystem {
    effects: Vec<EffectKind>,
    failures: usize,
    closed_run: Option<ClosedRun>,
}

impl RunsClosedApps for RecordingSystem {
    fn closed_run(&mut self) -> &mut Option<ClosedRun> {
        &mut self.closed_run
    }
}

impl SystemValues for RecordingSystem {
    fn read(&self, _value: SystemValue) -> Value {
        0
    }

    fn random_number(&self) -> u32 {
        4
    }
}

impl System for RecordingSystem {
    fn carry_out(&mut self, effect: &AppEffect) {
        if self.closed_run.as_mut().is_some_and(|run| run.hold_back(effect)) {
            return;
        }
        self.effects.push(effect.kind);
    }

    fn app_failed(&mut self, _failure: crate::system::AppFailure) {
        self.failures += 1;
    }
}

fn code(build: impl FnOnce(&mut CodeWriter)) -> Vec<u8> {
    let mut writer = CodeWriter::new();
    build(&mut writer);
    writer.finish().unwrap()
}

/// An app that rings at `ring_at`, counting its rings, and keeps the heart rate each minute.
///
/// Its ring clears the time and asks for every effect a closed app may and may not carry out.
fn ringer_image(id: &str, wakes: bool) -> Vec<u8> {
    use Instruction::*;

    let mut writer = ImageWriter::new(id, "Ringer", AppKind::App, 1);
    writer.variable("ring_at", ValueType::Instant, true, NONE);
    writer.variable("rings", ValueType::Int, true, 0);
    writer.variable("heart_rate", ValueType::Int, false, 0);
    let main = writer.string("main");
    let ringing = writer.string("ringing");
    let ring = writer.handler(code(|code| {
        code.emit(ReadState(RINGS))
            .emit(PushSmall(1))
            .emit(Add)
            .emit(WriteState(RINGS))
            .emit(PushNone)
            .emit(WriteState(RING_AT))
            .emit(PushSmall(30))
            .emit(Effect(EffectKind::Vibrate))
            .emit(PushText(ringing))
            .emit(Effect(EffectKind::Go))
            .emit(PushSmall(1))
            .emit(Effect(EffectKind::KeepScreenOn))
            .emit(PushSmall(2))
            .emit(Effect(EffectKind::SetBrightness))
            .emit(Effect(EffectKind::Close));
        if wakes {
            code.emit(Effect(EffectKind::WakeScreen));
        }
    }));
    let keep_heart_rate = writer.handler(code(|code| {
        code.emit(ReadEvent(0)).emit(WriteState(HEART_RATE));
    }));
    writer.subscription(Subscription {
        kind: SubscriptionKind::At,
        only_while_open: false,
        handler: ring,
        detail: 0,
        days: 0,
        value: SubscriptionValue::Variable(RING_AT),
    });
    writer.subscription(Subscription {
        kind: SubscriptionKind::HeartRate,
        only_while_open: id != "heart-log",
        handler: keep_heart_rate,
        detail: 0,
        days: 0,
        value: SubscriptionValue::Constant(MINUTE),
    });
    let constant = |string: u16| PropertySource::Constant(string as i32);
    writer.widget(WidgetKind::Column, 0, &[(Property::ScreenName, constant(main))]);
    writer.widget(
        WidgetKind::Column,
        0,
        &[(Property::ScreenName, constant(ringing))],
    );
    writer.build()
}

/// The app's host, loaded closed from saved state that sets its ring for `ring_at`.
fn closed_ringer<'image>(
    image: &'image [u8],
    ring_at: Value,
    system: &mut RecordingSystem,
) -> AppHost<'image> {
    closed_ringer_at(image, ring_at, START_TIME, system)
}

fn closed_ringer_at<'image>(
    image: &'image [u8],
    ring_at: Value,
    now: Value,
    system: &mut RecordingSystem,
) -> AppHost<'image> {
    let parsed = AppImage::parse(image).unwrap();
    let mut state = AppState::new(&parsed).unwrap();
    state.set(RING_AT, ring_at).unwrap();
    let mut saved = vec![0; crate::LARGEST_SAVED_STATE_BYTES];
    let length = state.save_persistent(&parsed, &mut saved).unwrap();
    AppHost::open_in_background(image, DEFAULT_THEME, &saved[..length], now, system).unwrap()
}

// ============================================================================
// The table
// ============================================================================

#[test]
fn a_closed_app_is_recorded_with_its_next_due_time_and_forgotten_once_nothing_is_left() {
    let image = ringer_image("timer", true);
    let mut system = RecordingSystem::default();
    let mut closed = ClosedApps::NONE;
    let host = closed_ringer(&image, START_TIME + 5 * MINUTE, &mut system);

    closed.record("timer", &host, START_TIME);

    assert_eq!(closed.next_due(""), Some(START_TIME + 5 * MINUTE));
    assert_eq!(closed.due(START_TIME, ""), None);
    assert_eq!(closed.due(START_TIME + 5 * MINUTE, ""), Some("timer"));
    assert!(closed.run_bytes("timer").unwrap() > image.len() as u32);
    assert_eq!(closed.ids().collect::<Vec<_>>(), vec!["timer"]);

    let idle = closed_ringer(&image, NONE, &mut system);
    closed.record("timer", &idle, START_TIME);
    assert_eq!(closed.next_due(""), None);
    assert_eq!(closed.ids().count(), 0);
}

#[test]
fn an_open_app_is_never_recorded_and_its_entry_is_left_out() {
    let image = ringer_image("timer", true);
    let mut system = RecordingSystem::default();
    let mut closed = ClosedApps::NONE;
    let host = closed_ringer(&image, START_TIME + MINUTE, &mut system);
    closed.record("timer", &host, START_TIME);

    assert_eq!(closed.next_due("timer"), None);
    assert_eq!(closed.due(START_TIME + MINUTE, "timer"), None);

    let open = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    closed.record("timer", &open, START_TIME);
    assert_eq!(closed.ids().count(), 0);
}

#[test]
fn the_soonest_due_app_runs_first() {
    let image = ringer_image("timer", true);
    let mut system = RecordingSystem::default();
    let mut closed = ClosedApps::NONE;
    let later = closed_ringer(&image, START_TIME + 2 * MINUTE, &mut system);
    let sooner = closed_ringer(&image, START_TIME + MINUTE, &mut system);
    closed.record("later", &later, START_TIME);
    closed.record("sooner", &sooner, START_TIME);

    assert_eq!(closed.due(START_TIME + 3 * MINUTE, ""), Some("sooner"));
    closed.forget("sooner");
    assert_eq!(closed.due(START_TIME + 3 * MINUTE, ""), Some("later"));
    closed.clear();
    assert_eq!(closed.next_due(""), None);
}

#[test]
fn next_alarm_is_the_closed_alarm_apps_next_ring() {
    let image = ringer_image(ALARM_APP_ID, true);
    let mut system = RecordingSystem::default();
    let mut closed = ClosedApps::NONE;
    let timer = closed_ringer(&image, START_TIME + MINUTE, &mut system);
    closed.record("timer", &timer, START_TIME);
    assert_eq!(closed.next_alarm(""), NONE);

    let alarm = closed_ringer(&image, START_TIME + 8 * MINUTE, &mut system);
    closed.record(ALARM_APP_ID, &alarm, START_TIME);

    assert_eq!(closed.next_alarm(""), START_TIME + 8 * MINUTE);
    assert_eq!(closed.next_alarm(ALARM_APP_ID), NONE);
}

// ============================================================================
// Running
// ============================================================================

#[test]
fn a_due_ring_runs_while_closed_and_asks_to_wake_on_the_screen_it_named() {
    let image = ringer_image("timer", true);
    let mut system = RecordingSystem::default();
    let mut host = closed_ringer(&image, START_TIME + MINUTE, &mut system);

    let wake = run_while_closed(&mut host, START_TIME, START_TIME + MINUTE, &mut system);

    assert_eq!(
        wake,
        Some(WakeRequest {
            screen: Some(String::from("ringing"))
        })
    );
    assert_eq!(host.state().get(RINGS), Some(1));
    assert_eq!(host.state().get(RING_AT), Some(NONE));
    assert!(!host.is_open());
    assert_eq!(system.effects, vec![EffectKind::Vibrate]);
    assert_eq!(system.failures, 0);
    assert_eq!(system.closed_run, None);
}

#[test]
fn a_ring_that_does_not_wake_asks_for_nothing() {
    let image = ringer_image("timer", false);
    let mut system = RecordingSystem::default();
    let mut host = closed_ringer(&image, START_TIME + MINUTE, &mut system);

    assert_eq!(
        run_while_closed(&mut host, START_TIME, START_TIME + MINUTE - 1, &mut system),
        None
    );
    assert_eq!(host.state().get(RINGS), Some(0));

    assert_eq!(
        run_while_closed(&mut host, START_TIME, START_TIME + MINUTE, &mut system),
        None
    );
    assert_eq!(host.state().get(RINGS), Some(1));
}

#[test]
fn a_passed_ring_comes_once_at_once() {
    let image = ringer_image("timer", true);
    let mut system = RecordingSystem::default();
    let mut host = closed_ringer(&image, START_TIME - 3 * MINUTE, &mut system);
    let mut closed = ClosedApps::NONE;
    closed.record("timer", &host, START_TIME);
    assert_eq!(closed.due(START_TIME, ""), Some("timer"));

    assert!(run_while_closed(&mut host, START_TIME, START_TIME, &mut system).is_some());
    closed.record("timer", &host, START_TIME);

    assert_eq!(host.state().get(RINGS), Some(1));
    assert_eq!(closed.ids().count(), 0);
}

#[test]
fn a_closed_heart_rate_subscription_gets_the_reading_kept_for_it() {
    let image = ringer_image("heart-log", false);
    let mut system = RecordingSystem::default();
    let mut closed = ClosedApps::NONE;
    closed.record("heart-log", &closed_ringer(&image, NONE, &mut system), START_TIME);
    assert_eq!(closed.next_due(""), Some(START_TIME + MINUTE));
    let checked_at = closed.checked_at("heart-log").unwrap();

    // Loaded as it comes due, its timers count from the record, not from the load.
    let mut host = closed_ringer_at(&image, NONE, START_TIME + MINUTE, &mut system);
    host.remember_heart_rate(64, 90);
    run_while_closed(&mut host, checked_at, START_TIME + MINUTE, &mut system);

    assert_eq!(host.state().get(HEART_RATE), Some(64));
}

#[test]
fn a_closed_app_loaded_into_a_slot_is_the_same_as_one_returned() {
    let image = ringer_image("timer", true);
    let mut system = RecordingSystem::default();
    let mut slot = None;

    let host =
        AppHost::open_in_background_into(&mut slot, &image, DEFAULT_THEME, &[], START_TIME, &mut system)
            .unwrap();

    assert!(!host.is_open());
    assert_eq!(host.state().get(RING_AT), Some(NONE));
    assert!(AppHost::open_in_background_into(
        &mut slot,
        b"not an image",
        DEFAULT_THEME,
        &[],
        START_TIME,
        &mut system
    )
    .is_err());
}

#[test]
fn a_closed_app_never_reads_a_file_ahead_into_the_open_apps_slot() {
    let bytes = ImageWriter::new("reader", "Reader", AppKind::App, 1).build();
    let image = AppImage::parse(&bytes).unwrap();
    let interpreter = std::boxed::Box::new(neotime_runtime::Interpreter::new());
    let effect = AppEffect {
        kind: EffectKind::PrefetchFile,
        arguments: [0; neotime_runtime::instruction::MOST_EFFECT_ARGUMENTS],
        image: &image,
        interpreter: &interpreter,
    };

    assert!(ClosedRun::default().hold_back(&effect));
}

#[test]
fn a_closed_app_never_rolls_the_firmware_back_but_may_confirm_it() {
    let bytes = ImageWriter::new("updater", "Updater", AppKind::App, 1).build();
    let image = AppImage::parse(&bytes).unwrap();
    let interpreter = std::boxed::Box::new(neotime_runtime::Interpreter::new());
    let effect_of = |kind| AppEffect {
        kind,
        arguments: [0; neotime_runtime::instruction::MOST_EFFECT_ARGUMENTS],
        image: &image,
        interpreter: &interpreter,
    };

    assert!(ClosedRun::default().hold_back(&effect_of(EffectKind::RollBackFirmware)));
    assert!(!ClosedRun::default().hold_back(&effect_of(EffectKind::ConfirmFirmware)));
}
