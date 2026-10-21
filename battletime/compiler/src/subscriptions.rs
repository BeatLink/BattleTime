//! The events an app connects to with `on`, written into the image's subscriptions section as
//! `runtime/src/subscriptions.rs` reads them.
//!
//! Every rule the runtime's install checks apply to a subscription is checked here first, so a
//! mistake is a compile error at the call rather than an image the watch refuses.

use std::rc::Rc;

use battletime_runtime::instruction::Instruction;
use battletime_runtime::subscriptions::{
    Subscription, SubscriptionKind, SubscriptionValue, MESSAGE_KEYS, NO_MESSAGE_KEY,
    SHORTEST_BACKGROUND_PERIOD_IN_MILLISECONDS, SHORTEST_PERIOD_IN_MILLISECONDS,
};
use battletime_runtime::{SystemValue, ValueType, NONE};
use oxc_ast::ast::{
    Argument, CallExpression, Expression, ObjectPropertyKind, TSSignature, TSType, TSTypeLiteral, TSTypeName,
};
use oxc_span::{GetSpan, Span};

use crate::compiler::{Compiler, DeclaredType};
use crate::constants::{static_key, strip_parentheses, Constant};
use crate::diagnostics::{fail, Compiled};
use crate::emitter::{CodeKind, Emitter};
use crate::permissions::{runs_in_background, subscription_permission, Permission};
use crate::scope::{FunctionReference, Reference, Scope};
use crate::statements::ArgumentSource;
use crate::types::{Kind, NumberHint, Type};

// ============================================================================
// Names
// ============================================================================

const ALL_DAYS: i64 = 0b111_1111;

/// The subscription and detail each event name given to `on` stands for.
const EVENTS: [(&str, SubscriptionKind, u8); 33] = {
    use SubscriptionKind::*;
    [
        ("open", Open, 0),
        ("close", Close, 0),
        ("screen_on", ScreenOn, 0),
        ("screen_off", ScreenOff, 0),
        ("tick", Every, 0),
        ("time", At, 0),
        ("daily", Daily, 0),
        ("minute", ClockChange, 0),
        ("hour", ClockChange, 1),
        ("day", ClockChange, 2),
        ("heart_rate", HeartRate, 0),
        ("steps", Steps, 0),
        ("step_goal", StepGoal, 0),
        ("wrist_raise", Motion, 0),
        ("shake", Motion, 1),
        ("wrist_tap", Motion, 2),
        ("wrist_double_tap", Motion, 3),
        ("still", Motion, 4),
        ("moving", Motion, 5),
        ("acceleration", Acceleration, 0),
        ("battery_low", Battery, 0),
        ("charging_starts", Battery, 1),
        ("charging_stops", Battery, 2),
        ("charging_completes", Battery, 3),
        ("phone_connects", Phone, 0),
        ("phone_disconnects", Phone, 1),
        ("notification", Notification, 0),
        ("call", Call, 0),
        ("music", Music, 0),
        ("weather", Weather, 0),
        ("navigation", Navigation, 0),
        ("settings_change", SettingsChange, 0),
        ("message", Message, 0),
    ]
};

/// What `on`'s options say, before they are checked against the event.
#[derive(Default)]
struct ListenerOptions<'a> {
    while_open: bool,
    every: Option<&'a Expression<'a>>,
    at: Option<&'a Expression<'a>>,
    days: Option<&'a Expression<'a>>,
    at_most_every: Option<&'a Expression<'a>>,
}

// ============================================================================
// Compiling subscriptions
// ============================================================================

