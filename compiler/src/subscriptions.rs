//! `define_app`'s subscriptions and lifecycle handlers, written into the image's subscriptions
//! section as `runtime/src/subscriptions.rs` reads them.
//!
//! Every rule the runtime's install checks apply to a subscription is checked here first, so a
//! mistake is a compile error at the call rather than an image the watch refuses.

use std::rc::Rc;

use neotime_runtime::subscriptions::{
    Subscription, SubscriptionKind, SubscriptionValue, SHORTEST_BACKGROUND_PERIOD_IN_MILLISECONDS,
    SHORTEST_PERIOD_IN_MILLISECONDS,
};
use neotime_runtime::{ValueType, NONE};
use oxc_ast::ast::{Argument, CallExpression, Expression};
use oxc_span::{GetSpan, Span};

use crate::compiler::Compiler;
use crate::constants::{strip_parentheses, Constant};
use crate::diagnostics::{fail, Compiled};
use crate::permissions::{runs_in_background, subscription_permission, Permission};
use crate::scope::{LibraryModule, Reference, Scope};
use crate::types::{NumberHint, Type};

// ============================================================================
// Names
// ============================================================================

/// The lifecycle options of `define_app` and the subscription each becomes.
pub fn lifecycle_kind(option: &str) -> Option<SubscriptionKind> {
    Some(match option {
        "onOpen" => SubscriptionKind::Open,
        "onClose" => SubscriptionKind::Close,
        "onScreenOn" => SubscriptionKind::ScreenOn,
        "onScreenOff" => SubscriptionKind::ScreenOff,
        _ => return None,
    })
}

/// The subscription a `neotime` function makes.
fn subscription_kind(name: &str) -> Option<SubscriptionKind> {
    use SubscriptionKind::*;
    Some(match name {
        "every" => Every,
        "at" => At,
        "daily" => Daily,
        "clock_change" => ClockChange,
        "heart_rate_reading" => HeartRate,
        "steps" => Steps,
        "step_goal" => StepGoal,
        "motion" => Motion,
        "acceleration" => Acceleration,
        "battery" => Battery,
        "phone" => Phone,
        "notification" => Notification,
        "call" => Call,
        "music" => Music,
        "weather" => Weather,
        "navigation" => Navigation,
        "settings_change" => SettingsChange,
        _ => return None,
    })
}

/// The names a subscription's `detail` numbers, in order, for the kinds that take one.
fn detail_names(kind: SubscriptionKind) -> &'static [&'static str] {
    match kind {
        SubscriptionKind::ClockChange => &["minute", "hour", "day"],
        SubscriptionKind::Motion => &["wrist_raise", "shake", "tap", "double_tap", "still", "moving"],
        SubscriptionKind::Battery => &["low", "starts", "stops", "completes"],
        SubscriptionKind::Phone => &["connects", "disconnects"],
        _ => &[],
    }
}

/// Where the handler is among a subscription's arguments, after its period, time or kind.
fn handler_position(kind: SubscriptionKind) -> usize {
    use SubscriptionKind::*;
    match kind {
        Every | At | Daily | ClockChange | HeartRate | Motion | Acceleration | Battery | Phone => 1,
        _ => 0,
    }
}

const ALL_DAYS: i64 = 0b111_1111;

// ============================================================================
// Compiling subscriptions
// ============================================================================

impl<'a> Compiler<'a> {
    /// `onOpen`, `onClose`, `onScreenOn` or `onScreenOff`: a subscription to the app's own life.
    pub fn compile_lifecycle(
        &mut self,
        kind: SubscriptionKind,
        value: &'a Expression<'a>,
        root: &Rc<Scope<'a>>,
    ) -> Compiled<()> {
        let handler = self.subscription_handler(value, root, kind, value.span())?;
        self.writer.subscription(Subscription {
            kind,
            only_while_open: false,
            handler,
            detail: 0,
            days: 0,
            value: SubscriptionValue::Constant(0),
        });
        Ok(())
    }

