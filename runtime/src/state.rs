//! An open app's state: every variable and array packed into one buffer, each element stored in
//! only as many bytes as the image asks for.

use crate::image::{AppImage, ImageError, StateVariable, MOST_STATE_BYTES, MOST_STATE_VARIABLES};
use crate::interpreter::Value;

// ============================================================================
// Reading and writing packed values
// ============================================================================

/// Reads a signed little-endian value of 1, 2, 4 or 8 bytes.
pub(crate) fn read_packed(bytes: &[u8]) -> Option<Value> {
    Some(match bytes.len() {
        1 => bytes[0] as i8 as Value,
        2 => i16::from_le_bytes(bytes.try_into().ok()?) as Value,
        4 => i32::from_le_bytes(bytes.try_into().ok()?) as Value,
        8 => i64::from_le_bytes(bytes.try_into().ok()?),
        _ => return None,
    })
}

/// Writes a value into 1, 2, 4 or 8 bytes, or returns `None` if it does not fit.
pub(crate) fn write_packed(bytes: &mut [u8], value: Value) -> Option<()> {
    match bytes.len() {
        1 => bytes.copy_from_slice(&i8::try_from(value).ok()?.to_le_bytes()),
        2 => bytes.copy_from_slice(&i16::try_from(value).ok()?.to_le_bytes()),
        4 => bytes.copy_from_slice(&i32::try_from(value).ok()?.to_le_bytes()),
        8 => bytes.copy_from_slice(&value.to_le_bytes()),
        _ => return None,
    }
    Some(())
}

// ============================================================================
// Layout
// ============================================================================

/// Where one variable lives in the state buffer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Placement {
    pub offset: u16,
    /// Zero for a single value.
    pub length: u16,
    pub element_bytes: u8,
}

impl Placement {
    const EMPTY: Placement = Placement {
        offset: 0,
        length: 0,
        element_bytes: 0,
    };

    fn new(offset: usize, variable: &StateVariable) -> Placement {
        Placement {
            offset: offset as u16,
            length: variable.length,
            element_bytes: variable.element_bytes,
        }
    }

    pub fn is_array(&self) -> bool {
        self.length > 0
    }

    pub fn stored_bytes(&self) -> usize {
        self.length.max(1) as usize * self.element_bytes as usize
    }

    /// Where element `index` lies inside the variable's bytes, if it exists.
    pub fn element_range(&self, index: Value) -> Option<core::ops::Range<usize>> {
        let index = usize::try_from(index).ok()?;
        if index >= self.length.max(1) as usize {
            return None;
        }
        let start = index * self.element_bytes as usize;
        Some(start..start + self.element_bytes as usize)
    }
}

// ============================================================================
// App state
// ============================================================================

/// The state of the open app.
pub struct AppState {
    bytes: [u8; MOST_STATE_BYTES],
    placements: [Placement; MOST_STATE_VARIABLES],
    count: usize,
}

impl AppState {
    /// A state holding no variables, to be laid out in place with `load_layout`.
    pub const EMPTY: AppState = AppState {
        bytes: [0; MOST_STATE_BYTES],
        placements: [Placement::EMPTY; MOST_STATE_VARIABLES],
        count: 0,
    };

    /// Lays out the image's variables and fills each with its default value.
    pub fn new(image: &AppImage) -> Result<AppState, ImageError> {
        let mut state = AppState::EMPTY;
        state.load_layout(image)?;
        Ok(state)
    }

    /// Does what `new` does in place, so a state kept on the heap is never built on the stack.
    pub fn load_layout(&mut self, image: &AppImage) -> Result<(), ImageError> {
        *self = AppState {
            count: image.state_count() as usize,
            ..AppState::EMPTY
        };
        let state = self;
        let mut offset = 0;
        for index in 0..image.state_count() {
            let variable = image.state_variable(index).ok_or(ImageError::TableOutOfBounds)?;
            let placement = Placement::new(offset, &variable);
            offset += placement.stored_bytes();
            if offset > MOST_STATE_BYTES {
                return Err(ImageError::StateTooLarge);
            }
            state.placements[index as usize] = placement;
            state
                .fill(index as u8, variable.default_value)
                .ok_or(ImageError::BadDefaultValue(index as u8))?;
        }
        for index in 0..image.initial_values_count() {
            let (variable, bytes) = image
                .initial_values(index)
                .ok_or(ImageError::BadInitialValues(index))?;
            let stored = state
                .variable_bytes_mut(variable)
                .ok_or(ImageError::BadInitialValues(index))?;
            if stored.len() != bytes.len() {
                return Err(ImageError::BadInitialValues(index));
            }
            stored.copy_from_slice(bytes);
        }
        Ok(())
    }

    pub fn placement(&self, variable: u8) -> Option<Placement> {
        self.placements[..self.count].get(variable as usize).copied()
    }

    /// The bytes one variable is stored in.
    pub fn variable_bytes(&self, variable: u8) -> Option<&[u8]> {
        let placement = self.placement(variable)?;
        let start = placement.offset as usize;
        self.bytes.get(start..start + placement.stored_bytes())
    }

    pub fn variable_bytes_mut(&mut self, variable: u8) -> Option<&mut [u8]> {
        let placement = self.placement(variable)?;
        let start = placement.offset as usize;
        self.bytes.get_mut(start..start + placement.stored_bytes())
    }

    /// A single value, or element 0 of an array.
    pub fn get(&self, variable: u8) -> Option<Value> {
        self.element(variable, 0)
    }

