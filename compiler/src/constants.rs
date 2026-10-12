//! Values known when the app is compiled, and the evaluator that works them out.
//!
//! Anything that reads no state, system value or event is worked out here and stored as a constant:
//! tables such as `CELLS.map(...)`, props passed as literals, `HOUR * 2`. The evaluator runs the
//! same subset handlers use, and gives up quietly on anything else, which the code generator then
//! compiles or reports.

use std::rc::Rc;

use oxc_ast::ast::{
    Argument, ArrayExpressionElement, ArrowFunctionBody, AssignmentTarget, BindingPattern, Expression,
    ForStatementLeft, ObjectPropertyKind, PropertyKey, SimpleAssignmentTarget, Statement,
};
use oxc_syntax::operator::{
    AssignmentOperator, BinaryOperator, LogicalOperator, UnaryOperator, UpdateOperator,
};

use crate::compiler::Compiler;
use crate::scope::{FunctionBodyReference, FunctionReference, LibraryModule, Reference, Scope};
use crate::types::icon_number;

// ============================================================================
// Constants
// ============================================================================

/// A value known at compile time.
#[derive(Clone, Debug, PartialEq)]
pub enum Constant {
    Number(i64),
    Bool(bool),
    Text(Rc<str>),
    Null,
    Undefined,
    Icon(i64),
    Array(Rc<Vec<Constant>>),
    Object(Rc<Vec<(String, Constant)>>),
}

impl Constant {
    pub fn is_truthy(&self) -> bool {
        match self {
            Constant::Number(number) => *number != 0,
            Constant::Bool(value) => *value,
            Constant::Text(text) => !text.is_empty(),
            Constant::Null | Constant::Undefined => false,
            Constant::Icon(_) | Constant::Array(_) | Constant::Object(_) => true,
        }
    }

    pub fn as_number(&self) -> Option<i64> {
        match self {
            Constant::Number(number) => Some(*number),
            _ => None,
        }
    }

    pub fn field(&self, name: &str) -> Option<Constant> {
        match self {
            Constant::Object(fields) => Some(
                fields
                    .iter()
                    .find(|(field, _)| field == name)
                    .map_or(Constant::Undefined, |(_, value)| value.clone()),
            ),
            Constant::Array(elements) if name == "length" => Some(Constant::Number(elements.len() as i64)),
            Constant::Text(text) if name == "length" => Some(Constant::Number(text.chars().count() as i64)),
            _ => None,
        }
    }

    /// The text JavaScript would show for this value in a template literal.
    pub fn display_text(&self) -> Option<String> {
        match self {
            Constant::Number(number) => Some(number.to_string()),
            Constant::Bool(value) => Some(value.to_string()),
            Constant::Text(text) => Some(text.to_string()),
            Constant::Null => Some("null".to_string()),
            _ => None,
        }
    }

    /// Whether every element of an array is the same value.
    pub fn is_uniform_array(elements: &[Constant]) -> bool {
        elements.windows(2).all(|pair| pair[0] == pair[1])
    }

    /// The start and step of an array of numbers that go up evenly, such as `[0, 1, 2]`.
    pub fn progression(elements: &[Constant]) -> Option<Progression> {
        let numbers: Option<Vec<i64>> = elements.iter().map(Constant::as_number).collect();
        let numbers = numbers?;
        let start = *numbers.first()?;
        let step = numbers.get(1).map_or(1, |second| second - start);
        let follows = numbers.iter().enumerate().all(|(position, value)| {
            step.checked_mul(position as i64)
                .and_then(|offset| start.checked_add(offset))
                == Some(*value)
        });
        follows.then_some(Progression { start, step })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progression {
    pub start: i64,
    pub step: i64,
}

/// The constants `neotime` exports.
pub fn library_constant(name: &str) -> Option<Constant> {
    const SECOND: i64 = 1000;
    Some(Constant::Number(match name {
        "SECOND" => SECOND,
        "MINUTE" => 60 * SECOND,
        "HOUR" => 3600 * SECOND,
        "DAY" => 86_400 * SECOND,
        "EVERY_DAY" => 0b111_1111,
        "SCREEN_SIZE" => 240,
        "STEP_HISTORY_DAYS" => 7,
        _ => return None,
    }))
}

// ============================================================================
// Evaluation
// ============================================================================

/// How many steps one evaluation may take before it gives up.
const EVALUATION_STEPS: u32 = 2_000_000;

/// How deep calls may nest while evaluating, which also stops recursion.
const DEEPEST_CALLS: usize = 32;

/// Where names are looked up while evaluating: variables of the function being run, then a scope.
struct Environment<'a> {
    scope: Rc<Scope<'a>>,
    variables: Vec<(String, Constant)>,
}

impl Environment<'_> {
    fn variable(&self, name: &str) -> Option<&Constant> {
        self.variables
            .iter()
            .rev()
            .find(|(variable, _)| variable == name)
            .map(|(_, value)| value)
    }

