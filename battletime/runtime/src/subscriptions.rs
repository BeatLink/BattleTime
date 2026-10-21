//! An app's subscriptions: the times, sensors and phone data it asks to hear about, and the points
//! in its life it wants to answer, each with the handler to call.
//!
//! The image lists them in section 6. Timers, alarms, daily times and clock changes have a next
//! time, which `next_due` works out, so the runtime can sleep until the earliest one. Everything
//! else is pushed to the app when it happens.

use crate::events::EventKind;
use crate::image::{AppImage, ImageError, ValueType};
use crate::instruction::TimeUnit;
use crate::interpreter::{Value, NONE};
use crate::state::AppState;

// ============================================================================
// Layout
// ============================================================================

pub const SUBSCRIPTION_BYTES: usize = 16;

/// The shortest period a subscription may have while the app is closed.
pub const SHORTEST_BACKGROUND_PERIOD_IN_MILLISECONDS: Value = 60_000;

/// The shortest period any subscription may have; under a minute it must also be `only_while_open`.
pub const SHORTEST_PERIOD_IN_MILLISECONDS: Value = 20;

const MILLISECONDS_PER_MINUTE: Value = 60_000;
const MILLISECONDS_PER_DAY: Value = 86_400_000;
const ALL_DAYS: u16 = 0b111_1111;

/// How many keys a message subscription names: its text first, then its numbers.
pub const MESSAGE_KEYS: usize = 5;

/// A message key that names no field.
pub const NO_MESSAGE_KEY: u16 = u16::MAX;

// ============================================================================
// Kinds
// ============================================================================

/// What a subscription waits for, in the order the kind byte numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubscriptionKind {
    /// The app comes to the front.
    Open,
    /// The app leaves the front.
    Close,
    ScreenOn,
    ScreenOff,
    /// Repeatedly, every `value` milliseconds.
    Every,
    /// Once, at the instant in `value` or in the state variable it names.
    At,
    /// Each day at the local minute of the day in `value` or the variable, on the weekdays in `days`.
    Daily,
    /// Each time the local minute, hour or day changes; `detail` is the `TimeUnit`.
    ClockChange,
    /// A heart rate reading every `value` milliseconds.
    HeartRate,
    Steps,
    StepGoal,
    /// `detail`: 0 wrist raise, 1 shake, 2 tap, 3 double tap, 4 still, 5 moving.
    Motion,
    /// Raw readings every `value` milliseconds; only while the app is open.
    Acceleration,
    /// `detail`: 0 low, 1 charging starts, 2 stops, 3 completes.
    Battery,
    /// `detail`: 0 connects, 1 disconnects.
    Phone,
    Notification,
    Call,
    Music,
    Weather,
    Navigation,
    SettingsChange,
    /// Rebuilds a hidden array, such as a `computed` array a widget shows. The runtime runs the
    /// handler when the app opens and whenever anything the handler reads changes.
    Derive,
    /// A message from the phone for this app. `days` names the string of its text key and `value`
    /// holds the strings of up to four number keys, as `message_keys` reads them.
    Message,
}

impl SubscriptionKind {
    const ALL: [SubscriptionKind; 23] = {
        use SubscriptionKind::*;
        [
            Open,
            Close,
            ScreenOn,
            ScreenOff,
            Every,
            At,
            Daily,
            ClockChange,
            HeartRate,
            Steps,
            StepGoal,
            Motion,
            Acceleration,
            Battery,
            Phone,
            Notification,
            Call,
            Music,
            Weather,
            Navigation,
            SettingsChange,
            Derive,
            Message,
        ]
    };

    pub fn from_byte(byte: u8) -> Option<SubscriptionKind> {
        Self::ALL.get(byte as usize).copied()
    }

    /// The event the handler receives.
    pub fn event(self) -> EventKind {
        use SubscriptionKind::*;
        match self {
            Open => EventKind::Open,
            Close => EventKind::Close,
            ScreenOn => EventKind::ScreenOn,
            ScreenOff => EventKind::ScreenOff,
            Every | At | Daily | ClockChange => EventKind::Time,
            HeartRate => EventKind::HeartRate,
            Steps | StepGoal => EventKind::Steps,
            Motion => EventKind::Motion,
            Acceleration => EventKind::Acceleration,
            Battery => EventKind::Battery,
            Phone => EventKind::Phone,
            Notification => EventKind::Notification,
            Call => EventKind::Call,
            Music | Weather | Navigation => EventKind::PhoneData,
            SettingsChange => EventKind::SettingsChange,
            Derive => EventKind::Derive,
            Message => EventKind::Message,
        }
    }

    /// How many values `detail` may take, for the kinds that use it.
    fn detail_count(self) -> u8 {
        match self {
            SubscriptionKind::ClockChange => 3,
            SubscriptionKind::Motion => 6,
            SubscriptionKind::Battery => 4,
            SubscriptionKind::Phone => 2,
            _ => 1,
        }
    }

