//! The native file: an image's translated units, as the watch keeps it beside the image.
//!
//! It starts with a header naming the image it was made from, by the CRC field and length of the
//! image's header, then a table with each unit's kind, number and where its code lies, then the
//! code, each unit's starting on a 4-byte boundary. Only units with a loop are translated, since
//! code that runs once per event gains little and native code sits in RAM while the app is open.

use alloc::vec::Vec;

use battletime_runtime::instruction::{decode, Instruction, StackEffect};
use battletime_runtime::interpreter::MOST_LOCALS;
use battletime_runtime::paging::CodeUnit;
use battletime_runtime::AppImage;

use crate::translate;

// ============================================================================
// Layout
// ============================================================================

pub const MAGIC: [u8; 4] = *b"NTNC";
pub const FORMAT_VERSION: u16 = 1;
pub const HEADER_BYTES: usize = 16;
pub const ENTRY_BYTES: usize = 12;

const FORMAT_VERSION_AT: usize = 4;
const UNIT_COUNT_AT: usize = 6;
const IMAGE_CRC_AT: usize = 8;
const IMAGE_LENGTH_AT: usize = 12;

/// Where an image keeps its header's CRC field.
const IMAGE_HEADER_CRC_AT: usize = 12;

const EXPRESSION_KIND: u8 = 0;
const HANDLER_KIND: u8 = 1;
const FUNCTION_KIND: u8 = 2;

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn image_crc(image_bytes: &[u8]) -> u32 {
    read_u32(image_bytes, IMAGE_HEADER_CRC_AT).unwrap_or(0)
}

// ============================================================================
// Writing
// ============================================================================

/// Whether a unit's code holds a loop, so translating it pays.
fn has_loop(code: &[u8]) -> bool {
    let mut offset = 0;
    while let Ok((instruction, next)) = decode(code, offset) {
        if instruction == Instruction::LoopStart {
            return true;
        }
        offset = next;
    }
    false
}

/// Every function's stack effect, for translating calls.
pub fn call_effects(image: &AppImage) -> Vec<StackEffect> {
    (0..image.function_count())
        .map(|index| match image.function(index) {
            Some(function) => StackEffect {
                taken: function.argument_count as usize,
                left: function.result_count as usize,
            },
            None => StackEffect { taken: 0, left: 0 },
        })
        .collect()
}

/// The native file for a verified image, with every unit that has a loop and translates.
pub fn write(image_bytes: &[u8], image: &AppImage) -> Vec<u8> {
    let effects = call_effects(image);
    let mut units: Vec<(u8, u16, Vec<u8>)> = Vec::new();
    let mut add = |kind: u8, index: u16, code: Option<&[u8]>, local_count: usize| {
        let Some(code) = code.filter(|code| has_loop(code)) else {
            return;
        };
        if let Ok(native) = translate(code, local_count, &effects) {
            units.push((kind, index, native));
        }
    };
    for index in 0..image.expression_count() {
        add(EXPRESSION_KIND, index, image.expression_code(index), MOST_LOCALS);
    }
    for index in 0..image.handler_count() {
        add(HANDLER_KIND, index, image.handler_code(index), MOST_LOCALS);
    }
    for index in 0..image.function_count() {
        if let Some(function) = image.function(index) {
            add(
                FUNCTION_KIND,
                index,
                Some(function.code),
                function.local_count as usize,
            );
        }
    }

    let mut file = Vec::new();
    file.extend(MAGIC);
    file.extend(FORMAT_VERSION.to_le_bytes());
    file.extend((units.len() as u16).to_le_bytes());
    file.extend(image_crc(image_bytes).to_le_bytes());
    file.extend((image_bytes.len() as u32).to_le_bytes());
    let mut code_at = HEADER_BYTES + units.len() * ENTRY_BYTES;
    for (kind, index, native) in &units {
        file.push(*kind);
        file.push(0);
        file.extend(index.to_le_bytes());
        file.extend((code_at as u32).to_le_bytes());
        file.extend((native.len() as u32).to_le_bytes());
        code_at = (code_at + native.len()).next_multiple_of(4);
    }
    for (_, _, native) in &units {
        file.extend(native);
        file.resize(file.len().next_multiple_of(4), 0);
    }
    file
}

// ============================================================================
// Reading
// ============================================================================

/// A native file read where it lies.
#[derive(Clone, Copy)]
pub struct NativeFile<'file> {
    bytes: &'file [u8],
    unit_count: usize,
}

impl<'file> NativeFile<'file> {
    /// The file, if it is whole and was made from the image `image_bytes`.
    pub fn parse(bytes: &'file [u8], image_bytes: &[u8]) -> Option<NativeFile<'file>> {
        if bytes.get(..4)? != MAGIC || read_u16(bytes, FORMAT_VERSION_AT)? != FORMAT_VERSION {
            return None;
        }
        let made_from_image = read_u32(bytes, IMAGE_CRC_AT)? == image_crc(image_bytes)
            && read_u32(bytes, IMAGE_LENGTH_AT)? as usize == image_bytes.len();
        let unit_count = read_u16(bytes, UNIT_COUNT_AT)? as usize;
        let file = NativeFile { bytes, unit_count };
        let whole = (0..unit_count).all(|entry| file.entry(entry).is_some());
        (made_from_image && whole).then_some(file)
    }

    pub fn unit_count(&self) -> usize {
        self.unit_count
    }

    fn entry(&self, entry: usize) -> Option<(CodeUnit, &'file [u8])> {
        let at = HEADER_BYTES + entry * ENTRY_BYTES;
        let index = read_u16(self.bytes, at + 2)?;
        let unit = match *self.bytes.get(at)? {
            EXPRESSION_KIND => CodeUnit::Expression(index),
            HANDLER_KIND => CodeUnit::Handler(index),
            FUNCTION_KIND => CodeUnit::Function(u8::try_from(index).ok()?),
            _ => return None,
        };
        let start = read_u32(self.bytes, at + 4)? as usize;
        let length = read_u32(self.bytes, at + 8)? as usize;
        let code = self.bytes.get(start..start.checked_add(length)?)?;
        start.is_multiple_of(4).then_some((unit, code))
    }

    /// The native code for a unit, if the file has it.
    pub fn unit(&self, unit: CodeUnit) -> Option<&'file [u8]> {
        (0..self.unit_count)
            .filter_map(|entry| self.entry(entry))
            .find(|(translated, _)| *translated == unit)
            .map(|(_, code)| code)
    }

    /// Every unit with its code.
    pub fn units(&self) -> impl Iterator<Item = (CodeUnit, &'file [u8])> + '_ {
        (0..self.unit_count).filter_map(|entry| self.entry(entry))
    }
}
