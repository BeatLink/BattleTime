//! Reads an app image in place: its header, its section table and the tables inside each section.
//!
//! `AppImage::parse` checks the image's structure, so every later lookup stays inside the bytes.
//! The code inside it is checked separately by `verify`.

use crate::resources::ImageResource;
use crate::subscriptions::{Subscription, SubscriptionProblem, SUBSCRIPTION_BYTES};
use crate::theme::ColorValue;
use crate::widgets::WidgetProblem;

// ============================================================================
// Layout
// ============================================================================

pub const MAGIC: [u8; 4] = *b"NTAP";
pub const FORMAT_VERSION: u16 = 1;

const HEADER_BYTES: usize = 20;
const SECTION_ENTRY_BYTES: usize = 12;
const TABLE_ENTRY_BYTES: usize = 4;
const STATE_ENTRY_BYTES: usize = 16;

/// The bytes the CRC covers start after the CRC field itself.
const CRC_COVERS_FROM: usize = 16;

/// The most state variables an app may have, so that each fits one bit of an input mask.
pub const MOST_STATE_VARIABLES: usize = 64;

/// The most functions an image may have, so that `Call` names each with one byte.
pub const MOST_FUNCTIONS: usize = 256;

/// The bytes before a function's code: its argument count, result count and local count.
const FUNCTION_HEADER_BYTES: usize = 3;

// ============================================================================
// Types
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppKind {
    App,
    Face,
    /// The shared library: images such as icons that every app can use, and no code.
    Library,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionKind {
    Strings,
    State,
    Expressions,
    Handlers,
    Widgets,
    Subscriptions,
    Resources,
    /// The starting elements of arrays whose elements do not all start the same.
    InitialValues,
    /// The shared library's text fonts, in the order `font` numbers its bitmap fonts.
    Fonts,
    /// Code that expressions, handlers and other functions run with `Call`.
    Functions,
}

impl SectionKind {
    fn from_number(number: u16) -> Option<SectionKind> {
        use SectionKind::*;
        Some(match number {
            1 => Strings,
            2 => State,
            3 => Expressions,
            4 => Handlers,
            5 => Widgets,
            6 => Subscriptions,
            7 => Resources,
            8 => InitialValues,
            9 => Fonts,
            10 => Functions,
            _ => return None,
        })
    }
}

/// The type of a state variable. Every value is stored in 64 bits whatever its type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueType {
    Int,
    Bool,
    Instant,
    Duration,
    Color,
    Text,
}

impl ValueType {
    fn from_byte(byte: u8) -> Option<ValueType> {
        use ValueType::*;
        Some(match byte {
            0 => Int,
            1 => Bool,
            2 => Instant,
            3 => Duration,
            4 => Color,
            5 => Text,
            _ => return None,
        })
    }
}

/// One state variable: a single value, or a fixed-length array of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateVariable {
    pub key_string: u16,
    pub value_type: ValueType,
    pub persists: bool,
    /// Zero for a single value, otherwise how many elements the array has.
    pub length: u16,
    /// How many bytes each value is stored in: 8 for a single value, 1, 2, 4 or 8 in an array.
    pub element_bytes: u8,
    /// The starting value, or for an array the value every element starts as.
    pub default_value: i64,
}

impl StateVariable {
    pub fn is_array(&self) -> bool {
        self.length > 0
    }

    /// How many bytes the variable takes in RAM.
    pub fn stored_bytes(&self) -> usize {
        self.length.max(1) as usize * self.element_bytes as usize
    }

    /// Whether a value fits the variable's element width.
    pub fn fits(&self, value: i64) -> bool {
        match self.element_bytes {
            1 => i8::try_from(value).is_ok(),
            2 => i16::try_from(value).is_ok(),
            4 => i32::try_from(value).is_ok(),
            _ => true,
        }
    }
}

/// The most RAM an app's state may take, single values and arrays together.
pub const MOST_STATE_BYTES: usize = 2048;

/// The most RAM one variable may take, so that a handler can always copy it to change it.
pub const MOST_VARIABLE_BYTES: usize = 1024;

