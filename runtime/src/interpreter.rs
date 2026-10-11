//! Runs an app's expressions and handlers.
//!
//! An expression works out one value for a widget. A handler answers an event: its state writes are
//! held back and applied together when it finishes, and its effects are only collected, so a
//! handler that fails part way changes nothing. Every check is repeated here even for verified
//! code, so a bad image fails a call instead of crashing the watch.

use crate::image::AppImage;
use crate::instruction::{decode, EffectKind, Instruction, SystemValue, MOST_EFFECT_ARGUMENTS};
use crate::pattern;
use crate::text::{DateTime, TextReference, TextWriter};

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
pub const MOST_STATE_WRITES: usize = 16;
pub const MOST_EFFECTS: usize = 8;

/// Where the interpreter reads the values the system keeps current.
pub trait SystemValues {
    fn read(&self, value: SystemValue) -> Value;

    /// A fresh random number, such as one from the chip's random number generator.
    fn random_number(&self) -> u32;
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
    BadStateSlot,
    BadEventField,
    BadText,
    TextBufferFull,
    TooManyStateWrites,
    TooManyEffects,
    WrongResultCount,
    NotAllowedHere,
}

// ============================================================================
// Handler results
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateWrite {
    pub slot: u8,
    pub value: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effect {
    pub kind: EffectKind,
    pub arguments: [Value; MOST_EFFECT_ARGUMENTS],
}

/// What a handler asked for: state writes, at most one per slot, and effects in call order.
#[derive(Clone, Copy, Debug)]
pub struct HandlerOutcome {
    writes: [StateWrite; MOST_STATE_WRITES],
    write_count: usize,
    effects: [Effect; MOST_EFFECTS],
    effect_count: usize,
}

impl HandlerOutcome {
    const fn empty() -> Self {
        HandlerOutcome {
            writes: [StateWrite { slot: 0, value: 0 }; MOST_STATE_WRITES],
            write_count: 0,
            effects: [Effect {
                kind: EffectKind::Back,
                arguments: [0; MOST_EFFECT_ARGUMENTS],
            }; MOST_EFFECTS],
            effect_count: 0,
        }
    }

    pub fn writes(&self) -> &[StateWrite] {
        &self.writes[..self.write_count]
    }

    pub fn effects(&self) -> &[Effect] {
        &self.effects[..self.effect_count]
    }

    fn pending_value(&self, slot: u8) -> Option<Value> {
        self.writes()
            .iter()
            .find(|write| write.slot == slot)
            .map(|write| write.value)
    }

    fn write(&mut self, slot: u8, value: Value) -> Result<(), Failure> {
        if let Some(existing) = self.writes[..self.write_count]
            .iter_mut()
            .find(|write| write.slot == slot)
        {
            existing.value = value;
            return Ok(());
        }
        let free = self
            .writes
            .get_mut(self.write_count)
            .ok_or(Failure::TooManyStateWrites)?;
        *free = StateWrite { slot, value };
        self.write_count += 1;
        Ok(())
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

// ============================================================================
// Interpreter
// ============================================================================

/// What a call may read and where its handler results go.
struct Call<'call, System: SystemValues> {
    image: &'call AppImage<'call>,
    state: &'call [Value],
    event: &'call [Value],
    system: &'call System,
    outcome: Option<&'call mut HandlerOutcome>,
}

/// The interpreter's working memory, reused for every call.
pub struct Interpreter {
    stack: [Value; STACK_DEPTH],
    depth: usize,
    text_buffer: [u8; TEXT_BUFFER_BYTES],
    text_used: usize,
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
            text_buffer: [0; TEXT_BUFFER_BYTES],
            text_used: 0,
        }
    }

    /// Works out an expression's value. A text result stays readable until the next call.
    pub fn evaluate_expression(
        &mut self,
        image: &AppImage,
        expression: u16,
        state: &[Value],
        system: &impl SystemValues,
    ) -> Result<Value, Failure> {
        let code = image.expression_code(expression).ok_or(Failure::NoSuchCode)?;
        let call = Call {
            image,
            state,
            event: &[],
            system,
            outcome: None,
        };
        self.run(code, call)?;

        if self.depth != 1 {
            return Err(Failure::WrongResultCount);
        }
        Ok(self.stack[0])
    }

    /// Runs a handler for an event and returns its writes and effects without applying them.
    pub fn run_handler(
        &mut self,
        image: &AppImage,
        handler: u16,
        state: &[Value],
        event: &[Value],
        system: &impl SystemValues,
    ) -> Result<HandlerOutcome, Failure> {
        let code = image.handler_code(handler).ok_or(Failure::NoSuchCode)?;
        let mut outcome = HandlerOutcome::empty();
        let call = Call {
            image,
            state,
            event,
            system,
            outcome: Some(&mut outcome),
        };
        self.run(code, call)?;

        if self.depth != 0 {
            return Err(Failure::WrongResultCount);
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

    fn run<System: SystemValues>(&mut self, code: &[u8], mut call: Call<System>) -> Result<(), Failure> {
        self.depth = 0;
        self.text_used = 0;
        let mut offset = 0;

        while offset < code.len() {
            let (instruction, next_offset) = decode(code, offset).map_err(|_| Failure::BadInstruction)?;
            offset = next_offset;
            if let Some(distance) = self.execute(instruction, &mut call)? {
                offset += distance as usize;
            }
        }
        Ok(())
    }

    /// Carries out one instruction, and returns the distance to skip when it jumps.
    fn execute<System: SystemValues>(
        &mut self,
        instruction: Instruction,
        call: &mut Call<System>,
    ) -> Result<Option<u16>, Failure> {
        use Instruction::*;

        match instruction {
            PushSmall(value) => self.push(value as Value)?,
            PushInt(value) => self.push(value as Value)?,
            PushLong(value) => self.push(value)?,
            PushNone => self.push(NONE)?,
            PushText(index) => self.push(TextReference::ImageString(index).to_value())?,
            ReadState(slot) => {
                let pending = call
                    .outcome
                    .as_ref()
                    .and_then(|outcome| outcome.pending_value(slot));
                let value = match pending {
                    Some(value) => value,
                    None => *call.state.get(slot as usize).ok_or(Failure::BadStateSlot)?,
                };
                self.push(value)?;
            }
            ReadSystem(value) => self.push(call.system.read(value))?,
            ReadEvent(field) => {
                if call.outcome.is_none() {
                    return Err(Failure::NotAllowedHere);
                }
                self.push(*call.event.get(field as usize).ok_or(Failure::BadEventField)?)?;
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

            Jump(distance) => return Ok(Some(distance)),
            JumpIfFalse(distance) => {
                if self.pop()? == 0 {
                    return Ok(Some(distance));
                }
            }

            TimePart(part) => {
                let time = self.local_time(call)?;
                self.push(time.part(part))?;
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

            WriteState(slot) => {
                let value = self.pop()?;
                if slot as usize >= call.state.len() {
                    return Err(Failure::BadStateSlot);
                }
                call.outcome
                    .as_mut()
                    .ok_or(Failure::NotAllowedHere)?
                    .write(slot, value)?;
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

    fn push(&mut self, value: Value) -> Result<(), Failure> {
        let free = self.stack.get_mut(self.depth).ok_or(Failure::StackOverflow)?;
        *free = value;
        self.depth += 1;
        Ok(())
    }

    fn pop(&mut self) -> Result<Value, Failure> {
        self.depth = self.depth.checked_sub(1).ok_or(Failure::StackUnderflow)?;
        Ok(self.stack[self.depth])
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
