//! A simulated ST7789 panel that the real display driver talks to.
//!
//! It keeps the panel's 240x320 memory and follows the commands the driver sends, the way the
//! datasheet describes them, so the driver's scrolling and drawing are checked against a model of
//! the chip rather than against the driver's own idea of it.

use std::{cell::RefCell, convert::Infallible, rc::Rc};

use battletime_display::PanelInterface;
use battletime_display::{MEMORY_ROWS, SCREEN_HEIGHT, SCREEN_WIDTH};
use embedded_graphics::{
    pixelcolor::{raw::RawU16, Rgb565},
    prelude::RgbColor,
};

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
const IDLE_MODE_OFF: u8 = 0x38;
const IDLE_MODE_ON: u8 = 0x39;

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
    in_idle_mode: bool,
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
            in_idle_mode: false,
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
            IDLE_MODE_OFF => self.in_idle_mode = false,
            IDLE_MODE_ON => self.in_idle_mode = true,
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
        let colour = Rgb565::from(RawU16::new(raw_colour));

        if self.in_idle_mode {
            eight_colour_version(colour)
        } else {
            colour
        }
    }
}

/// Idle mode keeps only the top bit of each channel, so each is fully on or fully off.
fn eight_colour_version(colour: Rgb565) -> Rgb565 {
    let channel = |value: u8, max_value: u8| if value > max_value / 2 { max_value } else { 0 };
    Rgb565::new(
        channel(colour.r(), Rgb565::MAX_R),
        channel(colour.g(), Rgb565::MAX_G),
        channel(colour.b(), Rgb565::MAX_B),
    )
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

impl PanelInterface for SimulatedPanel {
    type Error = Infallible;

    fn send_command(&mut self, code: u8, arguments: &[u8]) -> Result<(), Infallible> {
        self.state.borrow_mut().run_command(code, arguments);
        Ok(())
    }

    fn send_pixels(&mut self, bytes: &[u8]) -> Result<(), Infallible> {
        let mut state = self.state.borrow_mut();
        for &pixel_bytes in bytes.as_chunks::<2>().0 {
            state.write_pixel(u16::from_be_bytes(pixel_bytes));
        }
        Ok(())
    }

    fn send_repeated_pixel(&mut self, pixel_bytes: [u8; 2], repeat_count: u32) -> Result<(), Infallible> {
        let mut state = self.state.borrow_mut();
        let raw_colour = u16::from_be_bytes(pixel_bytes);
        for _ in 0..repeat_count {
            state.write_pixel(raw_colour);
        }
        Ok(())
    }
}

// ============================================================================
// The SPI wires, for drivers that send bytes and drive the data/command line themselves
// ============================================================================

#[cfg(test)]
pub use wires::{DataOrCommandPin, PanelSpi};

/// Only the tests drive the panel over its SPI wires so far, for the status screen, which sends its bytes itself.
#[cfg(test)]
mod wires {
    use std::{cell::RefCell, convert::Infallible, rc::Rc};

    use embedded_hal::{
        digital::{ErrorType as PinErrorType, OutputPin},
        spi::{ErrorType as SpiErrorType, Operation, SpiDevice},
    };

    use super::{
        SimulatedPanel, SET_COLUMN_ADDRESS, SET_ROW_ADDRESS, SET_SCROLL_AREA, SET_SCROLL_START,
        WRITE_MEMORY_START,
    };

    const MEMORY_ACCESS_CONTROL: u8 = 0x36;
    const PIXEL_FORMAT: u8 = 0x3A;

    /// What the chip has seen on its wires: a byte with the line low is a command, and the bytes after
    /// it with the line high are its arguments or, after WRITE_MEMORY_START, pixels two bytes each.
    #[derive(Default)]
    struct WireState {
        data_mode: bool,
        command: Option<u8>,
        arguments: Vec<u8>,
        writing_pixels: bool,
        pixel_high_byte: Option<u8>,
    }

    /// How many argument bytes each command takes, so it runs as soon as they have all arrived.
    fn argument_count(code: u8) -> usize {
        match code {
            SET_COLUMN_ADDRESS | SET_ROW_ADDRESS => 4,
            SET_SCROLL_AREA => 6,
            SET_SCROLL_START => 2,
            MEMORY_ACCESS_CONTROL | PIXEL_FORMAT => 1,
            _ => 0,
        }
    }

    impl SimulatedPanel {
        /// The panel's SPI bus and data/command line, for a driver that sends the bytes itself rather than through the display driver.
        pub fn spi_wires(&self) -> (PanelSpi, DataOrCommandPin) {
            let wires = Rc::new(RefCell::new(WireState::default()));
            (
                PanelSpi {
                    panel: self.clone(),
                    wires: wires.clone(),
                },
                DataOrCommandPin { wires },
            )
        }
    }

    pub struct PanelSpi {
        panel: SimulatedPanel,
        wires: Rc<RefCell<WireState>>,
    }

    impl PanelSpi {
        fn receive_byte(&self, byte: u8) {
            let mut wires = self.wires.borrow_mut();
            let mut state = self.panel.state.borrow_mut();
            if !wires.data_mode {
                wires.command = Some(byte);
                wires.arguments.clear();
                wires.writing_pixels = byte == WRITE_MEMORY_START;
                wires.pixel_high_byte = None;
                if argument_count(byte) == 0 {
                    state.run_command(byte, &[]);
                }
                return;
            }

            if wires.writing_pixels {
                match wires.pixel_high_byte.take() {
                    None => wires.pixel_high_byte = Some(byte),
                    Some(high) => state.write_pixel(u16::from_be_bytes([high, byte])),
                }
                return;
            }

            let Some(code) = wires.command else {
                return;
            };
            wires.arguments.push(byte);
            if wires.arguments.len() == argument_count(code) {
                state.run_command(code, &wires.arguments);
            }
        }
    }

    impl SpiErrorType for PanelSpi {
        type Error = Infallible;
    }

    impl SpiDevice for PanelSpi {
        fn transaction(&mut self, operations: &mut [Operation<'_, u8>]) -> Result<(), Infallible> {
            for operation in operations {
                if let Operation::Write(bytes) = operation {
                    for byte in bytes.iter() {
                        self.receive_byte(*byte);
                    }
                }
            }
            Ok(())
        }
    }

    pub struct DataOrCommandPin {
        wires: Rc<RefCell<WireState>>,
    }

    impl PinErrorType for DataOrCommandPin {
        type Error = Infallible;
    }

    impl OutputPin for DataOrCommandPin {
        fn set_low(&mut self) -> Result<(), Infallible> {
            self.wires.borrow_mut().data_mode = false;
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), Infallible> {
            self.wires.borrow_mut().data_mode = true;
            Ok(())
        }
    }
}

#[cfg(test)]
#[path = "panel_tests.rs"]
mod panel_tests;
