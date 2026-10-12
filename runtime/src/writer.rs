//! Writes app images and their bytecode, for the compiler and for tests.
//!
//! `ImageWriter` lays out an image exactly as `AppImage::parse` reads it, and `CodeWriter` assembles
//! instructions with named jump targets, working out every jump's distance when the code is
//! finished. Only built with the `writer` feature, since the watch never writes images.

mod sharing;

use alloc::string::String;
use alloc::vec::Vec;

use crate::image::{crc32, AppKind, ValueType, FORMAT_VERSION, MAGIC};
use crate::instruction::Instruction;
use crate::resources::ImageFormat;
use crate::subscriptions::Subscription;
use crate::widgets::{Property, PropertySource, WidgetKind};

// ============================================================================
// Section numbers
// ============================================================================

const STRINGS_SECTION: u16 = 1;
const STATE_SECTION: u16 = 2;
const EXPRESSIONS_SECTION: u16 = 3;
const HANDLERS_SECTION: u16 = 4;
const WIDGETS_SECTION: u16 = 5;
const SUBSCRIPTIONS_SECTION: u16 = 6;
const RESOURCES_SECTION: u16 = 7;
const INITIAL_VALUES_SECTION: u16 = 8;
const FONTS_SECTION: u16 = 9;
const FUNCTIONS_SECTION: u16 = 10;

const HEADER_BYTES: usize = 20;
const SECTION_ENTRY_BYTES: usize = 12;

// ============================================================================
// Image writer
// ============================================================================

struct VariableEntry {
    key_string: u16,
    value_type: ValueType,
    persists: bool,
    length: u16,
    element_bytes: u8,
    default_value: i64,
}

/// Collects an app's parts and writes them out as one image.
pub struct ImageWriter {
    has_icon: bool,
    kind: AppKind,
    shared_library_version: u16,
    id_string: u16,
    name_string: u16,
    strings: Vec<String>,
    variables: Vec<VariableEntry>,
    expressions: Vec<Vec<u8>>,
    handlers: Vec<Vec<u8>>,
    widgets: Vec<Vec<u8>>,
    initial_values: Vec<Vec<u8>>,
    subscriptions: Vec<Subscription>,
    resources: Vec<Vec<u8>>,
    fonts: Vec<Vec<u8>>,
    functions: Vec<Vec<u8>>,
}

impl ImageWriter {
    pub fn new(id: &str, name: &str, kind: AppKind, shared_library_version: u16) -> ImageWriter {
        let mut writer = ImageWriter {
            kind,
            shared_library_version,
            has_icon: false,
            id_string: 0,
            name_string: 0,
            strings: Vec::new(),
            variables: Vec::new(),
            expressions: Vec::new(),
            handlers: Vec::new(),
            widgets: Vec::new(),
            initial_values: Vec::new(),
            subscriptions: Vec::new(),
            resources: Vec::new(),
            fonts: Vec::new(),
            functions: Vec::new(),
        };
        writer.id_string = writer.string(id);
        writer.name_string = writer.string(name);
        writer
    }

    /// The number of a string, adding it if the image does not hold it yet.
    pub fn string(&mut self, text: &str) -> u16 {
        if let Some(index) = self.strings.iter().position(|existing| existing == text) {
            return index as u16;
        }
        self.strings.push(text.into());
        (self.strings.len() - 1) as u16
    }

    /// Adds a single-value state variable and returns its number.
    pub fn variable(&mut self, key: &str, value_type: ValueType, persists: bool, default_value: i64) -> u8 {
        self.add_variable(key, value_type, persists, 0, 8, default_value)
    }

    /// Adds a fixed-length array whose elements are stored in 1, 2, 4 or 8 bytes.
    pub fn array(
        &mut self,
        key: &str,
        value_type: ValueType,
        persists: bool,
        length: u16,
        element_bytes: u8,
        default_value: i64,
    ) -> u8 {
        self.add_variable(key, value_type, persists, length, element_bytes, default_value)
    }