/// Why an image was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageError {
    TooShort,
    WrongMagic,
    UnsupportedVersion(u16),
    UnknownKind(u8),
    WrongCrc,
    SectionOutOfBounds,
    UnknownSection(u16),
    RepeatedSection,
    TableOutOfBounds,
    BadString(u16),
    UnknownValueType(u8),
    TooManyStateVariables,
    StateTooLarge,
    BadInitialValues(u16),
    BadSubscription {
        subscription: u16,
        problem: SubscriptionProblem,
    },
    BadResource(u16),
    BadDefaultValue(u8),
    BadCode {
        block: CodeBlock,
        offset: usize,
        problem: CodeProblem,
    },
    BadWidget {
        widget: u16,
        problem: WidgetProblem,
    },
}

/// Which piece of code a problem was found in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeBlock {
    Expression(u16),
    Handler(u16),
    Function(u16),
}

/// One function: what a call to it takes and leaves, how many locals it uses, and its code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Function<'image> {
    /// The arguments arrive in its first locals, the first argument in local 0.
    pub argument_count: u8,
    /// 0 or 1.
    pub result_count: u8,
    pub local_count: u8,
    pub code: &'image [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeProblem {
    Undecodable,
    StackUnderflow,
    StackTooDeep,
    MismatchedStackAtJump,
    JumpIntoInstruction,
    JumpPastEnd,
    TooManyOpenJumps,
    WrongResultCount,
    StateSlotOutOfRange,
    StringOutOfRange,
    EventFieldOutOfRange,
    EventInExpression,
    WriteInExpression,
    EffectInExpression,
    RandomInExpression,
    BadPattern,
    WrongVariableKind,
    LocalOutOfRange,
    LoopWithStack,
    BadLoop,
    TooManyLoops,
    /// A list system value read whole, or a single one read by element.
    WrongSystemValueKind,
    /// A call to a function that does not exist, or to one not numbered below the caller.
    BadCall,
    /// A function whose header asks for more than one result or more locals than allowed.
    BadFunctionHeader,
    /// An expression calls a function that writes state, reads the event or causes effects.
    HandlerFunctionInExpression,
    /// Calls nested deeper than the interpreter keeps room for.
    CallsTooDeep,
    /// A shared library function that changes state, reads an event, causes effects, draws a random
    /// number, or calls the library from inside it; library functions are the same for every app.
    LibraryFunctionNotPure,
}

// ============================================================================
// Reading little-endian numbers
// ============================================================================

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_i64(bytes: &[u8], offset: usize) -> Option<i64> {
    Some(i64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

/// The standard CRC-32 (IEEE 802.3), computed a bit at a time to keep no table in flash.
pub fn crc32(bytes: &[u8]) -> u32 {
    const REVERSED_POLYNOMIAL: u32 = 0xEDB8_8320;
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            let low_bit_set = crc & 1 != 0;
            crc >>= 1;
            if low_bit_set {
                crc ^= REVERSED_POLYNOMIAL;
            }
        }
    }
    !crc
}

// ============================================================================
// Tables
// ============================================================================

/// A section made of a table of (offset, length) entries followed by the data they point into.
#[derive(Clone, Copy, Debug, Default)]
struct EntryTable<'image> {
    count: u16,
    entries: &'image [u8],
    data: &'image [u8],
}

impl<'image> EntryTable<'image> {
    fn parse(count: u16, bytes: &'image [u8]) -> Result<Self, ImageError> {
        let entries_end = count as usize * TABLE_ENTRY_BYTES;
        let entries = bytes.get(..entries_end).ok_or(ImageError::TableOutOfBounds)?;
        let data = &bytes[entries_end..];
        let table = EntryTable { count, entries, data };
        for index in 0..count {
            table.get(index).ok_or(ImageError::TableOutOfBounds)?;
        }
        Ok(table)
    }

    fn get(&self, index: u16) -> Option<&'image [u8]> {
        if index >= self.count {
            return None;
        }
        let entry_start = index as usize * TABLE_ENTRY_BYTES;
        let start = read_u16(self.entries, entry_start)? as usize;
        let length = read_u16(self.entries, entry_start + 2)? as usize;
        self.data.get(start..start + length)
    }
}

