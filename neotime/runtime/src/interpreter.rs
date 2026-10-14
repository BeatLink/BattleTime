//! Runs an app's expressions and handlers.
//!
//! An expression works out one value for a widget. A handler answers an event. The first time a
//! handler writes to a variable, the variable is copied into a scratch area and the handler works
//! on the copy; the copies are written back only when it finishes, so a handler that fails part
//! way changes nothing. Its effects are only collected. Every check is repeated here even for
//! verified code, so a bad image fails a call instead of crashing the watch.
//!
//! A function call keeps its own locals above its caller's, and sees only the stack above the
//! arguments it took; the caller's place is kept in a frame until the function's code ends.

#[path = "array_work.rs"]
mod array_work;

pub use array_work::MOST_GRID_SIDE;

use crate::image::{AppImage, Function};
use crate::instruction::{
    decode, ArrayWork, EffectKind, Instruction, SystemValue, TimeUnit, MOST_EFFECT_ARGUMENTS,
};
use crate::pattern;
use crate::state::{read_packed, write_packed, AppState, Placement};
use crate::text::{DateTime, TextReference, TextWriter, MOST_FIXED_PLACES};

// ============================================================================
// Values and limits
// ============================================================================

/// Every value is 64 bits; the compiler has already checked what type it is.
pub type Value = i64;

/// The value of an empty optional, such as a stopwatch that has not started.
pub const NONE: Value = i64::MIN;

pub const STACK_DEPTH: usize = 32;
pub const TEXT_BUFFER_BYTES: usize = 256;
pub const MOST_EVENT_FIELDS: usize = 8;
pub const MOST_LOCALS: usize = 16;

/// How many calls may be under way at once.
pub const MOST_CALL_DEPTH: usize = 8;

/// Room for the locals of an expression or handler and of every call under way inside it.
pub const LOCAL_POOL: usize = 48;
pub const MOST_EFFECTS: usize = 8;

/// How many variables one handler may change.
pub const MOST_CHANGED_VARIABLES: usize = 16;

/// Room for the copies of the variables a handler changes; it holds the largest variable allowed.
pub const SCRATCH_BYTES: usize = crate::image::MOST_VARIABLE_BYTES;

/// The most instructions one call may run, about a fifth of a second on the watch.
pub const MOST_STEPS: u32 = 200_000;

/// Where the interpreter reads the values the system keeps current.
pub trait SystemValues {
    fn read(&self, value: SystemValue) -> Value;

    /// A system value that is text, such as the playing track's title; empty when there is none.
    fn read_text(&self, _value: SystemValue) -> &str {
        ""
    }

    /// One element of a list value, such as one day of the forecast; none past its end.
    fn read_element(&self, _value: SystemValue, _index: Value) -> Value {
        NONE
    }

    /// One element of a text list, such as an installed app's name; empty past its end.
    fn read_element_text(&self, _value: SystemValue, _index: Value) -> &str {
        ""
    }

    /// A fresh random number, such as one from the chip's random number generator.
    fn random_number(&self) -> u32;

    /// Reads part of a file from the watch's storage into `out`, from `offset` bytes in.
    ///
    /// Gives how many bytes were read, fewer than `out` holds only at the file's end, or none when
    /// the file cannot be read.
    fn read_file(&self, _path: &str, _offset: u32, _out: &mut [u8]) -> Option<usize> {
        None
    }
}

/// Why a call stopped. Its writes and effects are dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    NoSuchCode,
    BadInstruction,
    StackUnderflow,
    StackOverflow,
    Overflow,
    DivideByZero,
    BadVariable,
    IndexOutOfRange,
    BadLocal,
    BadEventField,
    BadText,
    TextBufferFull,
    TooManyChangedVariables,
    ScratchFull,
    TooManyEffects,
    TooManySteps,
    WrongResultCount,
    NotAllowedHere,
    CallsTooDeep,
    /// The code called the shared library before the host was given one.
    NoLibrary,
}

// ============================================================================
// Handler results
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effect {
    pub kind: EffectKind,
    pub arguments: [Value; MOST_EFFECT_ARGUMENTS],
}

/// What a finished handler did: which variables it changed, and the effects it asked for in order.
#[derive(Clone, Copy, Debug)]
pub struct HandlerOutcome {
    changed_variables: u64,
    effects: [Effect; MOST_EFFECTS],
    effect_count: usize,
}

impl HandlerOutcome {
    const fn empty() -> Self {
        HandlerOutcome {
            changed_variables: 0,
            effects: [Effect {
                kind: EffectKind::Back,
                arguments: [0; MOST_EFFECT_ARGUMENTS],
            }; MOST_EFFECTS],
            effect_count: 0,
        }
    }

    /// One bit per state variable the handler wrote to, for working out what to redraw.
    pub fn changed_variables(&self) -> u64 {
        self.changed_variables
    }

    pub fn effects(&self) -> &[Effect] {
        &self.effects[..self.effect_count]
    }