    /// Adds an array whose elements start with the given values, each stored in `element_bytes`.
    ///
    /// Returns `None` if a value does not fit that width.
    pub fn array_with_values(
        &mut self,
        key: &str,
        value_type: ValueType,
        persists: bool,
        element_bytes: u8,
        values: &[i64],
    ) -> Option<u8> {
        let mut entry: Vec<u8> = Vec::new();
        let variable = self.add_variable(key, value_type, persists, values.len() as u16, element_bytes, 0);
        entry.push(variable);
        for &value in values {
            let mut element = [0u8; 8];
            crate::state::write_packed(&mut element[..element_bytes as usize], value)?;
            entry.extend_from_slice(&element[..element_bytes as usize]);
        }
        self.initial_values.push(entry);
        Some(variable)
    }

    /// Marks image 0 as the app's icon for the launcher; add that image first.
    pub fn set_has_icon(&mut self) {
        self.has_icon = true;
    }

    /// Adds a subscription, such as a timer or `onOpen`, and returns its number.
    pub fn subscription(&mut self, subscription: Subscription) -> u16 {
        self.subscriptions.push(subscription);
        (self.subscriptions.len() - 1) as u16
    }

    /// Adds an image from its pixels: RGB565 colours, or coverage from 0 to 255 for a mask.
    /// Runs are found here, so the caller gives every pixel, left to right and top to bottom.
    /// Adds a text font, as `tools/generate_fonts.py` writes it; only the shared library has fonts.
    pub fn font(&mut self, bytes: &[u8]) -> u16 {
        self.fonts.push(bytes.to_vec());
        (self.fonts.len() - 1) as u16
    }

    /// Adds an image that only names a shared library image, such as an app's icon.
    pub fn library_image_reference(&mut self, library_image: u16) -> u16 {
        self.resources
            .push(crate::resources::library_image_reference_bytes(library_image).to_vec());
        (self.resources.len() - 1) as u16
    }

    pub fn image_resource(&mut self, format: ImageFormat, width: u16, height: u16, pixels: &[u16]) -> u16 {
        let mut bytes: Vec<u8> = alloc::vec![
            match format {
                ImageFormat::Color => 0,
                ImageFormat::Mask => 1,
            },
            0
        ];
        bytes.extend(width.to_le_bytes());
        bytes.extend(height.to_le_bytes());
        let mut index = 0;
        while index < pixels.len() {
            let value = pixels[index];
            let mut count = 1;
            while count < u8::MAX as usize && index + count < pixels.len() && pixels[index + count] == value {
                count += 1;
            }
            bytes.push(count as u8);
            match format {
                ImageFormat::Color => bytes.extend(value.to_le_bytes()),
                ImageFormat::Mask => bytes.push(value.min(u8::MAX as u16) as u8),
            }
            index += count;
        }
        self.resources.push(bytes);
        (self.resources.len() - 1) as u16
    }

    fn add_variable(
        &mut self,
        key: &str,
        value_type: ValueType,
        persists: bool,
        length: u16,
        element_bytes: u8,
        default_value: i64,
    ) -> u8 {
        let key_string = self.string(key);
        self.variables.push(VariableEntry {
            key_string,
            value_type,
            persists,
            length,
            element_bytes,
            default_value,
        });
        (self.variables.len() - 1) as u8
    }

    /// How many expressions the image holds so far.
    pub fn expression_count(&self) -> usize {
        self.expressions.len()
    }

    pub fn expression(&mut self, code: Vec<u8>) -> u16 {
        self.expressions.push(code);
        (self.expressions.len() - 1) as u16
    }

    pub fn handler(&mut self, code: Vec<u8>) -> u16 {
        self.handlers.push(code);
        (self.handlers.len() - 1) as u16
    }

    /// How many functions the image holds so far, which is also the number the next one gets.
    pub fn function_count(&self) -> usize {
        self.functions.len()
    }

    /// Adds a function that `Instruction::Call` runs; its arguments arrive in its first locals.
    pub fn function(&mut self, argument_count: u8, result_count: u8, local_count: u8, code: Vec<u8>) -> u8 {
        let mut bytes = alloc::vec![argument_count, result_count, local_count];
        bytes.extend(code);
        self.functions.push(bytes);
        (self.functions.len() - 1) as u8
    }