    /// Every subscription in `define_app({ subscriptions: [...] })`.
    pub fn compile_subscriptions(&mut self, value: &'a Expression<'a>, root: &Rc<Scope<'a>>) -> Compiled<()> {
        let Expression::ArrayExpression(array) = strip_parentheses(value) else {
            return fail(
                value.span(),
                "write `subscriptions` as an array, such as `[every({ minutes: 1 }, tick)]`",
            );
        };
        for element in &array.elements {
            let Some(Expression::CallExpression(call)) = element.as_expression().map(strip_parentheses)
            else {
                return fail(
                    element.span(),
                    "each subscription is a call, such as `every({ minutes: 1 }, tick)`",
                );
            };
            self.compile_subscription(call, root)?;
        }
        Ok(())
    }

    fn compile_subscription(&mut self, call: &'a CallExpression<'a>, root: &Rc<Scope<'a>>) -> Compiled<()> {
        let span = call.span;
        let name = match self.resolve_reference(&call.callee, root) {
            Some(Reference::Library {
                module: LibraryModule::Neotime,
                name,
            }) => name,
            _ => {
                return fail(
                    span,
                    "each subscription is a call to a `neotime` subscription, such as `every`",
                )
            }
        };
        if matches!(name.as_str(), "message" | "http_response") {
            return fail(
                span,
                format!("the `{name}` subscription is not in the image format yet: the runtime has no subscription kind for it"),
            );
        }
        let Some(kind) = subscription_kind(&name) else {
            return fail(span, format!("`{name}` is not a subscription"));
        };
        let mut arguments: Vec<&'a Expression<'a>> = Vec::new();
        for argument in &call.arguments {
            match argument {
                Argument::SpreadElement(spread) => {
                    return fail(spread.span, "`...` in a subscription is not supported")
                }
                other => arguments.extend(other.as_expression()),
            }
        }
        let argument = |position: usize| -> Compiled<&'a Expression<'a>> {
            arguments.get(position).copied().ok_or_else(|| {
                crate::diagnostics::Diagnostic::error(span, format!("`{name}` needs more arguments"))
            })
        };

        let handler_at = handler_position(kind);
        let options_at = if kind == SubscriptionKind::Daily {
            3
        } else {
            handler_at + 1
        };
        if arguments.len() > options_at + 1 {
            return fail(
                span,
                format!("`{name}` takes at most {} arguments", options_at + 1),
            );
        }
        let only_while_open = match arguments.get(options_at) {
            Some(options) => self.only_while_open(options, root)?,
            None => false,
        };

        let mut detail = 0;
        let mut days = 0;
        let value = match kind {
            SubscriptionKind::Every | SubscriptionKind::HeartRate | SubscriptionKind::Acceleration => {
                let period = argument(0)?;
                let milliseconds = self.period_in_milliseconds(period, root)?;
                check_period(kind, &name, milliseconds, only_while_open, period.span())?;
                SubscriptionValue::Constant(milliseconds)
            }
            SubscriptionKind::At => self.at_value(argument(0)?, root)?,
            SubscriptionKind::Daily => {
                days = match arguments.get(2) {
                    Some(days) => self.days_mask(days, root)?,
                    None => ALL_DAYS as u16,
                };
                self.daily_value(argument(0)?, root)?
            }
            SubscriptionKind::ClockChange
            | SubscriptionKind::Motion
            | SubscriptionKind::Battery
            | SubscriptionKind::Phone => {
                detail = self.detail_number(kind, &name, argument(0)?, root)?;
                SubscriptionValue::Constant(0)
            }
            _ => SubscriptionValue::Constant(0),
        };
        if kind == SubscriptionKind::Acceleration && !only_while_open {
            return fail(
                span,
                "`acceleration` runs only while the app is open; add `{ only_while_open: true }`",
            );
        }

        let handler = self.subscription_handler(argument(handler_at)?, root, kind, span)?;
        self.note_permission(
            subscription_permission(kind),
            format!("the `{name}` subscription"),
            span,
        );
        if runs_in_background(kind, only_while_open) {
            self.note_permission(
                Some(Permission::Background),
                format!("the `{name}` subscription, which runs while the app is closed,"),
                span,
            );
        }
        self.writer.subscription(Subscription {
            kind,
            only_while_open,
            handler,
            detail,
            days,
            value,
        });
        Ok(())
    }