    fn add_effect(&mut self, effect: Effect) -> Result<(), Failure> {
        let free = self
            .effects
            .get_mut(self.effect_count)
            .ok_or(Failure::TooManyEffects)?;
        *free = effect;
        self.effect_count += 1;
        Ok(())
    }
}

/// A variable a handler has written to, and where its working copy is in the scratch area.
#[derive(Clone, Copy, Debug, Default)]
struct WorkingCopy {
    variable: u8,
    offset: u16,
    length: u16,
}

/// Where the next instruction is, after a jump or a call.
enum JumpTo {
    Forward(u16),
    Back(u16),
    Call(u8),
    CallLibrary {
        function: u16,
        arguments: u8,
        results: u8,
    },
}

/// Which code is running: the expression or handler itself, one of the app's functions, or one of
/// the shared library's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Running {
    #[default]
    Top,
    App(u8),
    Library(u16),
}

/// Where to carry on when a called function ends.
#[derive(Clone, Copy, Debug, Default)]
struct Frame {
    /// The code that called, to carry on in when the function ends.
    caller: Running,
    return_offset: u16,
    caller_locals_base: u8,
    caller_local_count: u8,
    caller_stack_floor: u8,
    result_count: u8,
}

// ============================================================================
// Interpreter
// ============================================================================

/// What a call may read and where its handler results go.
struct Call<'call, System: SystemValues> {
    image: &'call AppImage<'call>,
    /// The shared library, whose functions `CallLibrary` runs.
    library: Option<&'call AppImage<'call>>,
    state: &'call AppState,
    event: &'call [Value],
    event_texts: &'call [&'call str],
    system: &'call System,
    outcome: Option<&'call mut HandlerOutcome>,
}

/// The interpreter's working memory, reused for every call.
pub struct Interpreter {
    stack: [Value; STACK_DEPTH],
    depth: usize,
    /// The running code may not take values below this, which belong to its callers.
    stack_floor: usize,
    locals: [Value; LOCAL_POOL],
    locals_base: usize,
    local_count: usize,
    frames: [Frame; MOST_CALL_DEPTH],
    frame_count: usize,
    /// The row of a Repeat that code runs for, which `ReadRow` pushes.
    row: Value,
    text_buffer: [u8; TEXT_BUFFER_BYTES],
    text_used: usize,
    scratch: [u8; SCRATCH_BYTES],
    scratch_used: usize,
    copies: [WorkingCopy; MOST_CHANGED_VARIABLES],
    copy_count: usize,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    pub const fn new() -> Self {
        Interpreter {
            stack: [0; STACK_DEPTH],
            depth: 0,
            stack_floor: 0,
            locals: [0; LOCAL_POOL],
            locals_base: 0,
            local_count: MOST_LOCALS,
            frames: [Frame {
                caller: Running::Top,
                return_offset: 0,
                caller_locals_base: 0,
                caller_local_count: 0,
                caller_stack_floor: 0,
                result_count: 0,
            }; MOST_CALL_DEPTH],
            frame_count: 0,
            row: 0,
            text_buffer: [0; TEXT_BUFFER_BYTES],
            text_used: 0,
            scratch: [0; SCRATCH_BYTES],
            scratch_used: 0,
            copies: [WorkingCopy {
                variable: 0,
                offset: 0,
                length: 0,
            }; MOST_CHANGED_VARIABLES],
            copy_count: 0,
        }
    }

    /// Sets the row of a Repeat that the next calls run for; 0 outside one.
    pub fn set_row(&mut self, row: Value) {
        self.row = row;
    }

    /// Works out an expression's value. A text result stays readable until the next call.
    pub fn evaluate_expression(
        &mut self,
        image: &AppImage,
        expression: u16,
        state: &AppState,
        system: &impl SystemValues,
    ) -> Result<Value, Failure> {
        self.evaluate_expression_with_library(image, None, expression, state, system)
    }

    /// Like `evaluate_expression`, with the shared library whose functions the code may call.
    pub fn evaluate_expression_with_library(
        &mut self,
        image: &AppImage,
        library: Option<&AppImage>,
        expression: u16,
        state: &AppState,
        system: &impl SystemValues,
    ) -> Result<Value, Failure> {
        let code = image.expression_code(expression).ok_or(Failure::NoSuchCode)?;
        let call = Call {
            image,
            library,
            state,
            event: &[],
            event_texts: &[],
            system,
            outcome: None,
        };
        self.run(code, call)?;

        if self.depth != 1 {
            return Err(Failure::WrongResultCount);
        }
        Ok(self.stack[0])
    }

    /// Runs a handler for an event. Only if it finishes are its writes applied to `state`.
    pub fn run_handler(
        &mut self,
        image: &AppImage,
        handler: u16,
        state: &mut AppState,
        event: &[Value],
        system: &impl SystemValues,
    ) -> Result<HandlerOutcome, Failure> {
        self.run_handler_with_text(image, handler, state, event, &[], system)
    }