    /// Adds a widget. Widgets must be added depth first: each one followed by its children.
    pub fn widget(
        &mut self,
        kind: WidgetKind,
        child_count: u8,
        properties: &[(Property, PropertySource)],
    ) -> u16 {
        let mut record = alloc::vec![kind as u8, child_count, properties.len() as u8, 0];
        for (property, source) in properties {
            let (source_byte, value) = match *source {
                PropertySource::Constant(value) => (0u8, value),
                PropertySource::Expression(index) => (1, index as i32),
                PropertySource::Handler(index) => (2, index as i32),
            };
            record.push(*property as u8);
            record.push(source_byte);
            record.extend(value.to_le_bytes());
        }
        self.widgets.push(record);
        (self.widgets.len() - 1) as u16
    }

    /// Changes how many children a widget has, for a writer that learns it after adding them.
    pub fn set_child_count(&mut self, widget: u16, child_count: u8) {
        if let Some(record) = self.widgets.get_mut(widget as usize) {
            record[1] = child_count;
        }
    }

    pub fn build(&self) -> Vec<u8> {
        let strings: Vec<Vec<u8>> = self.strings.iter().map(|text| text.as_bytes().to_vec()).collect();
        let mut sections = alloc::vec![
            (STRINGS_SECTION, self.strings.len() as u16, entry_table(&strings)),
            (STATE_SECTION, self.variables.len() as u16, self.state_section()),
            (
                EXPRESSIONS_SECTION,
                self.expressions.len() as u16,
                entry_table(&self.expressions),
            ),
            (
                HANDLERS_SECTION,
                self.handlers.len() as u16,
                entry_table(&self.handlers),
            ),
            (
                WIDGETS_SECTION,
                self.widgets.len() as u16,
                entry_table(&self.widgets),
            ),
            (
                SUBSCRIPTIONS_SECTION,
                self.subscriptions.len() as u16,
                self.subscriptions
                    .iter()
                    .flat_map(|subscription| subscription.to_bytes())
                    .collect(),
            ),
            (
                RESOURCES_SECTION,
                self.resources.len() as u16,
                entry_table(&self.resources),
            ),
            (
                INITIAL_VALUES_SECTION,
                self.initial_values.len() as u16,
                entry_table(&self.initial_values),
            ),
        ];
        // Only the shared library has fonts, so app images keep their eight sections.
        if !self.fonts.is_empty() {
            sections.push((FONTS_SECTION, self.fonts.len() as u16, entry_table(&self.fonts)));
        }
        if !self.functions.is_empty() {
            sections.push((
                FUNCTIONS_SECTION,
                self.functions.len() as u16,
                entry_table(&self.functions),
            ));
        }

        let mut image: Vec<u8> = Vec::new();
        image.extend(MAGIC);
        image.extend(FORMAT_VERSION.to_le_bytes());
        image.push(match self.kind {
            AppKind::App => 0,
            AppKind::Face => 1,
            AppKind::Library => 2,
        });
        image.push(self.has_icon as u8);
        image.extend(self.shared_library_version.to_le_bytes());
        image.extend((sections.len() as u16).to_le_bytes());
        image.extend([0; 4]);
        image.extend(self.id_string.to_le_bytes());
        image.extend(self.name_string.to_le_bytes());

        let mut offset = HEADER_BYTES + sections.len() * SECTION_ENTRY_BYTES;
        for (number, count, bytes) in &sections {
            image.extend(number.to_le_bytes());
            image.extend(count.to_le_bytes());
            image.extend((offset as u32).to_le_bytes());
            image.extend((bytes.len() as u32).to_le_bytes());
            offset += bytes.len();
        }
        for (_, _, bytes) in &sections {
            image.extend(bytes);
        }

        let crc = crc32(&image[16..]);
        image[12..16].copy_from_slice(&crc.to_le_bytes());
        image
    }

    fn state_section(&self) -> Vec<u8> {
        let mut section: Vec<u8> = Vec::new();
        for variable in &self.variables {
            let width_code = match (variable.length, variable.element_bytes) {
                (0, _) | (_, 8) => 0,
                (_, 1) => 1,
                (_, 2) => 2,
                _ => 3,
            };
            section.extend(variable.key_string.to_le_bytes());
            section.push(value_type_byte(variable.value_type));
            section.push(variable.persists as u8 | width_code << 1);
            section.extend(variable.length.to_le_bytes());
            section.extend([0, 0]);
            section.extend(variable.default_value.to_le_bytes());
        }
        section
    }
}