    /// The handler a subscription or lifecycle option calls, compiled for its event.
    fn subscription_handler(
        &mut self,
        value: &'a Expression<'a>,
        root: &Rc<Scope<'a>>,
        kind: SubscriptionKind,
        span: Span,
    ) -> Compiled<u16> {
        let reference = match self.resolve_reference(value, root) {
            Some(reference) => reference,
            None => Reference::Expression {
                expression: value,
                scope: root.clone(),
            },
        };
        match self.event_property(reference, kind.event(), value.span())? {
            Some(handler) => Ok(handler),
            None => fail(
                span,
                "give the subscription a handler, such as a function the app defines",
            ),
        }
    }

    // ------------------------------------------------------------------------
    // Arguments
    // ------------------------------------------------------------------------

    /// `{ only_while_open: true }` as a subscription's last argument.
    fn only_while_open(&self, options: &'a Expression<'a>, root: &Rc<Scope<'a>>) -> Compiled<bool> {
        let Some(Constant::Object(fields)) = self.evaluate_constant(options, root) else {
            return fail(options.span(), "write the options as `{ only_while_open: true }`");
        };
        let mut only_while_open = false;
        for (field, value) in fields.iter() {
            match (field.as_str(), value) {
                ("only_while_open", Constant::Bool(value)) => only_while_open = *value,
                ("only_while_open", _) => {
                    return fail(options.span(), "`only_while_open` must be true or false")
                }
                (other, _) => return fail(options.span(), format!("a subscription has no option `{other}`")),
            }
        }
        Ok(only_while_open)
    }

    /// A period such as `{ seconds: 1 }`, in milliseconds.
    fn period_in_milliseconds(&self, period: &'a Expression<'a>, root: &Rc<Scope<'a>>) -> Compiled<i64> {
        let Some(Constant::Object(fields)) = self.evaluate_constant(period, root) else {
            return fail(
                period.span(),
                "a period must be known when compiling, such as `{ seconds: 1 }` or `{ minutes: 10 }`",
            );
        };
        let mut total: i64 = 0;
        for (field, value) in fields.iter() {
            let unit = match field.as_str() {
                "milliseconds" => 1,
                "seconds" => 1000,
                "minutes" => 60_000,
                "hours" => 3_600_000,
                other => {
                    return fail(
                        period.span(),
                        format!("a period has no `{other}`; use milliseconds, seconds, minutes or hours"),
                    )
                }
            };
            let Constant::Number(count) = value else {
                return fail(period.span(), format!("`{field}` must be a whole number"));
            };
            total = count
                .checked_mul(unit)
                .and_then(|part| total.checked_add(part))
                .ok_or_else(|| {
                    crate::diagnostics::Diagnostic::error(period.span(), "this period is too long")
                })?;
        }
        Ok(total)
    }

    /// `at`'s time: a signal holding an Instant, a constant, or anything else kept in a hidden variable.
    fn at_value(&mut self, when: &'a Expression<'a>, root: &Rc<Scope<'a>>) -> Compiled<SubscriptionValue> {
        let span = when.span();
        match self.evaluate_constant(when, root) {
            Some(Constant::Number(instant)) => return Ok(SubscriptionValue::Constant(instant)),
            Some(Constant::Null) => return Ok(SubscriptionValue::Constant(NONE)),
            Some(_) => return fail(span, "`at` needs an Instant, such as a signal holding one"),
            None => {}
        }
        let reference = self
            .resolve_reference(when, root)
            .unwrap_or(Reference::Expression {
                expression: when,
                scope: root.clone(),
            });
        if let Reference::Signal(variable) = reference {
            let slot = &self.slots[variable as usize];
            if !slot.is_array() && slot.value_type.value_type(slot.hint) == ValueType::Instant {
                return Ok(SubscriptionValue::Variable(variable));
            }
        }
        let variable = self.derived_scalar(reference, Type::NUMBER, NumberHint::Instant, span)?;
        Ok(SubscriptionValue::Variable(variable))
    }