    /// Like `run_handler`, for an event with text fields; `event_texts` holds them by field number.
    pub fn run_handler_with_text(
        &mut self,
        image: &AppImage,
        handler: u16,
        state: &mut AppState,
        event: &[Value],
        event_texts: &[&str],
        system: &impl SystemValues,
    ) -> Result<HandlerOutcome, Failure> {
        self.run_handler_with_library(image, None, handler, state, event, event_texts, system)
    }

    /// Like `run_handler_with_text`, with the shared library whose functions the code may call.
    #[allow(clippy::too_many_arguments)]
    pub fn run_handler_with_library(
        &mut self,
        image: &AppImage,
        library: Option<&AppImage>,
        handler: u16,
        state: &mut AppState,
        event: &[Value],
        event_texts: &[&str],
        system: &impl SystemValues,
    ) -> Result<HandlerOutcome, Failure> {
        let code = image.handler_code(handler).ok_or(Failure::NoSuchCode)?;
        let mut outcome = HandlerOutcome::empty();
        let call = Call {
            image,
            library,
            state,
            event,
            event_texts,
            system,
            outcome: Some(&mut outcome),
        };
        self.run(code, call)?;

        if self.depth != 0 {
            return Err(Failure::WrongResultCount);
        }
        for copy in &self.copies[..self.copy_count] {
            let source = &self.scratch[copy.offset as usize..(copy.offset + copy.length) as usize];
            state
                .variable_bytes_mut(copy.variable)
                .ok_or(Failure::BadVariable)?
                .copy_from_slice(source);
            outcome.changed_variables |= 1 << copy.variable;
        }
        Ok(outcome)
    }