    fn set_variable(&mut self, name: &str, value: Constant) -> Option<()> {
        let slot = self
            .variables
            .iter_mut()
            .rev()
            .find(|(variable, _)| variable == name)?;
        slot.1 = value;
        Some(())
    }
}

enum Flow {
    Next,
    Return(Constant),
}

struct Evaluation {
    steps_left: u32,
    call_depth: usize,
    /// The functions being run, so recursion gives up as it does on the watch.
    active_functions: Vec<usize>,
}

impl Evaluation {
    fn step(&mut self) -> Option<()> {
        self.steps_left = self.steps_left.checked_sub(1)?;
        Some(())
    }
}

impl<'a> Compiler<'a> {
    /// The value of an expression if it can be known now, without reading state or events.
    pub fn evaluate_constant(
        &self,
        expression: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
    ) -> Option<Constant> {
        let mut environment = Environment {
            scope: scope.clone(),
            variables: Vec::new(),
        };
        let mut evaluation = Evaluation {
            steps_left: EVALUATION_STEPS,
            call_depth: 0,
            active_functions: Vec::new(),
        };
        self.constant_expression(expression, &mut environment, &mut evaluation)
    }

    /// The constant a reference stands for, if it stands for one.
    pub fn reference_constant(&self, reference: &Reference<'a>) -> Option<Constant> {
        match reference {
            Reference::Constant(constant) => Some(constant.clone()),
            Reference::Expression { expression, scope } => self.evaluate_constant(expression, scope),
            Reference::Object(fields) => {
                let fields: Option<Vec<(String, Constant)>> = fields
                    .iter()
                    .map(|(name, field)| Some((name.clone(), self.reference_constant(field)?)))
                    .collect();
                Some(Constant::Object(Rc::new(fields?)))
            }
            _ => None,
        }
    }

