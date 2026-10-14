//! How often the accelerometer must be read for an app, so it is read only while an app wants it.
//!
//! An `acceleration` subscription wants readings at its period, but never more than ten a second,
//! and a `motion("shake")` subscription wants the readings a shake is found in.

use neotime_runtime::{subscriptions::SubscriptionKind, Value};

use crate::{AppHost, MotionKind};

/// Acceleration readings come at most ten times a second.
pub const SHORTEST_READING_PERIOD_IN_MILLISECONDS: Value = 100;

/// How often the accelerometer is read while an app waits for a shake.
pub const SHAKE_READING_PERIOD_IN_MILLISECONDS: Value = 100;

/// How often to read the accelerometer for this app, or `None` when nothing it runs needs readings.
pub fn reading_period_in_milliseconds(host: &AppHost<'_>) -> Option<Value> {
    let acceleration = host
        .shortest_period_of(SubscriptionKind::Acceleration)
        .map(|period| period.max(SHORTEST_READING_PERIOD_IN_MILLISECONDS));
    let shake = host
        .listens_for(SubscriptionKind::Motion, Some(MotionKind::Shake as u8))
        .then_some(SHAKE_READING_PERIOD_IN_MILLISECONDS);
    [acceleration, shake].into_iter().flatten().min()
}