impl<'a> Compiler<'a> {
    /// `on("event", handler, { ...options })` at the top of the file: one subscription.
    pub fn compile_listener(&mut self, call: &'a CallExpression<'a>, root: &Rc<Scope<'a>>) -> Compiled<()> {
        let span = call.span;
        let mut arguments: Vec<&'a Expression<'a>> = Vec::new();
        for argument in &call.arguments {
            match argument {
                Argument::SpreadElement(spread) => {
                    return fail(spread.span, "`...` in `on` is not supported")
                }
                other => arguments.extend(other.as_expression()),
            }
        }
        let (Some(event), Some(handler)) = (arguments.first(), arguments.get(1)) else {
            return fail(
                span,
                "write `on(\"event\", handler)`, such as `on(\"shake\", roll)`",
            );
        };
        if arguments.len() > 3 {
            return fail(span, "`on` takes an event, a handler and its options");
        }
        let event_name = match self.evaluate_constant(event, root) {
            Some(Constant::Text(name)) => name.to_string(),
            _ => {
                return fail(
                    event.span(),
                    "name the event with text written out, such as `\"shake\"`",
                )
            }
        };
        if event_name == "http_response" {
            return fail(
                event.span(),
                format!("the `{event_name}` event is not in the image format yet: the runtime has no subscription kind for it"),
            );
        }
        let Some(&(_, kind, detail)) = EVENTS.iter().find(|(name, _, _)| *name == event_name) else {
            let known: Vec<&str> = EVENTS.iter().map(|(name, _, _)| *name).collect();
            return fail(
                event.span(),
                format!(
                    "there is no event `{event_name}`; choose from {}",
                    known.join(", ")
                ),
            );
        };
        let options = match arguments.get(2) {
            Some(options) => self.listener_options(options)?,
            None => ListenerOptions::default(),
        };
        let what = format!("the `{event_name}` event");
        let is_lifecycle = matches!(
            kind,
            SubscriptionKind::Open
                | SubscriptionKind::Close
                | SubscriptionKind::ScreenOn
                | SubscriptionKind::ScreenOff
        );
        if is_lifecycle && options.while_open {
            return fail(
                span,
                format!("{what} only comes while the app is open, so it takes no `while_open`"),
            );
        }

        let needs = |option: Option<&'a Expression<'a>>,
                     option_name: &str,
                     example: &str|
         -> Compiled<&'a Expression<'a>> {
            option.ok_or_else(|| {
                crate::diagnostics::Diagnostic::error(
                    span,
                    format!("{what} needs `{option_name}`, such as `{example}`"),
                )
            })
        };
        let takes_every = matches!(
            kind,
            SubscriptionKind::Every | SubscriptionKind::HeartRate | SubscriptionKind::Acceleration
        );
        let takes_at = matches!(kind, SubscriptionKind::At | SubscriptionKind::Daily);
        for (given, option_name, allowed) in [
            (options.every.is_some(), "every", takes_every),
            (options.at.is_some(), "at", takes_at),
            (options.days.is_some(), "days", kind == SubscriptionKind::Daily),
        ] {
            if given && !allowed {
                return fail(span, format!("{what} takes no `{option_name}`"));
            }
        }

        let mut days = 0;
        let value = match kind {
            SubscriptionKind::Every | SubscriptionKind::HeartRate | SubscriptionKind::Acceleration => {
                let every = needs(options.every, "every", "{ every: MINUTE }")?;
                let milliseconds = self.duration_in_milliseconds(every, root)?;
                check_period(kind, &event_name, milliseconds, options.while_open, every.span())?;
                SubscriptionValue::Constant(milliseconds)
            }
            SubscriptionKind::At => self.at_value(needs(options.at, "at", "{ at: next_ring }")?, root)?,
            SubscriptionKind::Daily => {
                days = match options.days {
                    Some(days) => self.days_mask(days, root)?,
                    None => ALL_DAYS as u16,
                };
                self.daily_value(needs(options.at, "at", "{ at: { hour: 7, minute: 30 } }")?, root)?
            }
            _ => SubscriptionValue::Constant(0),
        };
        if kind == SubscriptionKind::Acceleration && !options.while_open {
            return fail(
                span,
                "`acceleration` comes only while the app is open; add `while_open: true`",
            );
        }

        let mut message_keys = [NO_MESSAGE_KEY; MESSAGE_KEYS];
        let handler = match (options.at_most_every, kind) {
            (pause, SubscriptionKind::Message) => {
                let function = self.message_function(handler, root)?;
                let (argument, keys) = self.message_argument(&function)?;
                message_keys = keys;
                match pause {
                    Some(pause) => {
                        let milliseconds = self.duration_in_milliseconds(pause, root)?;
                        self.throttled_handler(&function, argument, milliseconds, pause.span())?
                    }
                    None => {
                        let mut emitter = Emitter::new(CodeKind::Handler);
                        let arguments = vec![ArgumentSource::Reference(argument)];
                        self.inline_call(&mut emitter, &function, arguments, false, None, span)?;
                        let code = emitter.finish(span)?;
                        self.add_handler(code)
                    }
                }
            }
            (Some(pause), _) => {
                let milliseconds = self.duration_in_milliseconds(pause, root)?;
                let Some(Reference::Function(function)) = self.resolve_reference(handler, root) else {
                    return fail(
                        handler.span(),
                        "`at_most_every` needs a function the app defines as the handler",
                    );
                };
                let argument = Reference::Event(kind.event());
                self.throttled_handler(&function, argument, milliseconds, pause.span())?
            }
            (None, _) => self.subscription_handler(handler, root, kind, span)?,
        };
        self.note_permission(subscription_permission(kind), what.clone(), span);
        if runs_in_background(kind, options.while_open) {
            self.note_permission(
                Some(Permission::Background),
                format!("{what}, which comes while the app is closed,"),
                span,
            );
        }
        let subscription = match kind {
            SubscriptionKind::Message => Subscription::for_message(handler, options.while_open, message_keys),
            _ => Subscription {
                kind,
                only_while_open: options.while_open,
                handler,
                detail,
                days,
                value,
            },
        };
        self.writer.subscription(subscription);
        Ok(())
    }