fn value_type_byte(value_type: ValueType) -> u8 {
    match value_type {
        ValueType::Int => 0,
        ValueType::Bool => 1,
        ValueType::Instant => 2,
        ValueType::Duration => 3,
        ValueType::Color => 4,
        ValueType::Text => 5,
    }
}

/// A table of (offset, length) pairs followed by the data they point into.
fn entry_table(entries: &[Vec<u8>]) -> Vec<u8> {
    let mut table: Vec<u8> = Vec::new();
    let mut data: Vec<u8> = Vec::new();
    for entry in entries {
        table.extend((data.len() as u16).to_le_bytes());
        table.extend((entry.len() as u16).to_le_bytes());
        data.extend(entry);
    }
    table.extend(data);
    table
}

// ============================================================================
// Code writer
// ============================================================================

/// A place in the code that jumps can go to, before or after it is placed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Label(usize);

/// Why code could not be finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeError {
    LabelNeverPlaced(Label),
    JumpBackward,
    LoopForward,
    JumpTooFar,
}

#[derive(Clone, Copy)]
enum JumpKind {
    Always,
    IfFalse,
    LoopBack,
}

struct PendingJump {
    /// Where the jump's 2-byte distance is.
    operand_offset: usize,
    kind: JumpKind,
    target: Label,
}

/// Assembles instructions, with jumps written against labels.
#[derive(Default)]
pub struct CodeWriter {
    bytes: Vec<u8>,
    label_offsets: Vec<Option<usize>>,
    jumps: Vec<PendingJump>,
}

impl CodeWriter {
    pub fn new() -> CodeWriter {
        CodeWriter::default()
    }

    pub fn emit(&mut self, instruction: Instruction) -> &mut Self {
        self.bytes.extend(instruction.encode().as_bytes());
        self
    }

    /// How many bytes of code are written so far.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// A new label, placed later with `place`.
    pub fn label(&mut self) -> Label {
        self.label_offsets.push(None);
        Label(self.label_offsets.len() - 1)
    }

    /// Places a label at the next instruction.
    pub fn place(&mut self, label: Label) -> &mut Self {
        self.label_offsets[label.0] = Some(self.bytes.len());
        self
    }

    /// Writes a `LoopStart` and returns a label for `loop_back` to return to.
    pub fn loop_start(&mut self) -> Label {
        let label = self.label();
        self.place(label);
        self.emit(Instruction::LoopStart);
        label
    }

    pub fn jump(&mut self, target: Label) -> &mut Self {
        self.jump_with(Instruction::Jump(0), JumpKind::Always, target)
    }

    pub fn jump_if_false(&mut self, target: Label) -> &mut Self {
        self.jump_with(Instruction::JumpIfFalse(0), JumpKind::IfFalse, target)
    }

    pub fn loop_back(&mut self, loop_start: Label) -> &mut Self {
        self.jump_with(Instruction::LoopBack(0), JumpKind::LoopBack, loop_start)
    }

    fn jump_with(&mut self, instruction: Instruction, kind: JumpKind, target: Label) -> &mut Self {
        self.emit(instruction);
        self.jumps.push(PendingJump {
            operand_offset: self.bytes.len() - 2,
            kind,
            target,
        });
        self
    }

    /// The finished code, with every jump distance filled in.
    pub fn finish(mut self) -> Result<Vec<u8>, CodeError> {
        for jump in &self.jumps {
            let target = self.label_offsets[jump.target.0].ok_or(CodeError::LabelNeverPlaced(jump.target))?;
            let after_jump = jump.operand_offset + 2;
            let distance = match jump.kind {
                JumpKind::Always | JumpKind::IfFalse => {
                    target.checked_sub(after_jump).ok_or(CodeError::JumpBackward)?
                }
                JumpKind::LoopBack => after_jump.checked_sub(target).ok_or(CodeError::LoopForward)?,
            };
            let distance = u16::try_from(distance).map_err(|_| CodeError::JumpTooFar)?;
            self.bytes[jump.operand_offset..after_jump].copy_from_slice(&distance.to_le_bytes());
        }
        Ok(self.bytes)
    }
}
