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
    /// Lays out the image's variables and fills each with its default value.
    pub fn new(image: &AppImage) -> Result<AppState, ImageError> {
        let mut state = AppState {
            bytes: [0; MOST_STATE_BYTES],
            placements: [Placement::default(); MOST_STATE_VARIABLES],
            count: image.state_count() as usize,
        };
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
        Ok(state)
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
