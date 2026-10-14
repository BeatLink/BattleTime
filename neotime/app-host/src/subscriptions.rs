//! What the host keeps to run an app's subscriptions, and the events the embedder pushes to it.
//!
//! Timers, alarms, daily times, clock changes and the periodic sensor readings are due at times
//! `Subscription::next_due` works out; the host fires them from `tick`. Everything else arrives as
//! a `PushedEvent`. Derive subscriptions rebuild hidden arrays whenever what they read changes,
//! found by scanning their handler's code the way the bindings scan an expression.

use alloc::vec::Vec;

use neotime_runtime::{
    for_each_reached_instruction_in,
    instruction::Instruction,
    subscriptions::{Subscription, SubscriptionKind},
    AppImage, Inputs, SystemValue, Value, NONE,
};

// ============================================================================
// Pushed events
// ============================================================================

/// What the accelerometer noticed, in the order the `motion` subscription's detail numbers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionKind {
    WristRaise,
    Shake,
    Tap,
    DoubleTap,
    Still,
    Moving,
}

/// What happened to the battery, in the order the `battery` subscription's detail numbers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryChange {
    Low,
    ChargingStarts,
    ChargingStops,
    ChargingCompletes,
}

/// A phone call's state, as the `call` event's `kind` field numbers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallKind {
    Incoming,
    Answered,
    Ended,
}

/// Something the system tells the app about, with the fields its subscription handlers read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushedEvent<'text> {
    /// A heart rate measurement, such as one `measure_heart_rate` asked for.
    HeartRate {
        beats_per_minute: Value,
        confidence: Value,
    },
    Steps {
        steps_today: Value,
    },
    StepGoal {
        steps_today: Value,
    },
    Motion(MotionKind),
    /// A raw reading in thousandths of g; acceleration subscriptions get the latest at their period.
    Acceleration {
        x: Value,
        y: Value,
        z: Value,
    },
    Battery {
        change: BatteryChange,
        percent: Value,
    },
    Phone {
        is_connected: bool,
    },
    Notification {
        id: Value,
        app: &'text str,
        title: &'text str,
        body: &'text str,
        time: Value,
    },
    Call {
        kind: CallKind,
        caller: &'text str,
    },
    /// The phone's player changed; the new values are system values.
    Music,
    Weather,
    Navigation,
    SettingsChange {
        key: &'text str,
    },
}

/// One event's fields, numbers and text, in `EventKind::fields` order.
pub struct EventFields<'text> {
    pub numbers: [Value; 5],
    pub texts: [&'text str; 4],
    pub count: usize,
}

impl<'text> PushedEvent<'text> {
    /// The subscription kind that hears this event, and the detail it must have, if it has one.
    pub fn subscription(&self) -> (SubscriptionKind, Option<u8>) {
        use PushedEvent::*;
        match self {
            HeartRate { .. } => (SubscriptionKind::HeartRate, None),
            Steps { .. } => (SubscriptionKind::Steps, None),
            StepGoal { .. } => (SubscriptionKind::StepGoal, None),
            Motion(kind) => (SubscriptionKind::Motion, Some(*kind as u8)),
            Acceleration { .. } => (SubscriptionKind::Acceleration, None),
            Battery { change, .. } => (SubscriptionKind::Battery, Some(*change as u8)),
            Phone { is_connected } => (SubscriptionKind::Phone, Some(!*is_connected as u8)),
            Notification { .. } => (SubscriptionKind::Notification, None),
            Call { .. } => (SubscriptionKind::Call, None),
            Music => (SubscriptionKind::Music, None),
            Weather => (SubscriptionKind::Weather, None),
            Navigation => (SubscriptionKind::Navigation, None),
            SettingsChange { .. } => (SubscriptionKind::SettingsChange, None),
        }
    }

