//! The `battletime` functions on whole arrays that the interpreter answers natively: `AskArray` for the
//! questions and `WorkArray` for comparing and rewriting grids.

use std::rc::Rc;

use battletime_runtime::instruction::{ArrayQuestion, ArrayWork, Instruction};
use oxc_ast::ast::Expression;
use oxc_span::{GetSpan, Span};

use crate::compiler::Compiler;
use crate::constants::Constant;
use crate::diagnostics::{fail, Compiled};
use crate::emitter::Emitter;
use crate::expressions::Typed;
use crate::scope::{Reference, Scope};
use crate::types::{ChoiceSet, Kind, Range, Type};

/// The highest tile level a slide writes, where its points stop doubling.
const MOST_SLIDE_LEVEL: i64 = 62;

/// `can_slide` asks `SlideChanges` about every direction with this one.
const ANY_DIRECTION: i64 = 4;

/// The functions that give back a new array, which only an assignment to an array signal can take.
pub const ARRAY_WORK_FUNCTIONS: [&str; 6] = [
    "slide_grid",
    "life_step",
    "push_shift",
    "unshift_pop",
    "shuffled",
    "file_bytes",
];

/// The largest byte a file holds, the most `file_bytes` writes to an element.
const LARGEST_BYTE: i64 = 255;

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
            | "count_inversions"
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
            "count_inversions" => (ArrayQuestion::Inversions, 0, Type::NUMBER),
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

    /// `target.value = slide_grid(...)`, or one of the other functions that give a whole array, written straight
    /// into the target.
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
        if name == "file_bytes" {
            return self.compile_file_bytes(emitter, target, arguments, scope, span);
        }
        let (work, numbers, takes) = match name {
            "slide_grid" => (ArrayWork::Slide, 2, "a grid, its width and a direction"),
            "life_step" => (ArrayWork::LifeStep, 2, "a grid, its width and the live value"),
            "push_shift" => (ArrayWork::PushLast, 1, "an array and a value"),
            "shuffled" => (
                ArrayWork::Shuffle,
                2,
                "an array, the first place to shuffle and how many",
            ),
            _ => (ArrayWork::PushFirst, 1, "an array and a value"),
        };
        let [source, rest @ ..] = arguments else {
            return fail(span, format!("`{name}` takes {takes}"));
        };
        if rest.len() != numbers {
            return fail(span, format!("`{name}` takes {takes}"));
        }
        let source = self.array_signal_argument(source, scope, name)?;
        if self.slots[source as usize].length != self.slots[target as usize].length {
            return fail(
                span,
                format!("`{name}` writes an array as long as the one it reads"),
            );
        }
        // What the target may hold afterwards, so the array is stored wide enough for it.
        let source_range = self.read_range(source);
        let written = match work {
            ArrayWork::PushLast | ArrayWork::PushFirst => {
                let element_type = self.slots[target as usize].value_type;
                let pushed = self.compile_value_of_type(emitter, rest[0], scope, element_type)?;
                source_range.union(pushed.range)
            }
            ArrayWork::Shuffle => {
                self.compile_array_arguments(emitter, false, rest, scope)?;
                source_range
            }
            ArrayWork::LifeStep => {
                let ranges = self.compile_array_arguments(emitter, false, rest, scope)?;
                Range::exactly(0).union(ranges[1])
            }
            _ => {
                self.compile_array_arguments(emitter, true, rest, scope)?;
                source_range.union(Range {
                    lowest: 0,
                    highest: MOST_SLIDE_LEVEL,
                })
            }
        };
        self.note_write(target, written);
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

    /// `target.value = file_bytes(path, length, offset)`: the file's bytes from `offset`, or its first bytes
    /// without one, read when the handler runs.
    fn compile_file_bytes(
        &mut self,
        emitter: &mut Emitter,
        target: u8,
        arguments: &[&'a Expression<'a>],
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<()> {
        let (path, length, offset) = match arguments {
            [path, length] => (path, length, None),
            [path, length, offset] => (path, length, Some(offset)),
            _ => return fail(span, "`file_bytes` takes a path, a length and an optional offset"),
        };
        let Some(Constant::Text(path_text)) = self.evaluate_constant(path, scope) else {
            return fail(
                path.span(),
                "`file_bytes` needs the path written out, such as \"/remote/buttons\"",
            );
        };
        let Some(Constant::Number(length_value)) = self.evaluate_constant(length, scope) else {
            return fail(length.span(), "`file_bytes` needs a length known when compiling");
        };
        if length_value != self.slots[target as usize].length as i64 {
            return fail(
                span,
                format!("`file_bytes` reads {length_value} bytes, so it needs an array of {length_value}"),
            );
        }
        let path_string = self.string(&path_text);
        emitter.emit(Instruction::PushText(path_string), span)?;
        if let Some(offset) = offset {
            self.compile_value_of_type(emitter, offset, scope, Type::NUMBER)?;
        }
        self.note_write(
            target,
            Range {
                lowest: 0,
                highest: LARGEST_BYTE,
            },
        );
        emitter.emit(
            Instruction::WorkArray {
                work: match offset {
                    Some(_) => ArrayWork::FileBytesFrom,
                    None => ArrayWork::FileBytes,
                },
                destination: target,
                source: target,
            },
            span,
        )?;
        Ok(())
    }

    /// Numbers, the second a swipe direction when `last_is_direction` is set; gives the range of each.
    fn compile_array_arguments(
        &mut self,
        emitter: &mut Emitter,
        last_is_direction: bool,
        arguments: &[&'a Expression<'a>],
        scope: &Rc<Scope<'a>>,
    ) -> Compiled<Vec<Range>> {
        let mut ranges = Vec::new();
        for (place, argument) in arguments.iter().enumerate() {
            let expected = match last_is_direction && place + 1 == arguments.len() {
                true => Type::choice(ChoiceSet::SwipeDirection),
                false => Type::NUMBER,
            };
            ranges.push(
                self.compile_value_of_type(emitter, argument, scope, expected)?
                    .range,
            );
        }
        Ok(ranges)
    }
}

fn question_takes_direction(question: ArrayQuestion) -> bool {
    matches!(question, ArrayQuestion::SlideChanges | ArrayQuestion::SlidePoints)
}

#[cfg(test)]
#[path = "array_builtins_tests.rs"]
mod array_builtins_tests;
