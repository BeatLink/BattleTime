//! The app's own functions compiled once and called, instead of inlined at every use.
//!
//! The first pass inlines every function and counts what each costs. Between passes,
//! `FunctionPlan::choose` picks the functions whose copies take more bytes than calls would.
//! Such a function is compiled once for each way it is called: in an expression or a handler,
//! with or without a result, and with the arguments known when compiling, such as an array
//! signal, built into it. The other arguments become its parameters. Anything that cannot be
//! compiled on its own this way, such as a function that needs a literal pattern, is inlined
//! instead and stays inlined in later passes.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use neotime_runtime::image::MOST_FUNCTIONS;
use neotime_runtime::interpreter::{LOCAL_POOL, MOST_CALL_DEPTH, MOST_LOCALS};
use oxc_ast::ast::{BindingPattern, Expression};
use oxc_span::Span;

use crate::compiler::Compiler;
use crate::constants::{strip_parentheses, Constant};
use crate::diagnostics::{fail, Compiled};
use crate::emitter::{CallNeeds, CodeKind, Emitter};
use crate::expressions::Typed;
use crate::scope::{FunctionReference, Reference, Scope};
use crate::types::{Kind, Type};

// ============================================================================
// Costs
// ============================================================================

/// A function's header and its entry in the image's table of functions.
const FUNCTION_OVERHEAD_BYTES: usize = 7;

/// About what pushing one argument costs at a call.
const ARGUMENT_BYTES: usize = 2;

/// The bytes of the `Call` itself.
const CALL_BYTES: usize = 2;

/// The deepest a caller's stack may be before a call, so the function has room above it.
const DEEPEST_STACK_BEFORE_A_CALL: usize = 16;

/// What inlining one function cost in a pass.
#[derive(Clone, Copy, Debug, Default)]
pub struct InlineCost {
    pub copies: usize,
    pub bytes: usize,
    pub parameters: usize,
}

// ============================================================================
// The plan between passes
// ============================================================================

/// Which functions are called rather than inlined, by `FunctionReference::identity`.
#[derive(Clone, Debug, Default)]
pub struct FunctionPlan {
    pub called: HashSet<usize>,
    /// Functions that could not be compiled on their own, so they are always inlined.
    pub inline_only: HashSet<usize>,
    /// The `.map` calls in lists, by where they start, that are unrolled because their row could not
    /// be drawn as one repeated row.
    pub unrolled_maps: HashSet<usize>,
}

impl FunctionPlan {
    /// Adds every function whose inlined copies cost more than one function and its calls.
    pub fn choose(&mut self, costs: &HashMap<usize, InlineCost>) {
        for (identity, cost) in costs {
            if cost.copies < 2 || self.inline_only.contains(identity) {
                continue;
            }
            let bytes_per_copy = cost.bytes / cost.copies;
            let inlined = cost.copies * bytes_per_copy;
            let called = bytes_per_copy
                + FUNCTION_OVERHEAD_BYTES
                + cost.copies * (cost.parameters * ARGUMENT_BYTES + CALL_BYTES);
            if inlined > called {
                self.called.insert(*identity);
            }
        }
    }

    /// Takes out the functions a pass could not compile on their own; true when there were any.
    pub fn give_up_on(&mut self, failed: &HashSet<usize>) -> bool {
        let mut changed = false;
        for identity in failed {
            changed |= self.inline_only.insert(*identity);
            self.called.remove(identity);
        }
        changed
    }

    fn calls(&self, identity: usize) -> bool {
        self.called.contains(&identity) && !self.inline_only.contains(&identity)
    }
}

// ============================================================================
// Compiled functions
// ============================================================================

/// How one parameter is filled: at run time with a value of this type, or built in.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ParameterKind {
    Runtime(Type),
    BuiltIn(String),
}

/// One way of calling a function, which has a compiled function of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CallShape {
    identity: usize,
    kind: CodeKind,
    wants_value: bool,
    expected: Option<Type>,
    parameters: Vec<ParameterKind>,
}

#[derive(Clone, Debug)]
pub struct CompiledFunction {
    shape: CallShape,
    pub(crate) number: u8,
    pub(crate) result: Option<Typed>,
    needs: CallNeeds,
}

/// What became of a call that could be compiled as a function.
pub enum CallOutcome<'a> {
    Called(Option<Typed>),
    /// Inline it after all, with its arguments already worked out.
    Inline(Vec<Reference<'a>>),
}