// ============================================================================
// App image
// ============================================================================

/// An app image, read where it lies without copying.
#[derive(Clone, Copy, Debug)]
pub struct AppImage<'image> {
    pub kind: AppKind,
    pub shared_library_version: u16,
    /// Whether the app's first image, number 0, is its icon for the launcher.
    pub has_icon: bool,
    pub id_string: u16,
    pub name_string: u16,
    byte_count: u32,
    strings: EntryTable<'image>,
    state_count: u16,
    state_entries: &'image [u8],
    expressions: EntryTable<'image>,
    handlers: EntryTable<'image>,
    widgets: EntryTable<'image>,
    initial_values: EntryTable<'image>,
    subscription_count: u16,
    subscriptions: &'image [u8],
    resources: EntryTable<'image>,
    fonts: EntryTable<'image>,
    functions: EntryTable<'image>,
}

impl<'image> AppImage<'image> {
    /// Checks the image's structure and returns a reader for it.
    pub fn parse(bytes: &'image [u8]) -> Result<Self, ImageError> {
        if bytes.len() < HEADER_BYTES {
            return Err(ImageError::TooShort);
        }
        if bytes[0..4] != MAGIC {
            return Err(ImageError::WrongMagic);
        }

        let version = read_u16(bytes, 4).ok_or(ImageError::TooShort)?;
        if version != FORMAT_VERSION {
            return Err(ImageError::UnsupportedVersion(version));
        }
        let kind = match bytes[6] {
            0 => AppKind::App,
            1 => AppKind::Face,
            2 => AppKind::Library,
            other => return Err(ImageError::UnknownKind(other)),
        };
        let shared_library_version = read_u16(bytes, 8).ok_or(ImageError::TooShort)?;
        let has_icon = bytes[7] & 1 != 0;
        let section_count = read_u16(bytes, 10).ok_or(ImageError::TooShort)?;
        let stored_crc = read_u32(bytes, 12).ok_or(ImageError::TooShort)?;
        if crc32(&bytes[CRC_COVERS_FROM..]) != stored_crc {
            return Err(ImageError::WrongCrc);
        }
        let id_string = read_u16(bytes, 16).ok_or(ImageError::TooShort)?;
        let name_string = read_u16(bytes, 18).ok_or(ImageError::TooShort)?;

        let mut image = AppImage {
            kind,
            shared_library_version,
            has_icon,
            id_string,
            name_string,
            byte_count: bytes.len() as u32,
            strings: EntryTable::default(),
            state_count: 0,
            state_entries: &[],
            expressions: EntryTable::default(),
            handlers: EntryTable::default(),
            widgets: EntryTable::default(),
            initial_values: EntryTable::default(),
            subscription_count: 0,
            subscriptions: &[],
            resources: EntryTable::default(),
            fonts: EntryTable::default(),
            functions: EntryTable::default(),
        };

        let mut seen_sections = 0u32;
        for section_index in 0..section_count as usize {
            let entry = HEADER_BYTES + section_index * SECTION_ENTRY_BYTES;
            let number = read_u16(bytes, entry).ok_or(ImageError::SectionOutOfBounds)?;
            let count = read_u16(bytes, entry + 2).ok_or(ImageError::SectionOutOfBounds)?;
            let offset = read_u32(bytes, entry + 4).ok_or(ImageError::SectionOutOfBounds)? as usize;
            let length = read_u32(bytes, entry + 8).ok_or(ImageError::SectionOutOfBounds)? as usize;
            let section_bytes = offset
                .checked_add(length)
                .and_then(|end| bytes.get(offset..end))
                .ok_or(ImageError::SectionOutOfBounds)?;

            let kind = SectionKind::from_number(number).ok_or(ImageError::UnknownSection(number))?;
            if seen_sections & (1 << number) != 0 {
                return Err(ImageError::RepeatedSection);
            }
            seen_sections |= 1 << number;

            match kind {
                SectionKind::Strings => image.strings = EntryTable::parse(count, section_bytes)?,
                SectionKind::State => image.read_state_section(count, section_bytes)?,
                SectionKind::Expressions => image.expressions = EntryTable::parse(count, section_bytes)?,
                SectionKind::Handlers => image.handlers = EntryTable::parse(count, section_bytes)?,
                SectionKind::Widgets => image.widgets = EntryTable::parse(count, section_bytes)?,
                SectionKind::InitialValues => image.initial_values = EntryTable::parse(count, section_bytes)?,
                SectionKind::Subscriptions => {
                    image.subscriptions = section_bytes
                        .get(..count as usize * SUBSCRIPTION_BYTES)
                        .ok_or(ImageError::TableOutOfBounds)?;
                    image.subscription_count = count;
                }
                SectionKind::Resources => image.resources = EntryTable::parse(count, section_bytes)?,
                SectionKind::Fonts => image.fonts = EntryTable::parse(count, section_bytes)?,
                SectionKind::Functions => {
                    if count as usize > MOST_FUNCTIONS {
                        return Err(ImageError::TableOutOfBounds);
                    }
                    image.functions = EntryTable::parse(count, section_bytes)?
                }
            }
        }

        for index in 0..image.strings.count {
            image.string(index).ok_or(ImageError::BadString(index))?;
        }
        for index in [id_string, name_string] {
            image.string(index).ok_or(ImageError::BadString(index))?;
        }
        for index in 0..image.state_count {
            image.check_state_variable(index)?;
        }
        if image.has_icon && image.resources.count == 0 {
            return Err(ImageError::BadResource(0));
        }
        for index in 0..image.initial_values.count {
            image.check_initial_values(index)?;
        }
        let state_bytes: usize = (0..image.state_count)
            .filter_map(|index| image.state_variable(index))
            .map(|variable| variable.stored_bytes())
            .sum();
        let largest_variable = (0..image.state_count())
            .filter_map(|index| image.state_variable(index))
            .map(|variable| variable.stored_bytes())
            .max()
            .unwrap_or(0);
        if state_bytes > MOST_STATE_BYTES || largest_variable > MOST_VARIABLE_BYTES {
            return Err(ImageError::StateTooLarge);
        }
        Ok(image)
    }