    /// `on`'s options object, each field kept as written so the event can check it.
    fn listener_options(&self, options: &'a Expression<'a>) -> Compiled<ListenerOptions<'a>> {
        let Expression::ObjectExpression(object) = strip_parentheses(options) else {
            return fail(
                options.span(),
                "write `on`'s options as an object, such as `{ while_open: true }`",
            );
        };
        let mut found = ListenerOptions::default();
        for property in &object.properties {
            let ObjectPropertyKind::ObjectProperty(property) = property else {
                return fail(object.span, "write `on`'s options out one by one, without `...`");
            };
            let Some(name) = static_key(&property.key) else {
                return fail(property.span, "write `on`'s options with plain names");
            };
            let value = &property.value;
            match name.as_str() {
                "while_open" => match strip_parentheses(value) {
                    Expression::BooleanLiteral(literal) => found.while_open = literal.value,
                    _ => return fail(value.span(), "`while_open` must be written out as true or false"),
                },
                "every" => found.every = Some(value),
                "at" => found.at = Some(value),
                "days" => found.days = Some(value),
                "at_most_every" => found.at_most_every = Some(value),
                other => {
                    return fail(
                        property.span,
                        format!("`on` has no option `{other}`; it takes while_open, every, at, days and at_most_every"),
                    )
                }
            }
        }
        Ok(found)
    }

    /// A length of time known when compiling: milliseconds such as `MINUTE`, or a period such as `{ seconds: 1 }`.
    fn duration_in_milliseconds(&self, duration: &'a Expression<'a>, root: &Rc<Scope<'a>>) -> Compiled<i64> {
        match self.evaluate_constant(duration, root) {
            Some(Constant::Number(milliseconds)) if milliseconds > 0 => Ok(milliseconds),
            Some(Constant::Object(_)) => self.period_in_milliseconds(duration, root),
            _ => fail(
                duration.span(),
                "a length of time must be known when compiling and above zero, such as `MINUTE` or `{ seconds: 1 }`",
            ),
        }
    }

    /// A handler that runs only when `pause` has passed since it last ran, timed by a hidden variable.
    fn throttled_handler(
        &mut self,
        function: &FunctionReference<'a>,
        argument: Reference<'a>,
        pause: i64,
        span: Span,
    ) -> Compiled<u16> {
        let key = format!("~last_run{}", self.slots.len());
        let declared = DeclaredType {
            value_type: Type::NUMBER,
            hint: NumberHint::Instant,
            is_array: false,
        };
        let last_run = self.add_signal_slot(key, false, declared, &Constant::Number(0), span)?;
        let mut emitter = Emitter::new(CodeKind::Handler);
        let skip = emitter.label();
        emitter.emit(Instruction::ReadSystem(SystemValue::Now), span)?;
        emitter.emit(Instruction::ReadState(last_run), span)?;
        emitter.emit(Instruction::Subtract, span)?;
        self.emit_number(&mut emitter, pause, span)?;
        emitter.emit(Instruction::GreaterOrEqual, span)?;
        emitter.jump_if_false(skip, span)?;
        emitter.emit(Instruction::ReadSystem(SystemValue::Now), span)?;
        emitter.emit(Instruction::WriteState(last_run), span)?;
        self.note_write(last_run, crate::types::Range::ANY);
        let arguments = vec![ArgumentSource::Reference(argument)];
        self.inline_call(&mut emitter, function, arguments, false, None, span)?;
        emitter.place(skip);
        let code = emitter.finish(span)?;
        Ok(self.add_handler(code))
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
    // Messages from the phone
    // ------------------------------------------------------------------------

    /// The function a `message` subscription calls, whose parameter's type names the fields it reads.
    fn message_function(
        &self,
        value: &'a Expression<'a>,
        root: &Rc<Scope<'a>>,
    ) -> Compiled<FunctionReference<'a>> {
        match self.resolve_reference(value, root) {
            Some(Reference::Function(function)) => Ok(function),
            _ => fail(
                value.span(),
                "the `message` event needs a function, such as `function heard(message: { on: number }) { ... }`",
            ),
        }
    }

    /// What a message handler's parameter stands for, and the strings of its keys, text first.
    fn message_argument(
        &mut self,
        function: &FunctionReference<'a>,
    ) -> Compiled<(Reference<'a>, [u16; MESSAGE_KEYS])> {
        let mut keys = [NO_MESSAGE_KEY; MESSAGE_KEYS];
        let mut fields: Vec<(String, Reference<'a>)> = Vec::new();
        let Some(parameter) = function.parameters.items.first() else {
            return Ok((Reference::Object(Rc::new(fields)), keys));
        };
        let example = "such as `message: { on: number; label: string }`";
        let Some(annotation) = &parameter.type_annotation else {
            return fail(
                parameter.span,
                format!("give the message's parameter a type naming each field the app reads, {example}"),
            );
        };
        let Some(literal) = self.type_literal(&annotation.type_annotation, function.scope.module) else {
            return fail(
                annotation.span,
                format!("a message's type lists its fields, written out or as a `type`, {example}"),
            );
        };
        let mut number_count = 0;
        for member in &literal.members {
            let TSSignature::TSPropertySignature(property) = member else {
                return fail(member.span(), "a message's type lists only plain fields");
            };
            let Some(name) = static_key(&property.key).filter(|name| is_message_key(name)) else {
                return fail(
                    property.span,
                    "a message field's name is 1 to 32 letters, digits, `_` or `-`",
                );
            };
            let declared = match &property.type_annotation {
                Some(annotation) => self.declared_type(&annotation.type_annotation, function.scope.module)?,
                None => None,
            };
            let kind = declared
                .filter(|declared| !declared.is_array)
                .map(|declared| declared.value_type.kind);
            let (position, value_type) = match kind {
                Some(Kind::Number | Kind::Bool) => {
                    number_count += 1;
                    if number_count >= MESSAGE_KEYS {
                        return fail(
                            property.span,
                            format!(
                                "a message carries at most {} numbers, so `{name}` is one too many",
                                MESSAGE_KEYS - 1
                            ),
                        );
                    }
                    let value_type = match kind {
                        Some(Kind::Bool) => Type::BOOL,
                        _ => Type::NUMBER,
                    };
                    (number_count, value_type)
                }
                Some(Kind::Name | Kind::Text) => {
                    if keys[0] != NO_MESSAGE_KEY {
                        return fail(
                            property.span,
                            format!("a message carries at most 1 text, so `{name}` is one too many"),
                        );
                    }
                    (0, Type::TEXT)
                }
                _ => {
                    return fail(
                        property.span,
                        format!("the message field `{name}` must be a number, a boolean or a string"),
                    )
                }
            };
            keys[position] = self.string(&name);
            fields.push((
                name,
                Reference::EventField {
                    field: position as u8,
                    value_type,
                },
            ));
        }
        Ok((Reference::Object(Rc::new(fields)), keys))
    }

    /// The object type an annotation writes out, or names through a `type` alias.
    fn type_literal(&self, annotation: &'a TSType<'a>, module: usize) -> Option<&'a TSTypeLiteral<'a>> {
        match annotation {
            TSType::TSTypeLiteral(literal) => Some(literal),
            TSType::TSParenthesizedType(inner) => self.type_literal(&inner.type_annotation, module),
            TSType::TSTypeReference(reference) => {
                let TSTypeName::IdentifierReference(identifier) = &reference.type_name else {
                    return None;
                };
                let alias = *self
                    .modules
                    .get(module)?
                    .type_aliases
                    .get(identifier.name.as_str())?;
                self.type_literal(alias, module)
            }
            _ => None,
        }
    }

    // ------------------------------------------------------------------------
    // Arguments
    // ------------------------------------------------------------------------

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
}

/// Whether a message field's name is 1 to 32 letters, digits, `_` or `-`, as the phone's line allows.
fn is_message_key(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
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
            format!("a period under a minute only runs while the app is open; add `while_open: true` to the `{name}` event, or make it at least {minute} milliseconds"),
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "subscriptions_tests.rs"]
mod subscriptions_tests;