/// Text that tells built-in arguments apart, or none for one that cannot be built in.
fn built_in_key(reference: &Reference) -> Option<String> {
    Some(match reference {
        Reference::Constant(Constant::Text(text)) => format!("text {text:?}"),
        Reference::Constant(constant @ (Constant::Array(_) | Constant::Object(_))) => {
            format!("constant {constant:?}")
        }
        Reference::Signal(variable) => format!("signal {variable}"),
        Reference::StateArray(variable) => format!("array {variable}"),
        Reference::Record(fields) => format!("record {fields:?}"),
        Reference::RecordValue(fields) => format!("record value {fields:?}"),
        Reference::RecordArray(fields) => format!("record array {fields:?}"),
        Reference::RecordArrayValue(fields) => format!("record array value {fields:?}"),
        Reference::Function(function) if function.scope.parent.is_none() => {
            format!("function {}", function.identity())
        }
        Reference::SystemGroup(name) => format!("system group {name}"),
        Reference::SystemGroupValue(name) => format!("system group value {name}"),
        Reference::SystemList(name) => format!("system list {name}"),
        Reference::System(system) => format!("system {system:?}"),
        _ => return None,
    })
}

/// Whether an argument is one value worked out while running, which a parameter can take.
fn is_runtime_value(reference: &Reference) -> bool {
    match reference {
        Reference::Constant(Constant::Number(_) | Constant::Bool(_) | Constant::Icon(_)) => true,
        Reference::Expression { expression, .. } => !matches!(
            strip_parentheses(expression),
            Expression::ArrayExpression(_)
                | Expression::ObjectExpression(_)
                | Expression::JSXElement(_)
                | Expression::JSXFragment(_)
                | Expression::ArrowFunctionExpression(_)
                | Expression::FunctionExpression(_)
        ),
        Reference::Local { .. }
        | Reference::StateScalar(_)
        | Reference::StateElement { .. }
        | Reference::StateElementAt { .. }
        | Reference::SystemRead(_)
        | Reference::SystemElement { .. }
        | Reference::EventField { .. }
        | Reference::ProgressionElement { .. } => true,
        _ => false,
    }
}

impl<'a> Compiler<'a> {
    /// Whether calls to this function are compiled as calls in this pass.
    pub fn calls_function(&self, function: &FunctionReference<'a>) -> bool {
        function.scope.parent.is_none() && self.function_plan.calls(function.identity())
    }

    /// Notes what one inlined copy of a function cost, for choosing what to call next pass.
    pub fn count_inlined(&mut self, function: &FunctionReference<'a>, bytes: usize) {
        let cost = self.inline_costs.entry(function.identity()).or_default();
        cost.copies += 1;
        cost.bytes += bytes;
        cost.parameters = function.parameters.items.len();
    }

    /// Calls a function compiled once, or says to inline it with the arguments worked out.
    #[allow(clippy::too_many_arguments)]
    pub fn call_function(
        &mut self,
        emitter: &mut Emitter,
        function: &FunctionReference<'a>,
        references: Vec<Reference<'a>>,
        expected_types: Vec<Option<Type>>,
        wants_value: bool,
        expected: Option<Type>,
        span: Span,
    ) -> Compiled<CallOutcome<'a>> {
        let parameters = &function.parameters.items;
        let simple_parameters = parameters
            .iter()
            .all(|parameter| matches!(parameter.pattern, BindingPattern::BindingIdentifier(_)));
        if function.parameters.rest.is_some() || !simple_parameters || references.len() != parameters.len() {
            return Ok(CallOutcome::Inline(references));
        }

        // Work out each runtime argument's type in a throwaway emitter, so nothing is written yet.
        let mut kinds = Vec::with_capacity(references.len());
        let mut argument_types = Vec::with_capacity(references.len());
        for (reference, expected_type) in references.iter().zip(&expected_types) {
            if let Some(key) = built_in_key(reference) {
                kinds.push(ParameterKind::BuiltIn(key));
                argument_types.push(None);
            } else if is_runtime_value(reference) {
                let mut probe = Emitter::new(emitter.kind);
                let typed =
                    match self.compile_reference_value(&mut probe, reference.clone(), span, *expected_type) {
                        Ok(typed) => typed,
                        Err(_) => return Ok(CallOutcome::Inline(references)),
                    };
                if matches!(typed.value_type.kind, Kind::Null) {
                    return Ok(CallOutcome::Inline(references));
                }
                kinds.push(ParameterKind::Runtime(typed.value_type));
                argument_types.push(Some(typed));
            } else {
                return Ok(CallOutcome::Inline(references));
            }
        }

        let shape = CallShape {
            identity: function.identity(),
            kind: emitter.kind,
            wants_value,
            expected,
            parameters: kinds,
        };
        for (index, typed) in argument_types.iter().enumerate() {
            if let Some(typed) = typed {
                let key = parameter_key(function, index);
                let range = self.parameter_ranges.entry(key).or_insert(typed.range);
                *range = range.union(typed.range);
            }
        }
        let compiled = match self
            .compiled_functions
            .iter()
            .find(|compiled| compiled.shape == shape)
        {
            Some(compiled) => compiled.clone(),
            None => match self.compile_function(function, &shape, &references, &argument_types, span) {
                Ok(compiled) => {
                    self.compiled_functions.push(compiled.clone());
                    compiled
                }
                Err(_) => {
                    self.failed_functions.insert(function.identity());
                    return Ok(CallOutcome::Inline(references));
                }
            },
        };

