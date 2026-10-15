//! Values worked out from other state and kept in hidden variables, each rebuilt by a derive
//! subscription: arrays a widget shows, such as a `computed` board, and times a subscription
//! waits for, such as a `computed` alarm time.
//!
//! The runtime runs a derive handler when the app opens and whenever anything it reads changes, so
//! the hidden variable always holds what its expression gives.

use std::rc::Rc;

use battletime_runtime::instruction::Instruction;
use battletime_runtime::subscriptions::{Subscription, SubscriptionKind, SubscriptionValue};
use oxc_ast::ast::{Expression, ObjectPropertyKind, Statement};
use oxc_span::Span;

use crate::compiler::{Compiler, DeclaredType};
use crate::constants::{static_key, strip_parentheses, Constant};
use crate::diagnostics::{fail, Compiled};
use crate::emitter::{CodeKind, Emitter};
use crate::scope::{FunctionBodyReference, FunctionReference, Reference, Scope};
use crate::types::{Kind, NumberHint, Type};

// ============================================================================
// Points
// ============================================================================

/// One coordinate of a point a Line joins: known now, or worked out while running.
enum Coordinate<'a> {
    Known(Constant),
    Worked(&'a Expression<'a>, Rc<Scope<'a>>),
}

// ============================================================================
// Derived arrays
// ============================================================================

impl<'a> Compiler<'a> {
    /// A hidden array kept equal to `expression`, which changes while the app runs, or `None` if
    /// the compiler cannot tell how long it is.
    pub fn derived_array(
        &mut self,
        expression: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        element_type: Type,
        span: Span,
    ) -> Compiled<Option<u8>> {
        let key = self.derived_key(expression, scope);
        if let Some((_, variable)) = self.derived_arrays.iter().find(|(existing, _)| *existing == key) {
            return Ok(Some(*variable));
        }
        let Some(length) = self.static_array_length(expression, scope, span) else {
            return Ok(None);
        };
        let variable = self.add_hidden_array(element_type, length, span)?;
        self.derived_arrays.push((key, variable));
        self.derived_scopes.push(scope.clone());
        let mut emitter = Emitter::new(CodeKind::Handler);
        self.compile_array_assignment(&mut emitter, variable, expression, scope, span)?;
        self.add_derive_subscription(emitter, true, span)?;
        Ok(Some(variable))
    }

    /// A hidden array of a Line's points, x and y one after the other, from an array literal of
    /// `{ x, y }` objects whose coordinates may change while the app runs.
    pub fn derived_points(
        &mut self,
        expression: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<Option<u8>> {
        let Some(coordinates) = self.point_coordinates(expression, scope) else {
            return Ok(None);
        };
        let key = self.derived_key(expression, scope);
        if let Some((_, variable)) = self.derived_arrays.iter().find(|(existing, _)| *existing == key) {
            return Ok(Some(*variable));
        }
        let variable = self.add_hidden_array(Type::NUMBER, coordinates.len(), span)?;
        self.derived_arrays.push((key, variable));
        self.derived_scopes.push(scope.clone());
        let mut emitter = Emitter::new(CodeKind::Handler);
        for (index, coordinate) in coordinates.into_iter().enumerate() {
            self.emit_number(&mut emitter, index as i64, span)?;
            let typed = match coordinate {
                Coordinate::Known(constant) => {
                    self.emit_constant(&mut emitter, &constant, Some(Type::NUMBER), span)?
                }
                Coordinate::Worked(expression, scope) => {
                    self.compile_value_of_type(&mut emitter, expression, &scope, Type::NUMBER)?
                }
            };
            emitter.emit(Instruction::WriteElement(variable), span)?;
            self.note_write(variable, typed.range);
        }
        self.add_derive_subscription(emitter, true, span)?;
        Ok(Some(variable))
    }

    /// The coordinates of `[{ x, y }, ...]` in order, or `None` if it is not written that way.
    fn point_coordinates(
        &self,
        expression: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
    ) -> Option<Vec<Coordinate<'a>>> {
        let Expression::ArrayExpression(array) = strip_parentheses(expression) else {
            return match self.resolve_reference(expression, scope)? {
                Reference::Expression {
                    expression: inner,
                    scope: inner_scope,
                } if !std::ptr::eq(inner, expression) => self.point_coordinates(inner, &inner_scope),
                _ => None,
            };
        };
        let mut coordinates = Vec::new();
        for element in &array.elements {
            let element = element.as_expression()?;
            if let Some(point) = self.evaluate_constant(element, scope) {
                coordinates.push(Coordinate::Known(point.field("x")?));
                coordinates.push(Coordinate::Known(point.field("y")?));
                continue;
            }
            let Expression::ObjectExpression(object) = strip_parentheses(element) else {
                return None;
            };
            for wanted in ["x", "y"] {
                let value = object.properties.iter().find_map(|property| match property {
                    ObjectPropertyKind::ObjectProperty(property)
                        if static_key(&property.key).as_deref() == Some(wanted) =>
                    {
                        Some(&property.value)
                    }
                    _ => None,
                })?;
                coordinates.push(match self.evaluate_constant(value, scope) {
                    Some(constant) => Coordinate::Known(constant),
                    None => Coordinate::Worked(value, scope.clone()),
                });
            }
        }
        (!coordinates.is_empty()).then_some(coordinates)
    }

