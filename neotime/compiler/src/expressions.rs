//! Compiles expressions to bytecode that leaves one value, and resolves names and members to what
//! they stand for.

use std::rc::Rc;

use neotime_runtime::events::EventKind;
use neotime_runtime::instruction::Instruction;
use neotime_runtime::{SystemValue, NONE};
use oxc_ast::ast::{ChainElement, Expression, TemplateLiteral};
use oxc_span::{GetSpan, Span};
use oxc_syntax::operator::{BinaryOperator, LogicalOperator, UnaryOperator};

use crate::compiler::{callable_reference, describe_constant, type_range, Compiler};
use crate::constants::{library_constant, strip_parentheses, Constant};
use crate::diagnostics::{fail, Compiled};
use crate::emitter::Emitter;
use crate::scope::{ElementIndex, LibraryModule, Reference, Scope, SystemSignal};
use crate::types::{icon_number, ChoiceSet, Kind, Range, Type};

// ============================================================================
// Typed values
// ============================================================================

/// What a compiled value left on the stack holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Typed {
    pub value_type: Type,
    pub range: Range,
}

impl Typed {
    pub fn new(value_type: Type) -> Typed {
        Typed {
            value_type,
            range: type_range(value_type),
        }
    }

    pub fn with_range(value_type: Type, range: Range) -> Typed {
        Typed { value_type, range }
    }
}

/// What a value is compared with to choose an entry of a table.
#[derive(Clone, Debug, PartialEq)]
pub enum KeyValue {
    Number(i64),
    /// A string of the image, compared by its number.
    Text(String),
}

/// The largest constant table that can be indexed by a number worked out at run time.
const LARGEST_LOOKUP_TABLE: usize = 64;

// ============================================================================
// System values
// ============================================================================

/// The system value a `neotime` name stands for.
pub fn system_signal(name: &str) -> Option<SystemSignal> {
    let instant = Type::NUMBER;
    let (value, value_type) = match name {
        "now" => (SystemValue::Now, instant),
        "utc_offset" => (SystemValue::UtcOffset, Type::NUMBER),
        "battery_percent" => (SystemValue::BatteryPercent, Type::NUMBER),
        "is_charging" => (SystemValue::IsCharging, Type::BOOL),
        "battery_millivolts" => (SystemValue::BatteryMillivolts, Type::NUMBER),
        "steps_today" => (SystemValue::StepsToday, Type::NUMBER),
        "heart_rate" => (SystemValue::HeartRate, Type::NUMBER),
        "heart_rate_measured_at" => (SystemValue::HeartRateMeasuredAt, instant),
        "is_phone_connected" => (SystemValue::IsPhoneConnected, Type::BOOL),
        "unread_notifications" => (SystemValue::UnreadNotifications, Type::NUMBER),
        "is_do_not_disturb" => (SystemValue::IsDoNotDisturb, Type::BOOL),
        "brightness" => (SystemValue::Brightness, Type::NUMBER),
        "is_heart_rate_running" => (SystemValue::IsHeartRateRunning, Type::BOOL),
        "is_bluetooth_enabled" => (SystemValue::IsBluetoothEnabled, Type::BOOL),
        "next_alarm" => (SystemValue::NextAlarm, instant.or_null()),
        "heart_rate_status" => (SystemValue::HeartRateStatus, Type::NUMBER),
        "settings.uses_24_hour_clock" => (SystemValue::Uses24HourClock, Type::BOOL),
        "settings.uses_metric_units" => (SystemValue::UsesMetricUnits, Type::BOOL),
        "settings.step_goal" => (SystemValue::StepGoal, Type::NUMBER),
        "settings.screen_timeout_seconds" => (SystemValue::ScreenTimeoutSeconds, Type::NUMBER),
        "settings.always_on_screen" => (SystemValue::AlwaysOnScreen, Type::BOOL),
        "settings.chimes" => (SystemValue::Chimes, Type::NUMBER),
        "settings.theme" => (SystemValue::ThemePreset, Type::NUMBER),
        "firmware_version" => (SystemValue::FirmwareVersion, Type::TEXT),
        "started_at" => (SystemValue::StartedAt, instant),
        "free_storage_kilobytes" => (SystemValue::FreeStorageKilobytes, Type::NUMBER),
        "reset_reason" => (SystemValue::ResetReason, Type::TEXT),
        "settings.allows_updates" => (SystemValue::AllowsUpdates, Type::BOOL),
        "settings.wake_on_touch" => (SystemValue::WakeOnTouch, Type::NUMBER),
        "settings.alarms_in_do_not_disturb" => (SystemValue::AlarmsInDoNotDisturb, Type::BOOL),
        "settings.heart_rate_interval" => (SystemValue::HeartRateInterval, Type::NUMBER),
        "current_weather.temperature" => (SystemValue::WeatherTemperature, Type::NUMBER.or_null()),
        "current_weather.condition" => (
            SystemValue::WeatherCondition,
            Type::choice(ChoiceSet::WeatherCondition).or_null(),
        ),
        "current_weather.low" => (SystemValue::WeatherLow, Type::NUMBER.or_null()),
        "current_weather.high" => (SystemValue::WeatherHigh, Type::NUMBER.or_null()),
        "current_weather.received_at" => (SystemValue::WeatherReceivedAt, instant.or_null()),
        "current_weather.sunrise" => (SystemValue::WeatherSunrise, instant.or_null()),
        "current_weather.sunset" => (SystemValue::WeatherSunset, instant.or_null()),
        "current_weather.location" => (SystemValue::WeatherLocation, Type::TEXT),
        "current_call.caller" => (SystemValue::CallCaller, Type::TEXT),
        "current_call.state" => (SystemValue::CallState, Type::choice(ChoiceSet::CallKind)),
        "current_music.artist" => (SystemValue::MusicArtist, Type::TEXT),
        "current_music.track" => (SystemValue::MusicTrack, Type::TEXT),
        "current_music.is_playing" => (SystemValue::MusicIsPlaying, Type::BOOL),
        "current_music.position" => (SystemValue::MusicPosition, Type::NUMBER),
        "current_music.length" => (SystemValue::MusicLength, Type::NUMBER),
        "current_navigation.turn" => (
            SystemValue::NavigationTurn,
            Type::choice(ChoiceSet::NavigationTurn),
        ),
        "current_navigation.instruction" => (SystemValue::NavigationInstruction, Type::TEXT),
        "current_navigation.distance" => (SystemValue::NavigationDistance, Type::NUMBER),
        "current_navigation.progress_percent" => (SystemValue::NavigationProgressPercent, Type::NUMBER),
        "step_history.trip" => (SystemValue::StepTrip, Type::NUMBER),
        "app_memory_budget" => (SystemValue::AppMemoryBudget, Type::NUMBER),
        "launcher_style" => (SystemValue::LauncherStyle, Type::choice(ChoiceSet::LauncherStyle)),
        "launcher_view" => (SystemValue::LauncherView, Type::choice(ChoiceSet::LauncherView)),
        _ => return None,
    };
    Some(SystemSignal { value, value_type })
}

