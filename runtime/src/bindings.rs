//! Works out what each expression reads, so the runtime redraws only what changed and only when the
//! clock makes a difference.
//!
//! The runtime derives this from the code itself when an app opens, so a wrong compiler can never
//! make a widget miss an update.

use crate::image::{AppImage, ImageError};
use crate::instruction::{decode, Instruction, SystemValue, TimePart};
use crate::pattern;

// ============================================================================
// Inputs
// ============================================================================

/// The state slots and system values a piece of code reads, one bit each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Inputs {
    pub state_slots: u64,
    pub system_values: u32,
}

impl Inputs {
    pub const NOTHING: Inputs = Inputs {
        state_slots: 0,
        system_values: 0,
    };

    pub fn state_slot(slot: u8) -> Inputs {
        Inputs {
            state_slots: 1 << slot,
            system_values: 0,
        }
    }

    pub fn system_value(value: SystemValue) -> Inputs {
        Inputs {
            state_slots: 0,
            system_values: value.bit(),
        }
    }

    pub fn union(self, other: Inputs) -> Inputs {
        Inputs {
            state_slots: self.state_slots | other.state_slots,
            system_values: self.system_values | other.system_values,
        }
    }

    pub fn overlaps(self, other: Inputs) -> bool {
        self.state_slots & other.state_slots != 0 || self.system_values & other.system_values != 0
    }

    pub fn reads_clock(self) -> bool {
        self.system_values & SystemValue::Now.bit() != 0
    }
}

// ============================================================================
// Redraw periods
// ============================================================================

const SECOND_IN_MILLISECONDS: u32 = 1000;
const MINUTE_IN_MILLISECONDS: u32 = 60 * SECOND_IN_MILLISECONDS;

/// How often a value shown through this instruction can change, if it depends on the clock.
fn smallest_visible_step(image: &AppImage, instruction: Instruction) -> Option<u32> {
    use Instruction::*;
    Some(match instruction {
        FormatDuration(pattern) => pattern::duration_pattern_step_in_milliseconds(image.string(pattern)?),
        FormatTime(pattern) => pattern::time_pattern_step_in_milliseconds(image.string(pattern)?),
        TimePart(self::TimePart::Second) => SECOND_IN_MILLISECONDS,
        // Coarser parts and starts still step on the minute, because time zones can be half an hour off.
        TimePart(_) | StartOf(_) => MINUTE_IN_MILLISECONDS,
        _ => return None,
    })
}

/// What one expression reads, and how often the clock makes it change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExpressionInfo {
    pub inputs: Inputs,
    /// Zero when the expression does not read the clock.
    pub redraw_period_in_milliseconds: u32,
}

/// Scans an expression's code for what it reads and the finest time step it shows.
///
/// An expression that reads the clock without formatting it, such as a comparison, redraws once a
/// second.
pub fn analyse_expression(image: &AppImage, code: &[u8]) -> Option<ExpressionInfo> {
    let mut inputs = Inputs::NOTHING;
    let mut finest_step: Option<u32> = None;
    let mut offset = 0;

    while offset < code.len() {
        let (instruction, next_offset) = decode(code, offset).ok()?;
        match instruction {
            Instruction::ReadState(slot)
            | Instruction::ReadElement(slot)
            | Instruction::TextFromCodes(slot)
                if (slot as usize) < 64 =>
            {
                inputs = inputs.union(Inputs::state_slot(slot))
            }
            Instruction::ReadSystem(value) | Instruction::ReadSystemElement(value) => {
                inputs = inputs.union(Inputs::system_value(value))
            }
            _ => {}
        }
        if let Some(step) = smallest_visible_step(image, instruction) {
            finest_step = Some(finest_step.map_or(step, |finest| finest.min(step)));
        }
        offset = next_offset;
    }

    let redraw_period_in_milliseconds = match inputs.reads_clock() {
        true => finest_step.unwrap_or(SECOND_IN_MILLISECONDS),
        false => 0,
    };
    Some(ExpressionInfo {
        inputs,
        redraw_period_in_milliseconds,
    })
}

// ============================================================================
// Bindings
// ============================================================================

