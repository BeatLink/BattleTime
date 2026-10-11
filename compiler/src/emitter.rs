//! Assembles one expression or handler: the runtime's `CodeWriter`, plus the stack depth and
//! locals the compiler must keep track of to produce code the verifier accepts.

use neotime_runtime::instruction::Instruction;
use neotime_runtime::interpreter::{MOST_LOCALS, STACK_DEPTH};
use neotime_runtime::writer::{CodeWriter, Label};
use oxc_span::Span;

use crate::diagnostics::{fail, Compiled, Diagnostic};
use crate::types::Type;

/// The most jumps that may wait for their target at once, as the runtime's checks allow.
const MOST_OPEN_JUMPS: usize = 16;

/// The most loops one piece of code may have, as the runtime's checks allow.
const MOST_LOOPS: usize = 16;

// ============================================================================
// Kinds of code
// ============================================================================

/// Whether code works out a widget's value or answers an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeKind {
    Expression,
    Handler,
}

/// A function being inlined, so `return` knows where to go and where to put its value.
pub struct InlineFrame {
    pub end: Label,
    pub result_local: Option<u8>,
    pub result_type: Option<Type>,
    /// The range of every value returned so far.
    pub result_range: Option<crate::types::Range>,
    /// The type the caller expects, which turns names such as colours into numbers.
    pub expected: Option<Type>,
}

// ============================================================================
// Emitter
// ============================================================================

pub struct Emitter {
    writer: CodeWriter,
    pub kind: CodeKind,
    /// The stack depth here, or `None` where no path reaches.
    depth: Option<usize>,
    label_depths: Vec<(Label, usize)>,
    locals_in_use: u8,
    loop_count: usize,
    /// The targets of jumps not yet placed, one entry per jump, as the runtime's checks count them.
    open_jumps: Vec<Label>,
    pub inline_frames: Vec<InlineFrame>,
}

impl Emitter {
    pub fn new(kind: CodeKind) -> Emitter {
        Emitter {
            writer: CodeWriter::new(),
            kind,
            depth: Some(0),
            label_depths: Vec::new(),
            locals_in_use: 0,
            loop_count: 0,
            open_jumps: Vec::new(),
            inline_frames: Vec::new(),
        }
    }

    pub fn is_handler(&self) -> bool {
        self.kind == CodeKind::Handler
    }

    pub fn depth(&self) -> Option<usize> {
        self.depth
    }

    /// Writes one instruction and follows its effect on the stack.
    pub fn emit(&mut self, instruction: Instruction, span: Span) -> Compiled<()> {
        if let Some(depth) = self.depth {
            let effect = instruction.stack_effect();
            let Some(after) = depth.checked_sub(effect.taken) else {
                return fail(span, "internal compiler error: the stack would go below empty");
            };
            let after = after + effect.left;
            if after > STACK_DEPTH {
                return fail(
                    span,
                    format!("this expression needs more than {STACK_DEPTH} values at once; split it with `const` in a handler"),
                );
            }
            self.depth = Some(after);
        }
        self.writer.emit(instruction);
        Ok(())
    }

    pub fn label(&mut self) -> Label {
        self.writer.label()
    }

    pub fn place(&mut self, label: Label) {
        let recorded = self
            .label_depths
            .iter()
            .find(|(placed, _)| *placed == label)
            .map(|(_, depth)| *depth);
        if self.depth.is_none() {
            self.depth = recorded;
        }
        self.open_jumps.retain(|open| *open != label);
        self.writer.place(label);
    }

    fn record_jump(&mut self, target: Label, span: Span) -> Compiled<()> {
        self.open_jumps.push(target);
        if self.open_jumps.len() > MOST_OPEN_JUMPS {
            return fail(
                span,
                format!("this code has more than {MOST_OPEN_JUMPS} branches waiting to join at once; split long `? :` or `&&` chains with `const`"),
            );
        }
        self.record_jump_depth(target);
        Ok(())
    }

    fn record_jump_depth(&mut self, target: Label) {
        if let Some(depth) = self.depth {
            if !self.label_depths.iter().any(|(placed, _)| *placed == target) {
                self.label_depths.push((target, depth));
            }
        }
    }

    pub fn jump(&mut self, target: Label, span: Span) -> Compiled<()> {
        self.record_jump(target, span)?;
        self.writer.jump(target);
        self.depth = None;
        Ok(())
    }

    pub fn jump_if_false(&mut self, target: Label, span: Span) -> Compiled<()> {
        match self.depth {
            Some(0) => return fail(span, "internal compiler error: a jump with nothing to test"),
            Some(depth) => self.depth = Some(depth - 1),
            None => {}
        }
        self.record_jump(target, span)?;
        self.writer.jump_if_false(target);
        Ok(())
    }

    /// Starts a loop, which needs an empty stack.
    pub fn loop_start(&mut self, span: Span) -> Compiled<Label> {
        if self.depth.is_some_and(|depth| depth != 0) {
            return fail(
                span,
                "a loop cannot run in the middle of an expression; store the function's result with `const` first",
            );
        }
        self.loop_count += 1;
        if self.loop_count > MOST_LOOPS {
            return fail(
                span,
                format!("one handler or expression may run at most {MOST_LOOPS} loops, counting those in the functions it calls; split the work across handlers"),
            );
        }
        Ok(self.writer.loop_start())
    }

    pub fn loop_back(&mut self, start: Label, span: Span) -> Compiled<()> {
        if self.depth.is_some_and(|depth| depth != 0) {
            return fail(
                span,
                "internal compiler error: a loop ends with values on the stack",
            );
        }
        self.writer.loop_back(start);
        self.depth = None;
        Ok(())
    }

    // ------------------------------------------------------------------------
    // Locals
    // ------------------------------------------------------------------------

    /// Takes the next free local, freed again with `release_locals`.
    pub fn take_local(&mut self, span: Span) -> Compiled<u8> {
        if self.locals_in_use as usize >= MOST_LOCALS {
            return fail(
                span,
                format!("a handler or expression may use at most {MOST_LOCALS} local values at once; move some work into another function"),
            );
        }
        self.locals_in_use += 1;
        Ok(self.locals_in_use - 1)
    }

    pub fn locals_in_use(&self) -> u8 {
        self.locals_in_use
    }

    /// Frees every local taken after `mark`, at the end of a block.
    pub fn release_locals(&mut self, mark: u8) {
        self.locals_in_use = mark;
    }

    /// The finished code, which must leave one value for an expression and none for a handler.
    pub fn finish(self, span: Span) -> Compiled<Vec<u8>> {
        let expected = match self.kind {
            CodeKind::Expression => 1,
            CodeKind::Handler => 0,
        };
        if self.depth.is_some_and(|depth| depth != expected) {
            return fail(
                span,
                "internal compiler error: the code leaves the wrong number of values",
            );
        }
        self.writer
            .finish()
            .map_err(|problem| Diagnostic::error(span, format!("internal compiler error: {problem:?}")))
    }
}