        let spilled = match emitter.depth().unwrap_or(0) > DEEPEST_STACK_BEFORE_A_CALL {
            true => self.spill_whole_stack(emitter, span)?,
            false => Vec::new(),
        };
        let mut argument_count = 0;
        for ((reference, typed), expected_type) in
            references.into_iter().zip(&argument_types).zip(&expected_types)
        {
            if typed.is_some() {
                self.compile_reference_value(emitter, reference, span, *expected_type)?;
                argument_count += 1;
            }
        }
        let result_count = compiled.result.is_some() as usize;
        emitter.emit_call(
            compiled.number,
            argument_count,
            result_count,
            compiled.needs,
            span,
        )?;
        self.restore_stack(emitter, &spilled, compiled.result.is_some(), span)?;
        Ok(CallOutcome::Called(compiled.result))
    }

    /// Compiles a shared library function on its own: a pure function, every parameter a value of the
    /// type its annotation names.
    pub(crate) fn compile_library_function(
        &mut self,
        function: &FunctionReference<'a>,
        parameter_types: &[Type],
        span: Span,
    ) -> Compiled<CompiledFunction> {
        let shape = CallShape {
            identity: function.identity(),
            kind: CodeKind::Expression,
            wants_value: true,
            expected: None,
            parameters: parameter_types
                .iter()
                .map(|value_type| ParameterKind::Runtime(*value_type))
                .collect(),
        };
        let references: Vec<Reference<'a>> = parameter_types
            .iter()
            .map(|_| Reference::Constant(Constant::Undefined))
            .collect();
        let argument_types: Vec<Option<Typed>> = parameter_types
            .iter()
            .map(|value_type| Some(Typed::with_range(*value_type, crate::types::Range::ANY)))
            .collect();
        self.compile_function(function, &shape, &references, &argument_types, span)
    }

    /// Compiles a function for one way of calling it, with its runtime arguments as its first locals.
    fn compile_function(
        &mut self,
        function: &FunctionReference<'a>,
        shape: &CallShape,
        references: &[Reference<'a>],
        argument_types: &[Option<Typed>],
        span: Span,
    ) -> Compiled<CompiledFunction> {
        if self.writer.function_count() >= MOST_FUNCTIONS {
            return fail(span, "the image holds as many functions as it can");
        }
        let mut emitter = Emitter::for_function(shape.kind, shape.wants_value);
        let mut scope: Rc<Scope<'a>> = function.scope.clone();
        for (index, ((parameter, reference), typed)) in function
            .parameters
            .items
            .iter()
            .zip(references)
            .zip(argument_types)
            .enumerate()
        {
            let bound = match typed {
                Some(typed) => {
                    let range = self
                        .assumed_ranges
                        .get(&parameter_key(function, index))
                        .copied()
                        .unwrap_or(typed.range);
                    Reference::Local {
                        index: emitter.take_local(span)?,
                        value_type: typed.value_type,
                        range,
                        is_constant: true,
                    }
                }
                None => reference.clone(),
            };
            let BindingPattern::BindingIdentifier(identifier) = &parameter.pattern else {
                return fail(span, "a parameter that takes its argument apart is inlined");
            };
            scope = scope.with(identifier.name.as_str(), bound);
        }
        let argument_count = argument_types.iter().filter(|typed| typed.is_some()).count();

        let outer_loops = std::mem::take(&mut self.loop_labels);
        self.inline_stack.push(function.identity());
        let result = self.compile_function_body(
            &mut emitter,
            function.body,
            &scope,
            shape.wants_value,
            shape.expected,
            span,
        );
        self.inline_stack.pop();
        self.loop_labels = outer_loops;
        let result = result?;

        let needs = emitter.call_needs();
        if needs.locals as usize > LOCAL_POOL - MOST_LOCALS || needs.depth as usize >= MOST_CALL_DEPTH {
            return fail(span, "the calls inside this function nest too deep");
        }
        let local_count = emitter.local_count();
        let code = emitter.finish(span)?;
        let number = self
            .writer
            .function(argument_count as u8, result.is_some() as u8, local_count, code);
        Ok(CompiledFunction {
            shape: shape.clone(),
            number,
            result,
            needs,
        })
    }
}

/// The name a parameter's range is kept under between passes.
fn parameter_key(function: &FunctionReference, index: usize) -> String {
    format!("~parameter:{}:{index}", function.identity())
}

#[cfg(test)]
#[path = "functions_tests.rs"]
mod functions_tests;
