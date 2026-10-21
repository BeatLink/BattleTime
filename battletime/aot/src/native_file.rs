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

/// What the translator needs of an image, which the firmware works out from the image it parsed.
///
/// It holds the image's CRC field and length, each function's stack effect, and each unit with a
/// loop: its kind, number, locals and code. The translator module reads only this, so it needs
/// none of the image checks.
pub fn translator_input<'image>(image_bytes: &[u8], image: &AppImage<'image>) -> Vec<u8> {
    let mut units: Vec<(u8, u16, u8, &'image [u8])> = Vec::new();
    let mut add = |kind: u8, index: u16, local_count: usize, code: Option<&'image [u8]>| {
        if let Some(code) = code.filter(|code| has_loop(code) && code.len() <= u16::MAX as usize) {
            units.push((kind, index, local_count as u8, code));
        }
    };
    for index in 0..image.expression_count() {
        add(EXPRESSION_KIND, index, MOST_LOCALS, image.expression_code(index));
    }
    for index in 0..image.handler_count() {
        add(HANDLER_KIND, index, MOST_LOCALS, image.handler_code(index));
    }
    for index in 0..image.function_count() {
        if let Some(function) = image.function(index) {
            add(
                FUNCTION_KIND,
                index,
                function.local_count as usize,
                Some(function.code),
            );
        }
    }

    let effects = call_effects(image);
    let mut input = Vec::new();
    input.extend(image_crc(image_bytes).to_le_bytes());
    input.extend((image_bytes.len() as u32).to_le_bytes());
    input.extend((effects.len() as u16).to_le_bytes());
    for effect in &effects {
        input.extend([effect.taken as u8, effect.left as u8]);
    }
    input.extend((units.len() as u16).to_le_bytes());
    for (kind, index, local_count, code) in units {
        input.extend([kind, local_count]);
        input.extend(index.to_le_bytes());
        input.extend((code.len() as u16).to_le_bytes());
        input.extend(code);
    }
    input
}

/// One unit as the translator input gives it.
struct UnitSource<'input> {
    kind: u8,
    index: u16,
    local_count: usize,
    code: &'input [u8],
}

/// The units in a translator input, after its effects.
fn unit_sources(input: &[u8], units_at: usize) -> impl Iterator<Item = Option<UnitSource<'_>>> + '_ {
    let count = read_u16(input, units_at).unwrap_or(0);
    let mut at = units_at + 2;
    (0..count).map(move |_| {
        let kind = *input.get(at)?;
        let local_count = *input.get(at + 1)? as usize;
        let index = read_u16(input, at + 2)?;
        let length = read_u16(input, at + 4)? as usize;
        let code = input.get(at + 6..at + 6 + length)?;
        at += 6 + length;
        Some(UnitSource {
            kind,
            index,
            local_count,
            code,
        })
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteError {
    BadInput,
    NoRoom,
}

/// The native file's length for a translator input, after writing the file to `out` when it is given.
///
/// A unit that does not translate is left out, so the interpreter runs it.
pub fn write_from_input(input: &[u8], out: Option<&mut [u8]>) -> Result<usize, WriteError> {
    let image_crc = read_u32(input, 0).ok_or(WriteError::BadInput)?;
    let image_length = read_u32(input, 4).ok_or(WriteError::BadInput)?;
    let effect_count = read_u16(input, 8).ok_or(WriteError::BadInput)? as usize;
    let effect_bytes = input.get(10..10 + effect_count * 2).ok_or(WriteError::BadInput)?;
    let effects: Vec<StackEffect> = effect_bytes
        .chunks(2)
        .map(|pair| StackEffect {
            taken: pair[0] as usize,
            left: pair[1] as usize,
        })
        .collect();
    let units_at = 10 + effect_count * 2;

    let mut translated = 0;
    let mut code_bytes = 0;
    for source in unit_sources(input, units_at) {
        let source = source.ok_or(WriteError::BadInput)?;
        if let Ok(native) = translate(source.code, source.local_count, &effects) {
            translated += 1;
            code_bytes += native.len().next_multiple_of(4);
        }
    }
    let table_bytes = HEADER_BYTES + translated * ENTRY_BYTES;
    let length = table_bytes + code_bytes;
    let Some(out) = out else {
        return Ok(length);
    };
    let out = out.get_mut(..length).ok_or(WriteError::NoRoom)?;
    out.fill(0);

    out[..4].copy_from_slice(&MAGIC);
    out[FORMAT_VERSION_AT..FORMAT_VERSION_AT + 2].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
    out[UNIT_COUNT_AT..UNIT_COUNT_AT + 2].copy_from_slice(&(translated as u16).to_le_bytes());
    out[IMAGE_CRC_AT..IMAGE_CRC_AT + 4].copy_from_slice(&image_crc.to_le_bytes());
    out[IMAGE_LENGTH_AT..IMAGE_LENGTH_AT + 4].copy_from_slice(&image_length.to_le_bytes());
    let mut entry_at = HEADER_BYTES;
    let mut code_at = table_bytes;
    for source in unit_sources(input, units_at).flatten() {
        let Ok(native) = translate(source.code, source.local_count, &effects) else {
            continue;
        };
        let entry = out
            .get_mut(entry_at..entry_at + ENTRY_BYTES)
            .ok_or(WriteError::NoRoom)?;
        entry[0] = source.kind;
        entry[2..4].copy_from_slice(&source.index.to_le_bytes());
        entry[4..8].copy_from_slice(&(code_at as u32).to_le_bytes());
        entry[8..12].copy_from_slice(&(native.len() as u32).to_le_bytes());
        out.get_mut(code_at..code_at + native.len())
            .ok_or(WriteError::NoRoom)?
            .copy_from_slice(&native);
        entry_at += ENTRY_BYTES;
        code_at += native.len().next_multiple_of(4);
    }
    Ok(length)
}

/// The native file for a verified image, with every unit that has a loop and translates.
pub fn write(image_bytes: &[u8], image: &AppImage) -> Vec<u8> {
    let input = translator_input(image_bytes, image);
    let length = write_from_input(&input, None).unwrap_or(0);
    let mut file = alloc::vec![0; length];
    match write_from_input(&input, Some(&mut file)) {
        Ok(_) => file,
        Err(_) => Vec::new(),
    }
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