    fn read_state_section(&mut self, count: u16, bytes: &'image [u8]) -> Result<(), ImageError> {
        if count as usize > MOST_STATE_VARIABLES {
            return Err(ImageError::TooManyStateVariables);
        }
        self.state_entries = bytes
            .get(..count as usize * STATE_ENTRY_BYTES)
            .ok_or(ImageError::TableOutOfBounds)?;
        self.state_count = count;
        Ok(())
    }

    fn check_state_variable(&self, index: u16) -> Result<(), ImageError> {
        let entry = index as usize * STATE_ENTRY_BYTES;
        let type_byte = self.state_entries[entry + 2];
        let variable = self
            .state_variable(index)
            .ok_or(ImageError::UnknownValueType(type_byte))?;
        self.string(variable.key_string)
            .ok_or(ImageError::BadString(variable.key_string))?;

        let value = variable.default_value;
        let default_is_valid = self.value_fits_type(variable.value_type, value) && variable.fits(value);
        if !default_is_valid {
            return Err(ImageError::BadDefaultValue(index as u8));
        }
        Ok(())
    }

    /// Whether a value is valid for its type, such as a flag being 0 or 1.
    fn value_fits_type(&self, value_type: ValueType, value: i64) -> bool {
        match value_type {
            ValueType::Bool => value == 0 || value == 1,
            ValueType::Color => ColorValue::from_value(value).is_some(),
            ValueType::Text => (0..self.strings.count as i64).contains(&value),
            ValueType::Int | ValueType::Instant | ValueType::Duration => true,
        }
    }

    /// How long the whole image is.
    pub fn byte_count(&self) -> usize {
        self.byte_count as usize
    }