    /// A number that is the same each time the same expression in the same place is derived.
    fn derived_key(&self, expression: &'a Expression<'a>, scope: &Rc<Scope<'a>>) -> (usize, usize) {
        if let Expression::StaticMemberExpression(member) = strip_parentheses(expression) {
            if member.property.name == "value" {
                if let Some(Reference::Computed(function)) = self.resolve_reference(&member.object, scope) {
                    return (function.identity(), 0);
                }
            }
        }
        (
            expression as *const Expression as usize,
            Rc::as_ptr(scope) as usize,
        )
    }

    /// Adds a non-persisted array, keyed `~derivedN`, that only derive handlers write.
    fn add_hidden_array(&mut self, element_type: Type, length: usize, span: Span) -> Compiled<u8> {
        let start = match element_type.kind {
            Kind::Color => Constant::Text("#000000".into()),
            Kind::Text | Kind::Name => Constant::Text("".into()),
            Kind::Bool => Constant::Bool(false),
            _ => Constant::Number(0),
        };
        let key = format!("~derived{}", self.derived_count);
        self.derived_count += 1;
        let declared = DeclaredType {
            value_type: Type::plain(element_type.kind),
            hint: NumberHint::Plain,
            is_array: true,
        };
        let elements = Constant::Array(Rc::new(vec![start; length.max(1)]));
        let variable = self.add_signal_slot(key, false, declared, &elements, span)?;
        self.derived_targets.push(variable);
        Ok(variable)
    }

    /// Adds the derive subscription that runs the code in `emitter`.
    fn add_derive_subscription(
        &mut self,
        emitter: Emitter,
        only_while_open: bool,
        span: Span,
    ) -> Compiled<()> {
        let code = emitter.finish(span)?;
        let handler = self.add_handler(code);
        self.writer.subscription(Subscription {
            kind: SubscriptionKind::Derive,
            only_while_open,
            handler,
            detail: 0,
            days: 0,
            value: SubscriptionValue::Constant(0),
        });
        Ok(())
    }

    // ------------------------------------------------------------------------
    // Lengths
    // ------------------------------------------------------------------------