    /// `daily`'s time of day: `{ hour, minute }` known now, or a signal holding one, as a minute of the day.
    fn daily_value(
        &mut self,
        time_of_day: &'a Expression<'a>,
        root: &Rc<Scope<'a>>,
    ) -> Compiled<SubscriptionValue> {
        let span = time_of_day.span();
        match self.evaluate_constant(time_of_day, root) {
            Some(Constant::Null) => return Ok(SubscriptionValue::Constant(NONE)),
            Some(time) => {
                let (Some(Constant::Number(hour)), Some(Constant::Number(minute))) =
                    (time.field("hour"), time.field("minute"))
                else {
                    return fail(
                        span,
                        "a time of day is `{ hour, minute }`, such as `{ hour: 7, minute: 30 }`",
                    );
                };
                if !(0..24).contains(&hour) || !(0..60).contains(&minute) {
                    return fail(span, "the hour must be 0 to 23 and the minute 0 to 59");
                }
                return Ok(SubscriptionValue::Constant(hour * 60 + minute));
            }
            None => {}
        }
        match self.resolve_reference(time_of_day, root) {
            Some(Reference::Record(fields)) => {
                let variable = self.derived_scalar(Reference::Record(fields), Type::NUMBER, NumberHint::Plain, span)?;
                Ok(SubscriptionValue::Variable(variable))
            }
            Some(Reference::Signal(variable)) if self.is_scalar_int(variable) => {
                Ok(SubscriptionValue::Variable(variable))
            }
            _ => fail(
                span,
                "`daily` needs a time of day known when compiling, a signal holding `{ hour, minute }`, or a signal holding the minute of the day",
            ),
        }
    }

    fn is_scalar_int(&self, variable: u8) -> bool {
        let slot = &self.slots[variable as usize];
        !slot.is_array() && slot.value_type.value_type(slot.hint) == ValueType::Int
    }

    /// `daily`'s weekdays, a 7-bit mask with Monday first, which must be known when compiling.
    fn days_mask(&self, days: &'a Expression<'a>, root: &Rc<Scope<'a>>) -> Compiled<u16> {
        match self.evaluate_constant(days, root) {
            Some(Constant::Number(mask)) if (1..=ALL_DAYS).contains(&mask) => Ok(mask as u16),
            Some(Constant::Undefined) => Ok(ALL_DAYS as u16),
            Some(_) => fail(days.span(), "the days are a mask from 1 to 127, Monday first, such as `EVERY_DAY`"),
            None => fail(
                days.span(),
                "the days must be known when compiling: the image keeps them as a constant, so a signal cannot choose them yet",
            ),
        }
    }

    /// The `detail` of a clock change, motion, battery or phone subscription, from its name.
    fn detail_number(
        &self,
        kind: SubscriptionKind,
        function: &str,
        name: &'a Expression<'a>,
        root: &Rc<Scope<'a>>,
    ) -> Compiled<u8> {
        let names = detail_names(kind);
        let found = match self.evaluate_constant(name, root) {
            Some(Constant::Text(text)) => names.iter().position(|candidate| **candidate == *text),
            _ => None,
        };
        match found {
            Some(position) => Ok(position as u8),
            None => fail(
                name.span(),
                format!("`{function}` takes one of: {}", names.join(", ")),
            ),
        }
    }
}

/// The period rules of `runtime/src/subscriptions.rs`, checked when compiling.
fn check_period(
    kind: SubscriptionKind,
    name: &str,
    milliseconds: i64,
    only_while_open: bool,
    span: Span,
) -> Compiled<()> {
    if milliseconds < SHORTEST_PERIOD_IN_MILLISECONDS {
        return fail(
            span,
            format!("`{name}` runs at most every {SHORTEST_PERIOD_IN_MILLISECONDS} milliseconds, and this period is {milliseconds}"),
        );
    }
    let minute = SHORTEST_BACKGROUND_PERIOD_IN_MILLISECONDS;
    if milliseconds < minute && !only_while_open && kind != SubscriptionKind::Acceleration {
        return fail(
            span,
            format!("a period under a minute only runs while the app is open; add `{{ only_while_open: true }}` to `{name}`, or make it at least {minute} milliseconds"),
        );
    }
    Ok(())
}