    pub fn string(&self, index: u16) -> Option<&'image str> {
        core::str::from_utf8(self.strings.get(index)?).ok()
    }

    pub fn string_count(&self) -> u16 {
        self.strings.count
    }

    pub fn state_count(&self) -> u16 {
        self.state_count
    }

    pub fn state_variable(&self, index: u16) -> Option<StateVariable> {
        if index >= self.state_count {
            return None;
        }
        let entry = index as usize * STATE_ENTRY_BYTES;
        let flags = self.state_entries[entry + 3];
        let length = read_u16(self.state_entries, entry + 4)?;
        let element_bytes = match (length, (flags >> 1) & 0b11) {
            (0, _) | (_, 0) => 8,
            (_, 1) => 1,
            (_, 2) => 2,
            _ => 4,
        };
        Some(StateVariable {
            key_string: read_u16(self.state_entries, entry)?,
            value_type: ValueType::from_byte(self.state_entries[entry + 2])?,
            persists: flags & 1 != 0,
            length,
            element_bytes,
            default_value: read_i64(self.state_entries, entry + 8)?,
        })
    }

    pub fn expression_count(&self) -> u16 {
        self.expressions.count
    }

    pub fn expression_code(&self, index: u16) -> Option<&'image [u8]> {
        self.expressions.get(index)
    }

    pub fn handler_count(&self) -> u16 {
        self.handlers.count
    }

    pub fn handler_code(&self, index: u16) -> Option<&'image [u8]> {
        self.handlers.get(index)
    }

    pub fn function_count(&self) -> u16 {
        self.functions.count
    }

    /// A function's header and code; none when it is missing or shorter than its header.
    pub fn function(&self, index: u16) -> Option<Function<'image>> {
        let bytes = self.functions.get(index)?;
        let header = bytes.get(..FUNCTION_HEADER_BYTES)?;
        Some(Function {
            argument_count: header[0],
            result_count: header[1],
            local_count: header[2],
            code: &bytes[FUNCTION_HEADER_BYTES..],
        })
    }

    pub fn widget_count(&self) -> u16 {
        self.widgets.count
    }

    pub fn widget_bytes(&self, index: u16) -> Option<&'image [u8]> {
        self.widgets.get(index)
    }

    pub fn subscription_count(&self) -> u16 {
        self.subscription_count
    }

    pub fn subscription(&self, index: u16) -> Option<Subscription> {
        let start = index as usize * SUBSCRIPTION_BYTES;
        if index >= self.subscription_count {
            return None;
        }
        Subscription::parse(self.subscriptions.get(start..start + SUBSCRIPTION_BYTES)?)
    }

    pub fn resource_count(&self) -> u16 {
        self.resources.count
    }

    pub fn resource(&self, index: u16) -> Option<ImageResource<'image>> {
        ImageResource::parse(self.resources.get(index)?)
    }

    /// One image's bytes as stored, which `ImageResource::parse` reads, such as to keep an app's icon alone.
    pub fn resource_bytes(&self, index: u16) -> Option<&'image [u8]> {
        self.resources.get(index)
    }

    pub fn font_count(&self) -> u16 {
        self.fonts.count
    }

    /// One text font's bytes, as `tools/generate_fonts.py` writes them.
    pub fn font(&self, index: u16) -> Option<&'image [u8]> {
        self.fonts.get(index)
    }

    pub fn initial_values_count(&self) -> u16 {
        self.initial_values.count
    }

    /// One array's starting elements: the array's number, and its bytes as they are stored in RAM.
    pub fn initial_values(&self, index: u16) -> Option<(u8, &'image [u8])> {
        let entry = self.initial_values.get(index)?;
        Some((*entry.first()?, &entry[1..]))
    }

    fn check_initial_values(&self, index: u16) -> Result<(), ImageError> {
        let (variable_number, bytes) = self
            .initial_values(index)
            .ok_or(ImageError::BadInitialValues(index))?;
        let variable = self
            .state_variable(variable_number as u16)
            .filter(|variable| variable.is_array() && variable.stored_bytes() == bytes.len())
            .ok_or(ImageError::BadInitialValues(index))?;
        for element in bytes.chunks(variable.element_bytes as usize) {
            let value = crate::state::read_packed(element).ok_or(ImageError::BadInitialValues(index))?;
            if !self.value_fits_type(variable.value_type, value) {
                return Err(ImageError::BadInitialValues(index));
            }
        }
        Ok(())
    }
}