    fn has_period(self) -> bool {
        matches!(
            self,
            SubscriptionKind::Every | SubscriptionKind::HeartRate | SubscriptionKind::Acceleration
        )
    }
}

// ============================================================================
// Subscriptions
// ============================================================================

/// Where a subscription's `value` comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubscriptionValue {
    Constant(Value),
    /// The number of a state variable, so changing it, such as an alarm's time, reschedules.
    Variable(u8),
}

/// One subscription as the image lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Subscription {
    pub kind: SubscriptionKind,
    /// Stops when the app closes, and needs no `background` permission.
    pub only_while_open: bool,
    pub handler: u16,
    pub detail: u8,
    /// For `Daily`: the weekdays it rings on, bit 0 for Monday.
    pub days: u16,
    pub value: SubscriptionValue,
}

impl Subscription {
    pub fn parse(bytes: &[u8]) -> Option<Subscription> {
        let bytes: &[u8; SUBSCRIPTION_BYTES] = bytes.try_into().ok()?;
        let raw_value = i64::from_le_bytes(bytes[8..16].try_into().ok()?);
        let value = match bytes[4] {
            0 => SubscriptionValue::Constant(raw_value),
            1 => SubscriptionValue::Variable(u8::try_from(raw_value).ok()?),
            _ => return None,
        };
        Some(Subscription {
            kind: SubscriptionKind::from_byte(bytes[0])?,
            only_while_open: bytes[1] & 1 != 0,
            handler: u16::from_le_bytes([bytes[2], bytes[3]]),
            detail: bytes[5],
            days: u16::from_le_bytes([bytes[6], bytes[7]]),
            value,
        })
    }

    /// The subscription's bytes, as the image stores them.
    pub fn to_bytes(&self) -> [u8; SUBSCRIPTION_BYTES] {
        let mut bytes = [0u8; SUBSCRIPTION_BYTES];
        let (source, raw_value) = match self.value {
            SubscriptionValue::Constant(value) => (0, value),
            SubscriptionValue::Variable(variable) => (1, variable as Value),
        };
        bytes[0] = self.kind as u8;
        bytes[1] = self.only_while_open as u8;
        bytes[2..4].copy_from_slice(&self.handler.to_le_bytes());
        bytes[4] = source;
        bytes[5] = self.detail;
        bytes[6..8].copy_from_slice(&self.days.to_le_bytes());
        bytes[8..16].copy_from_slice(&raw_value.to_le_bytes());
        bytes
    }

    /// A message subscription made from the strings of its keys: its text first, then its numbers, `NO_MESSAGE_KEY` for none.
    pub fn for_message(handler: u16, only_while_open: bool, keys: [u16; MESSAGE_KEYS]) -> Subscription {
        let mut value = [0u8; 8];
        for (place, key) in keys[1..].iter().enumerate() {
            value[place * 2..place * 2 + 2].copy_from_slice(&key.to_le_bytes());
        }
        Subscription {
            kind: SubscriptionKind::Message,
            only_while_open,
            handler,
            detail: 0,
            days: keys[0],
            value: SubscriptionValue::Constant(i64::from_le_bytes(value)),
        }
    }

    /// The strings of a message subscription's keys, in the order of the event's fields: its text, then its numbers.
    pub fn message_keys(&self) -> [u16; MESSAGE_KEYS] {
        let mut keys = [NO_MESSAGE_KEY; MESSAGE_KEYS];
        keys[0] = self.days;
        if let SubscriptionValue::Constant(value) = self.value {
            let bytes = value.to_le_bytes();
            for (place, key) in keys[1..].iter_mut().enumerate() {
                *key = u16::from_le_bytes([bytes[place * 2], bytes[place * 2 + 1]]);
            }
        }
        keys
    }

    /// The subscription's value now: its constant, or what its state variable holds.
    pub fn current_value(&self, state: &AppState) -> Option<Value> {
        match self.value {
            SubscriptionValue::Constant(value) => Some(value),
            SubscriptionValue::Variable(variable) => state.get(variable),
        }
    }

    /// Whether the runtime keeps a time for it while the app is closed: a timer, an alarm, a daily
    /// time, a clock change or a heart rate period, without `only_while_open`.
    pub fn runs_while_closed(&self) -> bool {
        use SubscriptionKind::*;
        !self.only_while_open && matches!(self.kind, Every | At | Daily | ClockChange | HeartRate)
    }

    /// When a timer-like subscription is next due after `after`, or `None` if it waits on events
    /// or is turned off. `utc_offset` places daily times and clock changes in local time.
    ///
    /// An `At` time is returned even when it has passed, so the runtime fires it once at once; the
    /// runtime remembers that it fired.
    pub fn next_due(&self, after: Value, state: &AppState, utc_offset: Value) -> Option<Value> {
        let value = self.current_value(state)?;
        match self.kind {
            SubscriptionKind::Every | SubscriptionKind::HeartRate | SubscriptionKind::Acceleration => {
                let period = value.max(SHORTEST_PERIOD_IN_MILLISECONDS);
                Some((after.div_euclid(period) + 1) * period)
            }
            SubscriptionKind::At => (value != NONE).then_some(value),
            SubscriptionKind::Daily => {
                if value == NONE {
                    return None;
                }
                next_daily(after, value, self.days, utc_offset)
            }
            SubscriptionKind::ClockChange => {
                let unit = match self.detail {
                    0 => MILLISECONDS_PER_MINUTE,
                    1 => 60 * MILLISECONDS_PER_MINUTE,
                    _ => MILLISECONDS_PER_DAY,
                };
                let local = after.checked_add(utc_offset)?;
                Some((local.div_euclid(unit) + 1) * unit - utc_offset)
            }
            _ => None,
        }
    }
}