    /// The fields the handler reads, numbers by position and text by position.
    pub fn fields(&self) -> EventFields<'text> {
        use PushedEvent::*;
        let mut fields = EventFields {
            numbers: [0; 5],
            texts: [""; 4],
            count: 0,
        };
        let mut numbers = |values: &[Value]| {
            fields.numbers[..values.len()].copy_from_slice(values);
            fields.count = values.len();
        };
        match *self {
            HeartRate {
                beats_per_minute,
                confidence,
            } => numbers(&[beats_per_minute, confidence]),
            Steps { steps_today } | StepGoal { steps_today } => numbers(&[steps_today]),
            Motion(kind) => numbers(&[kind as Value]),
            Acceleration { x, y, z } => numbers(&[x, y, z]),
            Battery { percent, .. } => numbers(&[percent]),
            Phone { is_connected } => numbers(&[is_connected as Value]),
            Notification {
                id,
                app,
                title,
                body,
                time,
            } => {
                numbers(&[id, 0, 0, 0, time]);
                fields.texts = ["", app, title, body];
            }
            Call { kind, caller } => {
                numbers(&[kind as Value, 0]);
                fields.texts[1] = caller;
            }
            SettingsChange { key } => {
                numbers(&[0]);
                fields.texts[0] = key;
            }
            Music | Weather | Navigation => numbers(&[]),
        }
        fields
    }

    /// The system values the event changes, so what shows them redraws.
    pub fn changed_system_values(&self) -> u32 {
        use PushedEvent::*;
        use SystemValue as Values;
        let bits = |values: &[SystemValue]| values.iter().fold(0, |bits, value| bits | value.bit());
        match self {
            HeartRate { .. } => bits(&[Values::HeartRate]),
            Steps { .. } | StepGoal { .. } => bits(&[Values::StepsToday]),
            Battery { .. } => bits(&[Values::BatteryPercent, Values::IsCharging]),
            Phone { .. } => bits(&[Values::IsPhoneConnected]),
            Notification { .. } => bits(&[Values::UnreadNotifications]),
            Music => bits(&[Values::MusicIsPlaying]),
            Weather => bits(&[Values::WeatherTemperature]),
            Navigation => bits(&[Values::NavigationTurn]),
            SettingsChange { .. } => bits(&[
                Values::Uses24HourClock,
                Values::UsesMetricUnits,
                Values::StepGoal,
                Values::Chimes,
                Values::IsDoNotDisturb,
                Values::Brightness,
                Values::IsBluetoothEnabled,
            ]),
            Call { .. } => bits(&[Values::CallState]),
            Motion(_) | Acceleration { .. } => 0,
        }
    }
}

// ============================================================================
// Running subscriptions
// ============================================================================

/// After this many failures in a row a subscription stops until the app is opened again.
pub const FAILURES_BEFORE_TURNING_OFF: u8 = 3;

/// What the host remembers about one subscription.
#[derive(Clone, Copy, Debug)]
pub struct SubscriptionMemory {
    pub subscription: Subscription,
    /// For an `At`: the time it last fired for, so a passed time fires only once until it changes.
    pub fired_for: Value,
    pub failures_in_a_row: u8,
    /// For a derive: what its handler reads, and the variables it writes, which never re-run it.
    pub derive_inputs: Inputs,
}

impl SubscriptionMemory {
    pub fn is_turned_off(&self) -> bool {
        self.failures_in_a_row >= FAILURES_BEFORE_TURNING_OFF
    }

    pub fn is_timer(&self) -> bool {
        use SubscriptionKind::*;
        matches!(
            self.subscription.kind,
            Every | At | Daily | ClockChange | HeartRate | Acceleration
        )
    }
}

/// Reads every subscription of an image, with what each derive reads.
pub fn load_subscriptions(image: &AppImage) -> Vec<SubscriptionMemory> {
    (0..image.subscription_count())
        .filter_map(|index| image.subscription(index))
        .map(|subscription| SubscriptionMemory {
            subscription,
            fired_for: NONE,
            failures_in_a_row: 0,
            derive_inputs: match subscription.kind {
                SubscriptionKind::Derive => derive_inputs(image, subscription.handler),
                _ => Inputs::NOTHING,
            },
        })
        .collect()
}

/// What a derive handler reads, leaving out the variables it writes itself.
pub fn derive_inputs(image: &AppImage, handler: u16) -> Inputs {
    derive_inputs_in(image, None, handler)
}

/// Like `derive_inputs`, looking into the shared library's functions the handler calls.
pub fn derive_inputs_in(image: &AppImage, library: Option<&AppImage>, handler: u16) -> Inputs {
    let Some(code) = image.handler_code(handler) else {
        return Inputs::NOTHING;
    };
    let mut reads = Inputs::NOTHING;
    let mut writes = 0u64;
    let bit = |slot: u8| if slot < 64 { 1u64 << slot } else { 0 };
    for_each_reached_instruction_in(image, library, code, |instruction, _| match instruction {
        Instruction::ReadState(slot) | Instruction::ReadElement(slot) | Instruction::TextFromCodes(slot) => {
            reads.state_slots |= bit(slot)
        }
        Instruction::CopyArray { destination, source } => {
            reads.state_slots |= bit(source);
            writes |= bit(destination);
        }
        Instruction::WriteState(slot) | Instruction::WriteElement(slot) | Instruction::FillArray(slot) => {
            writes |= bit(slot)
        }
        Instruction::AskArray { array, .. } => reads.state_slots |= bit(array),
        Instruction::WorkArray {
            work,
            destination,
            source,
        } => {
            reads.state_slots |= bit(source);
            match work.writes() {
                true => writes |= bit(destination),
                false => reads.state_slots |= bit(destination),
            }
        }
        Instruction::ReadSystem(value) => reads.system_values |= value.bit(),
        _ => {}
    });
    reads.state_slots &= !writes;
    reads
}
