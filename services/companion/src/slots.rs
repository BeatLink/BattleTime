//! The `slots` message: where the update and recovery slots sit in the external flash, and moving them.
//!
//! `{"t":"slots"}` asks where they are. `{"t":"slots","recovery":[start,size]}`, with `update` too
//! or instead, asks to move them, in bytes; a slot left out stays where it is. The watch answers both
//! with a report, or a refusal saying why.

use core::fmt::Write;

use serde::Deserialize;

use crate::{Message, MessageError, ReplyLine};

// ============================================================================
// Reading a request
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotRange {
    pub start: u32,
    pub size: u32,
}

/// A slot named here moves; one left out stays put. With neither, the phone only asks where they are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlotsRequest {
    pub update: Option<SlotRange>,
    pub recovery: Option<SlotRange>,
}

impl SlotsRequest {
    pub const fn moves_anything(&self) -> bool {
        self.update.is_some() || self.recovery.is_some()
    }
}

/// Read as `i64`, which the other messages' numbers already use, so no second number reader is built in.
#[derive(Deserialize)]
struct SlotsFields {
    #[serde(default)]
    update: Option<[i64; 2]>,
    #[serde(default)]
    recovery: Option<[i64; 2]>,
}

#[inline(never)]
pub(crate) fn parse_slots(json: &str) -> Result<Message<'_>, MessageError> {
    let (fields, _) = serde_json_core::from_str::<SlotsFields>(json).map_err(|_| MessageError::NotJson)?;
    let range = |pair: Option<[i64; 2]>| -> Result<Option<SlotRange>, MessageError> {
        let Some([start, size]) = pair else {
            return Ok(None);
        };
        let number = |value: i64| u32::try_from(value).map_err(|_| MessageError::NotJson);
        Ok(Some(SlotRange {
            start: number(start)?,
            size: number(size)?,
        }))
    };
    Ok(Message::Slots(SlotsRequest {
        update: range(fields.update)?,
        recovery: range(fields.recovery)?,
    }))
}

// ============================================================================
// Answering
// ============================================================================

/// What the watch reports: both slots, the limits a move must keep to, and the recovery image's length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotsReport {
    pub update: SlotRange,
    pub recovery: SlotRange,
    /// Slots must end at or before this offset, where the file system starts.
    pub area_end: u32,
    /// The firmware slot's size; the update slot is one page more.
    pub firmware_size: u32,
    /// The whole recovery image with its header, or 0 when the slot holds none.
    pub recovery_image_size: u32,
    /// True when this answers a move, after which the watch restarts.
    pub restarting: bool,
}

pub fn slots_report(report: &SlotsReport) -> ReplyLine {
    let mut line = ReplyLine::new();
    let _ = writeln!(
        line,
        "{{\"t\":\"slots\",\"update\":[{},{}],\"recovery\":[{},{}],\"area\":{},\"firmware\":{},\"image\":{},\"restart\":{}}}",
        report.update.start,
        report.update.size,
        report.recovery.start,
        report.recovery.size,
        report.area_end,
        report.firmware_size,
        report.recovery_image_size,
        report.restarting,
    );
    line
}

pub fn slots_refusal(reason: &str) -> ReplyLine {
    let mut line = ReplyLine::new();
    let _ = writeln!(line, "{{\"t\":\"slots\",\"err\":\"{reason}\"}}");
    line
}