/// The first instant after `after` at a local minute of the day, on one of `days`.
fn next_daily(after: Value, minute_of_day: Value, days: u16, utc_offset: Value) -> Option<Value> {
    let local_after = after.checked_add(utc_offset)?;
    let today = local_after.div_euclid(MILLISECONDS_PER_DAY);
    for day in today..today + 8 {
        // 1 January 1970 was a Thursday, which is 3 when Monday is 0.
        let weekday = (day + 3).rem_euclid(7);
        if days & (1 << weekday) == 0 {
            continue;
        }
        let local = day * MILLISECONDS_PER_DAY + minute_of_day * MILLISECONDS_PER_MINUTE;
        if local > local_after {
            return Some(local - utc_offset);
        }
    }
    None
}

// ============================================================================
// Checking
// ============================================================================

/// What is wrong with a subscription.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubscriptionProblem {
    Unreadable,
    HandlerOutOfRange,
    BadVariable,
    BadDetail,
    BadDays,
    PeriodTooShort,
    MustBeOnlyWhileOpen,
    BadTimeOfDay,
    /// A message key that names no string, or the same string twice.
    BadMessageKey,
}

/// Checks every subscription in the image.
pub fn verify_subscriptions(image: &AppImage) -> Result<(), ImageError> {
    for index in 0..image.subscription_count() {
        let fail = |problem| ImageError::BadSubscription {
            subscription: index,
            problem,
        };
        let subscription = image
            .subscription(index)
            .ok_or(fail(SubscriptionProblem::Unreadable))?;
        check_subscription(image, &subscription).map_err(fail)?;
    }
    Ok(())
}

fn check_subscription(image: &AppImage, subscription: &Subscription) -> Result<(), SubscriptionProblem> {
    use SubscriptionKind::*;

    if subscription.handler >= image.handler_count() {
        return Err(SubscriptionProblem::HandlerOutOfRange);
    }
    if subscription.detail >= subscription.kind.detail_count() {
        return Err(SubscriptionProblem::BadDetail);
    }
    if subscription.kind == Daily && (subscription.days == 0 || subscription.days > ALL_DAYS) {
        return Err(SubscriptionProblem::BadDays);
    }
    if subscription.kind == Message {
        check_message_keys(image, subscription)?;
    }

    match subscription.value {
        SubscriptionValue::Variable(variable) => {
            let wanted = match subscription.kind {
                At => ValueType::Instant,
                Daily => ValueType::Int,
                _ => return Err(SubscriptionProblem::BadVariable),
            };
            let found = image
                .state_variable(variable as u16)
                .ok_or(SubscriptionProblem::BadVariable)?;
            if found.is_array() || found.value_type != wanted {
                return Err(SubscriptionProblem::BadVariable);
            }
        }
        SubscriptionValue::Constant(value) => {
            if subscription.kind.has_period() && value < SHORTEST_PERIOD_IN_MILLISECONDS {
                return Err(SubscriptionProblem::PeriodTooShort);
            }
            if subscription.kind == Daily && !(0..24 * 60).contains(&value) && value != NONE {
                return Err(SubscriptionProblem::BadTimeOfDay);
            }
            let short_period = value < SHORTEST_BACKGROUND_PERIOD_IN_MILLISECONDS;
            let needs_open =
                subscription.kind == Acceleration || (subscription.kind.has_period() && short_period);
            if needs_open && !subscription.only_while_open {
                return Err(SubscriptionProblem::MustBeOnlyWhileOpen);
            }
        }
    }
    Ok(())
}

/// Every key of a message subscription names a string, and no two the same one.
fn check_message_keys(image: &AppImage, subscription: &Subscription) -> Result<(), SubscriptionProblem> {
    let keys = subscription.message_keys();
    for (place, &key) in keys.iter().enumerate() {
        if key == NO_MESSAGE_KEY {
            continue;
        }
        if image.string(key).is_none() || keys[..place].contains(&key) {
            return Err(SubscriptionProblem::BadMessageKey);
        }
    }
    Ok(())
}

/// Kept so the time-unit numbering of `ClockChange` follows `TimeUnit`.
const _: () = assert!(TimeUnit::Minute as u8 == 0 && TimeUnit::Hour as u8 == 1 && TimeUnit::Day as u8 == 2);

#[cfg(test)]
#[path = "subscriptions_tests.rs"]
mod subscriptions_tests;
