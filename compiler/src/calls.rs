//! Calls: the app's own functions, which are inlined, and the `neotime` functions, which become
//! instructions such as `FormatDuration`, `TimePart` and `Effect`.

use std::rc::Rc;

use neotime_runtime::instruction::{EffectKind, Instruction, TimePart, TimeUnit};
use neotime_runtime::pattern::{is_valid_duration_pattern, is_valid_time_pattern};
use neotime_runtime::SystemValue;
use oxc_ast::ast::{Argument, CallExpression, Expression};
use oxc_span::{GetSpan, Span};
use oxc_syntax::operator::BinaryOperator;

use crate::compiler::Compiler;
use crate::constants::{strip_parentheses, Constant};
use crate::diagnostics::{fail, Compiled};
use crate::emitter::Emitter;
use crate::expressions::{KeyValue, Typed};
use crate::permissions::effect_permission;
use crate::scope::{LibraryModule, Reference, Scope};
use crate::statements::{ArgumentSource, Search};
use crate::types::{icon_number, ChoiceSet, Kind, Range, Type, ICON_NAMES};

// ============================================================================
// Effects
// ============================================================================

/// The effect a `neotime` function asks for, and the types of its arguments.
pub fn effect_signature(name: &str) -> Option<(EffectKind, &'static [Type])> {
    const NUMBER: Type = Type::NUMBER;
    const BOOL: Type = Type::BOOL;
    const TEXT: Type = Type::TEXT;
    const NAME: Type = Type::NAME;
    const PATTERN: Type = Type::choice(ChoiceSet::VibrationPattern);
    const MUSIC: Type = Type::choice(ChoiceSet::MusicCommand);
    const CALL: Type = Type::choice(ChoiceSet::CallCommand);
    Some(match name {
        "vibrate" => (EffectKind::Vibrate, &[NUMBER]),
        "vibrate_pattern" => (EffectKind::VibratePattern, &[PATTERN]),
        "stop_vibrating" => (EffectKind::StopVibrating, &[]),
        "set_brightness" => (EffectKind::SetBrightness, &[NUMBER]),
        "keep_screen_on" => (EffectKind::KeepScreenOn, &[BOOL]),
        "wake_screen" => (EffectKind::WakeScreen, &[]),
        "screen_off" => (EffectKind::ScreenOff, &[]),
        "go" => (EffectKind::Go, &[NAME]),
        "back" => (EffectKind::Back, &[]),
        "open_app" => (EffectKind::OpenApp, &[TEXT]),
        "close" => (EffectKind::Close, &[]),
        "show_toast" => (EffectKind::ShowToast, &[TEXT]),
        "notify" => (EffectKind::Notify, &[TEXT, TEXT]),
        "dismiss_notification" => (EffectKind::DismissNotification, &[NUMBER]),
        "mark_notifications_read" => (EffectKind::MarkNotificationsRead, &[]),
        "music_control" => (EffectKind::MusicControl, &[MUSIC]),
        "call_control" => (EffectKind::CallControl, &[CALL]),
        "find_phone" => (EffectKind::FindPhone, &[BOOL]),
        "measure_heart_rate" => (EffectKind::MeasureHeartRate, &[]),
        "start_heart_rate" => (EffectKind::StartHeartRate, &[]),
        "stop_heart_rate" => (EffectKind::StopHeartRate, &[]),
        "reset_steps" => (EffectKind::ResetSteps, &[]),
        "set_setting" => (EffectKind::SetSetting, &[NAME, NUMBER]),
        "set_do_not_disturb" => (EffectKind::SetDoNotDisturb, &[BOOL]),
        "log" => (EffectKind::Log, &[TEXT]),
        "request_music_state" => (EffectKind::RequestMusicState, &[]),
        "set_heart_rate_led" => (EffectKind::SetHeartRateLed, &[BOOL, NUMBER, NUMBER]),
        "set_bluetooth" => (EffectKind::SetBluetooth, &[BOOL]),
        "request_navigation_state" => (EffectKind::RequestNavigationState, &[]),
        _ => return None,
    })
}

/// The part of an instant a `neotime` function reads.
fn time_part(name: &str) -> Option<TimePart> {
    Some(match name {
        "year_of" => TimePart::Year,
        "month_of" => TimePart::Month,
        "day_of" => TimePart::Day,
        "hour_of" => TimePart::Hour,
        "minute_of" => TimePart::Minute,
        "second_of" => TimePart::Second,
        "weekday_of" => TimePart::Weekday,
        _ => return None,
    })
}

/// What to write instead of a function the watch does not have.
fn rejected_call_advice(name: &str) -> Option<&'static str> {
    Some(match name {
        "useState" | "useReducer" | "useRef" => "is not available; use `signal` at the top of the file, or `useSignal`",
        "useMemo" | "useCallback" => "is not available; use `computed`",
        "useEffect" | "useLayoutEffect" | "effect" | "setTimeout" | "setInterval" => {
            "is not available; use `subscriptions` in define_app, or a binding that reads `now`"
        }
        "fetch" => "is not available; data from the phone arrives through a subscription",
        "signal" | "persisted" | "computed" => "cannot be called here; declare it at the top of the file",
        "every" | "at" | "daily" | "clock_change" | "heart_rate_reading" | "steps" | "step_goal" | "motion"
        | "acceleration" | "battery" | "phone" | "notification" | "call" | "music" | "weather" | "navigation"
        | "message" | "http_response" | "settings_change" => "is a subscription; list it in define_app's `subscriptions`",
        "vibrate_sequence" | "http_request" | "send_to_phone" => {
            "is not in the bytecode yet, because it needs an array or object argument"
        }
        "sine" | "cosine" | "point_on_circle" => {
            "needs arguments known when compiling: the shared library has no code yet to work it out on the watch"
        }
        _ => return None,
    })
}