/// The most expressions an open app may have.
pub const MOST_EXPRESSIONS: usize = 64;

/// What every expression of the open app reads, kept in RAM while the app is open.
///
/// The parts are kept in separate arrays, which avoids padding and makes each entry 16 bytes.
pub struct Bindings {
    state_slots: [u64; MOST_EXPRESSIONS],
    system_values: [u32; MOST_EXPRESSIONS],
    redraw_periods_in_milliseconds: [u32; MOST_EXPRESSIONS],
    count: usize,
}

impl Bindings {
    pub fn load(image: &AppImage) -> Result<Bindings, ImageError> {
        let count = image.expression_count() as usize;
        if count > MOST_EXPRESSIONS {
            return Err(ImageError::TableOutOfBounds);
        }
        let mut bindings = Bindings {
            state_slots: [0; MOST_EXPRESSIONS],
            system_values: [0; MOST_EXPRESSIONS],
            redraw_periods_in_milliseconds: [0; MOST_EXPRESSIONS],
            count,
        };
        for index in 0..count {
            let code = image
                .expression_code(index as u16)
                .ok_or(ImageError::TableOutOfBounds)?;
            let info = analyse_expression(image, code).ok_or(ImageError::TableOutOfBounds)?;
            bindings.state_slots[index] = info.inputs.state_slots;
            bindings.system_values[index] = info.inputs.system_values;
            bindings.redraw_periods_in_milliseconds[index] = info.redraw_period_in_milliseconds;
        }
        Ok(bindings)
    }

    pub fn info(&self, expression: usize) -> Option<ExpressionInfo> {
        if expression >= self.count {
            return None;
        }
        Some(ExpressionInfo {
            inputs: Inputs {
                state_slots: self.state_slots[expression],
                system_values: self.system_values[expression],
            },
            redraw_period_in_milliseconds: self.redraw_periods_in_milliseconds[expression],
        })
    }

    fn infos(&self) -> impl Iterator<Item = (usize, ExpressionInfo)> + '_ {
        (0..self.count).filter_map(|index| Some((index, self.info(index)?)))
    }

    /// The expressions that read any of `changed`, and so need working out again.
    pub fn affected_by(&self, changed: Inputs) -> impl Iterator<Item = usize> + '_ {
        self.infos()
            .filter(move |(_, info)| info.inputs.overlaps(changed))
            .map(|(index, _)| index)
    }

    /// Like `due_between`, but only among the expressions whose bits are set in `expressions`, such
    /// as those on the screen that is open.
    pub fn due_between_among(
        &self,
        last_redraw: i64,
        now: i64,
        expressions: u64,
    ) -> impl Iterator<Item = usize> + '_ {
        self.due_between(last_redraw, now)
            .filter(move |&index| expressions & (1 << index) != 0)
    }

    /// Like `next_redraw_after`, but only among the expressions whose bits are set in `expressions`.
    pub fn next_redraw_after_among(&self, now: i64, expressions: u64) -> Option<i64> {
        self.redraw_periods_in_milliseconds[..self.count]
            .iter()
            .enumerate()
            .filter(|(index, &period)| period != 0 && expressions & (1 << index) != 0)
            .map(|(_, &period)| (now.div_euclid(period as i64) + 1) * period as i64)
            .min()
    }

    /// The clock-driven expressions whose shown value may have changed between two redraws.
    ///
    /// Steps are counted on the clock, so `mm:ss` redraws as each wall-clock second begins.
    pub fn due_between(&self, last_redraw: i64, now: i64) -> impl Iterator<Item = usize> + '_ {
        self.infos()
            .filter(move |(_, info)| {
                let period = info.redraw_period_in_milliseconds as i64;
                period != 0 && last_redraw.div_euclid(period) != now.div_euclid(period)
            })
            .map(|(index, _)| index)
    }

    /// When the clock next changes something on screen, or `None` if nothing shown reads it.
    pub fn next_redraw_after(&self, now: i64) -> Option<i64> {
        self.redraw_periods_in_milliseconds[..self.count]
            .iter()
            .map(|&period| period as i64)
            .filter(|&period| period != 0)
            .map(|period| (now.div_euclid(period) + 1) * period)
            .min()
    }
}
