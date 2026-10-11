//! Statements, inlined function calls, handlers and assignments to state, including whole arrays.

use std::rc::Rc;

use neotime_runtime::events::EventKind;
use neotime_runtime::instruction::Instruction;
use oxc_ast::ast::{
    AssignmentExpression, AssignmentTarget, BindingPattern, ChainElement, Expression, ForOfStatement,
    SimpleAssignmentTarget, Statement, VariableDeclaration, VariableDeclarationKind,
};
use oxc_span::{GetSpan, Span};
use oxc_syntax::operator::{AssignmentOperator, BinaryOperator, LogicalOperator, UpdateOperator};

use crate::compiler::{function_reference, Compiler, StateSlot};
use crate::constants::{compound_operator, static_key, strip_parentheses, Constant};
use crate::diagnostics::{fail, Compiled};
use crate::emitter::{CodeKind, Emitter, InlineFrame};
use crate::expressions::Typed;
use crate::scope::{ElementIndex, FunctionBodyReference, FunctionReference, Reference, Scope};
use crate::types::{Kind, Range, Type};

// ============================================================================
// Arguments
// ============================================================================

/// What a parameter of an inlined function is bound to.
#[derive(Clone, Debug)]
pub enum ArgumentSource<'a> {
    /// An argument written at the call, compiled in the caller's scope.
    Expression(&'a Expression<'a>, Rc<Scope<'a>>),
    /// A value the compiler already has, such as a loop element in a local.
    Reference(Reference<'a>),
}

/// The largest array written from a literal by pushing every element first.
const LARGEST_ARRAY_LITERAL_ON_STACK: usize = 12;

// ============================================================================
// Handlers
// ============================================================================

impl<'a> Compiler<'a> {
    /// Compiles a function as a handler for an event and returns the handler's number.
    pub fn compile_handler(
        &mut self,
        function: &FunctionReference<'a>,
        event: Option<EventKind>,
    ) -> Compiled<u16> {
        let mut emitter = Emitter::new(CodeKind::Handler);
        let mut sources = Vec::new();
        if let Some(event) = event {
            sources.push(ArgumentSource::Reference(Reference::Event(event)));
        }
        self.inline_call(&mut emitter, function, sources, false, None, function.span)?;
        let code = emitter.finish(function.span)?;
        Ok(self.add_handler(code))
    }

    /// Compiles an expression that works out a widget property, and returns its number.
    pub fn compile_property_expression(
        &mut self,
        expression: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        expected: Option<Type>,
    ) -> Compiled<(u16, Typed)> {
        let mut emitter = Emitter::new(CodeKind::Expression);
        let typed = self.compile_value(&mut emitter, expression, scope, expected)?;
        let code = emitter.finish(expression.span())?;
        Ok((self.add_expression(code), typed))
    }

    // ------------------------------------------------------------------------
    // Inlining
    // ------------------------------------------------------------------------

    /// Compiles a call to an app function in place, with its parameters bound to the arguments.
    pub fn inline_call(
        &mut self,
        emitter: &mut Emitter,
        function: &FunctionReference<'a>,
        arguments: Vec<ArgumentSource<'a>>,
        wants_value: bool,
        expected: Option<Type>,
        span: Span,
    ) -> Compiled<Option<Typed>> {
        if function.is_async {
            return fail(
                function.span,
                "`async` functions are not available; data from the phone arrives through a subscription",
            );
        }
        let identity = function.identity();
        if self.inline_stack.contains(&identity) {
            return fail(
                span,
                "recursion is not available; use `for ... of` over a fixed-length array",
            );
        }
        self.inline_stack.push(identity);
        let spill_mark = emitter.locals_in_use();
        let spilled = self.spill_stack(emitter, function, span);
        let mark = emitter.locals_in_use();
        let result = spilled.and_then(|spilled| {
            let scope = self.bind_parameters(emitter, function, arguments, span)?;
            let result =
                self.compile_function_body(emitter, function.body, &scope, wants_value, expected, span)?;
            emitter.release_locals(mark);
            self.restore_stack(emitter, &spilled, result.is_some(), span)?;
            Ok(result)
        });
        emitter.release_locals(spill_mark);
        self.inline_stack.pop();
        result
    }

    /// Moves the stack into locals before inlining a function that may loop, as a loop needs an empty stack.
    fn spill_stack(
        &mut self,
        emitter: &mut Emitter,
        function: &FunctionReference<'a>,
        span: Span,
    ) -> Compiled<Vec<u8>> {
        let has_statements = match function.body {
            FunctionBodyReference::Statements(body) => {
                !matches!(body.statements.as_slice(), [Statement::ReturnStatement(_)])
            }
            FunctionBodyReference::Expression(_) => false,
        };
        if !has_statements {
            return Ok(Vec::new());
        }
        self.spill_whole_stack(emitter, span)
    }

    /// Moves every value on the stack into new locals, so code that loops can run here.
    pub fn spill_whole_stack(&mut self, emitter: &mut Emitter, span: Span) -> Compiled<Vec<u8>> {
        let depth = emitter.depth().unwrap_or(0);
        let mut locals = Vec::new();
        for _ in 0..depth {
            locals.push(emitter.take_local(span)?);
        }
        for local in locals.iter().rev() {
            emitter.emit(Instruction::WriteLocal(*local), span)?;
        }
        Ok(locals)
    }

    /// Puts spilled values back under the function's result.
    pub fn restore_stack(
        &mut self,
        emitter: &mut Emitter,
        spilled: &[u8],
        has_result: bool,
        span: Span,
    ) -> Compiled<()> {
        if spilled.is_empty() {
            return Ok(());
        }
        let result = if has_result {
            let local = emitter.take_local(span)?;
            emitter.emit(Instruction::WriteLocal(local), span)?;
            Some(local)
        } else {
            None
        };
        for local in spilled {
            emitter.emit(Instruction::ReadLocal(*local), span)?;
        }
        if let Some(local) = result {
            emitter.emit(Instruction::ReadLocal(local), span)?;
        }
        Ok(())
    }

    fn bind_parameters(
        &mut self,
        emitter: &mut Emitter,
        function: &FunctionReference<'a>,
        arguments: Vec<ArgumentSource<'a>>,
        span: Span,
    ) -> Compiled<Rc<Scope<'a>>> {
        if function.parameters.rest.is_some() {
            return fail(function.span, "`...` parameters are not supported");
        }
        let mut scope = function.scope.clone();
        let mut arguments = arguments.into_iter();
        for parameter in &function.parameters.items {
            let source = match arguments.next() {
                Some(source) => source,
                None => match &parameter.initializer {
                    Some(initializer) => ArgumentSource::Expression(initializer, scope.clone()),
                    None => ArgumentSource::Reference(Reference::Constant(Constant::Undefined)),
                },
            };
            let expected = parameter
                .type_annotation
                .as_ref()
                .and_then(|annotation| {
                    self.declared_type(&annotation.type_annotation, function.scope.module)
                        .ok()
                        .flatten()
                })
                .filter(|declared| !declared.is_array)
                .map(|declared| declared.value_type);
            let reference = self.argument_reference(emitter, source, expected, span)?;
            scope = self.bind_pattern(&parameter.pattern, reference, &scope)?;
        }
        Ok(scope)
    }

    /// Turns an argument into what its parameter stands for: a constant, an alias, or a local.
    fn argument_reference(
        &mut self,
        emitter: &mut Emitter,
        source: ArgumentSource<'a>,
        expected: Option<Type>,
        span: Span,
    ) -> Compiled<Reference<'a>> {
        let (expression, scope) = match source {
            ArgumentSource::Reference(reference) => return Ok(reference),
            ArgumentSource::Expression(expression, scope) => (expression, scope),
        };
        if let Some(constant) = self.evaluate_constant(expression, &scope) {
            return Ok(Reference::Constant(constant));
        }
        if let Some(reference) = self.resolve_reference(expression, &scope) {
            // A widget's value event gives a number even for a switch, so a typed parameter decides.
            if let (Reference::EventField { field, value_type }, Some(expected)) = (&reference, expected) {
                if expected.kind == Kind::Bool && value_type.kind == Kind::Number {
                    return Ok(Reference::EventField {
                        field: *field,
                        value_type: expected,
                    });
                }
            }
            if !matches!(reference, Reference::Expression { .. }) {
                return Ok(reference);
            }
        }
        match strip_parentheses(expression) {
            Expression::ArrayExpression(_) => return Ok(Reference::Expression { expression, scope }),
            Expression::ObjectExpression(object) => {
                let mut fields: Vec<(String, Reference<'a>)> = Vec::new();
                for property in &object.properties {
                    let (name, field) = match property {
                        oxc_ast::ast::ObjectPropertyKind::ObjectProperty(property) => {
                            let Some(name) = static_key(&property.key) else {
                                return fail(property.span, "give each field a plain name");
                            };
                            let value = ArgumentSource::Expression(&property.value, scope.clone());
                            (name, self.argument_reference(emitter, value, None, span)?)
                        }
                        oxc_ast::ast::ObjectPropertyKind::SpreadProperty(spread) => {
                            for (name, field) in self.record_sources(&spread.argument, &scope, span)? {
                                fields.retain(|(existing, _)| *existing != name);
                                fields.push((name, field));
                            }
                            continue;
                        }
                    };
                    fields.retain(|(existing, _)| *existing != name);
                    fields.push((name, field));
                }
                return Ok(Reference::Object(Rc::new(fields)));
            }
            _ => {}
        }

        // An expression cannot change state, so a cheap read is read again where it is used.
        if (!emitter.is_handler() && self.is_cheap_read(expression, &scope))
            || self.is_pure_of_constants(expression, &scope, 0)
        {
            return Ok(Reference::Expression { expression, scope });
        }
        let local = emitter.take_local(span)?;
        let typed = self.compile_value(emitter, expression, &scope, expected)?;
        emitter.emit(Instruction::WriteLocal(local), span)?;
        let value_type = match expected {
            Some(expected) if expected.kind == Kind::Bool && typed.value_type.kind == Kind::Number => {
                expected
            }
            _ => typed.value_type,
        };
        Ok(Reference::Local {
            index: local,
            value_type,
            is_constant: true,
        })
    }

    /// Binds a parameter or `const` pattern, taking fields apart for `{ a, b }`.
    fn bind_pattern(
        &mut self,
        pattern: &'a BindingPattern<'a>,
        reference: Reference<'a>,
        scope: &Rc<Scope<'a>>,
    ) -> Compiled<Rc<Scope<'a>>> {
        match pattern {
            BindingPattern::BindingIdentifier(identifier) => {
                Ok(scope.with(identifier.name.as_str(), reference))
            }
            BindingPattern::AssignmentPattern(assignment) => {
                let reference = match reference {
                    Reference::Constant(Constant::Undefined) => Reference::Expression {
                        expression: &assignment.right,
                        scope: scope.clone(),
                    },
                    other => other,
                };
                self.bind_pattern(&assignment.left, reference, scope)
            }
            BindingPattern::ObjectPattern(object) => {
                if object.rest.is_some() {
                    return fail(object.span, "`...rest` in a pattern is not supported");
                }
                let mut scope = scope.clone();
                for property in &object.properties {
                    let Some(name) = static_key(&property.key) else {
                        return fail(property.span, "name each field you take apart");
                    };
                    let field = self.field_of(&reference, &name, property.span)?;
                    scope = self.bind_pattern(&property.value, field, &scope)?;
                }
                Ok(scope)
            }
            BindingPattern::ArrayPattern(array) => fail(
                array.span,
                "taking arrays apart with `[a, b]` is not supported; read each element by its index",
            ),
        }
    }

    /// One field of an object reference, for taking it apart.
    fn field_of(&self, reference: &Reference<'a>, name: &str, span: Span) -> Compiled<Reference<'a>> {
        match reference {
            Reference::Object(fields) => Ok(fields
                .iter()
                .find(|(field, _)| field == name)
                .map_or(Reference::Constant(Constant::Undefined), |(_, field)| {
                    field.clone()
                })),
            Reference::Constant(constant) => match constant.field(name) {
                Some(field) => Ok(Reference::Constant(field)),
                None => fail(span, format!("this value has no field `{name}`")),
            },
            Reference::Event(kind) => match kind.field_index(name) {
                Some(field) => Ok(Reference::EventField {
                    field,
                    value_type: match (kind, name) {
                        (EventKind::Swipe, "direction") => {
                            Type::choice(crate::types::ChoiceSet::SwipeDirection)
                        }
                        (EventKind::Button, "kind") => Type::choice(crate::types::ChoiceSet::ButtonPress),
                        _ => Type::NUMBER,
                    },
                }),
                None => fail(span, format!("this event has no field `{name}`")),
            },
            Reference::Expression { expression, scope } => match self.evaluate_constant(expression, scope) {
                Some(constant) => match constant.field(name) {
                    Some(field) => Ok(Reference::Constant(field)),
                    None => fail(span, format!("this value has no field `{name}`")),
                },
                None => fail(
                    span,
                    "only objects known when the app is compiled can be taken apart",
                ),
            },
            _ => fail(span, "only objects such as props can be taken apart"),
        }
    }

    fn compile_function_body(
        &mut self,
        emitter: &mut Emitter,
        body: FunctionBodyReference<'a>,
        scope: &Rc<Scope<'a>>,
        wants_value: bool,
        expected: Option<Type>,
        span: Span,
    ) -> Compiled<Option<Typed>> {
        let statements = match body {
            FunctionBodyReference::Expression(expression) => {
                if wants_value {
                    return self.compile_value(emitter, expression, scope, expected).map(Some);
                }
                self.compile_expression_statement(emitter, expression, scope)?;
                return Ok(None);
            }
            FunctionBodyReference::Statements(body) => &body.statements,
        };

        // A body that is only `return value;` needs no jumps.
        if wants_value {
            if let [Statement::ReturnStatement(statement)] = statements.as_slice() {
                if let Some(argument) = &statement.argument {
                    return self.compile_value(emitter, argument, scope, expected).map(Some);
                }
            }
        }

        let end = emitter.label();
        let result_local = if wants_value {
            Some(emitter.take_local(span)?)
        } else {
            None
        };
        emitter.inline_frames.push(InlineFrame {
            end,
            result_local,
            result_type: None,
            expected,
        });
        let compiled = self.compile_statements(emitter, statements, scope);
        let frame = emitter.inline_frames.pop();
        compiled?;
        emitter.place(end);
        match (result_local, frame.and_then(|frame| frame.result_type)) {
            (Some(local), Some(result_type)) => {
                emitter.emit(Instruction::ReadLocal(local), span)?;
                Ok(Some(Typed::new(result_type)))
            }
            (Some(_), None) => fail(span, "this function returns no value"),
            (None, _) => Ok(None),
        }
    }

    // ------------------------------------------------------------------------
    // Statements
    // ------------------------------------------------------------------------

    /// Compiles statements in order, freeing their locals at the end.
    pub fn compile_statements(
        &mut self,
        emitter: &mut Emitter,
        statements: &'a [Statement<'a>],
        scope: &Rc<Scope<'a>>,
    ) -> Compiled<()> {
        let mark = emitter.locals_in_use();
        let mut scope = scope.clone();

        // Function declarations can be called before they appear.
        for statement in statements {
            if let Statement::FunctionDeclaration(function) = statement {
                if let Some(identifier) = &function.id {
                    let reference = function_reference(function, &scope)?;
                    scope = scope.with(identifier.name.as_str(), Reference::Function(reference));
                }
            }
        }
        for statement in statements {
            scope = self.compile_statement(emitter, statement, &scope)?;
        }
        emitter.release_locals(mark);
        Ok(())
    }

    fn compile_statement(
        &mut self,
        emitter: &mut Emitter,
        statement: &'a Statement<'a>,
        scope: &Rc<Scope<'a>>,
    ) -> Compiled<Rc<Scope<'a>>> {
        let span = statement.span();
        match statement {
            Statement::VariableDeclaration(declaration) => self.compile_declaration(emitter, declaration, scope),
            Statement::FunctionDeclaration(_) | Statement::EmptyStatement(_) => Ok(scope.clone()),
            Statement::TSTypeAliasDeclaration(_) | Statement::TSInterfaceDeclaration(_) => Ok(scope.clone()),
            Statement::ExpressionStatement(statement) => {
                self.compile_expression_statement(emitter, &statement.expression, scope)?;
                Ok(scope.clone())
            }
            Statement::BlockStatement(block) => {
                self.compile_statements(emitter, &block.body, scope)?;
                Ok(scope.clone())
            }
            Statement::IfStatement(statement) => {
                if let Some(test) = self.evaluate_constant(&statement.test, scope) {
                    if test.is_truthy() {
                        self.compile_branch(emitter, &statement.consequent, scope)?;
                    } else if let Some(alternate) = &statement.alternate {
                        self.compile_branch(emitter, alternate, scope)?;
                    }
                    return Ok(scope.clone());
                }
                let otherwise = emitter.label();
                self.compile_condition(emitter, &statement.test, scope)?;
                emitter.jump_if_false(otherwise, span)?;
                self.compile_branch(emitter, &statement.consequent, scope)?;
                match &statement.alternate {
                    Some(alternate) => {
                        let end = emitter.label();
                        emitter.jump(end, span)?;
                        emitter.place(otherwise);
                        self.compile_branch(emitter, alternate, scope)?;
                        emitter.place(end);
                    }
                    None => emitter.place(otherwise),
                }
                Ok(scope.clone())
            }
            Statement::ReturnStatement(statement) => {
                let Some(frame) = emitter.inline_frames.last() else {
                    return fail(span, "`return` must be inside a function");
                };
                let (end, result_local, result_type) = (frame.end, frame.result_local, frame.result_type);
                let hint = result_type.or(frame.expected);
                match (&statement.argument, result_local) {
                    (Some(argument), Some(local)) => {
                        let typed = self.compile_value(emitter, argument, scope, hint)?;
                        let unified = match result_type {
                            None => typed.value_type,
                            Some(previous) => previous.unify(typed.value_type).ok_or_else(|| {
                                crate::diagnostics::Diagnostic::error(
                                    span,
                                    format!(
                                        "this function returns both {} and {}",
                                        previous.describe(),
                                        typed.value_type.describe()
                                    ),
                                )
                            })?,
                        };
                        if let Some(frame) = emitter.inline_frames.last_mut() {
                            frame.result_type = Some(unified);
                        }
                        emitter.emit(Instruction::WriteLocal(local), span)?;
                    }
                    (Some(argument), None) => self.compile_expression_statement(emitter, argument, scope)?,
                    (None, _) => {}
                }
                emitter.jump(end, span)?;
                Ok(scope.clone())
            }
            Statement::ForOfStatement(statement) => {
                self.compile_for_of(emitter, statement, scope)?;
                Ok(scope.clone())
            }
            Statement::BreakStatement(statement) => {
                if statement.label.is_some() {
                    return fail(span, "labelled `break` is not supported");
                }
                let Some(labels) = self.loop_labels.last() else {
                    return fail(span, "`break` must be inside a `for ... of` loop");
                };
                let exit = labels.exit;
                emitter.jump(exit, span)?;
                Ok(scope.clone())
            }
            Statement::ContinueStatement(statement) => {
                if statement.label.is_some() {
                    return fail(span, "labelled `continue` is not supported");
                }
                let Some(labels) = self.loop_labels.last() else {
                    return fail(span, "`continue` must be inside a `for ... of` loop");
                };
                let next = labels.next;
                emitter.jump(next, span)?;
                Ok(scope.clone())
            }
            Statement::WhileStatement(_) | Statement::DoWhileStatement(_) => fail(
                span,
                "`while` loops are not available, because every call must end; use `for ... of` over a fixed-length array or `range(n)`",
            ),
            Statement::ForStatement(_) => fail(
                span,
                "counting `for` loops are not available; use `for ... of` over a fixed-length array or `range(n)`",
            ),
            Statement::ForInStatement(_) => fail(span, "`for ... in` is not available; use `for ... of` over a fixed-length array"),
            Statement::SwitchStatement(_) => fail(span, "`switch` is not supported; use `if` and `else`"),
            Statement::ThrowStatement(_) | Statement::TryStatement(_) => fail(
                span,
                "exceptions are not available; a handler that fails, such as by dividing by zero, changes nothing",
            ),
            Statement::ClassDeclaration(_) => fail(span, "classes are not available; keep state in signals"),
            _ => fail(span, "this statement is not supported on the watch"),
        }
    }

    /// Compiles one branch of an `if`, whose declarations end with it.
    fn compile_branch(
        &mut self,
        emitter: &mut Emitter,
        statement: &'a Statement<'a>,
        scope: &Rc<Scope<'a>>,
    ) -> Compiled<()> {
        let mark = emitter.locals_in_use();
        self.compile_statement(emitter, statement, scope)?;
        emitter.release_locals(mark);
        Ok(())
    }

    fn compile_declaration(
        &mut self,
        emitter: &mut Emitter,
        declaration: &'a VariableDeclaration<'a>,
        scope: &Rc<Scope<'a>>,
    ) -> Compiled<Rc<Scope<'a>>> {
        let is_constant = match declaration.kind {
            VariableDeclarationKind::Const => true,
            VariableDeclarationKind::Let => false,
            _ => return fail(declaration.span, "use `const` or `let`"),
        };
        let mut scope = scope.clone();
        for declarator in &declaration.declarations {
            let span = declarator.span;
            let Some(init) = &declarator.init else {
                return fail(span, "give the variable a starting value");
            };
            let wanted = match &declarator.type_annotation {
                Some(annotation) => self
                    .declared_type(&annotation.type_annotation, scope.module)
                    .ok()
                    .flatten()
                    .filter(|declared| !declared.is_array)
                    .map(|declared| declared.value_type),
                None => None,
            };

            if let Some(constant) = self.evaluate_constant(init, &scope) {
                let is_scalar = !matches!(constant, Constant::Array(_) | Constant::Object(_));
                if is_constant || !is_scalar {
                    if !is_constant {
                        return fail(
                            span,
                            "a local holds one value; keep arrays that change in a signal",
                        );
                    }
                    scope = self.bind_pattern(&declarator.id, Reference::Constant(constant), &scope)?;
                    continue;
                }
            }
            if let Some(reference) = self.resolve_reference(init, &scope) {
                let is_alias = !matches!(
                    reference,
                    Reference::Expression { .. }
                        | Reference::Constant(_)
                        | Reference::Local { .. }
                        | Reference::StateElement { .. }
                        | Reference::EventField { .. }
                        | Reference::ProgressionElement { .. }
                );
                if is_alias {
                    if !is_constant {
                        return fail(span, "only `const` can name a signal, array or function");
                    }
                    scope = self.bind_pattern(&declarator.id, reference, &scope)?;
                    continue;
                }
            }
            if is_constant
                && (self.is_choice_between_constants(init, &scope)
                    || self.is_pure_of_constants(init, &scope, 0))
            {
                scope = self.bind_pattern(
                    &declarator.id,
                    Reference::Expression {
                        expression: init,
                        scope: scope.clone(),
                    },
                    &scope,
                )?;
                continue;
            }
            if let Expression::ArrayExpression(_) = strip_parentheses(init) {
                return fail(
                    span,
                    "a local holds one value, not an array; keep the array in a signal, or read its elements where they are used",
                );
            }
            if let Some(name) = self.method_name(init) {
                if name == "map" {
                    return fail(
                        span,
                        "a local holds one value, not an array; assign the array to a signal's `.value` instead",
                    );
                }
            }
            let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
                return fail(
                    span,
                    "only objects known when the app is compiled can be taken apart",
                );
            };
            let local = emitter.take_local(span)?;
            let typed = self.compile_value(emitter, init, &scope, wanted)?;
            if typed.value_type.kind == Kind::Text && !is_constant {
                return fail(
                    span,
                    "text made while running only lasts until the next text is made; use `const`, or keep a number and format it where it is shown",
                );
            }
            let value_type = match wanted {
                Some(wanted) if typed.value_type.fits(wanted) => wanted,
                _ => typed.value_type,
            };
            emitter.emit(Instruction::WriteLocal(local), span)?;
            scope = scope.with(
                identifier.name.as_str(),
                Reference::Local {
                    index: local,
                    value_type,
                    is_constant,
                },
            );
        }
        Ok(scope)
    }

    /// Whether an expression is `test ? a : b` with both branches known now, such as a format pattern.
    fn is_choice_between_constants(&self, expression: &'a Expression<'a>, scope: &Rc<Scope<'a>>) -> bool {
        match strip_parentheses(expression) {
            Expression::ConditionalExpression(conditional) => {
                let is_known = |branch: &'a Expression<'a>| {
                    self.evaluate_constant(branch, scope).is_some()
                        || self.is_choice_between_constants(branch, scope)
                };
                is_known(&conditional.consequent) && is_known(&conditional.alternate)
            }
            _ => false,
        }
    }

    /// Whether an expression reads only constants and `const` locals, so it can be compiled where used.
    fn is_pure_of_constants(
        &self,
        expression: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        depth: usize,
    ) -> bool {
        const DEEPEST_ALIAS: usize = 4;
        if depth > DEEPEST_ALIAS {
            return false;
        }
        if self.evaluate_constant(expression, scope).is_some() {
            return true;
        }
        match strip_parentheses(expression) {
            Expression::BinaryExpression(binary) => {
                self.is_pure_of_constants(&binary.left, scope, depth + 1)
                    && self.is_pure_of_constants(&binary.right, scope, depth + 1)
            }
            Expression::LogicalExpression(logical) => {
                self.is_pure_of_constants(&logical.left, scope, depth + 1)
                    && self.is_pure_of_constants(&logical.right, scope, depth + 1)
            }
            Expression::UnaryExpression(unary) => {
                self.is_pure_of_constants(&unary.argument, scope, depth + 1)
            }
            Expression::Identifier(_) => match self.resolve_reference(expression, scope) {
                Some(Reference::Local { is_constant, .. }) => is_constant,
                Some(Reference::EventField { .. } | Reference::ProgressionElement { .. }) => true,
                Some(Reference::Expression { expression, scope }) => {
                    self.is_pure_of_constants(expression, &scope, depth + 1)
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Whether reading an expression twice gives the same as reading it once, cheaply.
    fn is_cheap_read(&self, expression: &'a Expression<'a>, scope: &Rc<Scope<'a>>) -> bool {
        match strip_parentheses(expression) {
            Expression::StaticMemberExpression(member) if member.property.name == "value" => matches!(
                self.resolve_reference(&member.object, scope),
                Some(Reference::Signal(_) | Reference::System(_))
            ),
            Expression::ComputedMemberExpression(member) => {
                matches!(
                    self.resolve_reference(&member.object, scope),
                    Some(Reference::StateArray(_))
                ) && (self.evaluate_constant(&member.expression, scope).is_some()
                    || self.is_cheap_read(&member.expression, scope))
            }
            other => matches!(
                self.resolve_reference(other, scope),
                Some(
                    Reference::Local { .. }
                        | Reference::EventField { .. }
                        | Reference::StateElement { .. }
                        | Reference::ProgressionElement { .. }
                )
            ),
        }
    }

    /// The method name of a call such as `cells.map(...)`, for error messages.
    fn method_name(&self, expression: &'a Expression<'a>) -> Option<String> {
        match strip_parentheses(expression) {
            Expression::CallExpression(call) => match strip_parentheses(&call.callee) {
                Expression::StaticMemberExpression(member) => Some(member.property.name.as_str().to_string()),
                _ => None,
            },
            _ => None,
        }
    }

    // ------------------------------------------------------------------------
    // Expression statements
    // ------------------------------------------------------------------------

    pub fn compile_expression_statement(
        &mut self,
        emitter: &mut Emitter,
        expression: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
    ) -> Compiled<()> {
        let span = expression.span();
        match strip_parentheses(expression) {
            Expression::AssignmentExpression(assignment) => {
                self.compile_assignment(emitter, assignment, scope)
            }
            Expression::UpdateExpression(update) => {
                let operator = match update.operator {
                    UpdateOperator::Increment => Instruction::Add,
                    UpdateOperator::Decrement => Instruction::Subtract,
                };
                self.compile_update(emitter, &update.argument, operator, scope, span)
            }
            Expression::CallExpression(call) => {
                self.compile_call(emitter, call, scope, None, true)?;
                Ok(())
            }
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::CallExpression(call) => {
                    self.compile_call(emitter, call, scope, None, true)?;
                    Ok(())
                }
                _ => fail(span, "this expression does nothing as a statement"),
            },
            Expression::LogicalExpression(logical) if logical.operator != LogicalOperator::Coalesce => {
                let end = emitter.label();
                self.compile_condition(emitter, &logical.left, scope)?;
                if logical.operator == LogicalOperator::Or {
                    emitter.emit(Instruction::Not, span)?;
                }
                emitter.jump_if_false(end, span)?;
                self.compile_expression_statement(emitter, &logical.right, scope)?;
                emitter.place(end);
                Ok(())
            }
            Expression::ConditionalExpression(conditional) => {
                let otherwise = emitter.label();
                let end = emitter.label();
                self.compile_condition(emitter, &conditional.test, scope)?;
                emitter.jump_if_false(otherwise, span)?;
                self.compile_expression_statement(emitter, &conditional.consequent, scope)?;
                emitter.jump(end, span)?;
                emitter.place(otherwise);
                self.compile_expression_statement(emitter, &conditional.alternate, scope)?;
                emitter.place(end);
                Ok(())
            }
            Expression::SequenceExpression(sequence) => {
                for inner in &sequence.expressions {
                    self.compile_expression_statement(emitter, inner, scope)?;
                }
                Ok(())
            }
            Expression::Identifier(identifier) if identifier.name == "undefined" => Ok(()),
            _ => fail(span, "this expression does nothing as a statement"),
        }
    }

    fn compile_update(
        &mut self,
        emitter: &mut Emitter,
        target: &'a SimpleAssignmentTarget<'a>,
        operator: Instruction,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<()> {
        let one = Constant::Number(1);
        match target {
            SimpleAssignmentTarget::AssignmentTargetIdentifier(identifier) => {
                match self.lookup(scope, identifier.name.as_str()) {
                    Some(Reference::Local {
                        index,
                        value_type,
                        is_constant: false,
                    }) if value_type.kind == Kind::Number => {
                        emitter.emit(Instruction::ReadLocal(index), span)?;
                        self.emit_constant(emitter, &one, None, span)?;
                        emitter.emit(operator, span)?;
                        emitter.emit(Instruction::WriteLocal(index), span)
                    }
                    _ => fail(
                        span,
                        "only a number in a `let` or a signal's `.value` can be counted up or down",
                    ),
                }
            }
            SimpleAssignmentTarget::StaticMemberExpression(member) if member.property.name == "value" => {
                let Some(Reference::Signal(variable)) = self.resolve_reference(&member.object, scope) else {
                    return fail(span, "only a signal's `.value` can be counted up or down");
                };
                self.require_state_write(emitter, span)?;
                self.require_scalar_number(variable, span)?;
                emitter.emit(Instruction::ReadState(variable), span)?;
                self.emit_constant(emitter, &one, None, span)?;
                emitter.emit(operator, span)?;
                self.note_write(variable, Range::ANY);
                emitter.emit(Instruction::WriteState(variable), span)
            }
            SimpleAssignmentTarget::ComputedMemberExpression(member) => {
                let Some(Reference::StateArray(variable)) = self.resolve_reference(&member.object, scope)
                else {
                    return fail(
                        span,
                        "only an element of an array signal can be counted up or down",
                    );
                };
                self.require_state_write(emitter, span)?;
                let mark = emitter.locals_in_use();
                let index = emitter.take_local(span)?;
                self.compile_value_of_type(emitter, &member.expression, scope, Type::NUMBER)?;
                emitter.emit(Instruction::WriteLocal(index), span)?;
                emitter.emit(Instruction::ReadLocal(index), span)?;
                emitter.emit(Instruction::ReadLocal(index), span)?;
                emitter.emit(Instruction::ReadElement(variable), span)?;
                self.emit_constant(emitter, &one, None, span)?;
                emitter.emit(operator, span)?;
                emitter.emit(Instruction::WriteElement(variable), span)?;
                emitter.release_locals(mark);
                self.note_write(variable, Range::ANY);
                Ok(())
            }
            _ => fail(
                span,
                "only a number in a `let` or a signal's `.value` can be counted up or down",
            ),
        }
    }

    fn require_state_write(&self, emitter: &Emitter, span: Span) -> Compiled<()> {
        if !emitter.is_handler() {
            return fail(
                span,
                "state may only be changed in a handler, never while working out what to show",
            );
        }
        Ok(())
    }

    fn require_scalar_number(&self, variable: u8, span: Span) -> Compiled<()> {
        let slot = &self.slots[variable as usize];
        if slot.value_type.kind != Kind::Number || slot.value_type.nullable {
            return fail(span, format!("`{}` does not hold a number", slot.key));
        }
        Ok(())
    }

    // ------------------------------------------------------------------------
    // Assignments
    // ------------------------------------------------------------------------

    fn compile_assignment(
        &mut self,
        emitter: &mut Emitter,
        assignment: &'a AssignmentExpression<'a>,
        scope: &Rc<Scope<'a>>,
    ) -> Compiled<()> {
        let span = assignment.span;
        let compound = match assignment.operator {
            AssignmentOperator::Assign => None,
            operator => match compound_operator(operator) {
                Some(BinaryOperator::Division) => {
                    return fail(
                        span,
                        "`/=` would give a fraction; write `x = Math.trunc(x / y)` to say how it rounds",
                    )
                }
                Some(operator) => Some(operator),
                None => return fail(span, "this kind of assignment is not supported"),
            },
        };
        let compound_instruction = compound.map(binary_instruction);

        match &assignment.left {
            AssignmentTarget::AssignmentTargetIdentifier(identifier) => {
                let name = identifier.name.as_str();
                match self.lookup(scope, name) {
                    Some(Reference::Local {
                        index,
                        value_type,
                        is_constant: false,
                    }) => {
                        if let Some(instruction) = compound_instruction {
                            emitter.emit(Instruction::ReadLocal(index), span)?;
                            self.compile_value_of_type(emitter, &assignment.right, scope, Type::NUMBER)?;
                            emitter.emit(instruction?, span)?;
                        } else {
                            let typed =
                                self.compile_value(emitter, &assignment.right, scope, Some(value_type))?;
                            if !typed.value_type.fits(value_type) {
                                return fail(
                                    span,
                                    format!(
                                        "`{name}` holds {}, but this is {}",
                                        value_type.describe(),
                                        typed.value_type.describe()
                                    ),
                                );
                            }
                        }
                        emitter.emit(Instruction::WriteLocal(index), span)
                    }
                    Some(Reference::Local { .. }) | Some(Reference::Constant(_)) => fail(
                        span,
                        format!("`{name}` is a `const`; declare it with `let` to change it"),
                    ),
                    Some(Reference::Signal(_)) => {
                        fail(span, format!("assign to `{name}.value`, not to `{name}` itself"))
                    }
                    _ => fail(span, format!("`{name}` cannot be assigned to")),
                }
            }
            AssignmentTarget::StaticMemberExpression(member) => {
                let object = self.resolve_reference(&member.object, scope);
                if let Some(Reference::RecordElement { fields, index }) = &object {
                    return self.compile_record_field_write(
                        emitter,
                        fields,
                        index,
                        member.property.name.as_str(),
                        &assignment.right,
                        compound_instruction,
                        scope,
                        span,
                    );
                }
                if let Some(Reference::RecordArray(_)) = &object {
                    return fail(
                        span,
                        "an array of records cannot be assigned whole; write its records or fields one at a time",
                    );
                }
                if let Some(Reference::RecordValue(fields)) = &object {
                    self.require_state_write(emitter, span)?;
                    let Some((_, variable)) = fields
                        .iter()
                        .find(|(field, _)| *field == member.property.name.as_str())
                    else {
                        return fail(
                            span,
                            format!("this record has no field `{}`", member.property.name.as_str()),
                        );
                    };
                    let slot = self.slots[*variable as usize].clone();
                    let range = if let Some(instruction) = compound_instruction {
                        emitter.emit(Instruction::ReadState(*variable), span)?;
                        self.compile_value_of_type(emitter, &assignment.right, scope, Type::NUMBER)?;
                        emitter.emit(instruction?, span)?;
                        Range::ANY
                    } else {
                        let typed =
                            self.compile_value(emitter, &assignment.right, scope, Some(slot.value_type))?;
                        self.check_stored_type(typed, &slot, span)?;
                        typed.range
                    };
                    self.note_write(*variable, range);
                    return emitter.emit(Instruction::WriteState(*variable), span);
                }
                if let (Some(Reference::Record(fields)), "value") = (&object, member.property.name.as_str()) {
                    self.require_state_write(emitter, span)?;
                    if compound.is_some() {
                        return fail(span, "a whole record can only be assigned with `=`");
                    }
                    return self.compile_record_assignment(emitter, fields, &assignment.right, scope, span);
                }
                if member.property.name != "value" {
                    return fail(
                        span,
                        "only a signal's `.value` or an element of an array signal can be assigned to; objects in state are not supported yet",
                    );
                }
                let Some(Reference::Signal(variable)) = self.resolve_reference(&member.object, scope) else {
                    return fail(span, "only a signal's `.value` can be assigned to");
                };
                self.require_state_write(emitter, span)?;
                let slot = self.slots[variable as usize].clone();
                if slot.is_array() {
                    if compound.is_some() {
                        return fail(span, "a whole array can only be assigned with `=`");
                    }
                    return self.compile_array_assignment(emitter, variable, &assignment.right, scope, span);
                }
                let range = if let Some(instruction) = compound_instruction {
                    self.require_scalar_number(variable, span)?;
                    emitter.emit(Instruction::ReadState(variable), span)?;
                    self.compile_value_of_type(emitter, &assignment.right, scope, Type::NUMBER)?;
                    emitter.emit(instruction?, span)?;
                    Range::ANY
                } else {
                    let typed =
                        self.compile_value(emitter, &assignment.right, scope, Some(slot.value_type))?;
                    self.check_stored_type(typed, &slot, span)?;
                    typed.range
                };
                self.note_write(variable, range);
                emitter.emit(Instruction::WriteState(variable), span)
            }
            AssignmentTarget::ComputedMemberExpression(member) => {
                if let Some(Reference::RecordArrayValue(fields)) =
                    self.resolve_reference(&member.object, scope)
                {
                    self.require_state_write(emitter, span)?;
                    if compound.is_some() {
                        return fail(span, "a whole record can only be assigned with `=`");
                    }
                    return self.compile_record_element_assignment(
                        emitter,
                        &fields,
                        &member.expression,
                        &assignment.right,
                        scope,
                        span,
                    );
                }
                let Some(Reference::StateArray(variable)) = self.resolve_reference(&member.object, scope)
                else {
                    return fail(span, "only an element of an array signal can be assigned to, as in `cells.value[index] = 1`");
                };
                self.require_state_write(emitter, span)?;
                let slot = self.slots[variable as usize].clone();
                let mark = emitter.locals_in_use();
                let typed = if let Some(instruction) = compound_instruction {
                    let index = emitter.take_local(span)?;
                    self.compile_value_of_type(emitter, &member.expression, scope, Type::NUMBER)?;
                    emitter.emit(Instruction::WriteLocal(index), span)?;
                    emitter.emit(Instruction::ReadLocal(index), span)?;
                    emitter.emit(Instruction::ReadLocal(index), span)?;
                    emitter.emit(Instruction::ReadElement(variable), span)?;
                    self.compile_value_of_type(emitter, &assignment.right, scope, Type::NUMBER)?;
                    emitter.emit(instruction?, span)?;
                    Typed::new(Type::NUMBER)
                } else {
                    self.compile_value_of_type(emitter, &member.expression, scope, Type::NUMBER)?;
                    let typed =
                        self.compile_value(emitter, &assignment.right, scope, Some(slot.value_type))?;
                    self.check_stored_type(typed, &slot, span)?;
                    typed
                };
                emitter.emit(Instruction::WriteElement(variable), span)?;
                emitter.release_locals(mark);
                self.note_write(variable, typed.range);
                Ok(())
            }
            _ => fail(
                span,
                "only a `let`, a signal's `.value` or an element of an array signal can be assigned to",
            ),
        }
    }

    /// Assigns a whole record: every field's new value is worked out before any is written.
    fn compile_record_assignment(
        &mut self,
        emitter: &mut Emitter,
        fields: &[(String, u8)],
        value: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<()> {
        let sources = self.record_sources(value, scope, span)?;
        for (field, variable) in fields {
            let Some(source) = sources
                .iter()
                .rev()
                .find(|(name, _)| name == field)
                .map(|(_, source)| source.clone())
            else {
                return fail(span, format!("the new value has no field `{field}`"));
            };
            let slot = self.slots[*variable as usize].clone();
            let typed = match source {
                Reference::Expression { expression, scope } => {
                    self.compile_value(emitter, expression, &scope, Some(slot.value_type))?
                }
                other => self.compile_reference_value(emitter, other, span, Some(slot.value_type))?,
            };
            self.check_stored_type(typed, &slot, span)?;
            self.note_write(*variable, typed.range);
        }
        for (_, variable) in fields.iter().rev() {
            emitter.emit(Instruction::WriteState(*variable), span)?;
        }
        Ok(())
    }

    /// Writes one field of a record in an array of records, which is one element write.
    #[allow(clippy::too_many_arguments)]
    fn compile_record_field_write(
        &mut self,
        emitter: &mut Emitter,
        fields: &[(String, u8)],
        index: &ElementIndex<'a>,
        name: &str,
        value: &'a Expression<'a>,
        compound_instruction: Option<Compiled<Instruction>>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<()> {
        self.require_state_write(emitter, span)?;
        let Some((_, variable)) = fields.iter().find(|(field, _)| field == name) else {
            return fail(span, format!("this record has no field `{name}`"));
        };
        let variable = *variable;
        let slot = self.slots[variable as usize].clone();
        let mark = emitter.locals_in_use();
        let range = if let Some(instruction) = compound_instruction {
            let local = emitter.take_local(span)?;
            self.emit_element_index(emitter, index, span)?;
            emitter.emit(Instruction::WriteLocal(local), span)?;
            emitter.emit(Instruction::ReadLocal(local), span)?;
            emitter.emit(Instruction::ReadLocal(local), span)?;
            emitter.emit(Instruction::ReadElement(variable), span)?;
            self.compile_value_of_type(emitter, value, scope, Type::NUMBER)?;
            emitter.emit(instruction?, span)?;
            Range::ANY
        } else {
            self.emit_element_index(emitter, index, span)?;
            let typed = self.compile_value(emitter, value, scope, Some(slot.value_type))?;
            self.check_stored_type(typed, &slot, span)?;
            typed.range
        };
        emitter.emit(Instruction::WriteElement(variable), span)?;
        emitter.release_locals(mark);
        self.note_write(variable, range);
        Ok(())
    }

    /// Assigns one whole record of an array of records: every field is worked out, then written.
    fn compile_record_element_assignment(
        &mut self,
        emitter: &mut Emitter,
        fields: &[(String, u8)],
        index: &'a Expression<'a>,
        value: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<()> {
        let sources = self.record_sources(value, scope, span)?;
        let mark = emitter.locals_in_use();
        let index_local = emitter.take_local(span)?;
        self.compile_value_of_type(emitter, index, scope, Type::NUMBER)?;
        emitter.emit(Instruction::WriteLocal(index_local), span)?;
        for (field, variable) in fields {
            let Some(source) = sources
                .iter()
                .rev()
                .find(|(name, _)| name == field)
                .map(|(_, source)| source.clone())
            else {
                return fail(span, format!("the new value has no field `{field}`"));
            };
            let slot = self.slots[*variable as usize].clone();
            emitter.emit(Instruction::ReadLocal(index_local), span)?;
            let typed = match source {
                Reference::Expression { expression, scope } => {
                    self.compile_value(emitter, expression, &scope, Some(slot.value_type))?
                }
                other => self.compile_reference_value(emitter, other, span, Some(slot.value_type))?,
            };
            self.check_stored_type(typed, &slot, span)?;
            self.note_write(*variable, typed.range);
        }
        for (_, variable) in fields.iter().rev() {
            emitter.emit(Instruction::WriteElement(*variable), span)?;
        }
        emitter.release_locals(mark);
        Ok(())
    }

    /// The fields a record is assigned from, later fields overriding earlier ones.
    pub fn record_sources(
        &mut self,
        value: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<Vec<(String, Reference<'a>)>> {
        if let Some(Constant::Object(fields)) = self.evaluate_constant(value, scope) {
            return Ok(fields
                .iter()
                .map(|(name, field)| (name.clone(), Reference::Constant(field.clone())))
                .collect());
        }
        if let Some(reference) = self.resolve_reference(value, scope) {
            return match reference {
                Reference::RecordValue(fields) => Ok(fields
                    .iter()
                    .map(|(name, variable)| (name.clone(), Reference::StateScalar(*variable)))
                    .collect()),
                Reference::RecordElement { fields, index } => Ok(fields
                    .iter()
                    .map(|(name, variable)| {
                        let field = match &index {
                            ElementIndex::Constant(index) => Reference::StateElement {
                                variable: *variable,
                                index: *index,
                            },
                            other => Reference::StateElementAt {
                                variable: *variable,
                                index: other.clone(),
                            },
                        };
                        (name.clone(), field)
                    })
                    .collect()),
                Reference::Object(fields) => Ok(fields.to_vec()),
                Reference::Expression { expression, scope } => self.record_sources(expression, &scope, span),
                _ => fail(span, "this is not a record"),
            };
        }
        let Expression::ObjectExpression(object) = strip_parentheses(value) else {
            return fail(
                span,
                "assign a record from an object literal such as `{ ...time.value, hour: 7 }`",
            );
        };
        let mut sources = Vec::new();
        for property in &object.properties {
            match property {
                oxc_ast::ast::ObjectPropertyKind::SpreadProperty(spread) => {
                    sources.extend(self.record_sources(&spread.argument, scope, span)?);
                }
                oxc_ast::ast::ObjectPropertyKind::ObjectProperty(property) => {
                    let Some(name) = static_key(&property.key) else {
                        return fail(property.span, "give each field a plain name");
                    };
                    sources.push((
                        name,
                        Reference::Expression {
                            expression: &property.value,
                            scope: scope.clone(),
                        },
                    ));
                }
            }
        }
        Ok(sources)
    }

    /// Checks a value may be stored in a state variable.
    fn check_stored_type(&self, typed: Typed, slot: &StateSlot, span: Span) -> Compiled<()> {
        if typed.value_type.kind == Kind::Text && slot.value_type.is_text() {
            return fail(
                span,
                "only fixed text can be kept in state, because text made while running does not last; keep the numbers and format them where they are shown",
            );
        }
        if !typed.value_type.fits(slot.value_type) {
            return fail(
                span,
                format!(
                    "`{}` holds {}, but this is {}",
                    slot.key,
                    slot.value_type.describe(),
                    typed.value_type.describe()
                ),
            );
        }
        Ok(())
    }

    // ------------------------------------------------------------------------
    // Whole arrays
    // ------------------------------------------------------------------------

    /// Assigns a whole new array. The new value is worked out from the old one, as in JavaScript.
    fn compile_array_assignment(
        &mut self,
        emitter: &mut Emitter,
        target: u8,
        value: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<()> {
        let slot = self.slots[target as usize].clone();

        if let Some(constant) = self.evaluate_constant(value, scope) {
            let Constant::Array(elements) = constant else {
                return fail(
                    span,
                    format!("`{}` holds an array, but this is not one", slot.key),
                );
            };
            self.assign_constant_array(emitter, target, &elements, span)?;
            return Ok(());
        }

        if let Some(reference) = self.resolve_reference(value, scope) {
            match reference {
                Reference::StateArray(source) => {
                    let source_slot = &self.slots[source as usize];
                    check_length(&slot, source_slot.length as usize, span)?;
                    if source != target {
                        emitter.emit(
                            Instruction::CopyArray {
                                destination: target,
                                source,
                            },
                            span,
                        )?;
                        self.copied_into.push((target, source));
                    }
                    return Ok(());
                }
                Reference::Expression { expression, scope } => {
                    return self.compile_array_assignment(emitter, target, expression, &scope, span);
                }
                _ => {}
            }
        }

        match strip_parentheses(value) {
            Expression::ArrayExpression(array) => {
                let elements: Vec<&'a Expression<'a>> = array
                    .elements
                    .iter()
                    .map(|element| element.as_expression())
                    .collect::<Option<_>>()
                    .ok_or_else(|| crate::diagnostics::Diagnostic::error(span, "write every element out, without `...` or gaps"))?;
                check_length(&slot, elements.len(), span)?;
                if elements.len() <= LARGEST_ARRAY_LITERAL_ON_STACK {
                    // Every element is worked out from the old array before any is written.
                    for (index, element) in elements.iter().enumerate() {
                        self.emit_number(emitter, index as i64, span)?;
                        let typed = self.compile_value(emitter, element, scope, Some(slot.value_type))?;
                        self.check_stored_type(typed, &slot, element.span())?;
                        self.note_write(target, typed.range);
                    }
                    for _ in &elements {
                        emitter.emit(Instruction::WriteElement(target), span)?;
                    }
                    return Ok(());
                }
                let rebuild = self.rebuild_array(target, span)?;
                for (index, element) in elements.iter().enumerate() {
                    self.emit_number(emitter, index as i64, span)?;
                    let typed = self.compile_value(emitter, element, scope, Some(slot.value_type))?;
                    self.check_stored_type(typed, &slot, element.span())?;
                    emitter.emit(Instruction::WriteElement(rebuild), span)?;
                    self.note_write(rebuild, typed.range);
                }
                self.copy_rebuilt(emitter, target, rebuild, span)
            }
            Expression::ComputedMemberExpression(member)
                if matches!(
                    self.evaluate_constant(&member.object, scope),
                    Some(Constant::Array(ref rows)) if rows.iter().all(|row| matches!(row, Constant::Array(_)))
                ) =>
            {
                // A row of a constant table chosen at run time: one test and assignment per row.
                let Some(Constant::Array(rows)) = self.evaluate_constant(&member.object, scope) else {
                    return fail(span, "this table is not known when compiling");
                };
                let mark = emitter.locals_in_use();
                let index_local = emitter.take_local(span)?;
                self.compile_value_of_type(emitter, &member.expression, scope, Type::NUMBER)?;
                emitter.emit(Instruction::WriteLocal(index_local), span)?;
                for (position, row) in rows.iter().enumerate() {
                    let Constant::Array(elements) = row else {
                        continue;
                    };
                    let next = emitter.label();
                    emitter.emit(Instruction::ReadLocal(index_local), span)?;
                    self.emit_number(emitter, position as i64, span)?;
                    emitter.emit(Instruction::Equal, span)?;
                    emitter.jump_if_false(next, span)?;
                    self.assign_constant_array(emitter, target, elements, span)?;
                    emitter.place(next);
                }
                emitter.release_locals(mark);
                Ok(())
            }
            Expression::ConditionalExpression(conditional) => {
                let otherwise = emitter.label();
                let end = emitter.label();
                self.compile_condition(emitter, &conditional.test, scope)?;
                emitter.jump_if_false(otherwise, span)?;
                self.compile_array_assignment(emitter, target, &conditional.consequent, scope, span)?;
                emitter.jump(end, span)?;
                emitter.place(otherwise);
                self.compile_array_assignment(emitter, target, &conditional.alternate, scope, span)?;
                emitter.place(end);
                Ok(())
            }
            Expression::CallExpression(call) => {
                if let Expression::StaticMemberExpression(member) = strip_parentheses(&call.callee) {
                    if member.property.name == "map" {
                        let Some(callback) = call.arguments.first().and_then(|argument| argument.as_expression()) else {
                            return fail(span, "give `.map` a function");
                        };
                        return self.compile_array_map(emitter, target, &member.object, callback, scope, span);
                    }
                }
                match self.resolve_reference(&call.callee, scope) {
                    Some(Reference::Function(function)) => {
                        let arguments = call
                            .arguments
                            .iter()
                            .filter_map(|argument| argument.as_expression())
                            .map(|argument| ArgumentSource::Expression(argument, scope.clone()))
                            .collect();
                        self.inline_array_function(emitter, target, &function, arguments, span)
                    }
                    _ => fail(span, "this call does not make an array the watch can build"),
                }
            }
            _ => fail(
                span,
                "this array cannot be built on the watch; use an array literal, `.map` over a fixed-length array, or another array signal",
            ),
        }
    }

    /// Writes a constant array into `target`, with one `FillArray` when every element is the same.
    fn assign_constant_array(
        &mut self,
        emitter: &mut Emitter,
        target: u8,
        elements: &[Constant],
        span: Span,
    ) -> Compiled<()> {
        let slot = self.slots[target as usize].clone();
        check_length(&slot, elements.len(), span)?;
        if Constant::is_uniform_array(elements) {
            let typed = self.emit_constant(emitter, &elements[0], Some(slot.value_type), span)?;
            self.check_stored_type(typed, &slot, span)?;
            emitter.emit(Instruction::FillArray(target), span)?;
            self.note_write(target, typed.range);
            return Ok(());
        }
        for (index, element) in elements.iter().enumerate() {
            self.emit_number(emitter, index as i64, span)?;
            let typed = self.emit_constant(emitter, element, Some(slot.value_type), span)?;
            self.check_stored_type(typed, &slot, span)?;
            emitter.emit(Instruction::WriteElement(target), span)?;
            self.note_write(target, typed.range);
        }
        Ok(())
    }

    /// Inlines a function that ends with `return` of an array, writing that array into `target`.
    fn inline_array_function(
        &mut self,
        emitter: &mut Emitter,
        target: u8,
        function: &FunctionReference<'a>,
        arguments: Vec<ArgumentSource<'a>>,
        span: Span,
    ) -> Compiled<()> {
        let identity = function.identity();
        if self.inline_stack.contains(&identity) {
            return fail(
                span,
                "recursion is not available; use `for ... of` over a fixed-length array",
            );
        }
        self.inline_stack.push(identity);
        let mark = emitter.locals_in_use();
        let result = (|| {
            let scope = self.bind_parameters(emitter, function, arguments, span)?;
            match function.body {
                FunctionBodyReference::Expression(expression) => {
                    self.compile_array_assignment(emitter, target, expression, &scope, span)
                }
                FunctionBodyReference::Statements(body) => {
                    let Some((Statement::ReturnStatement(last), before)) = body.statements.split_last()
                    else {
                        return fail(
                            function.span,
                            "a function that makes an array must end with `return` of the array",
                        );
                    };
                    let Some(returned) = &last.argument else {
                        return fail(last.span, "return the array");
                    };
                    let mut scope = scope.clone();
                    for statement in before {
                        match statement {
                            Statement::VariableDeclaration(declaration) => {
                                scope = self.compile_declaration(emitter, declaration, &scope)?;
                            }
                            Statement::FunctionDeclaration(_) | Statement::EmptyStatement(_) => {}
                            other => {
                                return fail(
                                    other.span(),
                                    "a function that makes an array may only declare constants before its `return`",
                                )
                            }
                        }
                    }
                    self.compile_array_assignment(emitter, target, returned, &scope, span)
                }
            }
        })();
        emitter.release_locals(mark);
        self.inline_stack.pop();
        result
    }

    /// `array.map(callback)` assigned to `target`: built in a scratch array, then copied in.
    fn compile_array_map(
        &mut self,
        emitter: &mut Emitter,
        target: u8,
        source: &'a Expression<'a>,
        callback: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<()> {
        let slot = self.slots[target as usize].clone();
        let elements = self.loop_source(source, scope, span)?;
        check_length(&slot, elements.length(), span)?;
        let function = match self.resolve_reference(callback, scope) {
            Some(Reference::Function(function)) => function,
            _ => return fail(callback.span(), "give `.map` a function, such as `(cell) => ...`"),
        };
        let rebuild = self.rebuild_array(target, span)?;

        let mark = emitter.locals_in_use();
        let index_local = emitter.take_local(span)?;
        let value_local = emitter.take_local(span)?;
        self.emit_number(emitter, 0, span)?;
        emitter.emit(Instruction::WriteLocal(index_local), span)?;
        let start = emitter.loop_start(span)?;
        let exit = emitter.label();
        emitter.emit(Instruction::ReadLocal(index_local), span)?;
        self.emit_number(emitter, elements.length() as i64, span)?;
        emitter.emit(Instruction::Less, span)?;
        emitter.jump_if_false(exit, span)?;

        let element = self.loop_element(emitter, &elements, index_local, span)?;
        let arguments = vec![
            ArgumentSource::Reference(element),
            ArgumentSource::Reference(Reference::Local {
                index: index_local,
                value_type: Type::NUMBER,
                is_constant: true,
            }),
        ];
        let Some(typed) =
            self.inline_call(emitter, &function, arguments, true, Some(slot.value_type), span)?
        else {
            return fail(callback.span(), "the function given to `.map` must give a value");
        };
        self.check_stored_type(typed, &slot, callback.span())?;
        emitter.emit(Instruction::WriteLocal(value_local), span)?;
        emitter.emit(Instruction::ReadLocal(index_local), span)?;
        emitter.emit(Instruction::ReadLocal(value_local), span)?;
        emitter.emit(Instruction::WriteElement(rebuild), span)?;
        self.note_write(rebuild, typed.range);
        self.emit_increment(emitter, index_local, span)?;
        emitter.loop_back(start, span)?;
        emitter.place(exit);
        emitter.release_locals(mark);
        self.copy_rebuilt(emitter, target, rebuild, span)
    }

    fn copy_rebuilt(&mut self, emitter: &mut Emitter, target: u8, rebuild: u8, span: Span) -> Compiled<()> {
        emitter.emit(
            Instruction::CopyArray {
                destination: target,
                source: rebuild,
            },
            span,
        )?;
        self.copied_into.push((target, rebuild));
        self.scratch_in_use.retain(|in_use| *in_use != rebuild);
        Ok(())
    }

    /// The scratch array a whole new value for `target` is built in, made the first time it is needed.
    fn rebuild_array(&mut self, target: u8, span: Span) -> Compiled<u8> {
        let slot = self.slots[target as usize].clone();
        let shape = ScratchShape {
            length: slot.length,
            value_type: slot.value_type,
            hint: slot.hint,
            element_bytes: slot.element_bytes,
        };
        let free = self
            .scratch_arrays
            .iter()
            .find(|(existing, rebuild)| *existing == shape && !self.scratch_in_use.contains(rebuild))
            .map(|(_, rebuild)| *rebuild);
        let rebuild = match free {
            Some(rebuild) => rebuild,
            None => {
                let key = format!("~new{}", self.scratch_arrays.len());
                let element_bytes = self.element_bytes_for(
                    &key,
                    crate::compiler::DeclaredType {
                        value_type: slot.value_type,
                        hint: slot.hint,
                        is_array: true,
                    },
                    slot.start_range(),
                );
                let rebuild = self.add_slot(StateSlot {
                    key,
                    persists: false,
                    start_values: None,
                    element_bytes: element_bytes.max(slot.element_bytes),
                    span,
                    ..slot
                })?;
                self.scratch_arrays.push((shape, rebuild));
                rebuild
            }
        };
        self.scratch_in_use.push(rebuild);
        Ok(rebuild)
    }

    // ------------------------------------------------------------------------
    // Loops
    // ------------------------------------------------------------------------

    /// `includes`, `indexOf` and `count_where`: a loop over an array signal or a constant array.
    pub fn compile_array_search(
        &mut self,
        emitter: &mut Emitter,
        array: &'a Expression<'a>,
        argument: &'a Expression<'a>,
        search: Search,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<Typed> {
        let spill_mark = emitter.locals_in_use();
        let spilled = self.spill_whole_stack(emitter, span)?;
        let mark = emitter.locals_in_use();
        let source = self.loop_source(array, scope, array.span())?;
        let result_local = emitter.take_local(span)?;
        let index_local = emitter.take_local(span)?;
        let start_value = if search == Search::IndexOf { -1 } else { 0 };
        self.emit_number(emitter, start_value, span)?;
        emitter.emit(Instruction::WriteLocal(result_local), span)?;

        let (value_local, test) = match search {
            Search::CountWhere => match self.resolve_reference(argument, scope) {
                Some(Reference::Function(function)) => (None, Some(function)),
                _ => {
                    return fail(
                        argument.span(),
                        "give `count_where` a test, such as `(cell) => cell === 1`",
                    )
                }
            },
            _ => {
                let local = emitter.take_local(span)?;
                self.compile_value(emitter, argument, scope, None)?;
                emitter.emit(Instruction::WriteLocal(local), span)?;
                (Some(local), None)
            }
        };

        self.emit_number(emitter, 0, span)?;
        emitter.emit(Instruction::WriteLocal(index_local), span)?;
        let start = emitter.loop_start(span)?;
        let exit = emitter.label();
        let next = emitter.label();
        emitter.emit(Instruction::ReadLocal(index_local), span)?;
        self.emit_number(emitter, source.length() as i64, span)?;
        emitter.emit(Instruction::Less, span)?;
        emitter.jump_if_false(exit, span)?;
        let element = self.loop_element(emitter, &source, index_local, span)?;
        match (value_local, &test) {
            (Some(value_local), _) => {
                let typed = self.compile_reference_value(emitter, element, span, None)?;
                if typed.value_type.kind == Kind::Text {
                    return fail(span, "text made while running cannot be compared; search an array of numbers or fixed text");
                }
                emitter.emit(Instruction::ReadLocal(value_local), span)?;
                emitter.emit(Instruction::Equal, span)?;
                emitter.jump_if_false(next, span)?;
                match search {
                    Search::IndexOf => emitter.emit(Instruction::ReadLocal(index_local), span)?,
                    _ => self.emit_number(emitter, 1, span)?,
                }
                emitter.emit(Instruction::WriteLocal(result_local), span)?;
                emitter.jump(exit, span)?;
            }
            (None, Some(test)) => {
                let arguments = vec![
                    ArgumentSource::Reference(element),
                    ArgumentSource::Reference(Reference::Local {
                        index: index_local,
                        value_type: Type::NUMBER,
                        is_constant: true,
                    }),
                ];
                let Some(typed) = self.inline_call(emitter, test, arguments, true, Some(Type::BOOL), span)?
                else {
                    return fail(argument.span(), "the test must give a value");
                };
                self.emit_truth_test(emitter, typed.value_type, span)?;
                emitter.jump_if_false(next, span)?;
                self.emit_increment(emitter, result_local, span)?;
            }
            (None, None) => {}
        }
        emitter.place(next);
        self.emit_increment(emitter, index_local, span)?;
        emitter.loop_back(start, span)?;
        emitter.place(exit);
        emitter.release_locals(mark);
        self.restore_stack(emitter, &spilled, false, span)?;
        emitter.emit(Instruction::ReadLocal(result_local), span)?;
        emitter.release_locals(spill_mark);
        Ok(match search {
            Search::Includes => Typed::new(Type::BOOL),
            Search::IndexOf => Typed::with_range(
                Type::NUMBER,
                Range {
                    lowest: -1,
                    highest: source.length() as i64,
                },
            ),
            Search::CountWhere => Typed::with_range(
                Type::NUMBER,
                Range {
                    lowest: 0,
                    highest: source.length() as i64,
                },
            ),
        })
    }

    /// What a `for ... of` or `.map` runs over.
    pub fn loop_source(
        &mut self,
        source: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<LoopSource> {
        if let Some(constant) = self.evaluate_constant(source, scope) {
            return match constant {
                Constant::Array(elements) => Ok(LoopSource::Constant(elements.to_vec())),
                _ => fail(span, "this is not an array"),
            };
        }
        match self.resolve_reference(source, scope) {
            Some(Reference::RecordArrayValue(fields)) => {
                let length = fields
                    .first()
                    .map_or(0, |(_, variable)| self.slots[*variable as usize].length as usize);
                Ok(LoopSource::Records { fields, length })
            }
            Some(Reference::StateArray(variable)) => {
                let slot = &self.slots[variable as usize];
                Ok(LoopSource::State {
                    variable,
                    length: slot.length as usize,
                    value_type: slot.value_type,
                })
            }
            _ => fail(
                span,
                "loops may only run over a fixed-length array: a constant, `range(n)` with a literal `n`, or an array signal",
            ),
        }
    }

    /// What a loop's element stands for: worked out from the index, a record, or a new local.
    pub fn loop_element(
        &mut self,
        emitter: &mut Emitter,
        source: &LoopSource,
        index_local: u8,
        span: Span,
    ) -> Compiled<Reference<'a>> {
        if let LoopSource::Constant(elements) = source {
            if let Some(progression) = Constant::progression(elements) {
                return Ok(Reference::ProgressionElement {
                    index_local,
                    start: progression.start,
                    step: progression.step,
                });
            }
        }
        if let LoopSource::Records { fields, .. } = source {
            return Ok(Reference::RecordElement {
                fields: fields.clone(),
                index: ElementIndex::Local(index_local),
            });
        }
        let element_local = emitter.take_local(span)?;
        let value_type = self.load_loop_element(emitter, source, index_local, element_local, span)?;
        Ok(Reference::Local {
            index: element_local,
            value_type,
            is_constant: true,
        })
    }

    /// Writes the element at the index in `index_local` into `element_local`, and returns its type.
    fn load_loop_element(
        &mut self,
        emitter: &mut Emitter,
        source: &LoopSource,
        index_local: u8,
        element_local: u8,
        span: Span,
    ) -> Compiled<Type> {
        let value_type = match source {
            LoopSource::State {
                variable, value_type, ..
            } => {
                emitter.emit(Instruction::ReadLocal(index_local), span)?;
                emitter.emit(Instruction::ReadElement(*variable), span)?;
                *value_type
            }
            LoopSource::Records { .. } => {
                return fail(span, "a record is not one value; read one of its fields")
            }
            LoopSource::Constant(elements) => {
                if let Some(progression) = Constant::progression(elements) {
                    emitter.emit(Instruction::ReadLocal(index_local), span)?;
                    if progression.step != 1 {
                        self.emit_number(emitter, progression.step, span)?;
                        emitter.emit(Instruction::Multiply, span)?;
                    }
                    if progression.start != 0 {
                        self.emit_number(emitter, progression.start, span)?;
                        emitter.emit(Instruction::Add, span)?;
                    }
                    Type::NUMBER
                } else {
                    self.compile_lookup_chain(emitter, elements, index_local, None, span)?
                        .value_type
                }
            }
        };
        emitter.emit(Instruction::WriteLocal(element_local), span)?;
        Ok(value_type)
    }

    pub fn emit_increment(&mut self, emitter: &mut Emitter, local: u8, span: Span) -> Compiled<()> {
        emitter.emit(Instruction::ReadLocal(local), span)?;
        self.emit_number(emitter, 1, span)?;
        emitter.emit(Instruction::Add, span)?;
        emitter.emit(Instruction::WriteLocal(local), span)
    }

    fn compile_for_of(
        &mut self,
        emitter: &mut Emitter,
        statement: &'a ForOfStatement<'a>,
        scope: &Rc<Scope<'a>>,
    ) -> Compiled<()> {
        let span = statement.span;
        if statement.r#await {
            return fail(span, "`for await` is not available");
        }
        let oxc_ast::ast::ForStatementLeft::VariableDeclaration(declaration) = &statement.left else {
            return fail(
                span,
                "declare the loop's variable with `const`, as in `for (const cell of CELLS)`",
            );
        };
        let Some(BindingPattern::BindingIdentifier(identifier)) =
            declaration.declarations.first().map(|first| &first.id)
        else {
            return fail(span, "give the loop's variable a plain name");
        };
        let source = self.loop_source(&statement.right, scope, statement.right.span())?;
        if source.length() == 0 {
            return Ok(());
        }

        let mark = emitter.locals_in_use();
        let index_local = emitter.take_local(span)?;
        self.emit_number(emitter, 0, span)?;
        emitter.emit(Instruction::WriteLocal(index_local), span)?;
        let start = emitter.loop_start(span)?;
        let exit = emitter.label();
        let next = emitter.label();
        emitter.emit(Instruction::ReadLocal(index_local), span)?;
        self.emit_number(emitter, source.length() as i64, span)?;
        emitter.emit(Instruction::Less, span)?;
        emitter.jump_if_false(exit, span)?;
        let element = self.loop_element(emitter, &source, index_local, span)?;
        let body_scope = scope.with(identifier.name.as_str(), element);
        self.loop_labels.push(LoopLabels { next, exit });
        let body = self.compile_branch(emitter, &statement.body, &body_scope);
        self.loop_labels.pop();
        body?;
        emitter.place(next);
        self.emit_increment(emitter, index_local, span)?;
        emitter.loop_back(start, span)?;
        emitter.place(exit);
        emitter.release_locals(mark);
        Ok(())
    }
}

/// The arrays that can share one scratch array: same length, type and element width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScratchShape {
    pub length: u16,
    pub value_type: Type,
    pub hint: crate::types::NumberHint,
    pub element_bytes: u8,
}

/// What a loop runs over.
pub enum LoopSource {
    Constant(Vec<Constant>),
    State {
        variable: u8,
        length: usize,
        value_type: Type,
    },
    Records {
        fields: Rc<Vec<(String, u8)>>,
        length: usize,
    },
}

impl LoopSource {
    pub fn length(&self) -> usize {
        match self {
            LoopSource::Constant(elements) => elements.len(),
            LoopSource::State { length, .. } | LoopSource::Records { length, .. } => *length,
        }
    }
}

/// What a search over an array works out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Search {
    Includes,
    IndexOf,
    CountWhere,
}

/// Where a loop's `break` and `continue` go.
#[derive(Clone, Copy, Debug)]
pub struct LoopLabels {
    pub next: neotime_runtime::writer::Label,
    pub exit: neotime_runtime::writer::Label,
}

fn check_length(slot: &StateSlot, length: usize, span: Span) -> Compiled<()> {
    if length != slot.length as usize {
        return fail(
            span,
            format!(
                "`{}` holds {} elements, but this array has {length}",
                slot.key, slot.length
            ),
        );
    }
    Ok(())
}

fn binary_instruction(operator: BinaryOperator) -> Compiled<Instruction> {
    Ok(match operator {
        BinaryOperator::Addition => Instruction::Add,
        BinaryOperator::Subtraction => Instruction::Subtract,
        BinaryOperator::Multiplication => Instruction::Multiply,
        BinaryOperator::Remainder => Instruction::Remainder,
        BinaryOperator::ShiftLeft => Instruction::ShiftLeft,
        BinaryOperator::ShiftRight => Instruction::ShiftRight,
        BinaryOperator::BitwiseOR => Instruction::BitOr,
        BinaryOperator::BitwiseXOR => Instruction::BitXor,
        BinaryOperator::BitwiseAnd => Instruction::BitAnd,
        _ => return fail(Span::default(), "this kind of assignment is not supported"),
    })
}
