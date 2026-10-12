//! The `neotime` functions on whole arrays that the interpreter answers natively: `AskArray` for the
//! questions and `WorkArray` for comparing and rewriting grids.

use std::rc::Rc;

use neotime_runtime::instruction::{ArrayQuestion, ArrayWork, Instruction};
use oxc_ast::ast::Expression;
use oxc_span::{GetSpan, Span};

use crate::compiler::Compiler;
use crate::diagnostics::{fail, Compiled};
use crate::emitter::Emitter;
use crate::expressions::Typed;
use crate::scope::{Reference, Scope};
use crate::types::{ChoiceSet, Kind, Type};

/// `can_slide` asks `SlideChanges` about every direction with this one.
const ANY_DIRECTION: i64 = 4;

/// The functions that give back a new array, which only an assignment to an array signal can take.
pub const ARRAY_WORK_FUNCTIONS: [&str; 2] = ["slide_grid", "life_step"];

/// Whether `name` is one of the array questions `compile_array_question` compiles.
pub fn is_array_question(name: &str) -> bool {
    matches!(
        name,
        "count_of"
            | "nth_index_of"
            | "line_through"
            | "slide_changes"
            | "can_slide"
            | "slide_points"
            | "arrays_equal"
    )
}

impl<'a> Compiler<'a> {
    /// The state array an argument names, such as `board.value`.
    pub(crate) fn array_signal_argument(
        &mut self,
        argument: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        function: &str,
    ) -> Compiled<u8> {
        match self.resolve_reference(argument, scope) {
            Some(Reference::StateArray(variable))
                if self.slots[variable as usize].value_type.kind != Kind::Text =>
            {
                Ok(variable)
            }
            _ => fail(
                argument.span(),
                format!("`{function}` needs an array signal of numbers, such as `board.value`"),
            ),
        }
    }

    /// One of the questions in `is_array_question`, asked of an array signal.
    pub(crate) fn compile_array_question(
        &mut self,
        emitter: &mut Emitter,
        name: &str,
        arguments: &[&'a Expression<'a>],
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<Typed> {
        let (question, numbers, result) = match name {
            "count_of" => (ArrayQuestion::CountOf, 1, Type::NUMBER),
            "nth_index_of" => (ArrayQuestion::NthIndexOf, 2, Type::NUMBER),
            "line_through" => (ArrayQuestion::LineThrough, 3, Type::NUMBER),
            "slide_changes" => (ArrayQuestion::SlideChanges, 2, Type::BOOL),
            "can_slide" => (ArrayQuestion::SlideChanges, 1, Type::BOOL),
            "slide_points" => (ArrayQuestion::SlidePoints, 2, Type::NUMBER),
            _ => {
                let [first, second] = arguments else {
                    return fail(span, "`arrays_equal` takes two arrays");
                };
                let first = self.array_signal_argument(first, scope, name)?;
                let second = self.array_signal_argument(second, scope, name)?;
                if self.slots[first as usize].length != self.slots[second as usize].length {
                    return fail(span, "`arrays_equal` compares two arrays of the same length");
                }
                emitter.emit(
                    Instruction::WorkArray {
                        work: ArrayWork::Equal,
                        destination: first,
                        source: second,
                    },
                    span,
                )?;
                return Ok(Typed::new(Type::BOOL));
            }
        };
        if arguments.len() != numbers + 1 {
            return fail(
                span,
                format!("`{name}` takes an array and {numbers} more arguments"),
            );
        }
        let array = self.array_signal_argument(arguments[0], scope, name)?;
        let takes_direction = name != "can_slide" && question_takes_direction(question);
        self.compile_array_arguments(emitter, takes_direction, &arguments[1..], scope)?;
        if name == "can_slide" {
            self.emit_number(emitter, ANY_DIRECTION, span)?;
        }
        emitter.emit(Instruction::AskArray { question, array }, span)?;
        Ok(Typed::new(result))
    }

    /// `target.value = slide_grid(...)` or `life_step(...)`, written straight into the target.
    pub(crate) fn compile_array_work_assignment(
        &mut self,
        emitter: &mut Emitter,
        target: u8,
        name: &str,
        arguments: &[&'a Expression<'a>],
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<()> {
        if !emitter.is_handler() {
            return fail(
                span,
                format!("`{name}` rewrites an array, which only a handler may do"),
            );
        }
        let [source, rest @ ..] = arguments else {
            return fail(
                span,
                format!("`{name}` takes a grid, its width and one more argument"),
            );
        };
        if rest.len() != 2 {
            return fail(
                span,
                format!("`{name}` takes a grid, its width and one more argument"),
            );
        }
        let source = self.array_signal_argument(source, scope, name)?;
        if self.slots[source as usize].length != self.slots[target as usize].length {
            return fail(
                span,
                format!("`{name}` writes an array as long as the one it reads"),
            );
        }
        let work = match name {
            "slide_grid" => ArrayWork::Slide,
            _ => ArrayWork::LifeStep,
        };
        self.compile_array_arguments(emitter, work == ArrayWork::Slide, rest, scope)?;
        emitter.emit(
            Instruction::WorkArray {
                work,
                destination: target,
                source,
            },
            span,
        )?;
        Ok(())
    }

    /// Numbers, the second a swipe direction when `last_is_direction` is set.
    fn compile_array_arguments(
        &mut self,
        emitter: &mut Emitter,
        last_is_direction: bool,
        arguments: &[&'a Expression<'a>],
        scope: &Rc<Scope<'a>>,
    ) -> Compiled<()> {
        for (place, argument) in arguments.iter().enumerate() {
            let expected = match last_is_direction && place + 1 == arguments.len() {
                true => Type::choice(ChoiceSet::SwipeDirection),
                false => Type::NUMBER,
            };
            self.compile_value_of_type(emitter, argument, scope, expected)?;
        }
        Ok(())
    }
}

fn question_takes_direction(question: ArrayQuestion) -> bool {
    matches!(question, ArrayQuestion::SlideChanges | ArrayQuestion::SlidePoints)
}

#[cfg(test)]
#[path = "array_builtins_tests.rs"]
mod array_builtins_tests;