    fn constant_expression(
        &self,
        expression: &'a Expression<'a>,
        environment: &mut Environment<'a>,
        evaluation: &mut Evaluation,
    ) -> Option<Constant> {
        evaluation.step()?;
        match expression {
            Expression::NumericLiteral(literal) => whole_number(literal.value).map(Constant::Number),
            Expression::BooleanLiteral(literal) => Some(Constant::Bool(literal.value)),
            Expression::StringLiteral(literal) => Some(Constant::Text(literal.value.as_str().into())),
            Expression::NullLiteral(_) => Some(Constant::Null),
            Expression::TemplateLiteral(template) => {
                let mut text = String::new();
                for (position, quasi) in template.quasis.iter().enumerate() {
                    text.push_str(quasi.value.cooked.as_ref()?.as_str());
                    if let Some(inner) = template.expressions.get(position) {
                        text.push_str(
                            &self
                                .constant_expression(inner, environment, evaluation)?
                                .display_text()?,
                        );
                    }
                }
                Some(Constant::Text(text.into()))
            }
            Expression::Identifier(identifier) => {
                let name = identifier.name.as_str();
                if name == "undefined" {
                    return Some(Constant::Undefined);
                }
                if let Some(value) = environment.variable(name) {
                    return Some(value.clone());
                }
                let reference = self.resolve_reference(expression, &environment.scope)?;
                self.reference_constant(&reference)
            }
            Expression::ParenthesizedExpression(inner) => {
                self.constant_expression(&inner.expression, environment, evaluation)
            }
            Expression::TSAsExpression(inner) => {
                self.constant_expression(&inner.expression, environment, evaluation)
            }
            Expression::TSSatisfiesExpression(inner) => {
                self.constant_expression(&inner.expression, environment, evaluation)
            }
            Expression::TSNonNullExpression(inner) => {
                self.constant_expression(&inner.expression, environment, evaluation)
            }
            Expression::TSTypeAssertion(inner) => {
                self.constant_expression(&inner.expression, environment, evaluation)
            }
            Expression::ArrayExpression(array) => {
                let mut elements = Vec::new();
                for element in &array.elements {
                    match element {
                        ArrayExpressionElement::SpreadElement(spread) => {
                            match self.constant_expression(&spread.argument, environment, evaluation)? {
                                Constant::Array(inner) => elements.extend(inner.iter().cloned()),
                                _ => return None,
                            }
                        }
                        ArrayExpressionElement::Elision(_) => return None,
                        other => elements.push(self.constant_expression(
                            other.as_expression()?,
                            environment,
                            evaluation,
                        )?),
                    }
                }
                Some(Constant::Array(Rc::new(elements)))
            }
            Expression::ObjectExpression(object) => {
                let mut fields: Vec<(String, Constant)> = Vec::new();
                for property in &object.properties {
                    match property {
                        ObjectPropertyKind::ObjectProperty(property) => {
                            let name = static_key(&property.key)?;
                            let value = self.constant_expression(&property.value, environment, evaluation)?;
                            fields.retain(|(existing, _)| *existing != name);
                            fields.push((name, value));
                        }
                        ObjectPropertyKind::SpreadProperty(spread) => {
                            match self.constant_expression(&spread.argument, environment, evaluation)? {
                                Constant::Object(inner) => {
                                    for (name, value) in inner.iter() {
                                        fields.retain(|(existing, _)| existing != name);
                                        fields.push((name.clone(), value.clone()));
                                    }
                                }
                                _ => return None,
                            }
                        }
                    }
                }
                Some(Constant::Object(Rc::new(fields)))
            }
            Expression::UnaryExpression(unary) => {
                let value = self.constant_expression(&unary.argument, environment, evaluation)?;
                match unary.operator {
                    UnaryOperator::LogicalNot => Some(Constant::Bool(!value.is_truthy())),
                    UnaryOperator::UnaryNegation => value.as_number()?.checked_neg().map(Constant::Number),
                    UnaryOperator::UnaryPlus => value.as_number().map(Constant::Number),
                    UnaryOperator::BitwiseNot => value.as_number().map(|number| Constant::Number(!number)),
                    _ => None,
                }
            }
            Expression::BinaryExpression(binary) => {
                let left = self.constant_expression(&binary.left, environment, evaluation)?;
                let right = self.constant_expression(&binary.right, environment, evaluation)?;
                binary_constant(binary.operator, &left, &right)
            }
            Expression::LogicalExpression(logical) => {
                let left = self.constant_expression(&logical.left, environment, evaluation)?;
                let take_left = match logical.operator {
                    LogicalOperator::And => !left.is_truthy(),
                    LogicalOperator::Or => left.is_truthy(),
                    LogicalOperator::Coalesce => !matches!(left, Constant::Null | Constant::Undefined),
                };
                if take_left {
                    Some(left)
                } else {
                    self.constant_expression(&logical.right, environment, evaluation)
                }
            }
            Expression::ConditionalExpression(conditional) => {
                let test = self.constant_expression(&conditional.test, environment, evaluation)?;
                let branch = if test.is_truthy() {
                    &conditional.consequent
                } else {
                    &conditional.alternate
                };
                self.constant_expression(branch, environment, evaluation)
            }
            Expression::StaticMemberExpression(member) => {
                let field = member.property.name.as_str();
                if let Some(reference) = self.resolve_in_environment(expression, environment) {
                    return self.reference_constant(&reference);
                }
                let object = self.constant_expression(&member.object, environment, evaluation)?;
                if matches!(object, Constant::Null | Constant::Undefined) && member.optional {
                    return Some(Constant::Undefined);
                }
                object.field(field)
            }
            Expression::ComputedMemberExpression(member) => {
                let object = self.constant_expression(&member.object, environment, evaluation)?;
                let index = self.constant_expression(&member.expression, environment, evaluation)?;
                match (object, index) {
                    (Constant::Array(elements), Constant::Number(index)) => Some(
                        usize::try_from(index)
                            .ok()
                            .and_then(|index| elements.get(index).cloned())
                            .unwrap_or(Constant::Undefined),
                    ),
                    (object @ Constant::Object(_), Constant::Text(name)) => object.field(&name),
                    _ => None,
                }
            }
            Expression::CallExpression(call) => self.constant_call(call, environment, evaluation),
            Expression::ChainExpression(chain) => match &chain.expression {
                oxc_ast::ast::ChainElement::StaticMemberExpression(member) => {
                    let object = self.constant_expression(&member.object, environment, evaluation)?;
                    if matches!(object, Constant::Null | Constant::Undefined) {
                        return Some(Constant::Undefined);
                    }
                    object.field(member.property.name.as_str())
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Resolves a name or member chain, seeing the evaluation's own variables first.
    fn resolve_in_environment(
        &self,
        expression: &'a Expression<'a>,
        environment: &Environment<'a>,
    ) -> Option<Reference<'a>> {
        let mut root = expression;
        while let Expression::StaticMemberExpression(member) = root {
            root = &member.object;
        }
        if let Expression::Identifier(identifier) = root {
            if environment.variable(identifier.name.as_str()).is_some() {
                return None;
            }
        }
        self.resolve_reference(expression, &environment.scope)
    }

    fn constant_call(
        &self,
        call: &'a oxc_ast::ast::CallExpression<'a>,
        environment: &mut Environment<'a>,
        evaluation: &mut Evaluation,
    ) -> Option<Constant> {
        let mut arguments = Vec::new();
        let argument_expressions: Vec<&'a Expression<'a>> = call
            .arguments
            .iter()
            .map(|argument| match argument {
                Argument::SpreadElement(_) => None,
                other => other.as_expression(),
            })
            .collect::<Option<_>>()?;

        // `Array.from({ length }, callback)` maps the numbers from 0 to length - 1.
        if let Some((length, callback)) = self.array_from_length(call, &environment.scope) {
            let mut results = Vec::new();
            for index in 0..length {
                let values = [Constant::Undefined, Constant::Number(index)];
                results.push(self.constant_callback(callback, &values, environment, evaluation)?);
            }
            return Some(Constant::Array(Rc::new(results)));
        }

        // Array methods take a function, so they are handled before the arguments are worked out.
        if let Expression::StaticMemberExpression(member) = &call.callee {
            let method = member.property.name.as_str();
            if self.resolve_in_environment(&call.callee, environment).is_none() {
                let object = self.constant_expression(&member.object, environment, evaluation)?;
                if let Constant::Text(text) = &object {
                    let mut values = Vec::new();
                    for argument in &argument_expressions {
                        values.push(self.constant_expression(argument, environment, evaluation)?);
                    }
                    let mut arguments = vec![Constant::Text(text.clone())];
                    arguments.extend(values);
                    let function = match method {
                        "toUpperCase" => "to_upper",
                        "slice" => "slice_text",
                        "padStart" => "pad_start",
                        _ => return None,
                    };
                    return library_call_constant(LibraryModule::Neotime, function, &arguments);
                }
                if let Constant::Array(elements) = &object {
                    if method == "map" {
                        let callback = argument_expressions.first()?;
                        let mut results = Vec::new();
                        for (index, element) in elements.iter().enumerate() {
                            let values = [element.clone(), Constant::Number(index as i64)];
                            results.push(self.constant_callback(
                                callback,
                                &values,
                                environment,
                                evaluation,
                            )?);
                        }
                        return Some(Constant::Array(Rc::new(results)));
                    }
                    let first = match argument_expressions.first() {
                        Some(argument) => {
                            Some(self.constant_expression(argument, environment, evaluation)?)
                        }
                        None => None,
                    };
                    return match (method, first) {
                        ("includes", Some(value)) => Some(Constant::Bool(elements.contains(&value))),
                        ("indexOf", Some(value)) => Some(Constant::Number(
                            elements
                                .iter()
                                .position(|element| *element == value)
                                .map_or(-1, |index| index as i64),
                        )),
                        _ => None,
                    };
                }
                return None;
            }
        }

        let callee = match &call.callee {
            Expression::Identifier(identifier)
                if environment.variable(identifier.name.as_str()).is_some() =>
            {
                return None
            }
            callee => self.resolve_in_environment(callee, environment)?,
        };
        let is_math_division = matches!(&callee, Reference::Library { name, .. } if name.starts_with("Math."))
            && matches!(
                argument_expressions.first().map(|argument| strip_parentheses(argument)),
                Some(Expression::BinaryExpression(binary)) if binary.operator == BinaryOperator::Division
            );
        if !is_math_division {
            for argument in &argument_expressions {
                arguments.push(self.constant_expression(argument, environment, evaluation)?);
            }
        }
        match callee {
            Reference::Function(function) => self.constant_function_call(&function, &arguments, evaluation),
            Reference::Library { module, name } => {
                let is_division = |position: usize| {
                    matches!(
                        argument_expressions.get(position).map(|argument| strip_parentheses(argument)),
                        Some(Expression::BinaryExpression(binary)) if binary.operator == BinaryOperator::Division
                    )
                };
                if module == LibraryModule::Neotime && name == "Math" {
                    return None;
                }
                if name.starts_with("Math.") && is_division(0) {
                    let Some(Expression::BinaryExpression(binary)) = argument_expressions
                        .first()
                        .map(|argument| strip_parentheses(argument))
                    else {
                        return None;
                    };
                    let dividend = self
                        .constant_expression(&binary.left, environment, evaluation)?
                        .as_number()?;
                    let divisor = self
                        .constant_expression(&binary.right, environment, evaluation)?
                        .as_number()?;
                    return divide_constant(&name, dividend, divisor).map(Constant::Number);
                }
                library_call_constant(module, &name, &arguments)
            }
            _ => None,
        }
    }

    /// The length and callback of `Array.from({ length: n }, callback)` with `n` known now.
    pub fn array_from_length(
        &self,
        call: &'a oxc_ast::ast::CallExpression<'a>,
        scope: &Rc<Scope<'a>>,
    ) -> Option<(i64, &'a Expression<'a>)> {
        let Expression::StaticMemberExpression(member) = strip_parentheses(&call.callee) else {
            return None;
        };
        let Expression::Identifier(object) = strip_parentheses(&member.object) else {
            return None;
        };
        if object.name != "Array" || member.property.name != "from" || self.lookup(scope, "Array").is_some() {
            return None;
        }
        let size = call.arguments.first()?.as_expression()?;
        let length = self
            .evaluate_constant(size, scope)?
            .field("length")?
            .as_number()?;
        let callback = call.arguments.get(1)?.as_expression()?;
        (0..=4096).contains(&length).then_some((length, callback))
    }

    /// Calls an inline arrow or a named function with arguments already worked out.
    fn constant_callback(
        &self,
        callback: &'a Expression<'a>,
        values: &[Constant],
        environment: &mut Environment<'a>,
        evaluation: &mut Evaluation,
    ) -> Option<Constant> {
        match strip_parentheses(callback) {
            Expression::ArrowFunctionExpression(arrow) => {
                let function = FunctionReference {
                    name: String::new(),
                    parameters: &arrow.params,
                    body: match &arrow.body {
                        ArrowFunctionBody::FunctionBody(body) => FunctionBodyReference::Statements(body),
                        other => FunctionBodyReference::Expression(other.as_expression()?),
                    },
                    scope: environment.scope.clone(),
                    span: arrow.span,
                    is_async: arrow.r#async,
                };
                let saved = environment.variables.len();
                let result = self.run_constant_function(&function, values, environment, evaluation);
                environment.variables.truncate(saved);
                result
            }
            other => match self.resolve_in_environment(other, environment)? {
                Reference::Function(function) => self.constant_function_call(&function, values, evaluation),
                _ => None,
            },
        }
    }

    fn constant_function_call(
        &self,
        function: &FunctionReference<'a>,
        arguments: &[Constant],
        evaluation: &mut Evaluation,
    ) -> Option<Constant> {
        let mut environment = Environment {
            scope: function.scope.clone(),
            variables: Vec::new(),
        };
        self.run_constant_function(function, arguments, &mut environment, evaluation)
    }

    fn run_constant_function(
        &self,
        function: &FunctionReference<'a>,
        arguments: &[Constant],
        environment: &mut Environment<'a>,
        evaluation: &mut Evaluation,
    ) -> Option<Constant> {
        let identity = function.identity();
        if function.is_async
            || evaluation.call_depth >= DEEPEST_CALLS
            || evaluation.active_functions.contains(&identity)
        {
            return None;
        }
        evaluation.call_depth += 1;
        evaluation.active_functions.push(identity);
        let result = (|| {
            for (position, parameter) in function.parameters.items.iter().enumerate() {
                let mut value = arguments.get(position).cloned().unwrap_or(Constant::Undefined);
                if value == Constant::Undefined {
                    if let Some(initializer) = &parameter.initializer {
                        value = self.constant_expression(initializer, environment, evaluation)?;
                    }
                }
                self.bind_constant_pattern(&parameter.pattern, value, environment, evaluation)?;
            }
            match function.body {
                FunctionBodyReference::Expression(expression) => {
                    self.constant_expression(expression, environment, evaluation)
                }
                FunctionBodyReference::Statements(body) => {
                    match self.constant_statements(&body.statements, environment, evaluation)? {
                        Flow::Return(value) => Some(value),
                        Flow::Next => Some(Constant::Undefined),
                    }
                }
            }
        })();
        evaluation.call_depth -= 1;
        evaluation.active_functions.pop();
        result
    }

    fn bind_constant_pattern(
        &self,
        pattern: &'a BindingPattern<'a>,
        value: Constant,
        environment: &mut Environment<'a>,
        evaluation: &mut Evaluation,
    ) -> Option<()> {
        match pattern {
            BindingPattern::BindingIdentifier(identifier) => {
                environment
                    .variables
                    .push((identifier.name.as_str().to_string(), value));
                Some(())
            }
            BindingPattern::AssignmentPattern(assignment) => {
                let value = if value == Constant::Undefined {
                    self.constant_expression(&assignment.right, environment, evaluation)?
                } else {
                    value
                };
                self.bind_constant_pattern(&assignment.left, value, environment, evaluation)
            }
            BindingPattern::ObjectPattern(object) => {
                if object.rest.is_some() {
                    return None;
                }
                for property in &object.properties {
                    let name = static_key(&property.key)?;
                    let field = value.field(&name)?;
                    self.bind_constant_pattern(&property.value, field, environment, evaluation)?;
                }
                Some(())
            }
            BindingPattern::ArrayPattern(_) => None,
        }
    }

    fn constant_statements(
        &self,
        statements: &'a [Statement<'a>],
        environment: &mut Environment<'a>,
        evaluation: &mut Evaluation,
    ) -> Option<Flow> {
        let saved = environment.variables.len();
        let mut flow = Flow::Next;
        for statement in statements {
            flow = self.constant_statement(statement, environment, evaluation)?;
            if matches!(flow, Flow::Return(_)) {
                break;
            }
        }
        environment.variables.truncate(saved);
        Some(flow)
    }

    fn constant_statement(
        &self,
        statement: &'a Statement<'a>,
        environment: &mut Environment<'a>,
        evaluation: &mut Evaluation,
    ) -> Option<Flow> {
        evaluation.step()?;
        match statement {
            Statement::VariableDeclaration(declaration) => {
                for declarator in &declaration.declarations {
                    let value = match &declarator.init {
                        Some(init) => self.constant_expression(init, environment, evaluation)?,
                        None => Constant::Undefined,
                    };
                    self.bind_constant_pattern(&declarator.id, value, environment, evaluation)?;
                }
                Some(Flow::Next)
            }
            Statement::ExpressionStatement(statement) => {
                self.constant_side_effect(&statement.expression, environment, evaluation)?;
                Some(Flow::Next)
            }
            Statement::BlockStatement(block) => {
                self.constant_statements(&block.body, environment, evaluation)
            }
            Statement::IfStatement(statement) => {
                if self
                    .constant_expression(&statement.test, environment, evaluation)?
                    .is_truthy()
                {
                    self.constant_statement_scoped(&statement.consequent, environment, evaluation)
                } else if let Some(alternate) = &statement.alternate {
                    self.constant_statement_scoped(alternate, environment, evaluation)
                } else {
                    Some(Flow::Next)
                }
            }
            Statement::ReturnStatement(statement) => Some(Flow::Return(match &statement.argument {
                Some(argument) => self.constant_expression(argument, environment, evaluation)?,
                None => Constant::Undefined,
            })),
            Statement::ForOfStatement(statement) => {
                let Constant::Array(elements) =
                    self.constant_expression(&statement.right, environment, evaluation)?
                else {
                    return None;
                };
                let ForStatementLeft::VariableDeclaration(declaration) = &statement.left else {
                    return None;
                };
                let pattern = &declaration.declarations.first()?.id;
                for element in elements.iter() {
                    let saved = environment.variables.len();
                    self.bind_constant_pattern(pattern, element.clone(), environment, evaluation)?;
                    let flow = self.constant_statement_scoped(&statement.body, environment, evaluation)?;
                    environment.variables.truncate(saved);
                    if let Flow::Return(value) = flow {
                        return Some(Flow::Return(value));
                    }
                }
                Some(Flow::Next)
            }
            Statement::EmptyStatement(_) => Some(Flow::Next),
            _ => None,
        }
    }

    fn constant_statement_scoped(
        &self,
        statement: &'a Statement<'a>,
        environment: &mut Environment<'a>,
        evaluation: &mut Evaluation,
    ) -> Option<Flow> {
        let saved = environment.variables.len();
        let flow = self.constant_statement(statement, environment, evaluation);
        environment.variables.truncate(saved);
        flow
    }

    /// Runs an expression statement: an assignment or update to a variable of the function.
    fn constant_side_effect(
        &self,
        expression: &'a Expression<'a>,
        environment: &mut Environment<'a>,
        evaluation: &mut Evaluation,
    ) -> Option<()> {
        match expression {
            Expression::AssignmentExpression(assignment) => {
                let AssignmentTarget::AssignmentTargetIdentifier(target) = &assignment.left else {
                    return None;
                };
                let name = target.name.as_str();
                let right = self.constant_expression(&assignment.right, environment, evaluation)?;
                let value = match assignment.operator {
                    AssignmentOperator::Assign => right,
                    operator => {
                        let left = environment.variable(name)?.clone();
                        binary_constant(compound_operator(operator)?, &left, &right)?
                    }
                };
                environment.set_variable(name, value)
            }
            Expression::UpdateExpression(update) => {
                let SimpleAssignmentTarget::AssignmentTargetIdentifier(target) = &update.argument else {
                    return None;
                };
                let name = target.name.as_str();
                let current = environment.variable(name)?.as_number()?;
                let next = match update.operator {
                    UpdateOperator::Increment => current.checked_add(1)?,
                    UpdateOperator::Decrement => current.checked_sub(1)?,
                };
                environment.set_variable(name, Constant::Number(next))
            }
            Expression::ParenthesizedExpression(inner) => {
                self.constant_side_effect(&inner.expression, environment, evaluation)
            }
            _ => None,
        }
    }
}

// ============================================================================
// Operators
// ============================================================================

/// A JavaScript number as a whole number, or `None` for a fraction or one too large.
pub fn whole_number(value: f64) -> Option<i64> {
    (value.fract() == 0.0 && value.abs() < 9.0e18).then_some(value as i64)
}

pub fn strip_parentheses<'b, 'a>(expression: &'b Expression<'a>) -> &'b Expression<'a> {
    match expression {
        Expression::ParenthesizedExpression(inner) => strip_parentheses(&inner.expression),
        Expression::TSAsExpression(inner) => strip_parentheses(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => strip_parentheses(&inner.expression),
        Expression::TSNonNullExpression(inner) => strip_parentheses(&inner.expression),
        Expression::TSTypeAssertion(inner) => strip_parentheses(&inner.expression),
        other => other,
    }
}

pub fn static_key(key: &PropertyKey) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(identifier) => Some(identifier.name.as_str().to_string()),
        PropertyKey::StringLiteral(literal) => Some(literal.value.as_str().to_string()),
        PropertyKey::NumericLiteral(literal) => whole_number(literal.value).map(|number| number.to_string()),
        _ => None,
    }
}

/// The binary operator a compound assignment such as `+=` applies.
pub fn compound_operator(operator: AssignmentOperator) -> Option<BinaryOperator> {
    Some(match operator {
        AssignmentOperator::Addition => BinaryOperator::Addition,
        AssignmentOperator::Subtraction => BinaryOperator::Subtraction,
        AssignmentOperator::Multiplication => BinaryOperator::Multiplication,
        AssignmentOperator::Remainder => BinaryOperator::Remainder,
        AssignmentOperator::ShiftLeft => BinaryOperator::ShiftLeft,
        AssignmentOperator::ShiftRight => BinaryOperator::ShiftRight,
        AssignmentOperator::BitwiseOR => BinaryOperator::BitwiseOR,
        AssignmentOperator::BitwiseXOR => BinaryOperator::BitwiseXOR,
        AssignmentOperator::BitwiseAnd => BinaryOperator::BitwiseAnd,
        AssignmentOperator::Division => BinaryOperator::Division,
        _ => return None,
    })
}

fn binary_constant(operator: BinaryOperator, left: &Constant, right: &Constant) -> Option<Constant> {
    use BinaryOperator as Operator;
    if operator == Operator::Addition {
        if let (Constant::Text(_), _) | (_, Constant::Text(_)) = (left, right) {
            return Some(Constant::Text(
                format!("{}{}", left.display_text()?, right.display_text()?).into(),
            ));
        }
    }
    match operator {
        Operator::StrictEquality | Operator::Equality => {
            return Some(Constant::Bool(loosely_equal(left, right)))
        }
        Operator::StrictInequality | Operator::Inequality => {
            return Some(Constant::Bool(!loosely_equal(left, right)))
        }
        _ => {}
    }
    let (left, right) = (number_like(left)?, number_like(right)?);
    Some(match operator {
        Operator::LessThan => Constant::Bool(left < right),
        Operator::LessEqualThan => Constant::Bool(left <= right),
        Operator::GreaterThan => Constant::Bool(left > right),
        Operator::GreaterEqualThan => Constant::Bool(left >= right),
        Operator::Addition => Constant::Number(left.checked_add(right)?),
        Operator::Subtraction => Constant::Number(left.checked_sub(right)?),
        Operator::Multiplication => Constant::Number(left.checked_mul(right)?),
        Operator::Remainder => Constant::Number(left.checked_rem(right)?),
        Operator::Division if right != 0 && left.checked_rem(right)? == 0 => Constant::Number(left / right),
        Operator::BitwiseAnd => Constant::Number(left & right),
        Operator::BitwiseOR => Constant::Number(left | right),
        Operator::BitwiseXOR => Constant::Number(left ^ right),
        Operator::ShiftLeft if (0..64).contains(&right) => Constant::Number(left << right),
        Operator::ShiftRight if (0..64).contains(&right) => Constant::Number(left >> right),
        _ => return None,
    })
}

fn number_like(constant: &Constant) -> Option<i64> {
    match constant {
        Constant::Number(number) => Some(*number),
        Constant::Bool(value) => Some(*value as i64),
        _ => None,
    }
}

fn loosely_equal(left: &Constant, right: &Constant) -> bool {
    match (left, right) {
        (Constant::Null | Constant::Undefined, Constant::Null | Constant::Undefined) => true,
        _ => left == right,
    }
}

/// Divides as `Math.floor`, `Math.trunc` or `Math.round` around a `/` would.
pub fn divide_constant(function: &str, dividend: i64, divisor: i64) -> Option<i64> {
    if divisor == 0 {
        return None;
    }
    let quotient = dividend.checked_div(divisor)?;
    let has_remainder = dividend.checked_rem(divisor)? != 0;
    let signs_differ = (dividend < 0) != (divisor < 0);
    match function {
        "Math.trunc" => Some(quotient),
        "Math.floor" if has_remainder && signs_differ => quotient.checked_sub(1),
        "Math.floor" => Some(quotient),
        "Math.ceil" if has_remainder && !signs_differ => quotient.checked_add(1),
        "Math.ceil" => Some(quotient),
        "Math.round" => {
            let doubled = dividend.checked_mul(2)?.checked_add(divisor)?;
            divide_constant("Math.floor", doubled, divisor.checked_mul(2)?)
        }
        _ => None,
    }
}

/// Library functions whose result is known when every argument is.
fn library_call_constant(module: LibraryModule, name: &str, arguments: &[Constant]) -> Option<Constant> {
    let number = |position: usize| arguments.get(position).and_then(Constant::as_number);
    let text = |position: usize| match arguments.get(position) {
        Some(Constant::Text(value)) => Some(value.to_string()),
        _ => None,
    };
    match (module, name) {
        (_, "Math.floor") | (_, "Math.trunc") | (_, "Math.round") | (_, "Math.ceil") => {
            number(0).map(Constant::Number)
        }
        (_, "Math.abs") => number(0)?.checked_abs().map(Constant::Number),
        (_, "Math.min") => {
            let numbers: Option<Vec<i64>> = arguments.iter().map(Constant::as_number).collect();
            numbers?.into_iter().min().map(Constant::Number)
        }
        (_, "Math.max") => {
            let numbers: Option<Vec<i64>> = arguments.iter().map(Constant::as_number).collect();
            numbers?.into_iter().max().map(Constant::Number)
        }
        (LibraryModule::Neotime, "clamp") => {
            Some(Constant::Number(number(0)?.max(number(1)?).min(number(2)?)))
        }
        (LibraryModule::Neotime, "format_number") => {
            let value = number(0)?;
            let digits = number(1).unwrap_or(1).max(1) as usize;
            let sign = if value < 0 { "-" } else { "" };
            Some(Constant::Text(
                format!("{sign}{:0digits$}", value.unsigned_abs()).into(),
            ))
        }
        (LibraryModule::Neotime, "range") => {
            let count = number(0)?;
            (0..=4096)
                .contains(&count)
                .then(|| Constant::Array(Rc::new((0..count).map(Constant::Number).collect())))
        }
        (LibraryModule::Neotime, "to_upper") => {
            text(0).map(|value| Constant::Text(value.to_uppercase().into()))
        }
        (LibraryModule::Neotime, "text_length") => {
            text(0).map(|value| Constant::Number(value.chars().count() as i64))
        }
        (LibraryModule::Neotime, "slice_text") => {
            let value = text(0)?;
            let characters: Vec<char> = value.chars().collect();
            let length = characters.len() as i64;
            let clamp = |position: i64| {
                (if position < 0 { length + position } else { position }).clamp(0, length) as usize
            };
            let start = clamp(number(1)?);
            let end = clamp(number(2).unwrap_or(length));
            Some(Constant::Text(
                characters[start..end.max(start)]
                    .iter()
                    .collect::<String>()
                    .into(),
            ))
        }
        (LibraryModule::Neotime, "pad_start") => {
            let value = text(0)?;
            let width = number(1)? as usize;
            let filler = match arguments.get(2) {
                Some(Constant::Text(filler)) => filler.chars().next()?,
                _ => ' ',
            };
            let missing = width.saturating_sub(value.chars().count());
            Some(Constant::Text(
                format!("{}{value}", filler.to_string().repeat(missing)).into(),
            ))
        }
        (LibraryModule::Neotime, "join") => {
            let Constant::Array(elements) = arguments.first()? else {
                return None;
            };
            let separator = text(1)?;
            let pieces: Option<Vec<String>> = elements.iter().map(Constant::display_text).collect();
            Some(Constant::Text(pieces?.join(&separator).into()))
        }
        (LibraryModule::Icons, "icon_named") => match arguments.first()? {
            Constant::Text(name) => icon_number(name).map(Constant::Icon),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
#[path = "constants_tests.rs"]
mod constants_tests;