    /// The text a text value refers to, from the image or from the last call's text buffer.
    pub fn text<'result>(
        &'result self,
        image: &AppImage<'result>,
        value: Value,
    ) -> Result<&'result str, Failure> {
        match TextReference::from_value(value).ok_or(Failure::BadText)? {
            TextReference::ImageString(index) => image.string(index).ok_or(Failure::BadText),
            TextReference::Buffer { start, length } => {
                if start + length > self.text_used {
                    return Err(Failure::BadText);
                }
                core::str::from_utf8(&self.text_buffer[start..start + length]).map_err(|_| Failure::BadText)
            }
        }
    }

    fn run<System: SystemValues>(&mut self, top_code: &[u8], mut call: Call<System>) -> Result<(), Failure> {
        self.depth = 0;
        self.stack_floor = 0;
        self.text_used = 0;
        self.scratch_used = 0;
        self.copy_count = 0;
        self.locals_base = 0;
        self.local_count = MOST_LOCALS;
        self.locals[..MOST_LOCALS].fill(0);
        self.frame_count = 0;
        let image = call.image;
        let library = call.library;
        let mut code = top_code;
        let mut running = Running::Top;
        let mut offset = 0;
        let mut steps: u32 = 0;

        loop {
            if offset >= code.len() {
                let Some(frame) = self.leave_function()? else {
                    return Ok(());
                };
                running = frame.caller;
                code = match frame.caller {
                    Running::Top => top_code,
                    Running::App(caller) => image.function(caller as u16).ok_or(Failure::NoSuchCode)?.code,
                    Running::Library(caller) => {
                        library
                            .and_then(|library| library.function(caller))
                            .ok_or(Failure::NoSuchCode)?
                            .code
                    }
                };
                offset = frame.return_offset as usize;
                continue;
            }
            steps += 1;
            if steps > MOST_STEPS {
                return Err(Failure::TooManySteps);
            }
            let (instruction, next_offset) = decode(code, offset).map_err(|_| Failure::BadInstruction)?;
            let in_library = matches!(running, Running::Library(_));
            let code_image = match in_library {
                true => library.ok_or(Failure::NoSuchCode)?,
                false => image,
            };
            offset = match self.execute(instruction, &mut call, code_image, in_library)? {
                None => next_offset,
                Some(JumpTo::Forward(distance)) => next_offset + distance as usize,
                Some(JumpTo::Back(distance)) => next_offset
                    .checked_sub(distance as usize)
                    .ok_or(Failure::BadInstruction)?,
                // A call from library code is to another of the library's functions.
                Some(JumpTo::Call(callee)) => {
                    let function = code_image.function(callee as u16).ok_or(Failure::NoSuchCode)?;
                    self.enter_function(function, running, next_offset)?;
                    running = match in_library {
                        true => Running::Library(callee as u16),
                        false => Running::App(callee),
                    };
                    code = function.code;
                    0
                }
                Some(JumpTo::CallLibrary {
                    function: callee,
                    arguments,
                    results,
                }) => {
                    let function = library
                        .ok_or(Failure::NoLibrary)?
                        .function(callee)
                        .filter(|function| {
                            function.argument_count == arguments && function.result_count == results
                        })
                        .ok_or(Failure::NoSuchCode)?;
                    self.enter_function(function, running, next_offset)?;
                    running = Running::Library(callee);
                    code = function.code;
                    0
                }
            };
        }
    }

    /// Moves a call's arguments into its own locals and remembers where the caller was.
    fn enter_function(
        &mut self,
        function: Function,
        caller: Running,
        return_offset: usize,
    ) -> Result<(), Failure> {
        let locals_base = self.locals_base + self.local_count;
        let local_count = function.local_count as usize;
        let argument_count = function.argument_count as usize;
        if self.frame_count == MOST_CALL_DEPTH
            || locals_base + local_count > LOCAL_POOL
            || argument_count > local_count
            || function.result_count > 1
        {
            return Err(Failure::CallsTooDeep);
        }

        self.frames[self.frame_count] = Frame {
            caller,
            return_offset: u16::try_from(return_offset).map_err(|_| Failure::BadInstruction)?,
            caller_locals_base: self.locals_base as u8,
            caller_local_count: self.local_count as u8,
            caller_stack_floor: self.stack_floor as u8,
            result_count: function.result_count,
        };
        self.frame_count += 1;
        self.locals[locals_base..locals_base + local_count].fill(0);
        for local in (0..argument_count).rev() {
            self.locals[locals_base + local] = self.pop()?;
        }
        self.locals_base = locals_base;
        self.local_count = local_count;
        self.stack_floor = self.depth;
        Ok(())
    }

    /// Ends the running function and gives back its caller's frame; none when nothing was called.
    fn leave_function(&mut self) -> Result<Option<Frame>, Failure> {
        if self.frame_count == 0 {
            return Ok(None);
        }
        self.frame_count -= 1;
        let frame = self.frames[self.frame_count];
        if self.depth != self.stack_floor + frame.result_count as usize {
            return Err(Failure::WrongResultCount);
        }
        self.locals_base = frame.caller_locals_base as usize;
        self.local_count = frame.caller_local_count as usize;
        self.stack_floor = frame.caller_stack_floor as usize;
        Ok(Some(frame))
    }

    /// Carries out one instruction, and says where to go next when it jumps.
    fn execute<System: SystemValues>(
        &mut self,
        instruction: Instruction,
        call: &mut Call<System>,
        code_image: &AppImage,
        in_library: bool,
    ) -> Result<Option<JumpTo>, Failure> {
        use Instruction::*;

        // Library code is the same for every app, so it may not touch an app's state or its event.
        if in_library && touches_the_app(instruction) {
            return Err(Failure::NotAllowedHere);
        }
        match instruction {
            // The library's own text is copied out, so what it gives back never names its strings.
            PushText(index) if in_library => {
                let text = code_image.string(index).ok_or(Failure::BadText)?;
                self.write_text(|writer| writer.write_bytes(text.as_bytes()))?;
            }
            FormatDuration(pattern_string) if in_library => {
                let milliseconds = self.pop()?;
                let pattern = code_image.string(pattern_string).ok_or(Failure::BadText)?;
                self.write_text(|writer| pattern::write_duration(writer, milliseconds, pattern))?;
            }
            FormatTime(pattern_string) if in_library => {
                let time = self.local_time(call)?;
                let pattern = code_image.string(pattern_string).ok_or(Failure::BadText)?;
                let uses_24_hour_clock = call.system.read(SystemValue::Uses24HourClock) != 0;
                self.write_text(|writer| pattern::write_time(writer, &time, pattern, uses_24_hour_clock))?;
            }
            PushSmall(value) => self.push(value as Value)?,
            PushInt(value) => self.push(value as Value)?,
            PushLong(value) => self.push(value)?,
            PushNone => self.push(NONE)?,
            PushText(index) => self.push(TextReference::ImageString(index).to_value())?,
            ReadState(variable) => {
                let value = self.read_element(call.state, variable, 0, false)?;
                self.push(value)?;
            }
            ReadElement(variable) => {
                let index = self.pop()?;
                let value = self.read_element(call.state, variable, index, true)?;
                self.push(value)?;
            }
            ReadSystem(value) if value.is_text() => {
                let text = call.system.read_text(value);
                self.write_text(|writer| writer.write_bytes(text.as_bytes()))?;
            }
            ReadSystem(value) => self.push(call.system.read(value))?,
            ReadSystemElement(value) if value.is_text() => {
                let index = self.pop()?;
                let text = call.system.read_element_text(value, index);
                self.write_text(|writer| writer.write_bytes(text.as_bytes()))?;
            }
            ReadSystemElement(value) => {
                let index = self.pop()?;
                self.push(call.system.read_element(value, index))?;
            }
            ReadEvent(field) => {
                if call.outcome.is_none() {
                    return Err(Failure::NotAllowedHere);
                }
                self.push(*call.event.get(field as usize).ok_or(Failure::BadEventField)?)?;
            }
            ReadEventText(field) => {
                if call.outcome.is_none() {
                    return Err(Failure::NotAllowedHere);
                }
                let text = *call.event_texts.get(field as usize).unwrap_or(&"");
                self.write_text(|writer| writer.write_bytes(text.as_bytes()))?;
            }
            ReadLocal(local) => {
                let value = self.locals[self.local_index(local)?];
                self.push(value)?;
            }
            WriteLocal(local) => {
                let value = self.pop()?;
                let index = self.local_index(local)?;
                self.locals[index] = value;
            }

            Duplicate => {
                let value = self.pop()?;
                self.push(value)?;
                self.push(value)?;
            }
            Drop => {
                self.pop()?;
            }
            Swap => {
                let top = self.pop()?;
                let below = self.pop()?;
                self.push(top)?;
                self.push(below)?;
            }

            Add => self.arithmetic(|left, right| left.checked_add(right).ok_or(Failure::Overflow))?,
            Subtract => self.arithmetic(|left, right| left.checked_sub(right).ok_or(Failure::Overflow))?,
            Multiply => self.arithmetic(|left, right| left.checked_mul(right).ok_or(Failure::Overflow))?,
            Divide => self.arithmetic(|left, right| match right {
                0 => Err(Failure::DivideByZero),
                _ => left.checked_div(right).ok_or(Failure::Overflow),
            })?,
            Remainder => self.arithmetic(|left, right| match right {
                0 => Err(Failure::DivideByZero),
                _ => left.checked_rem(right).ok_or(Failure::Overflow),
            })?,
            Minimum => self.arithmetic(|left, right| Ok(left.min(right)))?,
            Maximum => self.arithmetic(|left, right| Ok(left.max(right)))?,
            RandomInt => {
                if call.outcome.is_none() {
                    return Err(Failure::NotAllowedHere);
                }
                let highest = self.pop()?;
                let lowest = self.pop()?;
                if highest < lowest {
                    return Err(Failure::Overflow);
                }
                let span = (highest as i128 - lowest as i128 + 1) as u128;
                let random = call.system.random_number() as u128;
                self.push((lowest as i128 + (random % span) as i128) as Value)?;
            }
            BitAnd => self.arithmetic(|left, right| Ok(left & right))?,
            BitOr => self.arithmetic(|left, right| Ok(left | right))?,
            BitXor => self.arithmetic(|left, right| Ok(left ^ right))?,
            BitNot => {
                let value = self.pop()?;
                self.push(!value)?;
            }
            ShiftLeft => self.arithmetic(|value, places| match u32::try_from(places) {
                Ok(places) if places < 64 => Ok(value << places),
                _ => Err(Failure::Overflow),
            })?,
            ShiftRight => self.arithmetic(|value, places| match u32::try_from(places) {
                Ok(places) if places < 64 => Ok(value >> places),
                _ => Err(Failure::Overflow),
            })?,
            Negate => {
                let value = self.pop()?;
                self.push(value.checked_neg().ok_or(Failure::Overflow)?)?;
            }
            Absolute => {
                let value = self.pop()?;
                self.push(value.checked_abs().ok_or(Failure::Overflow)?)?;
            }

            Equal => self.arithmetic(|left, right| Ok((left == right) as Value))?,
            NotEqual => self.arithmetic(|left, right| Ok((left != right) as Value))?,
            Less => self.arithmetic(|left, right| Ok((left < right) as Value))?,
            LessOrEqual => self.arithmetic(|left, right| Ok((left <= right) as Value))?,
            Greater => self.arithmetic(|left, right| Ok((left > right) as Value))?,
            GreaterOrEqual => self.arithmetic(|left, right| Ok((left >= right) as Value))?,
            Not => {
                let value = self.pop()?;
                self.push((value == 0) as Value)?;
            }
            IsNone => {
                let value = self.pop()?;
                self.push((value == NONE) as Value)?;
            }

            Jump(distance) => return Ok(Some(JumpTo::Forward(distance))),
            JumpIfFalse(distance) => {
                if self.pop()? == 0 {
                    return Ok(Some(JumpTo::Forward(distance)));
                }
            }
            LoopStart => {}
            LoopBack(distance) => return Ok(Some(JumpTo::Back(distance))),
            Call(function) => return Ok(Some(JumpTo::Call(function))),
            CallLibrary {
                function,
                arguments,
                results,
            } => {
                return Ok(Some(JumpTo::CallLibrary {
                    function,
                    arguments,
                    results,
                }))
            }
            ReadRow => self.push(self.row)?,

            TimePart(part) => {
                let time = self.local_time(call)?;
                self.push(time.part(part))?;
            }
            StartOf(unit) => {
                let instant = self.pop()?;
                let utc_offset = call.system.read(SystemValue::UtcOffset);
                let local = instant.checked_add(utc_offset).ok_or(Failure::Overflow)?;
                let unit_in_milliseconds: Value = match unit {
                    TimeUnit::Minute => 60_000,
                    TimeUnit::Hour => 3_600_000,
                    TimeUnit::Day => 86_400_000,
                };
                self.push(instant - local.rem_euclid(unit_in_milliseconds))?;
            }
            FormatNumber(minimum_digits) => {
                let number = self.pop()?;
                self.write_text(|writer| writer.write_number(number, minimum_digits))?;
            }
            FormatDuration(pattern_string) => {
                let milliseconds = self.pop()?;
                let pattern = call.image.string(pattern_string).ok_or(Failure::BadText)?;
                self.write_text(|writer| pattern::write_duration(writer, milliseconds, pattern))?;
            }
            FormatTime(pattern_string) => {
                let time = self.local_time(call)?;
                let pattern = call.image.string(pattern_string).ok_or(Failure::BadText)?;
                let uses_24_hour_clock = call.system.read(SystemValue::Uses24HourClock) != 0;
                self.write_text(|writer| pattern::write_time(writer, &time, pattern, uses_24_hour_clock))?;
            }
            Join => self.join(call.image)?,
            ToUpper => self.to_upper(call.image)?,
            TextLength => self.text_length(call.image)?,
            SliceText => self.slice_text(call.image)?,
            FormatFixed => {
                let trim = self.pop()? != 0;
                let places = self.pop()?.clamp(0, MOST_FIXED_PLACES as Value) as u8;
                let value = self.pop()?;
                self.write_text(|writer| writer.write_fixed(value, places, trim))?;
            }
            PadStart => self.pad_start(call.image)?,
            TextFromCodes(array) => {
                let count = self.pop()?;
                let length = Self::array_placement(call.state, array)?.length as Value;
                if !(0..=length).contains(&count) {
                    return Err(Failure::IndexOutOfRange);
                }
                let start = self.text_used;
                for index in 0..count {
                    let code = self.read_element(call.state, array, index, true)?;
                    let character = u32::try_from(code)
                        .ok()
                        .and_then(char::from_u32)
                        .ok_or(Failure::BadText)?;
                    let mut encoded = [0u8; 4];
                    let bytes = character.encode_utf8(&mut encoded).as_bytes();
                    let mut writer = TextWriter::new(&mut self.text_buffer, self.text_used);
                    writer.write_bytes(bytes).map_err(|_| Failure::TextBufferFull)?;
                    self.text_used = writer.position();
                }
                self.push(
                    TextReference::Buffer {
                        start,
                        length: self.text_used - start,
                    }
                    .to_value(),
                )?;
            }

            WriteState(variable) => {
                Self::require_handler(call)?;
                let value = self.pop()?;
                self.write_element(call.state, variable, 0, value, false)?;
            }
            WriteElement(variable) => {
                Self::require_handler(call)?;
                let value = self.pop()?;
                let index = self.pop()?;
                self.write_element(call.state, variable, index, value, true)?;
            }
            FillArray(variable) => {
                Self::require_handler(call)?;
                let value = self.pop()?;
                let length = Self::array_placement(call.state, variable)?.length;
                for index in 0..length as Value {
                    self.write_element(call.state, variable, index, value, true)?;
                }
            }
            CopyArray { destination, source } => {
                Self::require_handler(call)?;
                let length = Self::array_placement(call.state, destination)?.length;
                if Self::array_placement(call.state, source)?.length != length {
                    return Err(Failure::IndexOutOfRange);
                }
                if destination != source {
                    for index in 0..length as Value {
                        let value = self.read_element(call.state, source, index, true)?;
                        self.write_element(call.state, destination, index, value, true)?;
                    }
                }
            }
            AskArray { question, array } => self.ask_array(call.state, question, array)?,
            WorkArray {
                work: ArrayWork::FileBytes,
                destination,
                ..
            } => {
                Self::require_handler(call)?;
                let image = call.image;
                let path = match TextReference::from_value(self.pop()?) {
                    Some(TextReference::ImageString(index)) => image.string(index),
                    _ => None,
                }
                .ok_or(Failure::BadText)?;
                self.read_file_bytes(call.state, destination, path, call.system)?;
            }
            WorkArray {
                work,
                destination,
                source,
            } => {
                if work.writes() {
                    Self::require_handler(call)?;
                }
                let system = &call.system;
                self.work_array(call.state, work, destination, source, &|| system.random_number())?;
            }
            Effect(kind) => {
                let mut arguments = [0; MOST_EFFECT_ARGUMENTS];
                for index in (0..kind.argument_count()).rev() {
                    arguments[index] = self.pop()?;
                }
                let outcome = call.outcome.as_mut().ok_or(Failure::NotAllowedHere)?;
                outcome.add_effect(self::Effect { kind, arguments })?;
            }
        }
        Ok(None)
    }

    fn require_handler<System: SystemValues>(call: &Call<System>) -> Result<(), Failure> {
        match call.outcome {
            Some(_) => Ok(()),
            None => Err(Failure::NotAllowedHere),
        }
    }

    fn array_placement(state: &AppState, variable: u8) -> Result<Placement, Failure> {
        let placement = state.placement(variable).ok_or(Failure::BadVariable)?;
        match placement.is_array() {
            true => Ok(placement),
            false => Err(Failure::BadVariable),
        }
    }

    // ------------------------------------------------------------------------
    // Variables
    // ------------------------------------------------------------------------

    fn working_copy(&self, variable: u8) -> Option<WorkingCopy> {
        self.copies[..self.copy_count]
            .iter()
            .find(|copy| copy.variable == variable)
            .copied()
    }

    /// Reads a single value, or an array element when `is_array` is set.
    fn read_element(
        &self,
        state: &AppState,
        variable: u8,
        index: Value,
        is_array: bool,
    ) -> Result<Value, Failure> {
        let placement = state.placement(variable).ok_or(Failure::BadVariable)?;
        if placement.is_array() != is_array {
            return Err(Failure::BadVariable);
        }
        let range = placement.element_range(index).ok_or(Failure::IndexOutOfRange)?;
        let bytes = match self.working_copy(variable) {
            Some(copy) => &self.scratch[copy.offset as usize..(copy.offset + copy.length) as usize],
            None => state.variable_bytes(variable).ok_or(Failure::BadVariable)?,
        };
        read_packed(bytes.get(range).ok_or(Failure::BadVariable)?).ok_or(Failure::BadVariable)
    }

    /// Writes a single value, or an array element when `is_array` is set, into the working copy.
    fn write_element(
        &mut self,
        state: &AppState,
        variable: u8,
        index: Value,
        value: Value,
        is_array: bool,
    ) -> Result<(), Failure> {
        let placement = state.placement(variable).ok_or(Failure::BadVariable)?;
        if placement.is_array() != is_array {
            return Err(Failure::BadVariable);
        }
        let range = placement.element_range(index).ok_or(Failure::IndexOutOfRange)?;
        let copy = match self.working_copy(variable) {
            Some(copy) => copy,
            None => self.make_working_copy(state, variable)?,
        };
        let bytes = &mut self.scratch[copy.offset as usize..(copy.offset + copy.length) as usize];
        write_packed(&mut bytes[range], value).ok_or(Failure::Overflow)
    }

    fn make_working_copy(&mut self, state: &AppState, variable: u8) -> Result<WorkingCopy, Failure> {
        let original = state.variable_bytes(variable).ok_or(Failure::BadVariable)?;
        if self.copy_count == MOST_CHANGED_VARIABLES {
            return Err(Failure::TooManyChangedVariables);
        }
        let end = self.scratch_used + original.len();
        if end > SCRATCH_BYTES {
            return Err(Failure::ScratchFull);
        }
        self.scratch[self.scratch_used..end].copy_from_slice(original);
        let copy = WorkingCopy {
            variable,
            offset: self.scratch_used as u16,
            length: original.len() as u16,
        };
        self.copies[self.copy_count] = copy;
        self.copy_count += 1;
        self.scratch_used = end;
        Ok(copy)
    }

    // ------------------------------------------------------------------------
    // Stack, time and text
    // ------------------------------------------------------------------------

    fn push(&mut self, value: Value) -> Result<(), Failure> {
        let free = self.stack.get_mut(self.depth).ok_or(Failure::StackOverflow)?;
        *free = value;
        self.depth += 1;
        Ok(())
    }

    fn pop(&mut self) -> Result<Value, Failure> {
        if self.depth <= self.stack_floor {
            return Err(Failure::StackUnderflow);
        }
        self.depth -= 1;
        Ok(self.stack[self.depth])
    }

    /// Where one of the running code's locals is in the pool.
    fn local_index(&self, local: u8) -> Result<usize, Failure> {
        match (local as usize) < self.local_count {
            true => Ok(self.locals_base + local as usize),
            false => Err(Failure::BadLocal),
        }
    }

    fn arithmetic(
        &mut self,
        operation: impl FnOnce(Value, Value) -> Result<Value, Failure>,
    ) -> Result<(), Failure> {
        let right = self.pop()?;
        let left = self.pop()?;
        self.push(operation(left, right)?)
    }

    /// Takes an instant from the stack and splits it in the user's time zone.
    fn local_time<System: SystemValues>(&mut self, call: &Call<System>) -> Result<DateTime, Failure> {
        let instant = self.pop()?;
        let utc_offset = call.system.read(SystemValue::UtcOffset);
        Ok(DateTime::from_local_milliseconds(
            instant.checked_add(utc_offset).ok_or(Failure::Overflow)?,
        ))
    }

    // The text instructions below copy a text onto the stack, so each is kept out of line: inlined
    // into `execute`, their copies would stay in `run`'s frame for every handler.

    #[inline(never)]
    fn to_upper(&mut self, image: &AppImage) -> Result<(), Failure> {
        let value = self.pop()?;
        let (source, length) = self.copy_of_text(image, value)?;
        let text = core::str::from_utf8(&source[..length]).map_err(|_| Failure::BadText)?;
        self.write_text(|writer| {
            for character in text.chars().flat_map(char::to_uppercase) {
                let mut encoded = [0u8; 4];
                writer.write_bytes(character.encode_utf8(&mut encoded).as_bytes())?;
            }
            Ok(())
        })
    }

    #[inline(never)]
    fn text_length(&mut self, image: &AppImage) -> Result<(), Failure> {
        let value = self.pop()?;
        let (source, length) = self.copy_of_text(image, value)?;
        let text = core::str::from_utf8(&source[..length]).map_err(|_| Failure::BadText)?;
        self.push(text.chars().count() as Value)
    }

    #[inline(never)]
    fn slice_text(&mut self, image: &AppImage) -> Result<(), Failure> {
        let end = self.pop()?;
        let start = self.pop()?;
        let value = self.pop()?;
        let (source, length) = self.copy_of_text(image, value)?;
        let text = core::str::from_utf8(&source[..length]).map_err(|_| Failure::BadText)?;
        let count = text.chars().count() as Value;
        let first = javascript_index(start, count);
        let last = javascript_index(end, count).max(first);
        self.write_text(|writer| {
            for character in text.chars().skip(first as usize).take((last - first) as usize) {
                let mut encoded = [0u8; 4];
                writer.write_bytes(character.encode_utf8(&mut encoded).as_bytes())?;
            }
            Ok(())
        })
    }

    #[inline(never)]
    fn pad_start(&mut self, image: &AppImage) -> Result<(), Failure> {
        let code = self.pop()?;
        let width = self.pop()?;
        let value = self.pop()?;
        let padding = u32::try_from(code)
            .ok()
            .and_then(char::from_u32)
            .ok_or(Failure::BadText)?;
        let (source, length) = self.copy_of_text(image, value)?;
        let text = core::str::from_utf8(&source[..length]).map_err(|_| Failure::BadText)?;
        let missing = (width - text.chars().count() as Value).clamp(0, TEXT_BUFFER_BYTES as Value);
        self.write_text(|writer| {
            let mut encoded = [0u8; 4];
            let padding_bytes = padding.encode_utf8(&mut encoded).as_bytes();
            for _ in 0..missing {
                writer.write_bytes(padding_bytes)?;
            }
            writer.write_bytes(text.as_bytes())
        })
    }

    /// A copy of a text value's bytes, so new text can be written while reading it.
    fn copy_of_text(
        &self,
        image: &AppImage,
        value: Value,
    ) -> Result<([u8; TEXT_BUFFER_BYTES], usize), Failure> {
        let text = self.text(image, value)?;
        let mut copy = [0u8; TEXT_BUFFER_BYTES];
        copy.get_mut(..text.len())
            .ok_or(Failure::TextBufferFull)?
            .copy_from_slice(text.as_bytes());
        Ok((copy, text.len()))
    }

    /// Writes new text at the end of the text buffer and pushes a reference to it.
    fn write_text(
        &mut self,
        write: impl FnOnce(&mut TextWriter) -> Result<(), crate::text::BufferFull>,
    ) -> Result<(), Failure> {
        let start = self.text_used;
        let mut writer = TextWriter::new(&mut self.text_buffer, start);
        write(&mut writer).map_err(|_| Failure::TextBufferFull)?;
        self.text_used = writer.position();
        self.push(
            TextReference::Buffer {
                start,
                length: self.text_used - start,
            }
            .to_value(),
        )
    }

    fn join(&mut self, image: &AppImage) -> Result<(), Failure> {
        let second = self.pop()?;
        let first = self.pop()?;
        let start = self.text_used;

        for value in [first, second] {
            match TextReference::from_value(value).ok_or(Failure::BadText)? {
                TextReference::ImageString(index) => {
                    let bytes = image.string(index).ok_or(Failure::BadText)?.as_bytes();
                    let mut writer = TextWriter::new(&mut self.text_buffer, self.text_used);
                    writer.write_bytes(bytes).map_err(|_| Failure::TextBufferFull)?;
                    self.text_used = writer.position();
                }
                TextReference::Buffer { start: from, length } => {
                    if from + length > start {
                        return Err(Failure::BadText);
                    }
                    let end = self.text_used + length;
                    if end > TEXT_BUFFER_BYTES {
                        return Err(Failure::TextBufferFull);
                    }
                    self.text_buffer.copy_within(from..from + length, self.text_used);
                    self.text_used = end;
                }
            }
        }
        self.push(
            TextReference::Buffer {
                start,
                length: self.text_used - start,
            }
            .to_value(),
        )
    }
}

/// Whether an instruction reads or changes the app's state or event, or acts for the app, which
/// library code may not.
pub(crate) fn touches_the_app(instruction: Instruction) -> bool {
    use Instruction::*;
    matches!(
        instruction,
        ReadState(_)
            | ReadElement(_)
            | WriteState(_)
            | WriteElement(_)
            | FillArray(_)
            | CopyArray { .. }
            | AskArray { .. }
            | WorkArray { .. }
            | TextFromCodes(_)
            | ReadEvent(_)
            | ReadEventText(_)
            | Effect(_)
            | RandomInt
            | CallLibrary { .. }
    )
}

/// Turns a JavaScript `slice` index, which counts from the end when negative, into a position.
fn javascript_index(index: Value, count: Value) -> Value {
    match index < 0 {
        true => (count + index).max(0),
        false => index.min(count),
    }
}
