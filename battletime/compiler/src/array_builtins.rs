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
use crate::types::{Kind, Range, Type};

/// The functions that give back a new array, which only an assignment to an array signal can take.
pub const ARRAY_WORK_FUNCTIONS: [&str; 4] = ["push_shift", "unshift_pop", "shuffled", "file_bytes"];

/// The largest byte a file holds, the most `file_bytes` writes to an element.
const LARGEST_BYTE: i64 = 255;

/// Whether `name` is one of the array questions `compile_array_question` compiles.
pub fn is_array_question(name: &str) -> bool {
    matches!(
        name,
        "count_of" | "nth_index_of" | "line_through" | "arrays_equal" | "count_inversions"
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
        self.compile_array_arguments(emitter, &arguments[1..], scope)?;
        emitter.emit(Instruction::AskArray { question, array }, span)?;
        Ok(Typed::new(result))
    }

    /// `target.value = push_shift(...)`, or one of the other functions that give a whole array, written straight
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
            _ => {
                self.compile_array_arguments(emitter, rest, scope)?;
                source_range
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

    /// The numbers an array function takes after its array.
    fn compile_array_arguments(
        &mut self,
        emitter: &mut Emitter,
        arguments: &[&'a Expression<'a>],
        scope: &Rc<Scope<'a>>,
    ) -> Compiled<()> {
        for argument in arguments {
            self.compile_value_of_type(emitter, argument, scope, Type::NUMBER)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "array_builtins_tests.rs"]
mod array_builtins_tests;
