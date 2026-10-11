//! A simulated ST7789 panel that the real display driver talks to.
//!
//! It keeps the panel's 240x320 memory and follows the commands the driver sends, the way the
//! datasheet describes them, so the driver's scrolling and drawing are checked against a model of
//! the chip rather than against the driver's own idea of it.

use std::{cell::RefCell, convert::Infallible, rc::Rc};

use embedded_graphics::pixelcolor::{raw::RawU16, Rgb565};
use mipidsi::interface::{Interface, InterfaceKind};
use neotime_display::{MEMORY_ROWS, SCREEN_HEIGHT, SCREEN_WIDTH};

// ============================================================================
// Panel commands
// ============================================================================

const SLEEP_IN: u8 = 0x10;
const SLEEP_OUT: u8 = 0x11;
const INVERSION_OFF: u8 = 0x20;
const INVERSION_ON: u8 = 0x21;
const DISPLAY_OFF: u8 = 0x28;
const DISPLAY_ON: u8 = 0x29;
const SET_COLUMN_ADDRESS: u8 = 0x2A;
const SET_ROW_ADDRESS: u8 = 0x2B;
const WRITE_MEMORY_START: u8 = 0x2C;
const SET_SCROLL_AREA: u8 = 0x33;
const SET_SCROLL_START: u8 = 0x37;

const MEMORY_COLUMNS: u16 = 240;

// ============================================================================
// Panel state
// ============================================================================

struct AddressWindow {
    first_column: u16,
    last_column: u16,
    first_row: u16,
    last_row: u16,
}

struct WritePosition {
    column: u16,
    row: u16,
}

struct PanelState {
    memory: Vec<u16>,
    window: AddressWindow,
    write_position: WritePosition,
    top_fixed_rows: u16,
    scrolling_rows: u16,
    scroll_start_row: u16,
    colours_inverted: bool,
    sleeping: bool,
    display_on: bool,
}

impl PanelState {
    fn new() -> PanelState {
        PanelState {
            memory: vec![0; MEMORY_COLUMNS as usize * MEMORY_ROWS as usize],
            window: AddressWindow {
                first_column: 0,
                last_column: MEMORY_COLUMNS - 1,
                first_row: 0,
                last_row: MEMORY_ROWS - 1,
            },
            write_position: WritePosition { column: 0, row: 0 },
            top_fixed_rows: 0,
            scrolling_rows: MEMORY_ROWS,
            scroll_start_row: 0,
            colours_inverted: false,
            sleeping: true,
            display_on: false,
        }
    }

    fn run_command(&mut self, code: u8, arguments: &[u8]) {
        match code {
            SLEEP_IN => self.sleeping = true,
            SLEEP_OUT => self.sleeping = false,
            INVERSION_OFF => self.colours_inverted = false,
            INVERSION_ON => self.colours_inverted = true,
            DISPLAY_OFF => self.display_on = false,
            DISPLAY_ON => self.display_on = true,
            SET_COLUMN_ADDRESS => {
                self.window.first_column = read_u16(arguments, 0);
                self.window.last_column = read_u16(arguments, 2);
            }
            SET_ROW_ADDRESS => {
                self.window.first_row = read_u16(arguments, 0);
                self.window.last_row = read_u16(arguments, 2);
            }
            WRITE_MEMORY_START => {
                self.write_position = WritePosition {
                    column: self.window.first_column,
                    row: self.window.first_row,
                };
            }
            SET_SCROLL_AREA => {
                self.top_fixed_rows = read_u16(arguments, 0);
                self.scrolling_rows = read_u16(arguments, 2);
            }
            SET_SCROLL_START => self.scroll_start_row = read_u16(arguments, 0),
            _ => {}
        }
    }

    /// Writes one pixel and moves along the row, then down to the next row of the window.
    fn write_pixel(&mut self, raw_colour: u16) {
        let position = &mut self.write_position;
        if position.row > self.window.last_row {
            return;
        }

        let index = position.row as usize * MEMORY_COLUMNS as usize + position.column as usize;
        if let Some(memory_pixel) = self.memory.get_mut(index) {
            *memory_pixel = raw_colour;
        }

        if position.column >= self.window.last_column {
            position.column = self.window.first_column;
            position.row += 1;
        } else {
            position.column += 1;
        }
    }