/// The system value holding a list's length.
fn list_length(list: &str) -> Option<SystemValue> {
    match list {
        "current_weather.forecast" => Some(SystemValue::WeatherForecastLength),
        "installed_apps" => Some(SystemValue::InstalledAppCount),
        "launcher_groups" => Some(SystemValue::LauncherGroupCount),
        "notifications" => Some(SystemValue::NotificationCount),
        "packages" => Some(SystemValue::PackageCount),
        "step_history.days" => Some(SystemValue::StepDayCount),
        "step_history.hours" => Some(SystemValue::StepHourCount),
        _ => None,
    }
}

/// One field of an element of a system list: a day of the forecast, an installed app, a kept notification, or a day or hour of steps.
fn list_field(list: &str, name: &str) -> Option<SystemSignal> {
    let (value, value_type) = match (list, name) {
        ("current_weather.forecast", "condition") => (
            SystemValue::WeatherForecastCondition,
            Type::choice(ChoiceSet::WeatherCondition).or_null(),
        ),
        ("current_weather.forecast", "low") => (SystemValue::WeatherForecastLow, Type::NUMBER.or_null()),
        ("current_weather.forecast", "high") => (SystemValue::WeatherForecastHigh, Type::NUMBER.or_null()),
        ("installed_apps", "name") => (SystemValue::InstalledAppName, Type::TEXT),
        ("installed_apps", "id") => (SystemValue::InstalledAppId, Type::TEXT),
        ("installed_apps", "icon") => (SystemValue::InstalledAppIcon, Type::ICON),
        ("launcher_groups", "name") => (SystemValue::LauncherGroupName, Type::TEXT),
        ("launcher_groups", "icon") => (SystemValue::LauncherGroupIcon, Type::ICON),
        ("launcher_groups", "first") => (SystemValue::LauncherGroupFirst, Type::NUMBER),
        ("launcher_groups", "count") => (SystemValue::LauncherGroupSize, Type::NUMBER),
        ("notifications", "id") => (SystemValue::NotificationId, Type::NUMBER),
        ("notifications", "app") => (SystemValue::NotificationApp, Type::TEXT),
        ("notifications", "title") => (SystemValue::NotificationTitle, Type::TEXT),
        ("notifications", "body") => (SystemValue::NotificationBody, Type::TEXT),
        ("notifications", "time") => (SystemValue::NotificationTime, Type::NUMBER),
        ("packages", "name") => (SystemValue::PackageName, Type::TEXT),
        ("packages", "id") => (SystemValue::PackageId, Type::TEXT),
        ("packages", "enabled") => (SystemValue::PackageEnabled, Type::BOOL),
        ("packages", "is_face") => (SystemValue::PackageIsFace, Type::BOOL),
        ("packages", "memory") => (SystemValue::PackageMemory, Type::NUMBER),
        ("packages", "refusal") => (SystemValue::PackageRefusal, Type::NUMBER),
        ("step_history.days", "total") => (SystemValue::StepDayTotal, Type::NUMBER),
        ("step_history.hours", "steps") => (SystemValue::StepHourSteps, Type::NUMBER),
        _ => return None,
    };
    Some(SystemSignal { value, value_type })
}

/// The type of one field of an event.
pub fn event_field_type(kind: EventKind, field: &str) -> Type {
    if kind
        .field_index(field)
        .is_some_and(|position| kind.is_text_field(position))
    {
        return Type::TEXT;
    }
    match (kind, field) {
        (EventKind::Swipe, "direction") => Type::choice(ChoiceSet::SwipeDirection),
        (EventKind::Motion, "kind") => Type::choice(ChoiceSet::MotionKind),
        (EventKind::Call, "kind") => Type::choice(ChoiceSet::CallKind),
        (EventKind::Phone, "is_connected") => Type::BOOL,
        _ => Type::NUMBER,
    }
}

// ============================================================================
// Resolving names
// ============================================================================