    pub fn element(&self, variable: u8, index: Value) -> Option<Value> {
        let range = self.placement(variable)?.element_range(index)?;
        read_packed(self.variable_bytes(variable)?.get(range)?)
    }

    /// Sets a single value, such as one loaded from the key-value store.
    pub fn set(&mut self, variable: u8, value: Value) -> Option<()> {
        self.set_element(variable, 0, value)
    }

    pub fn set_element(&mut self, variable: u8, index: Value, value: Value) -> Option<()> {
        let range = self.placement(variable)?.element_range(index)?;
        write_packed(self.variable_bytes_mut(variable)?.get_mut(range)?, value)
    }

    /// Sets every element of a variable to one value.
    pub fn fill(&mut self, variable: u8, value: Value) -> Option<()> {
        let element_bytes = self.placement(variable)?.element_bytes as usize;
        for element in self.variable_bytes_mut(variable)?.chunks_mut(element_bytes) {
            write_packed(element, value)?;
        }
        Some(())
    }
}

// ============================================================================
// Saving and loading
// ============================================================================

/// Starts saved state, so a stray file is never read as one.
const SAVED_STATE_MAGIC: [u8; 4] = *b"NTST";
const SAVED_STATE_VERSION: u8 = 1;
const SAVED_HEADER_BYTES: usize = 6;

/// The saved state did not fit the buffer it was given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveBufferFull;

/// What loading saved state did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoadReport {
    /// Variables restored from the saved state.
    pub loaded: u8,
    /// Saved variables left out: the app no longer has them, or their type or length changed.
    pub skipped: u8,
    /// The bytes were not saved state at all, so every variable kept its default.
    pub unreadable: bool,
}

impl AppState {
    /// Writes every persistent variable, with its key, into `out`, and returns how many bytes it used.
    ///
    /// Each variable is saved as: key length (1), key, type (1), array length (2), element width (1),
    /// then its bytes as stored. Keys rather than numbers let a new version of the app load it.
    pub fn save_persistent(&self, image: &AppImage, out: &mut [u8]) -> Result<usize, SaveBufferFull> {
        let mut used = SAVED_HEADER_BYTES;
        let mut count: u8 = 0;
        for index in 0..image.state_count() {
            let Some(variable) = image.state_variable(index).filter(|variable| variable.persists) else {
                continue;
            };
            let key = image.string(variable.key_string).unwrap_or("").as_bytes();
            let data = self.variable_bytes(index as u8).unwrap_or(&[]);
            let mut entry_header = [0u8; 5];
            entry_header[0] = variable.value_type as u8;
            entry_header[1..3].copy_from_slice(&variable.length.to_le_bytes());
            entry_header[3] = variable.element_bytes;
            let key_length = u8::try_from(key.len()).map_err(|_| SaveBufferFull)?;

            for part in [&[key_length][..], key, &entry_header[..4], data] {
                let end = used + part.len();
                out.get_mut(used..end)
                    .ok_or(SaveBufferFull)?
                    .copy_from_slice(part);
                used = end;
            }
            count += 1;
        }
        let header = out.get_mut(..SAVED_HEADER_BYTES).ok_or(SaveBufferFull)?;
        header[..4].copy_from_slice(&SAVED_STATE_MAGIC);
        header[4] = SAVED_STATE_VERSION;
        header[5] = count;
        Ok(used)
    }

    /// Restores persistent variables from saved state, matching them by key, type and length.
    pub fn load_persistent(&mut self, image: &AppImage, saved: &[u8]) -> LoadReport {
        let mut report = LoadReport::default();
        let readable = saved.len() >= SAVED_HEADER_BYTES
            && saved[..4] == SAVED_STATE_MAGIC
            && saved[4] == SAVED_STATE_VERSION;
        if !readable {
            report.unreadable = true;
            return report;
        }

        let mut position = SAVED_HEADER_BYTES;
        for _ in 0..saved[5] {
            let Some(entry) = read_saved_entry(saved, &mut position) else {
                report.unreadable = true;
                return report;
            };
            let found = (0..image.state_count()).find(|&index| {
                image.state_variable(index).is_some_and(|variable| {
                    variable.persists
                        && image.string(variable.key_string).map(str::as_bytes) == Some(entry.key)
                        && variable.value_type as u8 == entry.value_type
                        && variable.length == entry.length
                        && variable.element_bytes == entry.element_bytes
                })
            });
            match found.and_then(|index| self.variable_bytes_mut(index as u8)) {
                Some(stored) if stored.len() == entry.data.len() => {
                    stored.copy_from_slice(entry.data);
                    report.loaded += 1;
                }
                _ => report.skipped += 1,
            }
        }
        report
    }
}

struct SavedEntry<'saved> {
    key: &'saved [u8],
    value_type: u8,
    length: u16,
    element_bytes: u8,
    data: &'saved [u8],
}

fn read_saved_entry<'saved>(saved: &'saved [u8], position: &mut usize) -> Option<SavedEntry<'saved>> {
    let key_length = *saved.get(*position)? as usize;
    let key = saved.get(*position + 1..*position + 1 + key_length)?;
    let header = saved.get(*position + 1 + key_length..*position + 5 + key_length)?;
    let value_type = header[0];
    let length = u16::from_le_bytes([header[1], header[2]]);
    let element_bytes = header[3];
    let data_bytes = length.max(1) as usize * element_bytes as usize;
    let data_start = *position + 5 + key_length;
    let data = saved.get(data_start..data_start + data_bytes)?;
    *position = data_start + data_bytes;
    Some(SavedEntry {
        key,
        value_type,
        length,
        element_bytes,
        data,
    })
}