    /// The scroll start row is shown at the top of the scrolling area, and the area wraps round.
    fn memory_row_shown_at(&self, screen_row: u16) -> u16 {
        let first_scrolling_row = self.top_fixed_rows;
        let end_of_scrolling_rows = first_scrolling_row + self.scrolling_rows;

        if screen_row < first_scrolling_row || screen_row >= end_of_scrolling_rows {
            return screen_row;
        }

        let start_within_area = self.scroll_start_row.saturating_sub(first_scrolling_row);
        let rows_into_area = screen_row - first_scrolling_row + start_within_area;
        first_scrolling_row + rows_into_area % self.scrolling_rows
    }

    /// The PineTime's glass inverts colours, so they look right only after INVERSION_ON.
    fn shown_colour(&self, raw_colour: u16) -> Rgb565 {
        let raw_colour = if self.colours_inverted {
            raw_colour
        } else {
            !raw_colour
        };
        Rgb565::from(RawU16::new(raw_colour))
    }
}

fn read_u16(arguments: &[u8], offset: usize) -> u16 {
    match arguments.get(offset..offset + 2) {
        Some(bytes) => u16::from_be_bytes([bytes[0], bytes[1]]),
        None => 0,
    }
}

// ============================================================================
// Panel
// ============================================================================

/// One handle goes to the display driver and a clone stays with the simulator to read the screen.
#[derive(Clone)]
pub struct SimulatedPanel {
    state: Rc<RefCell<PanelState>>,
}

impl SimulatedPanel {
    pub fn new() -> SimulatedPanel {
        SimulatedPanel {
            state: Rc::new(RefCell::new(PanelState::new())),
        }
    }

    /// The colour a person would see at a screen position, or black while the panel is off.
    pub fn shown_pixel(&self, column: u16, screen_row: u16) -> Rgb565 {
        let state = self.state.borrow();
        if state.sleeping || !state.display_on {
            return Rgb565::new(0, 0, 0);
        }

        let memory_row = state.memory_row_shown_at(screen_row);
        let index = memory_row as usize * MEMORY_COLUMNS as usize + column as usize;
        state.shown_colour(state.memory[index])
    }

    pub fn for_each_shown_pixel(&self, mut visit: impl FnMut(u16, u16, Rgb565)) {
        for screen_row in 0..SCREEN_HEIGHT {
            for column in 0..SCREEN_WIDTH {
                visit(column, screen_row, self.shown_pixel(column, screen_row));
            }
        }
    }
}

impl Interface for SimulatedPanel {
    type Word = u8;
    type Error = Infallible;
    const KIND: InterfaceKind = InterfaceKind::Serial4Line;

    fn send_command(&mut self, code: u8, arguments: &[u8]) -> Result<(), Infallible> {
        self.state.borrow_mut().run_command(code, arguments);
        Ok(())
    }

    fn send_pixels<const BYTES_PER_PIXEL: usize>(
        &mut self,
        pixels: impl IntoIterator<Item = [u8; BYTES_PER_PIXEL]>,
    ) -> Result<(), Infallible> {
        let mut state = self.state.borrow_mut();
        for pixel_bytes in pixels {
            state.write_pixel(u16::from_be_bytes([pixel_bytes[0], pixel_bytes[1]]));
        }
        Ok(())
    }

    fn send_repeated_pixel<const BYTES_PER_PIXEL: usize>(
        &mut self,
        pixel_bytes: [u8; BYTES_PER_PIXEL],
        repeat_count: u32,
    ) -> Result<(), Infallible> {
        let mut state = self.state.borrow_mut();
        let raw_colour = u16::from_be_bytes([pixel_bytes[0], pixel_bytes[1]]);
        for _ in 0..repeat_count {
            state.write_pixel(raw_colour);
        }
        Ok(())
    }
}