    /// How many elements an array expression has, when that is fixed while compiling.
    pub fn static_array_length(
        &mut self,
        expression: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Option<usize> {
        if let Some(constant) = self.evaluate_constant(expression, scope) {
            return match constant {
                Constant::Array(elements) => Some(elements.len()),
                _ => None,
            };
        }
        match self.resolve_reference(expression, scope) {
            Some(Reference::StateArray(variable)) => {
                return Some(self.slots[variable as usize].length as usize)
            }
            Some(Reference::Expression {
                expression: inner,
                scope: inner_scope,
            }) if !std::ptr::eq(inner, expression) => {
                return self.static_array_length(inner, &inner_scope, span)
            }
            _ => {}
        }
        match strip_parentheses(expression) {
            Expression::ArrayExpression(array) => Some(array.elements.len()),
            Expression::ConditionalExpression(conditional) => {
                let consequent = self.static_array_length(&conditional.consequent, scope, span)?;
                let alternate = self.static_array_length(&conditional.alternate, scope, span)?;
                Some(consequent.max(alternate))
            }
            Expression::StaticMemberExpression(member) if member.property.name == "value" => {
                match self.resolve_reference(&member.object, scope)? {
                    Reference::Computed(function) => self.function_result_length(&function, span),
                    _ => None,
                }
            }
            Expression::CallExpression(call) => {
                if let Some((length, _)) = self.array_from_length(call, scope) {
                    return usize::try_from(length).ok();
                }
                if let Expression::StaticMemberExpression(member) = strip_parentheses(&call.callee) {
                    if member.property.name == "map" {
                        return self
                            .loop_source(&member.object, scope, span)
                            .ok()
                            .map(|source| source.length());
                    }
                }
                match self.resolve_reference(&call.callee, scope)? {
                    Reference::Function(function) => self.function_result_length(&function, span),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// The length of the array a function returns, from its last `return`.
    fn function_result_length(&mut self, function: &FunctionReference<'a>, span: Span) -> Option<usize> {
        let returned = match function.body {
            FunctionBodyReference::Expression(expression) => expression,
            FunctionBodyReference::Statements(body) => match body.statements.last()? {
                Statement::ReturnStatement(statement) => statement.argument.as_ref()?,
                _ => return None,
            },
        };
        self.static_array_length(returned, &function.scope, span)
    }

    /// The element a constant array is padded with when it is shorter than the hidden array it
    /// fills, such as the 12 hour options of a picker that also has 24 hour ones.
    pub fn padding_for(&self, target: u8) -> Option<Constant> {
        if !self.derived_targets.contains(&target) {
            return None;
        }
        match self.slots.get(target as usize)?.value_type.kind {
            Kind::Text | Kind::Name => Some(Constant::Text("".into())),
            _ => None,
        }
    }

    // ------------------------------------------------------------------------
    // Derived scalars
    // ------------------------------------------------------------------------

    /// A hidden single value, keyed `~derivedN`, kept equal to what `source` gives, for a
    /// subscription that needs a state variable, such as `at` with a `computed` time.
    pub fn derived_scalar(
        &mut self,
        source: Reference<'a>,
        value_type: Type,
        hint: NumberHint,
        span: Span,
    ) -> Compiled<u8> {
        let key = format!("~derived{}", self.derived_count);
        self.derived_count += 1;
        let declared = DeclaredType {
            value_type: value_type.or_null(),
            hint,
            is_array: false,
        };
        let variable = self.add_signal_slot(key, false, declared, &Constant::Null, span)?;
        let mut emitter = Emitter::new(CodeKind::Handler);
        let typed = match source {
            Reference::Signal(signal) => {
                emitter.emit(Instruction::ReadState(signal), span)?;
                crate::expressions::Typed::with_range(
                    self.slots[signal as usize].value_type,
                    self.read_range(signal),
                )
            }
            Reference::Computed(function) => {
                match self.inline_call(&mut emitter, &function, Vec::new(), true, Some(value_type), span)? {
                    Some(typed) => typed,
                    None => return fail(function.span, "this computed gives no value"),
                }
            }
            Reference::Record(fields) => {
                // A time of day `{ hour, minute }` becomes its minute of the day.
                let field = |name: &str| {
                    fields
                        .iter()
                        .find(|(field, _)| field == name)
                        .map(|(_, slot)| *slot)
                };
                let (Some(hour), Some(minute)) = (field("hour"), field("minute")) else {
                    return fail(span, "a time of day needs an `hour` and a `minute`");
                };
                emitter.emit(Instruction::ReadState(hour), span)?;
                self.emit_number(&mut emitter, 60, span)?;
                emitter.emit(Instruction::Multiply, span)?;
                emitter.emit(Instruction::ReadState(minute), span)?;
                emitter.emit(Instruction::Add, span)?;
                crate::expressions::Typed::with_range(
                    Type::NUMBER,
                    crate::types::Range {
                        lowest: 0,
                        highest: 24 * 60 - 1,
                    },
                )
            }
            Reference::Expression { expression, scope } => {
                self.compile_value(&mut emitter, expression, &scope, Some(value_type))?
            }
            _ => return fail(span, "this cannot be kept in a state variable"),
        };
        if !typed.value_type.fits(value_type) {
            return fail(
                span,
                format!(
                    "expected {}, but this is {}",
                    value_type.describe(),
                    typed.value_type.describe()
                ),
            );
        }
        emitter.emit(Instruction::WriteState(variable), span)?;
        self.note_write(variable, typed.range);
        self.add_derive_subscription(emitter, false, span)?;
        Ok(variable)
    }
}

#[cfg(test)]
#[path = "derived_tests.rs"]
mod derived_tests;