// ============================================================================
// Calls
// ============================================================================

impl<'a> Compiler<'a> {
    /// Compiles a call. It returns the value's type, or `None` for a call that gives no value.
    pub fn compile_call(
        &mut self,
        emitter: &mut Emitter,
        call: &'a CallExpression<'a>,
        scope: &Rc<Scope<'a>>,
        expected: Option<Type>,
        as_statement: bool,
    ) -> Compiled<Option<Typed>> {
        let span = call.span;
        let mut arguments: Vec<&'a Expression<'a>> = Vec::new();
        for argument in &call.arguments {
            match argument {
                Argument::SpreadElement(spread) => {
                    return fail(spread.span, "`...` in a call is not supported")
                }
                other => arguments.extend(other.as_expression()),
            }
        }

        let callee = self.resolve_reference(&call.callee, scope);
        match callee {
            Some(Reference::Constant(Constant::Undefined)) if call.optional || as_statement => Ok(None),
            Some(Reference::Function(function)) => {
                let sources = arguments
                    .iter()
                    .map(|argument| ArgumentSource::Expression(argument, scope.clone()))
                    .collect();
                self.inline_call(emitter, &function, sources, !as_statement, expected, span)
            }
            Some(Reference::Library { module, name }) => {
                self.compile_library_call(emitter, module, &name, &arguments, scope, as_statement, span)
            }
            Some(Reference::Computed(_)) => fail(span, "read a computed with `.value` instead of calling it"),
            Some(_) => fail(span, "this cannot be called"),
            None => {
                // Text and array methods are the library's functions with the object first.
                if let Expression::StaticMemberExpression(member) = strip_parentheses(&call.callee) {
                    let method = member.property.name.as_str();
                    if matches!(method, "includes" | "indexOf") {
                        let Some(value) = arguments.first() else {
                            return fail(span, format!("`{method}` needs a value to look for"));
                        };
                        let search = if method == "includes" {
                            Search::Includes
                        } else {
                            Search::IndexOf
                        };
                        let typed =
                            self.compile_array_search(emitter, &member.object, value, search, scope, span)?;
                        return self.finish_value(emitter, typed, as_statement, span);
                    }
                    let function = match method {
                        "toUpperCase" => Some("to_upper"),
                        "slice" => Some("slice_text"),
                        "padStart" => Some("pad_start"),
                        _ => None,
                    };
                    if let Some(function) = function {
                        let mut with_object = vec![&member.object];
                        with_object.extend(arguments.iter().copied());
                        return self.compile_library_call(
                            emitter,
                            LibraryModule::Neotime,
                            function,
                            &with_object,
                            scope,
                            as_statement,
                            span,
                        );
                    }
                }
                let callee_text = match strip_parentheses(&call.callee) {
                    Expression::Identifier(identifier) => identifier.name.as_str().to_string(),
                    Expression::StaticMemberExpression(member) => member.property.name.as_str().to_string(),
                    _ => String::new(),
                };
                if let Some(advice) = rejected_call_advice(&callee_text) {
                    return fail(span, format!("`{callee_text}` {advice}"));
                }
                match callee_text.as_str() {
                    "map" => fail(
                        span,
                        "`.map` makes an array, which can only be assigned to an array signal or shown as widgets",
                    ),
                    "includes" | "indexOf" | "push" | "pop" | "splice" | "filter" | "reduce" | "forEach" | "slice"
                    | "fill" | "concat" | "find" | "some" | "every" => fail(
                        span,
                        format!("`.{callee_text}` is not supported on the watch; loop with `for ... of` over a fixed-length array"),
                    ),
                    _ => fail(span, "this function is not defined here"),
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn compile_library_call(
        &mut self,
        emitter: &mut Emitter,
        module: LibraryModule,
        name: &str,
        arguments: &[&'a Expression<'a>],
        scope: &Rc<Scope<'a>>,
        as_statement: bool,
        span: Span,
    ) -> Compiled<Option<Typed>> {
        if let Some(advice) = rejected_call_advice(name) {
            return fail(span, format!("`{name}` {advice}"));
        }
        let argument = |position: usize| -> Compiled<&'a Expression<'a>> {
            arguments.get(position).copied().ok_or_else(|| {
                crate::diagnostics::Diagnostic::error(span, format!("`{name}` needs more arguments"))
            })
        };

        if let Some((effect, argument_types)) = effect_signature(name) {
            if module != LibraryModule::Neotime {
                return fail(span, format!("`{name}` is not a function here"));
            }
            if !emitter.is_handler() {
                return fail(
                    span,
                    format!("`{name}` is an effect, and effects may only be asked for in a handler"),
                );
            }
            if !as_statement {
                return fail(span, format!("`{name}` gives no value"));
            }
            for (position, argument_type) in argument_types.iter().enumerate() {
                match arguments.get(position) {
                    Some(argument) => {
                        self.compile_value_of_type(emitter, argument, scope, *argument_type)?;
                    }
                    None if effect == EffectKind::FindPhone => self.emit_number(emitter, 1, span)?,
                    None => return fail(span, format!("`{name}` needs {} arguments", argument_types.len())),
                }
            }
            if arguments.len() > argument_types.len() {
                return fail(span, format!("`{name}` takes {} arguments", argument_types.len()));
            }
            emitter.emit(Instruction::Effect(effect), span)?;
            self.note_permission(effect_permission(effect), format!("`{name}`"), span);
            return Ok(None);
        }

        if let Some(part) = time_part(name) {
            self.compile_value_of_type(emitter, argument(0)?, scope, Type::NUMBER)?;
            emitter.emit(Instruction::TimePart(part), span)?;
            let range = match part {
                TimePart::Month => Range {
                    lowest: 1,
                    highest: 12,
                },
                TimePart::Day => Range {
                    lowest: 1,
                    highest: 31,
                },
                TimePart::Hour => Range {
                    lowest: 0,
                    highest: 23,
                },
                TimePart::Minute | TimePart::Second => Range {
                    lowest: 0,
                    highest: 59,
                },
                TimePart::Weekday => Range {
                    lowest: 0,
                    highest: 6,
                },
                TimePart::Year => Range::ANY,
            };
            return Ok(Some(Typed::with_range(Type::NUMBER, range)));
        }

        let value = match name {
            "Math.floor" | "Math.trunc" | "Math.round" | "Math.ceil" => {
                self.compile_rounding(emitter, name, argument(0)?, scope, span)?
            }
            "Math.min" | "Math.max" => {
                if arguments.is_empty() {
                    return fail(span, format!("`{name}` needs at least one argument"));
                }
                let instruction = if name == "Math.min" {
                    Instruction::Minimum
                } else {
                    Instruction::Maximum
                };
                let mut range: Option<Range> = None;
                for (position, argument) in arguments.iter().enumerate() {
                    let typed = self.compile_value_of_type(emitter, argument, scope, Type::NUMBER)?;
                    range = Some(range.map_or(typed.range, |range| range.union(typed.range)));
                    if position > 0 {
                        emitter.emit(instruction, span)?;
                    }
                }
                Typed::with_range(Type::NUMBER, range.unwrap_or(Range::ANY))
            }
            "Math.abs" => {
                self.compile_value_of_type(emitter, argument(0)?, scope, Type::NUMBER)?;
                emitter.emit(Instruction::Absolute, span)?;
                Typed::new(Type::NUMBER)
            }
            "clamp" => {
                self.compile_value_of_type(emitter, argument(0)?, scope, Type::NUMBER)?;
                let lowest = self.compile_value_of_type(emitter, argument(1)?, scope, Type::NUMBER)?;
                emitter.emit(Instruction::Maximum, span)?;
                let highest = self.compile_value_of_type(emitter, argument(2)?, scope, Type::NUMBER)?;
                emitter.emit(Instruction::Minimum, span)?;
                Typed::with_range(Type::NUMBER, lowest.range.union(highest.range))
            }
            "random_int" => {
                if !emitter.is_handler() {
                    return fail(
                        span,
                        "`random_int` may only be used in handlers, so a screen always shows the same thing for the same state",
                    );
                }
                let lowest = self.compile_value_of_type(emitter, argument(0)?, scope, Type::NUMBER)?;
                let highest = self.compile_value_of_type(emitter, argument(1)?, scope, Type::NUMBER)?;
                emitter.emit(Instruction::RandomInt, span)?;
                Typed::with_range(Type::NUMBER, lowest.range.union(highest.range))
            }
            "format_number" => {
                self.compile_value_of_type(emitter, argument(0)?, scope, Type::NUMBER)?;
                let digits = match arguments.get(1) {
                    Some(digits) => match self.evaluate_constant(digits, scope) {
                        Some(Constant::Number(digits)) if (1..=20).contains(&digits) => digits as u8,
                        _ => {
                            return fail(
                                digits.span(),
                                "the number of digits must be a literal from 1 to 20",
                            )
                        }
                    },
                    None => 1,
                };
                emitter.emit(Instruction::FormatNumber(digits), span)?;
                Typed::new(Type::TEXT)
            }
            "format_duration" | "format_time" => {
                self.compile_value_of_type(emitter, argument(0)?, scope, Type::NUMBER)?;
                self.compile_format_pattern(emitter, name == "format_time", argument(1)?, scope)?;
                Typed::new(Type::TEXT)
            }
            "format_temperature" => self.compile_temperature(emitter, argument(0)?, scope, span)?,
            "start_of_day" | "start_of_hour" | "start_of_minute" => {
                self.compile_start_of(emitter, name, argument(0)?, scope, span)?
            }
            "icon_named" => self.compile_icon_named(emitter, argument(0)?, scope, span)?,
            "day_of_year" => self.compile_day_of_year(emitter, argument(0)?, scope, span)?,
            "iso_week_of" => self.compile_iso_week(emitter, argument(0)?, scope, span)?,
            "count_where" => self.compile_array_search(
                emitter,
                argument(0)?,
                argument(1)?,
                Search::CountWhere,
                scope,
                span,
            )?,
            "to_upper" => {
                self.compile_text_argument(emitter, argument(0)?, scope)?;
                emitter.emit(Instruction::ToUpper, span)?;
                Typed::new(Type::TEXT)
            }
            "text_length" => {
                self.compile_text_argument(emitter, argument(0)?, scope)?;
                emitter.emit(Instruction::TextLength, span)?;
                Typed::with_range(
                    Type::NUMBER,
                    Range {
                        lowest: 0,
                        highest: 256,
                    },
                )
            }
            "slice_text" => {
                self.compile_text_argument(emitter, argument(0)?, scope)?;
                self.compile_value_of_type(emitter, argument(1)?, scope, Type::NUMBER)?;
                match arguments.get(2) {
                    Some(end) => {
                        self.compile_value_of_type(emitter, end, scope, Type::NUMBER)?;
                    }
                    None => self.emit_number(emitter, i32::MAX as i64, span)?,
                }
                emitter.emit(Instruction::SliceText, span)?;
                Typed::new(Type::TEXT)
            }
            "pad_start" => {
                self.compile_text_argument(emitter, argument(0)?, scope)?;
                self.compile_value_of_type(emitter, argument(1)?, scope, Type::NUMBER)?;
                let code = match arguments.get(2) {
                    Some(character) => match self.evaluate_constant(character, scope) {
                        Some(Constant::Text(text)) if text.chars().count() == 1 => {
                            text.chars().next().unwrap_or(' ')
                        }
                        _ => {
                            return fail(
                                character.span(),
                                "the padding must be one literal character, such as \"0\"",
                            )
                        }
                    },
                    None => ' ',
                };
                self.emit_number(emitter, code as i64, span)?;
                emitter.emit(Instruction::PadStart, span)?;
                Typed::new(Type::TEXT)
            }
            "next_time_of_day" => self.compile_next_time_of_day(
                emitter,
                argument(0)?,
                argument(1)?,
                arguments.get(2).copied(),
                scope,
                span,
            )?,
            "text_from_codes" => {
                self.compile_text_from_codes(emitter, argument(0)?, argument(1)?, scope, span)?
            }
            "join" => self.compile_join(emitter, argument(0)?, argument(1)?, scope, span)?,
            "define_app" => return fail(span, "call `define_app` once, in `export default`"),
            "useSignal" => {
                return fail(
                    span,
                    "`useSignal` may only be called at the top of a component's body",
                )
            }
            _ => return fail(span, format!("`{name}` is not supported on the watch")),
        };
        if as_statement {
            emitter.emit(Instruction::Drop, span)?;
            return Ok(None);
        }
        Ok(Some(value))
    }

    /// `icon_named(name)` with a name known only at run time: a table from names to icon numbers.
    fn compile_icon_named(
        &mut self,
        emitter: &mut Emitter,
        name: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<Typed> {
        let none = Constant::Icon(icon_number("none").unwrap_or(0));

        // A template such as `turn_${turn}` maps each choice to the icon with that name around it.
        if let Expression::TemplateLiteral(template) = strip_parentheses(name) {
            if let ([before, after], [inner]) = (template.quasis.as_slice(), template.expressions.as_slice())
            {
                let prefix = before.value.cooked.as_ref().map_or("", |text| text.as_str());
                let suffix = after.value.cooked.as_ref().map_or("", |text| text.as_str());
                let typed = self.compile_value(emitter, inner, scope, None)?;
                let Kind::Choice(set) = typed.value_type.kind else {
                    return fail(
                        name.span(),
                        "`icon_named` with a template needs a choice inside it, such as a navigation turn",
                    );
                };
                let mut cases: Vec<(KeyValue, Constant)> = set
                    .names()
                    .iter()
                    .enumerate()
                    .filter_map(|(position, choice)| {
                        icon_number(&format!("{prefix}{choice}{suffix}"))
                            .map(|icon| (KeyValue::Number(position as i64), Constant::Icon(icon)))
                    })
                    .collect();
                cases.push((KeyValue::Number(-1), none));
                return self.compile_match_on_stack(emitter, &cases, Some(Type::ICON), span);
            }
        }
        let typed = self.compile_value(emitter, name, scope, None)?;
        let mut cases: Vec<(KeyValue, Constant)> = match typed.value_type.kind {
            Kind::Choice(set) => set
                .names()
                .iter()
                .enumerate()
                .filter_map(|(position, name)| {
                    icon_number(name).map(|icon| (KeyValue::Number(position as i64), Constant::Icon(icon)))
                })
                .collect(),
            Kind::Name => {
                // Only names written in the source can reach here, so only those need a case.
                let source = self.modules.get(scope.module).map_or("", |module| module.source);
                ICON_NAMES
                    .iter()
                    .filter(|name| source.contains(&format!("\"{name}\"")))
                    .filter_map(|name| {
                        icon_number(name).map(|icon| (KeyValue::Text(name.to_string()), Constant::Icon(icon)))
                    })
                    .collect()
            }
            _ => return fail(name.span(), "`icon_named` needs the icon's name"),
        };
        cases.push((KeyValue::Number(-1), none));
        self.compile_match_on_stack(emitter, &cases, Some(Type::ICON), span)
    }

    /// `Math.floor`, `Math.trunc`, `Math.round` and `Math.ceil`, which only change a division.
    fn compile_rounding(
        &mut self,
        emitter: &mut Emitter,
        name: &str,
        argument: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<Typed> {
        let Expression::BinaryExpression(division) = strip_parentheses(argument) else {
            return self.compile_value_of_type(emitter, argument, scope, Type::NUMBER);
        };
        if division.operator != BinaryOperator::Division {
            return self.compile_value_of_type(emitter, argument, scope, Type::NUMBER);
        }
        let mark = emitter.locals_in_use();
        let divisor_constant = self
            .evaluate_constant(&division.right, scope)
            .and_then(|value| value.as_number());

        let dividend_local = emitter.take_local(span)?;
        let dividend = self.compile_value_of_type(emitter, &division.left, scope, Type::NUMBER)?;
        emitter.emit(Instruction::WriteLocal(dividend_local), span)?;

        // Each `push divisor` either pushes the literal or reads it back from a local.
        let divisor_local = match divisor_constant {
            Some(_) => None,
            None => {
                let local = emitter.take_local(span)?;
                self.compile_value_of_type(emitter, &division.right, scope, Type::NUMBER)?;
                emitter.emit(Instruction::WriteLocal(local), span)?;
                Some(local)
            }
        };
        let push_divisor =
            |compiler: &mut Compiler<'a>, emitter: &mut Emitter| match (divisor_constant, divisor_local) {
                (Some(divisor), _) => compiler.emit_number(emitter, divisor, span),
                (None, Some(local)) => emitter.emit(Instruction::ReadLocal(local), span),
                (None, None) => Ok(()),
            };

        if name == "Math.round" {
            // Rounding half up is the floor of (2a + b) / 2b.
            emitter.emit(Instruction::ReadLocal(dividend_local), span)?;
            self.emit_number(emitter, 2, span)?;
            emitter.emit(Instruction::Multiply, span)?;
            push_divisor(self, emitter)?;
            emitter.emit(Instruction::Add, span)?;
            emitter.emit(Instruction::WriteLocal(dividend_local), span)?;
            self.emit_floor_division(
                emitter,
                dividend_local,
                divisor_constant.map(|divisor| divisor * 2),
                divisor_local,
                true,
                span,
            )?;
        } else if name == "Math.ceil" {
            emitter.emit(Instruction::ReadLocal(dividend_local), span)?;
            emitter.emit(Instruction::Negate, span)?;
            emitter.emit(Instruction::WriteLocal(dividend_local), span)?;
            self.emit_floor_division(
                emitter,
                dividend_local,
                divisor_constant,
                divisor_local,
                false,
                span,
            )?;
            emitter.emit(Instruction::Negate, span)?;
        } else if name == "Math.trunc"
            || (dividend.range.lowest >= 0 && divisor_constant.is_some_and(|divisor| divisor > 0))
        {
            emitter.emit(Instruction::ReadLocal(dividend_local), span)?;
            push_divisor(self, emitter)?;
            emitter.emit(Instruction::Divide, span)?;
        } else {
            self.emit_floor_division(
                emitter,
                dividend_local,
                divisor_constant,
                divisor_local,
                false,
                span,
            )?;
        }
        emitter.release_locals(mark);

        let range = match divisor_constant {
            Some(divisor) if divisor > 0 && dividend.range != Range::ANY => Range {
                lowest: dividend.range.lowest.div_euclid(divisor),
                highest: dividend.range.highest.div_euclid(divisor) + 1,
            },
            _ => Range::ANY,
        };
        Ok(Typed::with_range(Type::NUMBER, range))
    }

    /// Pushes the floor of a local divided by a divisor: (a - ((a % b + b) % b)) / b.
    fn emit_floor_division(
        &mut self,
        emitter: &mut Emitter,
        dividend_local: u8,
        divisor_constant: Option<i64>,
        divisor_local: Option<u8>,
        divisor_is_doubled: bool,
        span: Span,
    ) -> Compiled<()> {
        let push_divisor = |compiler: &mut Compiler<'a>, emitter: &mut Emitter| -> Compiled<()> {
            match (divisor_constant, divisor_local) {
                (Some(divisor), _) => compiler.emit_number(emitter, divisor, span),
                (None, Some(local)) => {
                    emitter.emit(Instruction::ReadLocal(local), span)?;
                    if divisor_is_doubled {
                        compiler.emit_number(emitter, 2, span)?;
                        emitter.emit(Instruction::Multiply, span)?;
                    }
                    Ok(())
                }
                (None, None) => Ok(()),
            }
        };
        emitter.emit(Instruction::ReadLocal(dividend_local), span)?;
        emitter.emit(Instruction::ReadLocal(dividend_local), span)?;
        push_divisor(self, emitter)?;
        emitter.emit(Instruction::Remainder, span)?;
        push_divisor(self, emitter)?;
        emitter.emit(Instruction::Add, span)?;
        push_divisor(self, emitter)?;
        emitter.emit(Instruction::Remainder, span)?;
        emitter.emit(Instruction::Subtract, span)?;
        push_divisor(self, emitter)?;
        emitter.emit(Instruction::Divide, span)
    }

    /// Formats the value on the stack with a literal pattern, or one of two chosen with `? :`.
    fn compile_format_pattern(
        &mut self,
        emitter: &mut Emitter,
        is_time: bool,
        pattern: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
    ) -> Compiled<()> {
        let span = pattern.span();
        if let Some(Reference::Expression { expression, scope }) = self.resolve_reference(pattern, scope) {
            return self.compile_format_pattern(emitter, is_time, expression, &scope);
        }
        if let Some(constant) = self.evaluate_constant(pattern, scope) {
            let Constant::Text(text) = constant else {
                return fail(span, "the pattern must be literal text, such as \"mm:ss\"");
            };
            let is_valid = if is_time {
                is_valid_time_pattern(&text)
            } else {
                is_valid_duration_pattern(&text)
            };
            if !is_valid {
                let tokens = if is_time {
                    "YYYY, MMMM, MMM, MM, M, DD, D, dddd, ddd, HH, H, hh, h, mm, m, ss, s, a, A, or \"clock\", \"clock_with_seconds\" and \"date\""
                } else {
                    "hh, h, mm, m, ss, s and cc"
                };
                return fail(
                    span,
                    format!(
                        "\"{text}\" is not a valid pattern; use {tokens}, with other letters in [brackets]"
                    ),
                );
            }
            let string = self.string(&text);
            let instruction = if is_time {
                Instruction::FormatTime(string)
            } else {
                Instruction::FormatDuration(string)
            };
            return emitter.emit(instruction, span);
        }
        let Expression::ConditionalExpression(conditional) = strip_parentheses(pattern) else {
            return fail(
                span,
                "the pattern must be known when the app is compiled, such as \"mm:ss\", or chosen with `? :` between two",
            );
        };

        // Keep the value in a local while the test runs, then format it in each branch.
        let mark = emitter.locals_in_use();
        let value_local = emitter.take_local(span)?;
        emitter.emit(Instruction::WriteLocal(value_local), span)?;
        let otherwise = emitter.label();
        let end = emitter.label();
        self.compile_condition(emitter, &conditional.test, scope)?;
        emitter.jump_if_false(otherwise, span)?;
        emitter.emit(Instruction::ReadLocal(value_local), span)?;
        self.compile_format_pattern(emitter, is_time, &conditional.consequent, scope)?;
        emitter.jump(end, span)?;
        emitter.place(otherwise);
        emitter.emit(Instruction::ReadLocal(value_local), span)?;
        self.compile_format_pattern(emitter, is_time, &conditional.alternate, scope)?;
        emitter.place(end);
        emitter.release_locals(mark);
        Ok(())
    }

    /// `format_temperature`: degrees Celsius, or Fahrenheit when the user does not use metric units.
    fn compile_temperature(
        &mut self,
        emitter: &mut Emitter,
        celsius: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<Typed> {
        let mark = emitter.locals_in_use();
        let local = emitter.take_local(span)?;
        self.compile_value_of_type(emitter, celsius, scope, Type::NUMBER)?;
        emitter.emit(Instruction::WriteLocal(local), span)?;
        let fahrenheit = emitter.label();
        let end = emitter.label();
        emitter.emit(Instruction::ReadSystem(SystemValue::UsesMetricUnits), span)?;
        emitter.jump_if_false(fahrenheit, span)?;
        emitter.emit(Instruction::ReadLocal(local), span)?;
        emitter.emit(Instruction::FormatNumber(1), span)?;
        let celsius_unit = self.string("°C");
        emitter.emit(Instruction::PushText(celsius_unit), span)?;
        emitter.emit(Instruction::Join, span)?;
        emitter.jump(end, span)?;
        emitter.place(fahrenheit);

        // Fahrenheit is round(celsius * 9 / 5) + 32, which is floor((celsius * 18 + 5) / 10) + 32.
        emitter.emit(Instruction::ReadLocal(local), span)?;
        self.emit_number(emitter, 18, span)?;
        emitter.emit(Instruction::Multiply, span)?;
        self.emit_number(emitter, 5, span)?;
        emitter.emit(Instruction::Add, span)?;
        emitter.emit(Instruction::WriteLocal(local), span)?;
        self.emit_floor_division(emitter, local, Some(10), None, false, span)?;
        self.emit_number(emitter, 32, span)?;
        emitter.emit(Instruction::Add, span)?;
        emitter.emit(Instruction::FormatNumber(1), span)?;
        let fahrenheit_unit = self.string("°F");
        emitter.emit(Instruction::PushText(fahrenheit_unit), span)?;
        emitter.emit(Instruction::Join, span)?;
        emitter.place(end);
        emitter.release_locals(mark);
        Ok(Typed::new(Type::TEXT))
    }

    /// `start_of_day` and friends: the instant cut back to the start of its local minute, hour or day.
    fn compile_start_of(
        &mut self,
        emitter: &mut Emitter,
        name: &str,
        instant: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<Typed> {
        let unit = match name {
            "start_of_minute" => TimeUnit::Minute,
            "start_of_hour" => TimeUnit::Hour,
            _ => TimeUnit::Day,
        };
        self.compile_value_of_type(emitter, instant, scope, Type::NUMBER)?;
        emitter.emit(Instruction::StartOf(unit), span)?;
        Ok(Typed::new(Type::NUMBER))
    }

    /// `day_of_year(instant)`, 1 to 366, from the local month, day and year.
    fn compile_day_of_year(
        &mut self,
        emitter: &mut Emitter,
        instant: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<Typed> {
        let mark = emitter.locals_in_use();
        let local = emitter.take_local(span)?;
        self.compile_value_of_type(emitter, instant, scope, Type::NUMBER)?;
        emitter.emit(Instruction::WriteLocal(local), span)?;
        self.emit_day_of_year(emitter, local, span)?;
        emitter.release_locals(mark);
        Ok(Typed::with_range(
            Type::NUMBER,
            Range {
                lowest: 1,
                highest: 366,
            },
        ))
    }

    /// `iso_week_of(instant)`, 1 to 53: the week of the year that week's Thursday falls in.
    fn compile_iso_week(
        &mut self,
        emitter: &mut Emitter,
        instant: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<Typed> {
        const DAY: i64 = 86_400_000;
        let mark = emitter.locals_in_use();
        let local = emitter.take_local(span)?;
        self.compile_value_of_type(emitter, instant, scope, Type::NUMBER)?;
        emitter.emit(Instruction::WriteLocal(local), span)?;
        // The Thursday of the week, at noon so a daylight saving change cannot move it a day.
        emitter.emit(Instruction::ReadLocal(local), span)?;
        emitter.emit(Instruction::StartOf(TimeUnit::Day), span)?;
        self.emit_number(emitter, 3, span)?;
        emitter.emit(Instruction::ReadLocal(local), span)?;
        emitter.emit(Instruction::TimePart(TimePart::Weekday), span)?;
        emitter.emit(Instruction::Subtract, span)?;
        self.emit_number(emitter, DAY, span)?;
        emitter.emit(Instruction::Multiply, span)?;
        emitter.emit(Instruction::Add, span)?;
        self.emit_number(emitter, DAY / 2, span)?;
        emitter.emit(Instruction::Add, span)?;
        emitter.emit(Instruction::WriteLocal(local), span)?;
        // Weeks counted from 1 January of the Thursday's year.
        self.emit_day_of_year(emitter, local, span)?;
        self.emit_number(emitter, 1, span)?;
        emitter.emit(Instruction::Subtract, span)?;
        self.emit_number(emitter, 7, span)?;
        emitter.emit(Instruction::Divide, span)?;
        self.emit_number(emitter, 1, span)?;
        emitter.emit(Instruction::Add, span)?;
        emitter.release_locals(mark);
        Ok(Typed::with_range(
            Type::NUMBER,
            Range {
                lowest: 1,
                highest: 53,
            },
        ))
    }

    /// Pushes the local day of the year of the instant in `local`, with the usual month formula:
    /// `275 * month / 9 - (2 - leap) * ((month + 9) / 12) + day - 30`.
    fn emit_day_of_year(&mut self, emitter: &mut Emitter, local: u8, span: Span) -> Compiled<()> {
        let part = |emitter: &mut Emitter, part: TimePart| -> Compiled<()> {
            emitter.emit(Instruction::ReadLocal(local), span)?;
            emitter.emit(Instruction::TimePart(part), span)
        };
        // 275 * month / 9
        part(emitter, TimePart::Month)?;
        self.emit_number(emitter, 275, span)?;
        emitter.emit(Instruction::Multiply, span)?;
        self.emit_number(emitter, 9, span)?;
        emitter.emit(Instruction::Divide, span)?;
        // 2 - leap, where leap is (year % 4 == 0) - (year % 100 == 0) + (year % 400 == 0)
        self.emit_number(emitter, 2, span)?;
        for (divisor, sign) in [
            (4, Instruction::Subtract),
            (100, Instruction::Add),
            (400, Instruction::Subtract),
        ] {
            part(emitter, TimePart::Year)?;
            self.emit_number(emitter, divisor, span)?;
            emitter.emit(Instruction::Remainder, span)?;
            self.emit_number(emitter, 0, span)?;
            emitter.emit(Instruction::Equal, span)?;
            emitter.emit(sign, span)?;
        }
        // times (month + 9) / 12, which is 1 from March on
        part(emitter, TimePart::Month)?;
        self.emit_number(emitter, 9, span)?;
        emitter.emit(Instruction::Add, span)?;
        self.emit_number(emitter, 12, span)?;
        emitter.emit(Instruction::Divide, span)?;
        emitter.emit(Instruction::Multiply, span)?;
        emitter.emit(Instruction::Subtract, span)?;
        // plus the day of the month, less 30
        part(emitter, TimePart::Day)?;
        emitter.emit(Instruction::Add, span)?;
        self.emit_number(emitter, 30, span)?;
        emitter.emit(Instruction::Subtract, span)
    }

    /// Compiles a value that must be text, for the text functions.
    pub fn compile_text_argument(
        &mut self,
        emitter: &mut Emitter,
        text: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
    ) -> Compiled<()> {
        let typed = self.compile_value(emitter, text, scope, None)?;
        if !typed.value_type.is_text() {
            return fail(
                text.span(),
                format!("expected text, but this is {}", typed.value_type.describe()),
            );
        }
        Ok(())
    }

    /// Gives a call's value, or drops it when the call stands as a statement.
    fn finish_value(
        &mut self,
        emitter: &mut Emitter,
        typed: Typed,
        as_statement: bool,
        span: Span,
    ) -> Compiled<Option<Typed>> {
        if as_statement {
            emitter.emit(Instruction::Drop, span)?;
            return Ok(None);
        }
        Ok(Some(typed))
    }

    /// `next_time_of_day(after, { hour, minute }, days?)`: a loop over the next eight local days.
    fn compile_next_time_of_day(
        &mut self,
        emitter: &mut Emitter,
        after: &'a Expression<'a>,
        time_of_day: &'a Expression<'a>,
        days: Option<&'a Expression<'a>>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<Typed> {
        const MINUTE: i64 = 60_000;
        const DAY: i64 = 86_400_000;
        const TWO_HOURS: i64 = 7_200_000;
        const DAYS_TRIED: i64 = 8;

        let spill_mark = emitter.locals_in_use();
        let spilled = self.spill_whole_stack(emitter, span)?;
        let mark = emitter.locals_in_use();
        let after_local = emitter.take_local(span)?;
        let offset_local = emitter.take_local(span)?;
        let days_local = emitter.take_local(span)?;
        let day_local = emitter.take_local(span)?;
        let count_local = emitter.take_local(span)?;
        let result_local = emitter.take_local(span)?;
        let candidate_local = emitter.take_local(span)?;

        self.compile_value_of_type(emitter, after, scope, Type::NUMBER)?;
        emitter.emit(Instruction::WriteLocal(after_local), span)?;

        // The time of day as milliseconds after midnight.
        let fields = self.record_sources(time_of_day, scope, span)?;
        let field = |name: &str| {
            fields
                .iter()
                .rev()
                .find(|(field, _)| field == name)
                .map(|(_, source)| source.clone())
        };
        let (Some(hour), Some(minute)) = (field("hour"), field("minute")) else {
            return fail(
                time_of_day.span(),
                "the time of day needs an `hour` and a `minute`",
            );
        };
        for (part, scale) in [(hour, 60 * MINUTE), (minute, MINUTE)] {
            match part {
                Reference::Expression { expression, scope } => {
                    self.compile_value_of_type(emitter, expression, &scope, Type::NUMBER)?;
                }
                other => {
                    self.compile_reference_value(emitter, other, span, Some(Type::NUMBER))?;
                }
            }
            self.emit_number(emitter, scale, span)?;
            emitter.emit(Instruction::Multiply, span)?;
        }
        emitter.emit(Instruction::Add, span)?;
        emitter.emit(Instruction::WriteLocal(offset_local), span)?;

        match days {
            Some(days) => {
                self.compile_value_of_type(emitter, days, scope, Type::NUMBER)?;
            }
            None => self.emit_number(emitter, 0b111_1111, span)?,
        }
        emitter.emit(Instruction::WriteLocal(days_local), span)?;
        emitter.emit(Instruction::ReadLocal(after_local), span)?;
        emitter.emit(Instruction::StartOf(TimeUnit::Day), span)?;
        emitter.emit(Instruction::WriteLocal(day_local), span)?;
        self.emit_number(emitter, 0, span)?;
        emitter.emit(Instruction::WriteLocal(count_local), span)?;
        emitter.emit(Instruction::PushNone, span)?;
        emitter.emit(Instruction::WriteLocal(result_local), span)?;

        let start = emitter.loop_start(span)?;
        let exit = emitter.label();
        emitter.emit(Instruction::ReadLocal(count_local), span)?;
        self.emit_number(emitter, DAYS_TRIED, span)?;
        emitter.emit(Instruction::Less, span)?;
        emitter.jump_if_false(exit, span)?;

        emitter.emit(Instruction::ReadLocal(day_local), span)?;
        emitter.emit(Instruction::ReadLocal(offset_local), span)?;
        emitter.emit(Instruction::Add, span)?;
        emitter.emit(Instruction::WriteLocal(candidate_local), span)?;
        emitter.emit(Instruction::ReadLocal(result_local), span)?;
        emitter.emit(Instruction::IsNone, span)?;
        emitter.emit(Instruction::ReadLocal(candidate_local), span)?;
        emitter.emit(Instruction::ReadLocal(after_local), span)?;
        emitter.emit(Instruction::Greater, span)?;
        emitter.emit(Instruction::BitAnd, span)?;
        emitter.emit(Instruction::ReadLocal(days_local), span)?;
        emitter.emit(Instruction::ReadLocal(candidate_local), span)?;
        emitter.emit(Instruction::TimePart(TimePart::Weekday), span)?;
        emitter.emit(Instruction::ShiftRight, span)?;
        self.emit_number(emitter, 1, span)?;
        emitter.emit(Instruction::BitAnd, span)?;
        emitter.emit(Instruction::BitAnd, span)?;
        let not_this_day = emitter.label();
        emitter.jump_if_false(not_this_day, span)?;
        emitter.emit(Instruction::ReadLocal(candidate_local), span)?;
        emitter.emit(Instruction::WriteLocal(result_local), span)?;
        emitter.place(not_this_day);

        // The next day starts at the local midnight a little more than a day later.
        emitter.emit(Instruction::ReadLocal(day_local), span)?;
        self.emit_number(emitter, DAY + TWO_HOURS, span)?;
        emitter.emit(Instruction::Add, span)?;
        emitter.emit(Instruction::StartOf(TimeUnit::Day), span)?;
        emitter.emit(Instruction::WriteLocal(day_local), span)?;
        emitter.emit(Instruction::ReadLocal(count_local), span)?;
        self.emit_number(emitter, 1, span)?;
        emitter.emit(Instruction::Add, span)?;
        emitter.emit(Instruction::WriteLocal(count_local), span)?;
        emitter.loop_back(start, span)?;
        emitter.place(exit);

        // With no day in the mask, the time on the day after the last one tried, as the SDK gives.
        let found = emitter.label();
        emitter.emit(Instruction::ReadLocal(result_local), span)?;
        emitter.emit(Instruction::IsNone, span)?;
        emitter.jump_if_false(found, span)?;
        emitter.emit(Instruction::ReadLocal(day_local), span)?;
        emitter.emit(Instruction::ReadLocal(offset_local), span)?;
        emitter.emit(Instruction::Add, span)?;
        emitter.emit(Instruction::WriteLocal(result_local), span)?;
        emitter.place(found);
        emitter.release_locals(mark);
        self.restore_stack(emitter, &spilled, false, span)?;
        emitter.emit(Instruction::ReadLocal(result_local), span)?;
        emitter.release_locals(spill_mark);
        Ok(Typed::new(Type::NUMBER))
    }

    /// `join(texts, separator)` over a fixed-length array, unrolled into `Join`s.
    fn compile_join(
        &mut self,
        emitter: &mut Emitter,
        texts: &'a Expression<'a>,
        separator: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<Typed> {
        let Some(Constant::Text(separator)) = self.evaluate_constant(separator, scope) else {
            return fail(separator.span(), "the separator must be literal text");
        };
        let separator = self.string(&separator);
        let pieces: Vec<Reference<'a>> =
            if let Some(Constant::Array(elements)) = self.evaluate_constant(texts, scope) {
                elements.iter().cloned().map(Reference::Constant).collect()
            } else if let Some(Reference::StateArray(variable)) = self.resolve_reference(texts, scope) {
                let length = self.slots[variable as usize].length as i64;
                (0..length)
                    .map(|index| Reference::StateElement { variable, index })
                    .collect()
            } else if let Expression::ArrayExpression(array) = strip_parentheses(texts) {
                array
                    .elements
                    .iter()
                    .filter_map(|element| element.as_expression())
                    .map(|expression| Reference::Expression {
                        expression,
                        scope: scope.clone(),
                    })
                    .collect()
            } else {
                return fail(
                    texts.span(),
                    "`join` needs a fixed-length array: a constant, an array literal or an array signal",
                );
            };
        if pieces.is_empty() {
            let empty = self.string("");
            emitter.emit(Instruction::PushText(empty), span)?;
            return Ok(Typed::new(Type::NAME));
        }
        for (position, piece) in pieces.into_iter().enumerate() {
            if position > 0 {
                emitter.emit(Instruction::PushText(separator), span)?;
                emitter.emit(Instruction::Join, span)?;
            }
            let typed = self.compile_reference_value(emitter, piece, span, None)?;
            self.convert_to_text(emitter, typed, span)?;
            if position > 0 {
                emitter.emit(Instruction::Join, span)?;
            }
        }
        Ok(Typed::new(Type::TEXT))
    }

    /// `text_from_codes(codes, length)`: text made from the first `length` codes of an array signal.
    fn compile_text_from_codes(
        &mut self,
        emitter: &mut Emitter,
        codes: &'a Expression<'a>,
        length: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<Typed> {
        let Some(Reference::StateArray(variable)) = self.resolve_reference(codes, scope) else {
            return fail(
                codes.span(),
                "`text_from_codes` needs an array signal of character codes, such as `codes.value`",
            );
        };
        if self.slots[variable as usize].value_type.kind != Kind::Number {
            return fail(codes.span(), "the character codes must be numbers");
        }
        self.compile_value_of_type(emitter, length, scope, Type::NUMBER)?;
        emitter.emit(Instruction::TextFromCodes(variable), span)?;
        Ok(Typed::new(Type::TEXT))
    }
}