impl<'a> Compiler<'a> {
    /// What a name or member chain stands for when that needs no code, or `None` for a value to read.
    pub fn resolve_reference(
        &self,
        expression: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
    ) -> Option<Reference<'a>> {
        let reference = self.resolve_once(expression, scope)?;
        Some(self.follow_alias(reference))
    }

    /// Follows an alias to what its expression stands for, if that is more than a value.
    fn follow_alias(&self, reference: Reference<'a>) -> Reference<'a> {
        let mut reference = reference;
        for _ in 0..64 {
            match &reference {
                Reference::Expression { expression, scope } => match self.resolve_once(expression, scope) {
                    Some(inner) => reference = inner,
                    None => return reference,
                },
                _ => return reference,
            }
        }
        reference
    }

    fn resolve_once(&self, expression: &'a Expression<'a>, scope: &Rc<Scope<'a>>) -> Option<Reference<'a>> {
        match expression {
            Expression::ParenthesizedExpression(inner) => self.resolve_once(&inner.expression, scope),
            Expression::TSAsExpression(inner) => self.resolve_once(&inner.expression, scope),
            Expression::TSSatisfiesExpression(inner) => self.resolve_once(&inner.expression, scope),
            Expression::TSNonNullExpression(inner) => self.resolve_once(&inner.expression, scope),
            Expression::Identifier(identifier) => {
                let name = identifier.name.as_str();
                match self.lookup(scope, name) {
                    Some(Reference::Library { module, name }) => Some(library_reference(module, &name)),
                    Some(reference) => Some(reference),
                    None if name == "undefined" => Some(Reference::Constant(Constant::Undefined)),
                    None if name == "Math" => Some(Reference::Library {
                        module: LibraryModule::Neotime,
                        name: "Math".to_string(),
                    }),
                    None => None,
                }
            }
            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
                callable_reference("", expression, scope)
                    .ok()
                    .map(Reference::Function)
            }
            Expression::StaticMemberExpression(member) => {
                let object = self.resolve_reference(&member.object, scope)?;
                self.member_of(object, member.property.name.as_str())
            }
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::StaticMemberExpression(member) => {
                    let object = self.resolve_reference(&member.object, scope)?;
                    if matches!(object, Reference::Constant(Constant::Undefined)) {
                        return Some(object);
                    }
                    self.member_of(object, member.property.name.as_str())
                }
                _ => None,
            },
            Expression::ComputedMemberExpression(member) => {
                let object = self.resolve_reference(&member.object, scope)?;
                if let Reference::SystemList(list) = object {
                    let index = self.element_index(&member.expression, scope);
                    return Some(Reference::SystemListElement { list, index });
                }
                if let Reference::RecordArrayValue(fields) = &object {
                    return Some(Reference::RecordElement {
                        fields: fields.clone(),
                        index: self.element_index(&member.expression, scope),
                    });
                }
                let index = self.evaluate_constant(&member.expression, scope)?;
                match (object, index) {
                    (Reference::StateArray(variable), Constant::Number(index)) => {
                        Some(Reference::StateElement { variable, index })
                    }
                    (object @ Reference::Object(_), Constant::Text(name)) => self.member_of(object, &name),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// An index written in `array[index]`: a number known now, a local, or code to run.
    fn element_index(&self, expression: &'a Expression<'a>, scope: &Rc<Scope<'a>>) -> ElementIndex<'a> {
        match self.evaluate_constant(expression, scope) {
            Some(Constant::Number(index)) => ElementIndex::Constant(index),
            _ => match self.resolve_reference(expression, scope) {
                Some(Reference::Local { index, .. }) => ElementIndex::Local(index),
                Some(Reference::Row { .. }) => ElementIndex::Row,
                _ => ElementIndex::Expression {
                    expression,
                    scope: scope.clone(),
                },
            },
        }
    }

    /// What `object.name` stands for, when it needs no code.
    fn member_of(&self, object: Reference<'a>, name: &str) -> Option<Reference<'a>> {
        match object {
            Reference::Object(fields) => Some(
                fields
                    .iter()
                    .find(|(field, _)| field == name)
                    .map_or(Reference::Constant(Constant::Undefined), |(_, field)| {
                        self.follow_alias(field.clone())
                    }),
            ),
            Reference::Library {
                module: LibraryModule::Icons,
                name: namespace,
            } if namespace == "icons" => {
                icon_number(name).map(|number| Reference::Constant(Constant::Icon(number)))
            }
            Reference::Library {
                module,
                name: namespace,
            } if namespace == "Math" => Some(Reference::Library {
                module,
                name: format!("Math.{name}"),
            }),
            Reference::SystemGroup(
                list @ ("installed_apps" | "launcher_groups" | "notifications" | "packages"),
            ) if name == "value" => Some(Reference::SystemList(list)),
            Reference::SystemGroup(group) => {
                if name == "value" {
                    return Some(Reference::SystemGroupValue(group));
                }
                system_signal(&format!("{group}.{name}")).map(Reference::System)
            }
            Reference::SystemGroupValue("current_weather") if name == "forecast" => {
                Some(Reference::SystemList("current_weather.forecast"))
            }
            Reference::SystemGroupValue("step_history") if name == "days" => {
                Some(Reference::SystemList("step_history.days"))
            }
            Reference::SystemGroupValue("step_history") if name == "hours" => {
                Some(Reference::SystemList("step_history.hours"))
            }
            Reference::SystemList(list) if name == "length" => Some(Reference::SystemRead(SystemSignal {
                value: list_length(list)?,
                value_type: Type::NUMBER,
            })),
            Reference::SystemListElement { list, index } => {
                let system = list_field(list, name)?;
                Some(Reference::SystemElement { system, index })
            }
            Reference::Record(fields) if name == "value" => Some(Reference::RecordValue(fields)),
            Reference::RecordArray(fields) if name == "value" => Some(Reference::RecordArrayValue(fields)),
            Reference::RecordArrayValue(fields) if name == "length" => {
                let (_, first) = fields.first()?;
                Some(Reference::Constant(Constant::Number(
                    self.slots.get(*first as usize)?.length as i64,
                )))
            }
            Reference::RecordElement { fields, index } => {
                let Some((_, variable)) = fields.iter().find(|(field, _)| field == name) else {
                    return Some(Reference::Constant(Constant::Undefined));
                };
                Some(match index {
                    ElementIndex::Constant(index) => Reference::StateElement {
                        variable: *variable,
                        index,
                    },
                    index => Reference::StateElementAt {
                        variable: *variable,
                        index,
                    },
                })
            }
            Reference::RecordValue(fields) => Some(
                fields
                    .iter()
                    .find(|(field, _)| field == name)
                    .map_or(Reference::Constant(Constant::Undefined), |(_, slot)| {
                        Reference::StateScalar(*slot)
                    }),
            ),
            Reference::Signal(variable) if name == "value" => {
                let slot = self.slots.get(variable as usize)?;
                slot.is_array().then_some(Reference::StateArray(variable))
            }
            Reference::StateArray(variable) if name == "length" => {
                let slot = self.slots.get(variable as usize)?;
                Some(Reference::Constant(Constant::Number(slot.length as i64)))
            }
            Reference::Event(kind) => {
                let field = kind.field_index(name)?;
                Some(Reference::EventField {
                    field,
                    value_type: event_field_type(kind, name),
                })
            }
            Reference::Constant(constant) => constant.field(name).map(Reference::Constant),
            Reference::Expression { expression, scope } => {
                let constant = self.evaluate_constant(expression, &scope)?;
                constant.field(name).map(Reference::Constant)
            }
            _ => None,
        }
    }

    // ------------------------------------------------------------------------
    // Values
    // ------------------------------------------------------------------------

    /// Compiles an expression that leaves one value, with `expected` turning names into numbers.
    pub fn compile_value(
        &mut self,
        emitter: &mut Emitter,
        expression: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        expected: Option<Type>,
    ) -> Compiled<Typed> {
        let span = expression.span();
        if let Some(constant) = self.evaluate_constant(expression, scope) {
            if !matches!(constant, Constant::Array(_) | Constant::Object(_)) {
                return self.emit_constant(emitter, &constant, expected, span);
            }
        }
        let expression = strip_parentheses(expression);
        match expression {
            Expression::Identifier(_) => match self.resolve_reference(expression, scope) {
                Some(reference) => self.compile_reference_value(emitter, reference, span, expected),
                None => fail(span, format!("`{}` is not defined", source_text(self, scope, span))),
            },
            Expression::StaticMemberExpression(member) => {
                if let Some(reference) = self.resolve_reference(expression, scope) {
                    return self.compile_reference_value(emitter, reference, span, expected);
                }
                let name = member.property.name.as_str();
                let object = self.resolve_reference(&member.object, scope);
                let is_text_length = name == "length"
                    && matches!(object, None | Some(Reference::Expression { .. } | Reference::Local { .. }));
                if is_text_length {
                    self.compile_text_argument(emitter, &member.object, scope)?;
                    emitter.emit(Instruction::TextLength, span)?;
                    return Ok(Typed::with_range(Type::NUMBER, Range { lowest: 0, highest: 256 }));
                }
                self.compile_member_read(emitter, object, name, span, expected)
            }
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::StaticMemberExpression(member) => {
                    if let Some(reference) = self.resolve_reference(expression, scope) {
                        return self.compile_reference_value(emitter, reference, span, expected);
                    }
                    let object = self.resolve_reference(&member.object, scope);
                    self.compile_member_read(emitter, object, member.property.name.as_str(), span, expected)
                }
                ChainElement::CallExpression(call) => {
                    match self.compile_call(emitter, call, scope, expected, false)? {
                        Some(typed) => Ok(typed),
                        None => fail(span, "this call gives no value"),
                    }
                }
                _ => fail(span, "this kind of `?.` is not supported"),
            },
            Expression::ComputedMemberExpression(member) => {
                if let Some(reference) = self.resolve_reference(expression, scope) {
                    return self.compile_reference_value(emitter, reference, span, expected);
                }
                match self.resolve_reference(&member.object, scope) {
                    Some(Reference::StateArray(variable)) => {
                        self.compile_value_of_type(emitter, &member.expression, scope, Type::NUMBER)?;
                        emitter.emit(Instruction::ReadElement(variable), span)?;
                        let slot = &self.slots[variable as usize];
                        Ok(Typed::with_range(slot.value_type, self.read_range(variable)))
                    }
                    _ => match self.evaluate_constant(&member.object, scope) {
                        Some(Constant::Array(elements)) => {
                            self.compile_table_lookup(emitter, &elements, &member.expression, scope, expected, span)
                        }
                        Some(Constant::Object(fields)) => {
                            let key = self.compile_value(emitter, &member.expression, scope, None)?;
                            let cases = object_cases(&fields, key.value_type).ok_or_else(|| {
                                crate::diagnostics::Diagnostic::error(
                                    span,
                                    "an object can only be indexed by a choice such as a swipe direction, or by fixed text",
                                )
                            })?;
                            self.compile_match_on_stack(emitter, &cases, expected, span)
                        }
                        _ => fail(
                            span,
                            "only arrays in state and constant arrays can be indexed; keep the array in a signal",
                        ),
                    },
                }
            }
            Expression::NullLiteral(_) => {
                emitter.emit(Instruction::PushNone, span)?;
                Ok(Typed::with_range(
                    expected.map_or(Type::NULL, Type::or_null),
                    Range::exactly(NONE),
                ))
            }
            Expression::BinaryExpression(binary) => {
                self.compile_binary(emitter, binary.operator, &binary.left, &binary.right, scope, span)
            }
            Expression::LogicalExpression(logical) => {
                self.compile_logical(emitter, logical.operator, &logical.left, &logical.right, scope, expected, span)
            }
            Expression::UnaryExpression(unary) => match unary.operator {
                UnaryOperator::LogicalNot => {
                    self.compile_condition(emitter, &unary.argument, scope)?;
                    emitter.emit(Instruction::Not, span)?;
                    Ok(Typed::new(Type::BOOL))
                }
                UnaryOperator::UnaryNegation => {
                    self.compile_value_of_type(emitter, &unary.argument, scope, Type::NUMBER)?;
                    emitter.emit(Instruction::Negate, span)?;
                    Ok(Typed::new(Type::NUMBER))
                }
                UnaryOperator::UnaryPlus => self.compile_value_of_type(emitter, &unary.argument, scope, Type::NUMBER),
                UnaryOperator::BitwiseNot => {
                    self.compile_value_of_type(emitter, &unary.argument, scope, Type::NUMBER)?;
                    emitter.emit(Instruction::BitNot, span)?;
                    Ok(Typed::new(Type::NUMBER))
                }
                _ => fail(span, "`typeof`, `void` and `delete` are not available on the watch"),
            },
            Expression::ConditionalExpression(conditional) => {
                if let Some(test) = self.evaluate_constant(&conditional.test, scope) {
                    let branch = if test.is_truthy() {
                        &conditional.consequent
                    } else {
                        &conditional.alternate
                    };
                    return self.compile_value(emitter, branch, scope, expected);
                }
                let otherwise = emitter.label();
                let end = emitter.label();
                self.compile_condition(emitter, &conditional.test, scope)?;
                emitter.jump_if_false(otherwise, span)?;
                let first = self.compile_value(emitter, &conditional.consequent, scope, expected)?;
                emitter.jump(end, span)?;
                emitter.place(otherwise);
                let hint = expected.or(Some(first.value_type).filter(|value_type| value_type.kind != Kind::Null));
                let second = self.compile_value(emitter, &conditional.alternate, scope, hint)?;
                emitter.place(end);
                let Some(value_type) = first.value_type.unify(second.value_type) else {
                    return fail(
                        span,
                        format!(
                            "the two branches give different types: {} and {}",
                            first.value_type.describe(),
                            second.value_type.describe()
                        ),
                    );
                };
                Ok(Typed::with_range(value_type, first.range.union(second.range)))
            }
            Expression::TemplateLiteral(template) => self.compile_template(emitter, template, scope, span),
            Expression::CallExpression(call) => match self.compile_call(emitter, call, scope, expected, false)? {
                Some(typed) => Ok(typed),
                None => fail(span, "this call gives no value, so it cannot be used as one"),
            },
            Expression::AssignmentExpression(_) | Expression::UpdateExpression(_) => fail(
                span,
                "assignments may only stand on their own as statements inside a handler",
            ),
            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
                fail(span, "a function is not a value here; call it, or pass it to an event such as onClick")
            }
            Expression::JSXElement(_) | Expression::JSXFragment(_) => {
                fail(span, "widgets can only appear in a component's JSX")
            }
            Expression::AwaitExpression(_) => fail(
                span,
                "`await` is not available; data from the phone arrives through an event",
            ),
            Expression::NewExpression(_) | Expression::ClassExpression(_) | Expression::ThisExpression(_) => {
                fail(span, "classes and `new` are not available; keep state in signals and write plain functions")
            }
            Expression::ArrayExpression(_) | Expression::ObjectExpression(_) => fail(
                span,
                "an array or object is not a single value; keep it in a signal or a constant and read one element",
            ),
            Expression::NumericLiteral(_) => fail(
                span,
                "numbers on the watch are whole; keep a fixed number of places, such as thousandths, in a whole number",
            ),
            _ => fail(span, "this expression is not supported on the watch"),
        }
    }

    /// Compiles a value and checks it has the type given.
    pub fn compile_value_of_type(
        &mut self,
        emitter: &mut Emitter,
        expression: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        wanted: Type,
    ) -> Compiled<Typed> {
        let typed = self.compile_value(emitter, expression, scope, Some(wanted))?;
        if !typed.value_type.fits(wanted) {
            return fail(
                expression.span(),
                format!(
                    "expected {}, but this is {}",
                    wanted.describe(),
                    typed.value_type.describe()
                ),
            );
        }
        Ok(typed)
    }

    /// Compiles what a resolved reference stands for as one value.
    pub fn compile_reference_value(
        &mut self,
        emitter: &mut Emitter,
        reference: Reference<'a>,
        span: Span,
        expected: Option<Type>,
    ) -> Compiled<Typed> {
        match reference {
            Reference::Constant(Constant::Undefined) => {
                fail(span, "this has no value here; give the prop or variable a value")
            }
            Reference::Constant(constant) => self.emit_constant(emitter, &constant, expected, span),
            Reference::Expression { expression, scope } => self.compile_value(emitter, expression, &scope, expected),
            Reference::Local {
                index,
                value_type,
                range,
                ..
            } => {
                emitter.emit(Instruction::ReadLocal(index), span)?;
                Ok(Typed::with_range(value_type, range))
            }
            Reference::StateElementAt { variable, index } => {
                self.emit_element_index(emitter, &index, span)?;
                emitter.emit(Instruction::ReadElement(variable), span)?;
                let slot = &self.slots[variable as usize];
                Ok(Typed::with_range(slot.value_type, self.read_range(variable)))
            }
            Reference::RecordValue(_) | Reference::RecordElement { .. } => {
                fail(span, "a record is not one value; read one of its fields")
            }
            Reference::StateScalar(variable) => {
                emitter.emit(Instruction::ReadState(variable), span)?;
                Ok(Typed::with_range(self.slots[variable as usize].value_type, self.read_range(variable)))
            }
            Reference::Row { count } => {
                emitter.emit(Instruction::ReadRow, span)?;
                let highest = (count as i64 - 1).max(0);
                Ok(Typed::with_range(Type::NUMBER, Range { lowest: 0, highest }))
            }
            Reference::ProgressionElement { index_local, start, step } => {
                emitter.emit(Instruction::ReadLocal(index_local), span)?;
                if step != 1 {
                    self.emit_number(emitter, step, span)?;
                    emitter.emit(Instruction::Multiply, span)?;
                }
                if start != 0 {
                    self.emit_number(emitter, start, span)?;
                    emitter.emit(Instruction::Add, span)?;
                }
                Ok(Typed::new(Type::NUMBER))
            }
            Reference::StateElement { variable, index } => {
                self.emit_number(emitter, index, span)?;
                emitter.emit(Instruction::ReadElement(variable), span)?;
                let slot = &self.slots[variable as usize];
                Ok(Typed::with_range(slot.value_type, self.read_range(variable)))
            }
            Reference::EventField { field, value_type } => {
                if !emitter.is_handler() {
                    return fail(span, "events can only be read inside a handler");
                }
                // Text fields, such as a notification's title, have an instruction of their own.
                let instruction = if value_type.kind == Kind::Text {
                    Instruction::ReadEventText(field)
                } else {
                    Instruction::ReadEvent(field)
                };
                emitter.emit(instruction, span)?;
                Ok(Typed::new(value_type))
            }
            Reference::Signal(_) | Reference::Computed(_) | Reference::System(_) => {
                fail(span, "read a signal's value with `.value`")
            }
            Reference::SystemRead(system) => {
                emitter.emit(Instruction::ReadSystem(system.value), span)?;
                self.note_system_read(system.value, span);
                Ok(Typed::new(system.value_type))
            }
            Reference::SystemElement { system, index } => {
                self.emit_element_index(emitter, &index, span)?;
                emitter.emit(Instruction::ReadSystemElement(system.value), span)?;
                self.note_system_read(system.value, span);
                Ok(Typed::new(system.value_type))
            }
            Reference::SystemList(_) | Reference::SystemListElement { .. } => {
                fail(span, "a list is not one value; read one field of one element, such as `forecast[0].high`")
            }
            Reference::StateArray(_) => fail(
                span,
                "this is a whole array where one value is needed; read one element, such as `cells.value[index]`",
            ),
            Reference::Function(_) => {
                fail(span, "a function is not a value here; call it, or pass it to an event such as onClick")
            }
            Reference::Library { name, .. } => fail(span, format!("`{name}` is not a value; call it or read from it")),
            _ => fail(span, "this is not a single value"),
        }
    }

    /// Pushes the index of an element read or write.
    pub fn emit_element_index(
        &mut self,
        emitter: &mut Emitter,
        index: &ElementIndex<'a>,
        span: Span,
    ) -> Compiled<()> {
        match index {
            ElementIndex::Constant(index) => self.emit_number(emitter, *index, span),
            ElementIndex::Local(local) => emitter.emit(Instruction::ReadLocal(*local), span),
            ElementIndex::Row => emitter.emit(Instruction::ReadRow, span),
            ElementIndex::Expression { expression, scope } => {
                self.compile_value_of_type(emitter, expression, scope, Type::NUMBER)?;
                Ok(())
            }
        }
    }

    /// Compiles `object.name` where the member must be read with code, such as `count.value`.
    fn compile_member_read(
        &mut self,
        emitter: &mut Emitter,
        object: Option<Reference<'a>>,
        name: &str,
        span: Span,
        expected: Option<Type>,
    ) -> Compiled<Typed> {
        match (object, name) {
            (Some(Reference::Signal(variable)), "value") => {
                emitter.emit(Instruction::ReadState(variable), span)?;
                Ok(Typed::with_range(
                    self.slots[variable as usize].value_type,
                    self.read_range(variable),
                ))
            }
            (Some(Reference::Computed(function)), "value") => {
                match self.inline_call(emitter, &function, Vec::new(), true, expected, span)? {
                    Some(typed) => Ok(typed),
                    None => fail(function.span, "this computed gives no value"),
                }
            }
            (Some(Reference::System(system)), "value") => {
                emitter.emit(Instruction::ReadSystem(system.value), span)?;
                self.note_system_read(system.value, span);
                Ok(Typed::new(system.value_type))
            }
            (Some(Reference::SystemGroup(group) | Reference::SystemGroupValue(group)), field) => {
                match system_signal(&format!("{group}.{field}")) {
                    Some(system) => {
                        emitter.emit(Instruction::ReadSystem(system.value), span)?;
                        self.note_system_read(system.value, span);
                        Ok(Typed::new(system.value_type))
                    }
                    None => fail(span, format!("`{group}` has no value `{field}`")),
                }
            }
            (
                Some(Reference::Library {
                    name: library_name, ..
                }),
                _,
            ) if library_name == "current_screen" => fail(
                span,
                "`current_screen` cannot be read on the watch yet; keep the screen's name in a signal",
            ),
            (Some(Reference::Event(_)), field) => fail(span, format!("this event has no field `{field}`")),
            (Some(Reference::Expression { .. }), field) => {
                fail(span, format!("`.{field}` cannot be read from this value"))
            }
            (Some(_), "length") => fail(span, "only the length of a fixed-length array is known"),
            (Some(_), field) => fail(span, format!("`.{field}` cannot be read here")),
            (None, field) => fail(
                span,
                format!("`.{field}` is read from something that is not defined here"),
            ),
        }
    }

    // ------------------------------------------------------------------------
    // Constants
    // ------------------------------------------------------------------------

    /// Pushes a constant, turning names into numbers when `expected` asks for a colour or a choice.
    pub fn emit_constant(
        &mut self,
        emitter: &mut Emitter,
        constant: &Constant,
        expected: Option<Type>,
        span: Span,
    ) -> Compiled<Typed> {
        let value_type = match (constant, expected) {
            (Constant::Null, Some(expected)) => expected.or_null(),
            (Constant::Null, None) => Type::NULL,
            (Constant::Text(_), Some(expected)) if matches!(expected.kind, Kind::Color | Kind::Choice(_)) => {
                Type::plain(expected.kind)
            }
            (Constant::Number(_), _) => Type::NUMBER,
            (Constant::Bool(_), _) => Type::BOOL,
            (Constant::Text(_), _) => Type::NAME,
            (Constant::Icon(_), _) => Type::ICON,
            (Constant::Undefined, _) => return fail(span, "this has no value here"),
            (other, _) => {
                return fail(
                    span,
                    format!(
                        "{} is not a single value; read one element of it",
                        describe_constant(other)
                    ),
                )
            }
        };
        let number = self.constant_number(constant, value_type, span)?;
        match constant {
            Constant::Text(_) if value_type.is_text() => {
                emitter.emit(Instruction::PushText(number as u16), span)?
            }
            Constant::Null => emitter.emit(Instruction::PushNone, span)?,
            _ => self.emit_number(emitter, number, span)?,
        }
        Ok(Typed::with_range(value_type, Range::exactly(number)))
    }

    /// Pushes a number with the shortest instruction that holds it.
    pub fn emit_number(&mut self, emitter: &mut Emitter, number: i64, span: Span) -> Compiled<()> {
        let instruction = if number == NONE {
            Instruction::PushNone
        } else if let Ok(small) = i8::try_from(number) {
            Instruction::PushSmall(small)
        } else if let Ok(medium) = i32::try_from(number) {
            Instruction::PushInt(medium)
        } else {
            Instruction::PushLong(number)
        };
        emitter.emit(instruction, span)
    }

    /// Reads an element of a constant array at an index worked out at run time.
    pub fn compile_table_lookup(
        &mut self,
        emitter: &mut Emitter,
        elements: &[Constant],
        index: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        expected: Option<Type>,
        span: Span,
    ) -> Compiled<Typed> {
        if elements.is_empty() {
            return fail(span, "this array is empty, so it has no elements to read");
        }
        if Constant::is_uniform_array(elements) {
            self.compile_value_of_type(emitter, index, scope, Type::NUMBER)?;
            emitter.emit(Instruction::Drop, span)?;
            return self.emit_constant(emitter, &elements[0], expected, span);
        }
        if let Some(progression) = Constant::progression(elements) {
            self.compile_value_of_type(emitter, index, scope, Type::NUMBER)?;
            if progression.step != 1 {
                self.emit_number(emitter, progression.step, span)?;
                emitter.emit(Instruction::Multiply, span)?;
            }
            if progression.start != 0 {
                self.emit_number(emitter, progression.start, span)?;
                emitter.emit(Instruction::Add, span)?;
            }
            let last = progression.start + progression.step * (elements.len() as i64 - 1);
            return Ok(Typed::with_range(
                Type::NUMBER,
                Range::exactly(progression.start).union(Range::exactly(last)),
            ));
        }
        let local_mark = emitter.locals_in_use();
        let index_local = emitter.take_local(span)?;
        self.compile_value_of_type(emitter, index, scope, Type::NUMBER)?;
        emitter.emit(Instruction::WriteLocal(index_local), span)?;
        let typed = self.compile_lookup_chain(emitter, elements, index_local, expected, span)?;
        emitter.release_locals(local_mark);
        Ok(typed)
    }

    /// Picks the element of a constant array whose position is in a local, by comparing it with each.
    pub fn compile_lookup_chain(
        &mut self,
        emitter: &mut Emitter,
        elements: &[Constant],
        index_local: u8,
        expected: Option<Type>,
        span: Span,
    ) -> Compiled<Typed> {
        let cases: Vec<(KeyValue, Constant)> = elements
            .iter()
            .enumerate()
            .map(|(position, element)| (KeyValue::Number(position as i64), element.clone()))
            .collect();
        self.compile_match_chain(emitter, &cases, index_local, expected, span)
    }

    /// Leaves the result whose key equals the value in a local; the last case is the default.
    ///
    /// The result goes through a local, so only one jump is open at a time.
    pub fn compile_match_chain(
        &mut self,
        emitter: &mut Emitter,
        cases: &[(KeyValue, Constant)],
        value_local: u8,
        expected: Option<Type>,
        span: Span,
    ) -> Compiled<Typed> {
        if cases.len() > LARGEST_LOOKUP_TABLE {
            return fail(
                span,
                format!("a constant table indexed at run time may have at most {LARGEST_LOOKUP_TABLE} entries, unless its numbers go up evenly"),
            );
        }
        let Some(((_, last), rest)) = cases.split_last() else {
            return fail(span, "this table is empty, so it has no elements to read");
        };
        let mark = emitter.locals_in_use();
        let result_local = emitter.take_local(span)?;
        let mut result = self.emit_constant(emitter, last, expected, span)?;
        emitter.emit(Instruction::WriteLocal(result_local), span)?;
        for (key, element) in rest {
            let next = emitter.label();
            emitter.emit(Instruction::ReadLocal(value_local), span)?;
            match key {
                KeyValue::Number(number) => self.emit_number(emitter, *number, span)?,
                KeyValue::Text(text) => {
                    let string = self.string(text);
                    emitter.emit(Instruction::PushText(string), span)?;
                }
            }
            emitter.emit(Instruction::Equal, span)?;
            emitter.jump_if_false(next, span)?;
            let hint = expected.or(Some(result.value_type));
            let typed = self.emit_constant(emitter, element, hint, span)?;
            emitter.emit(Instruction::WriteLocal(result_local), span)?;
            emitter.place(next);
            result = match result.value_type.unify(typed.value_type) {
                Some(value_type) => Typed::with_range(value_type, result.range.union(typed.range)),
                None => return fail(span, "the elements of this table have different types"),
            };
        }
        emitter.emit(Instruction::ReadLocal(result_local), span)?;
        emitter.release_locals(mark);
        Ok(result)
    }

    /// Stores the value on the stack in a new local and looks it up among `cases`.
    pub fn compile_match_on_stack(
        &mut self,
        emitter: &mut Emitter,
        cases: &[(KeyValue, Constant)],
        expected: Option<Type>,
        span: Span,
    ) -> Compiled<Typed> {
        let mark = emitter.locals_in_use();
        let value_local = emitter.take_local(span)?;
        emitter.emit(Instruction::WriteLocal(value_local), span)?;
        let typed = self.compile_match_chain(emitter, cases, value_local, expected, span)?;
        emitter.release_locals(mark);
        Ok(typed)
    }

    // ------------------------------------------------------------------------
    // Operators
    // ------------------------------------------------------------------------

    fn compile_binary(
        &mut self,
        emitter: &mut Emitter,
        operator: BinaryOperator,
        left: &'a Expression<'a>,
        right: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<Typed> {
        use BinaryOperator as Operator;
        match operator {
            Operator::StrictEquality | Operator::Equality | Operator::StrictInequality | Operator::Inequality => {
                let is_equal = matches!(operator, Operator::StrictEquality | Operator::Equality);
                self.compile_equality(emitter, left, right, scope, span)?;
                if !is_equal {
                    emitter.emit(Instruction::Not, span)?;
                }
                Ok(Typed::new(Type::BOOL))
            }
            Operator::Division => fail(
                span,
                "a bare `/` would give a fraction; write `Math.trunc(a / b)` or `Math.floor(a / b)` to say how it rounds",
            ),
            Operator::Exponential => fail(span, "`**` is not available; multiply instead"),
            Operator::In | Operator::Instanceof => fail(span, "`in` and `instanceof` are not available on the watch"),
            Operator::ShiftRightZeroFill => fail(span, "`>>>` is not available; use `>>`"),
            Operator::Addition => {
                let first = self.compile_value(emitter, left, scope, None)?;
                if first.value_type.is_text() {
                    let second = self.compile_value(emitter, right, scope, None)?;
                    self.convert_to_text(emitter, second, right.span())?;
                    emitter.emit(Instruction::Join, span)?;
                    return Ok(Typed::new(Type::TEXT));
                }
                let second = self.compile_value(emitter, right, scope, None)?;
                if second.value_type.is_text() {
                    emitter.emit(Instruction::Swap, span)?;
                    self.convert_to_text(emitter, first, left.span())?;
                    emitter.emit(Instruction::Swap, span)?;
                    emitter.emit(Instruction::Join, span)?;
                    return Ok(Typed::new(Type::TEXT));
                }
                self.require_number(first, left.span())?;
                self.require_number(second, right.span())?;
                emitter.emit(Instruction::Add, span)?;
                Ok(Typed::with_range(Type::NUMBER, first.range.sum_with(second.range)))
            }
            _ => {
                let instruction = match operator {
                    Operator::Subtraction => Instruction::Subtract,
                    Operator::Multiplication => Instruction::Multiply,
                    Operator::Remainder => Instruction::Remainder,
                    Operator::BitwiseAnd => Instruction::BitAnd,
                    Operator::BitwiseOR => Instruction::BitOr,
                    Operator::BitwiseXOR => Instruction::BitXor,
                    Operator::ShiftLeft => Instruction::ShiftLeft,
                    Operator::ShiftRight => Instruction::ShiftRight,
                    Operator::LessThan => Instruction::Less,
                    Operator::LessEqualThan => Instruction::LessOrEqual,
                    Operator::GreaterThan => Instruction::Greater,
                    Operator::GreaterEqualThan => Instruction::GreaterOrEqual,
                    _ => return fail(span, "this operator is not supported on the watch"),
                };
                let first = self.compile_value(emitter, left, scope, None)?;
                self.require_number(first, left.span())?;
                let second = self.compile_value(emitter, right, scope, None)?;
                self.require_number(second, right.span())?;
                emitter.emit(instruction, span)?;
                let is_comparison = matches!(
                    instruction,
                    Instruction::Less | Instruction::LessOrEqual | Instruction::Greater | Instruction::GreaterOrEqual
                );
                if is_comparison {
                    return Ok(Typed::new(Type::BOOL));
                }
                let range = match (instruction, self.evaluate_constant(right, scope)) {
                    (Instruction::Remainder, Some(Constant::Number(divisor))) if divisor != 0 => {
                        let largest = divisor.unsigned_abs() as i64 - 1;
                        if first.range.lowest >= 0 {
                            Range { lowest: 0, highest: largest }
                        } else {
                            Range { lowest: -largest, highest: largest }
                        }
                    }
                    (Instruction::BitAnd, Some(Constant::Number(mask))) if mask >= 0 => Range { lowest: 0, highest: mask },
                    (Instruction::Subtract, _) => first.range.difference_with(second.range),
                    (Instruction::Multiply, _) => first.range.product_with(second.range),
                    _ => Range::ANY,
                };
                Ok(Typed::with_range(Type::NUMBER, range))
            }
        }
    }

    fn require_number(&self, typed: Typed, span: Span) -> Compiled<()> {
        if typed.value_type.kind != Kind::Number {
            return fail(
                span,
                format!("expected a number, but this is {}", typed.value_type.describe()),
            );
        }
        Ok(())
    }

    /// Leaves 1 if two values are equal and 0 if not; `x === null` becomes an emptiness test.
    fn compile_equality(
        &mut self,
        emitter: &mut Emitter,
        left: &'a Expression<'a>,
        right: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<()> {
        let is_null = |compiler: &Compiler<'a>, expression: &'a Expression<'a>| {
            matches!(
                compiler.evaluate_constant(expression, scope),
                Some(Constant::Null | Constant::Undefined)
            )
        };
        if is_null(self, right) || is_null(self, left) {
            let other = if is_null(self, right) { left } else { right };
            let typed = self.compile_value(emitter, other, scope, None)?;
            if !typed.value_type.nullable {
                emitter.emit(Instruction::Drop, span)?;
                emitter.emit(Instruction::PushSmall(0), span)?;
                return Ok(());
            }
            emitter.emit(Instruction::IsNone, span)?;
            return Ok(());
        }

        // Compile the side that is not a literal first, so the literal can take its type.
        let left_is_literal = matches!(self.evaluate_constant(left, scope), Some(Constant::Text(_)));
        let (first, second) = if left_is_literal {
            (right, left)
        } else {
            (left, right)
        };
        let first_type = self.compile_value(emitter, first, scope, None)?;
        let second_type = self.compile_value(emitter, second, scope, Some(first_type.value_type))?;
        let comparable = first_type.value_type.unify(second_type.value_type);
        match comparable {
            Some(value_type) if value_type.kind == Kind::Text => fail(
                span,
                "text made while running cannot be compared; compare the numbers it is made from, or keep fixed text in state",
            ),
            Some(_) => emitter.emit(Instruction::Equal, span),
            None => fail(
                span,
                format!(
                    "{} and {} cannot be compared",
                    first_type.value_type.describe(),
                    second_type.value_type.describe()
                ),
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn compile_logical(
        &mut self,
        emitter: &mut Emitter,
        operator: LogicalOperator,
        left: &'a Expression<'a>,
        right: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        expected: Option<Type>,
        span: Span,
    ) -> Compiled<Typed> {
        let end = emitter.label();
        let first = self.compile_value(emitter, left, scope, expected)?;
        emitter.emit(Instruction::Duplicate, span)?;
        match operator {
            LogicalOperator::Coalesce => {
                if !first.value_type.nullable {
                    emitter.emit(Instruction::Drop, span)?;
                    return Ok(first);
                }
                emitter.emit(Instruction::IsNone, span)?;
                emitter.jump_if_false(end, span)?;
            }
            LogicalOperator::And => {
                self.emit_truth_test(emitter, first.value_type, left.span())?;
                emitter.jump_if_false(end, span)?;
            }
            LogicalOperator::Or => {
                let keep = emitter.label();
                self.emit_truth_test(emitter, first.value_type, left.span())?;
                emitter.jump_if_false(keep, span)?;
                emitter.jump(end, span)?;
                emitter.place(keep);
            }
        }
        emitter.emit(Instruction::Drop, span)?;
        let hint = expected.or(Some(Type::plain(first.value_type.kind)));
        let second = self.compile_value(emitter, right, scope, hint)?;
        emitter.place(end);
        let first_type = if operator == LogicalOperator::Coalesce {
            Type::plain(first.value_type.kind)
        } else {
            first.value_type
        };
        let Some(value_type) = first_type.unify(second.value_type) else {
            return fail(
                span,
                format!(
                    "both sides must have the same type, but they are {} and {}; use `? :` instead",
                    first.value_type.describe(),
                    second.value_type.describe()
                ),
            );
        };
        Ok(Typed::with_range(value_type, first.range.union(second.range)))
    }

    /// Compiles a value and turns it into something `JumpIfFalse` tests the JavaScript way.
    pub fn compile_condition(
        &mut self,
        emitter: &mut Emitter,
        expression: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
    ) -> Compiled<()> {
        let typed = self.compile_value(emitter, expression, scope, None)?;
        self.emit_truth_test(emitter, typed.value_type, expression.span())
    }

    /// Turns the value on top of the stack into 0 for false and anything else for true.
    pub fn emit_truth_test(&mut self, emitter: &mut Emitter, value_type: Type, span: Span) -> Compiled<()> {
        match value_type.kind {
            Kind::Text | Kind::Name => fail(
                span,
                "text cannot be tested for being empty on the watch; compare it with `===` instead",
            ),
            Kind::Null => {
                emitter.emit(Instruction::Drop, span)?;
                emitter.emit(Instruction::PushSmall(0), span)
            }
            _ if value_type.nullable => {
                emitter.emit(Instruction::IsNone, span)?;
                emitter.emit(Instruction::Not, span)
            }
            Kind::Number | Kind::Bool => Ok(()),
            Kind::Color | Kind::Icon | Kind::Choice(_) => {
                emitter.emit(Instruction::Drop, span)?;
                emitter.emit(Instruction::PushSmall(1), span)
            }
        }
    }

    // ------------------------------------------------------------------------
    // Text
    // ------------------------------------------------------------------------

    /// Turns the value on top of the stack into text, as a template literal shows it.
    pub fn convert_to_text(&mut self, emitter: &mut Emitter, typed: Typed, span: Span) -> Compiled<()> {
        match typed.value_type.kind {
            Kind::Text | Kind::Name => Ok(()),
            Kind::Number => emitter.emit(Instruction::FormatNumber(1), span),
            Kind::Choice(set) => {
                let cases: Vec<(KeyValue, Constant)> = set
                    .names()
                    .iter()
                    .enumerate()
                    .map(|(position, name)| {
                        (KeyValue::Number(position as i64), Constant::Text((*name).into()))
                    })
                    .collect();
                self.compile_match_on_stack(emitter, &cases, None, span)?;
                Ok(())
            }
            _ => fail(
                span,
                format!(
                    "{} cannot be shown as text directly; choose the text with `? :`",
                    typed.value_type.describe()
                ),
            ),
        }
    }

    fn compile_template(
        &mut self,
        emitter: &mut Emitter,
        template: &'a TemplateLiteral<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<Typed> {
        let mut pieces = 0;
        for (position, quasi) in template.quasis.iter().enumerate() {
            let Some(text) = quasi.value.cooked.as_ref().map(|text| text.as_str()) else {
                return fail(span, "this template has an escape the compiler cannot read");
            };
            if !text.is_empty() {
                let string = self.string(text);
                emitter.emit(Instruction::PushText(string), span)?;
                pieces += 1;
                if pieces > 1 {
                    emitter.emit(Instruction::Join, span)?;
                }
            }
            if let Some(inner) = template.expressions.get(position) {
                let typed = self.compile_value(emitter, inner, scope, None)?;
                self.convert_to_text(emitter, typed, inner.span())?;
                pieces += 1;
                if pieces > 1 {
                    emitter.emit(Instruction::Join, span)?;
                }
            }
        }
        if pieces == 0 {
            let string = self.string("");
            emitter.emit(Instruction::PushText(string), span)?;
            return Ok(Typed::new(Type::NAME));
        }
        Ok(Typed::new(Type::TEXT))
    }
}

/// The library names that stand for something more specific than a library function.
fn library_reference<'a>(module: LibraryModule, name: &str) -> Reference<'a> {
    match module {
        LibraryModule::Neotime => {
            if let Some(constant) = library_constant(name) {
                return Reference::Constant(constant);
            }
            if let Some(system) = system_signal(name) {
                return Reference::System(system);
            }
            let group = match name {
                "settings" => Some("settings"),
                "current_weather" => Some("current_weather"),
                "current_music" => Some("current_music"),
                "current_call" => Some("current_call"),
                "current_navigation" => Some("current_navigation"),
                "installed_apps" => Some("installed_apps"),
                "launcher_groups" => Some("launcher_groups"),
                "notifications" => Some("notifications"),
                "packages" => Some("packages"),
                "step_history" => Some("step_history"),
                _ => None,
            };
            if let Some(group) = group {
                return Reference::SystemGroup(group);
            }
        }
        LibraryModule::Widgets => {
            if let Some(constant) = library_constant(name) {
                return Reference::Constant(constant);
            }
        }
        _ => {}
    }
    Reference::Library {
        module,
        name: name.to_string(),
    }
}

fn source_text<'a>(compiler: &Compiler<'a>, scope: &Rc<Scope<'a>>, span: Span) -> String {
    compiler
        .modules
        .get(scope.module)
        .and_then(|module| module.source.get(span.start as usize..span.end as usize))
        .unwrap_or("this name")
        .to_string()
}

/// The entries of a constant object as a table keyed the way a value of `key_type` is stored.
fn object_cases(fields: &[(String, Constant)], key_type: Type) -> Option<Vec<(KeyValue, Constant)>> {
    match key_type.kind {
        Kind::Choice(set) => set
            .names()
            .iter()
            .enumerate()
            .map(|(position, name)| {
                let value = fields.iter().find(|(field, _)| field == name)?.1.clone();
                Some((KeyValue::Number(position as i64), value))
            })
            .collect(),
        Kind::Name => Some(
            fields
                .iter()
                .map(|(field, value)| (KeyValue::Text(field.clone()), value.clone()))
                .collect(),
        ),
        _ => None,
    }
}
