//! Driver for the PineTime's 240x240 ST7789 display, with hardware vertical scrolling.
//!
//! The ST7789 holds 320 rows of memory but the panel shows only 240, and scrolling changes which
//! memory row appears at the top of the screen. Drawing takes screen positions and lands on whichever
//! memory rows are on screen, split in two where the memory wraps.
//!
//! The driver speaks to the panel through `PanelInterface`: commands, and pixels as bytes in the
//! panel's order. An interface may send a strip of panel pixels in the background, such as by DMA,
//! while the caller fills the next one; `MipidsiInterface` adapts any mipidsi interface.
//!
//! The panel also has a low-power mode for an always-on screen, and `Backlight` drives the three
//! backlight pins that light it.

#![no_std]

use embedded_graphics_core::{
    draw_target::DrawTarget,
    geometry::{Dimensions, OriginDimensions, Size},
    pixelcolor::{raw::RawU16, Rgb565},
    prelude::{PointsIter, RawData},
    primitives::Rectangle,
    Pixel,
};
use embedded_hal::{delay::DelayNs, digital::OutputPin};

// ============================================================================
// Panel size
// ============================================================================

pub const SCREEN_WIDTH: u16 = 240;
pub const SCREEN_HEIGHT: u16 = 240;
pub const MEMORY_ROWS: u16 = 320;

// ============================================================================
// Panel commands
// ============================================================================

const ENTER_SLEEP: u8 = 0x10;
const EXIT_SLEEP: u8 = 0x11;
const NORMAL_MODE_ON: u8 = 0x13;
const INVERSION_ON: u8 = 0x21;
const DISPLAY_ON: u8 = 0x29;
const SET_COLUMN_ADDRESS: u8 = 0x2A;
const SET_ROW_ADDRESS: u8 = 0x2B;
const WRITE_MEMORY: u8 = 0x2C;
const SET_SCROLL_AREA: u8 = 0x33;
const SET_MEMORY_ORDER: u8 = 0x36;
const SET_SCROLL_START: u8 = 0x37;
const IDLE_MODE_OFF: u8 = 0x38;
const IDLE_MODE_ON: u8 = 0x39;
const SET_PIXEL_FORMAT: u8 = 0x3A;
const SET_PORCHES: u8 = 0xB2;
const SET_IDLE_FRAME_RATE: u8 = 0xB3;

/// Rows top to bottom, columns left to right, red first.
const MEMORY_ORDER: u8 = 0x00;
/// 16 bits a pixel, for both the panel and its interface.
const SIXTEEN_BITS_A_PIXEL: u8 = 0x55;

/// Normal porches as the panel starts, with long porches in idle mode to slow its refresh.
const LOW_POWER_PORCHES: [u8; 5] = [0x0C, 0x0C, 0x01, 0xED, 0xED];
const DEFAULT_PORCHES: [u8; 5] = [0x0C, 0x0C, 0x00, 0x33, 0x33];

/// How long the reset pin is held low, and how long the panel then needs before its first command.
const RESET_PULSE_IN_MICROSECONDS: u32 = 10;
const AFTER_RESET_IN_MICROSECONDS: u32 = 150_000;
/// How long the panel needs after a step of starting up before the next.
const START_STEP_IN_MICROSECONDS: u32 = 10_000;
/// How long the panel needs after turning its display on.
const AFTER_DISPLAY_ON_IN_MICROSECONDS: u32 = 120_000;

/// How long the panel needs after going to sleep or waking before it takes the next command.
const SLEEP_SWITCH_SETTLE_TIME_IN_MICROSECONDS: u32 = 6_000;

/// How long the panel needs after going to sleep or waking before it switches the other way.
pub const SLEEP_SWITCH_GAP_IN_MILLISECONDS: u64 = 125;

/// Divides the idle frame rate by four.
const LOW_POWER_IDLE_FRAME_RATE: [u8; 3] = [0x12, 0x1E, 0x1E];
const DEFAULT_IDLE_FRAME_RATE: [u8; 3] = [0x00, 0x0F, 0x0F];

// ============================================================================
// Sending in pieces
// ============================================================================

/// How a send of many bytes splits into equal pieces, for a DMA engine that moves a limited number at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pieces {
    pub piece_bytes: usize,
    pub count: usize,
}

impl Pieces {
    /// The fewest equal pieces of at most `largest_piece_bytes` that `length` bytes split into.
    pub fn of(length: usize, largest_piece_bytes: usize) -> Pieces {
        let piece_bytes = (1..=largest_piece_bytes.min(length))
            .rev()
            .find(|piece_bytes| length.is_multiple_of(*piece_bytes))
            .unwrap_or(1);
        Pieces {
            piece_bytes,
            count: length.div_ceil(piece_bytes),
        }
    }
}

// ============================================================================
// Talking to the panel
// ============================================================================

/// A pixel as the panel takes it: RGB565 with its high byte first, ready to send as it lies in memory.
pub fn panel_pixel(color: Rgb565) -> u16 {
    RawU16::from(color).into_inner().to_be()
}

/// The colour of a pixel in the panel's order.
pub fn color_of_panel_pixel(pixel: u16) -> Rgb565 {
    Rgb565::from(RawU16::new(u16::from_be(pixel)))
}

/// How the driver reaches the panel.
pub trait PanelInterface {
    type Error;

    /// Sends a command byte and its arguments.
    fn send_command(&mut self, command: u8, arguments: &[u8]) -> Result<(), Self::Error>;

    /// Sends pixel bytes after a memory write, two a pixel, high byte first.
    fn send_pixels(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;

    /// Sends one pixel `count` times after a memory write.
    fn send_repeated_pixel(&mut self, pixel: [u8; 2], count: u32) -> Result<(), Self::Error> {
        const CHUNK_PIXELS: usize = 32;
        let mut chunk = [0u8; CHUNK_PIXELS * 2];
        chunk.as_chunks_mut::<2>().0.fill(pixel);
        let mut left = count as usize;
        while left > 0 {
            let pixels = left.min(CHUNK_PIXELS);
            self.send_pixels(&chunk[..pixels * 2])?;
            left -= pixels;
        }
        Ok(())
    }

    /// Sends panel pixels after a memory write; an interface may still be reading them after it returns.
    ///
    /// The caller leaves `pixels` untouched until the interface is next called, which finishes the send.
    fn send_strip_pixels(&mut self, pixels: &[u16]) -> Result<(), Self::Error> {
        for &pixel in pixels {
            self.send_pixels(&pixel.to_ne_bytes())?;
        }
        Ok(())
    }
}

/// Drives the panel through any mipidsi interface, such as its `SpiInterface`.
pub struct MipidsiInterface<Interface>(pub Interface);

impl<Interface> PanelInterface for MipidsiInterface<Interface>
where
    Interface: mipidsi::interface::Interface<Word = u8>,
{
    type Error = Interface::Error;

    fn send_command(&mut self, command: u8, arguments: &[u8]) -> Result<(), Self::Error> {
        self.0.send_command(command, arguments)
    }

    fn send_pixels(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.0.send_pixels(bytes.as_chunks::<2>().0.iter().copied())
    }

    fn send_repeated_pixel(&mut self, pixel: [u8; 2], count: u32) -> Result<(), Self::Error> {
        self.0.send_repeated_pixel(pixel, count)
    }
}

// ============================================================================
// Display
// ============================================================================

/// Which memory rows scroll: those after the top fixed rows, `scrolling_rows` of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollArea {
    pub top_fixed_rows: u16,
    pub scrolling_rows: u16,
}

/// Why the panel did not start: its interface or its reset pin failed.
#[derive(Debug)]
pub enum StartError<InterfaceError, PinError> {
    Interface(InterfaceError),
    ResetPin(PinError),
}

pub struct Display<Interface: PanelInterface, ResetPin: OutputPin> {
    interface: Interface,
    reset_pin: ResetPin,
    top_fixed_rows: u16,
    scrolling_rows: u16,
    scroll_offset: u16,
    in_low_power_mode: bool,
}

impl<Interface, ResetPin> Display<Interface, ResetPin>
where
    Interface: PanelInterface,
    ResetPin: OutputPin,
{
    /// Resets and starts the panel, with the whole of memory as one scrolling area.
    pub fn new(
        interface: Interface,
        mut reset_pin: ResetPin,
        delay: &mut impl DelayNs,
    ) -> Result<Self, StartError<Interface::Error, ResetPin::Error>> {
        reset_pin.set_low().map_err(StartError::ResetPin)?;
        delay.delay_us(RESET_PULSE_IN_MICROSECONDS);
        reset_pin.set_high().map_err(StartError::ResetPin)?;

        let mut display = Display {
            interface,
            reset_pin,
            top_fixed_rows: 0,
            scrolling_rows: MEMORY_ROWS,
            scroll_offset: 0,
            in_low_power_mode: false,
        };
        display.start(delay).map_err(StartError::Interface)?;
        display.set_scroll_area(0, 0).map_err(StartError::Interface)?;
        Ok(display)
    }

    fn start(&mut self, delay: &mut impl DelayNs) -> Result<(), Interface::Error> {
        delay.delay_us(AFTER_RESET_IN_MICROSECONDS);
        self.send_command(EXIT_SLEEP, &[])?;
        delay.delay_us(START_STEP_IN_MICROSECONDS);
        self.send_command(SET_MEMORY_ORDER, &[MEMORY_ORDER])?;
        self.send_command(INVERSION_ON, &[])?;
        self.send_command(SET_PIXEL_FORMAT, &[SIXTEEN_BITS_A_PIXEL])?;
        delay.delay_us(START_STEP_IN_MICROSECONDS);
        self.send_command(NORMAL_MODE_ON, &[])?;
        delay.delay_us(START_STEP_IN_MICROSECONDS);
        self.send_command(DISPLAY_ON, &[])?;
        delay.delay_us(AFTER_DISPLAY_ON_IN_MICROSECONDS);
        Ok(())
    }

    /// The bottom fixed rows count from the end of memory, so they are on screen only beyond 80.
    pub fn set_scroll_area(
        &mut self,
        top_fixed_rows: u16,
        bottom_fixed_rows: u16,
    ) -> Result<(), Interface::Error> {
        assert!(
            top_fixed_rows + bottom_fixed_rows < MEMORY_ROWS,
            "fixed areas leave no rows to scroll"
        );
        let scrolling_rows = MEMORY_ROWS - top_fixed_rows - bottom_fixed_rows;
        let mut arguments = [0u8; 6];
        arguments[..2].copy_from_slice(&top_fixed_rows.to_be_bytes());
        arguments[2..4].copy_from_slice(&scrolling_rows.to_be_bytes());
        arguments[4..].copy_from_slice(&bottom_fixed_rows.to_be_bytes());
        self.send_command(SET_SCROLL_AREA, &arguments)?;
        self.top_fixed_rows = top_fixed_rows;
        self.scrolling_rows = scrolling_rows;

        self.scroll(0)
    }

    /// Moves the scrolling area up, wrapping round its memory.
    pub fn scroll(&mut self, scroll_offset: u16) -> Result<(), Interface::Error> {
        self.scroll_offset = scroll_offset % self.scrolling_rows;
        let start = self.top_fixed_rows + self.scroll_offset;
        self.send_command(SET_SCROLL_START, &start.to_be_bytes())
    }

    pub fn scroll_offset(&self) -> u16 {
        self.scroll_offset
    }

    pub fn scroll_area(&self) -> ScrollArea {
        ScrollArea {
            top_fixed_rows: self.top_fixed_rows,
            scrolling_rows: self.scrolling_rows,
        }
    }

    /// Turns the panel off to save power, keeping its memory; the caller keeps waking `SLEEP_SWITCH_GAP_IN_MILLISECONDS` away.
    pub fn sleep(&mut self, delay: &mut impl DelayNs) -> Result<(), Interface::Error> {
        self.send_command(ENTER_SLEEP, &[])?;
        delay.delay_us(SLEEP_SWITCH_SETTLE_TIME_IN_MICROSECONDS);
        Ok(())
    }

    /// Turns the panel back on after `sleep`; the caller keeps sleeping `SLEEP_SWITCH_GAP_IN_MILLISECONDS` away.
    pub fn wake(&mut self, delay: &mut impl DelayNs) -> Result<(), Interface::Error> {
        self.send_command(EXIT_SLEEP, &[])?;
        delay.delay_us(SLEEP_SWITCH_SETTLE_TIME_IN_MICROSECONDS);
        Ok(())
    }

    /// Shows only 8 colours at a slow refresh, for an always-on screen; memory keeps every colour.
    pub fn enter_low_power_mode(&mut self) -> Result<(), Interface::Error> {
        self.send_command(SET_PORCHES, &LOW_POWER_PORCHES)?;
        self.send_command(SET_IDLE_FRAME_RATE, &LOW_POWER_IDLE_FRAME_RATE)?;
        self.send_command(IDLE_MODE_ON, &[])?;
        self.in_low_power_mode = true;
        Ok(())
    }

    pub fn leave_low_power_mode(&mut self) -> Result<(), Interface::Error> {
        self.send_command(IDLE_MODE_OFF, &[])?;
        self.send_command(SET_IDLE_FRAME_RATE, &DEFAULT_IDLE_FRAME_RATE)?;
        self.send_command(SET_PORCHES, &DEFAULT_PORCHES)?;
        self.in_low_power_mode = false;
        Ok(())
    }

    pub fn is_in_low_power_mode(&self) -> bool {
        self.in_low_power_mode
    }

    /// The interface the panel is driven through.
    pub fn interface_mut(&mut self) -> &mut Interface {
        &mut self.interface
    }

    pub fn release(self) -> (Interface, ResetPin) {
        (self.interface, self.reset_pin)
    }

    fn send_command(&mut self, command: u8, arguments: &[u8]) -> Result<(), Interface::Error> {
        self.interface.send_command(command, arguments)
    }

    /// Points the panel's memory writes at a rectangle of memory, rows `first_row` to `last_row`.
    fn set_window(
        &mut self,
        first_column: u16,
        last_column: u16,
        first_row: u16,
        last_row: u16,
    ) -> Result<(), Interface::Error> {
        let mut columns = [0u8; 4];
        columns[..2].copy_from_slice(&first_column.to_be_bytes());
        columns[2..].copy_from_slice(&last_column.to_be_bytes());
        let mut rows = [0u8; 4];
        rows[..2].copy_from_slice(&first_row.to_be_bytes());
        rows[2..].copy_from_slice(&last_row.to_be_bytes());
        self.send_command(SET_COLUMN_ADDRESS, &columns)?;
        self.send_command(SET_ROW_ADDRESS, &rows)?;
        self.send_command(WRITE_MEMORY, &[])
    }

    /// Calls `send` for each run of memory rows that `area`'s screen rows land on, after pointing the panel there.
    fn for_each_memory_run(
        &mut self,
        area: &Rectangle,
        mut send: impl FnMut(&mut Self, usize, usize) -> Result<(), Interface::Error>,
    ) -> Result<(), Interface::Error> {
        let first_column = area.top_left.x as u16;
        let last_column = first_column + area.size.width as u16 - 1;
        let mut first_pixel = 0;
        for run in self
            .memory_runs(area.top_left.y as u16, area.size.height as u16)
            .iter()
        {
            let pixel_count = area.size.width as usize * run.row_count as usize;
            let last_memory_row = run.first_memory_row + run.row_count - 1;
            self.set_window(first_column, last_column, run.first_memory_row, last_memory_row)?;
            send(self, first_pixel, pixel_count)?;
            first_pixel += pixel_count;
        }
        Ok(())
    }

    fn fill_area<Colors>(&mut self, area: &Rectangle, colors: Colors) -> Result<(), Interface::Error>
    where
        Colors: IntoIterator<Item = Rgb565>,
    {
        const CHUNK_PIXELS: usize = 32;
        let mut colors = colors.into_iter();
        self.for_each_memory_run(area, |display, _, pixel_count| {
            let mut chunk = [0u8; CHUNK_PIXELS * 2];
            let mut left = pixel_count;
            while left > 0 {
                let pixels = left.min(CHUNK_PIXELS);
                for pair in chunk[..pixels * 2].as_chunks_mut::<2>().0 {
                    let color = colors.next().unwrap_or(Rgb565::new(0, 0, 0));
                    *pair = RawU16::from(color).into_inner().to_be_bytes();
                }
                display.interface.send_pixels(&chunk[..pixels * 2])?;
                left -= pixels;
            }
            Ok(())
        })
    }

    fn fill_area_with(&mut self, area: &Rectangle, color: Rgb565) -> Result<(), Interface::Error> {
        let pixel = RawU16::from(color).into_inner().to_be_bytes();
        self.for_each_memory_run(area, |display, _, pixel_count| {
            display.interface.send_repeated_pixel(pixel, pixel_count as u32)
        })
    }

    /// Sends panel pixels to `area`, which must be all on screen; the interface may still be sending on return.
    ///
    /// `pixels` holds the area row by row and must stay untouched until the display is next used.
    pub fn send_strip(&mut self, area: &Rectangle, pixels: &[u16]) -> Result<(), Interface::Error> {
        if area.is_zero_sized() {
            return Ok(());
        }
        self.for_each_memory_run(area, |display, first, count| {
            display.interface.send_strip_pixels(&pixels[first..first + count])
        })
    }
}

// ============================================================================
// Mapping screen rows to memory rows
// ============================================================================

impl<Interface, ResetPin> Display<Interface, ResetPin>
where
    Interface: PanelInterface,
    ResetPin: OutputPin,
{
    fn memory_row_for_screen_row(&self, screen_row: u16) -> u16 {
        let first_scrolling_row = self.top_fixed_rows;
        let end_of_scrolling_rows = first_scrolling_row + self.scrolling_rows;

        if screen_row < first_scrolling_row || screen_row >= end_of_scrolling_rows {
            screen_row
        } else {
            let rows_into_scrolling_area = screen_row - first_scrolling_row + self.scroll_offset;
            first_scrolling_row + rows_into_scrolling_area % self.scrolling_rows
        }
    }

    fn memory_runs(&self, first_screen_row: u16, screen_row_count: u16) -> MemoryRuns {
        let mut runs = MemoryRuns::default();
        let end_of_screen_rows = first_screen_row + screen_row_count;
        let end_of_scrolling_rows = self.top_fixed_rows + self.scrolling_rows;
        let mut screen_row = first_screen_row;

        while screen_row < end_of_screen_rows {
            let memory_row = self.memory_row_for_screen_row(screen_row);

            let end_of_run = if screen_row < self.top_fixed_rows {
                self.top_fixed_rows
            } else if screen_row < end_of_scrolling_rows {
                screen_row + (end_of_scrolling_rows - memory_row)
            } else {
                end_of_screen_rows
            };
            let end_of_run = end_of_run.min(end_of_screen_rows);

            runs.push(MemoryRun {
                first_memory_row: memory_row,
                row_count: end_of_run - screen_row,
            });
            screen_row = end_of_run;
        }

        runs
    }
}

#[derive(Clone, Copy, Default)]
struct MemoryRun {
    first_memory_row: u16,
    row_count: u16,
}

/// At most four runs: the top fixed area, the scrolling area split at the wrap, and the bottom fixed area.
#[derive(Default)]
struct MemoryRuns {
    runs: [MemoryRun; 4],
    run_count: usize,
}

impl MemoryRuns {
    fn push(&mut self, run: MemoryRun) {
        self.runs[self.run_count] = run;
        self.run_count += 1;
    }

    fn iter(&self) -> core::slice::Iter<'_, MemoryRun> {
        self.runs[..self.run_count].iter()
    }
}

// ============================================================================
// Drawing with embedded-graphics
// ============================================================================

impl<Interface, ResetPin> OriginDimensions for Display<Interface, ResetPin>
where
    Interface: PanelInterface,
    ResetPin: OutputPin,
{
    fn size(&self) -> Size {
        Size::new(SCREEN_WIDTH.into(), SCREEN_HEIGHT.into())
    }
}

impl<Interface, ResetPin> DrawTarget for Display<Interface, ResetPin>
where
    Interface: PanelInterface,
    ResetPin: OutputPin,
{
    type Color = Rgb565;
    type Error = Interface::Error;

    fn draw_iter<Pixels>(&mut self, pixels: Pixels) -> Result<(), Self::Error>
    where
        Pixels: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let screen = self.bounding_box();

        for Pixel(point, color) in pixels {
            if screen.contains(point) {
                let memory_row = self.memory_row_for_screen_row(point.y as u16);
                let column = point.x as u16;
                self.set_window(column, column, memory_row, memory_row)?;
                self.interface
                    .send_pixels(&RawU16::from(color).into_inner().to_be_bytes())?;
            }
        }

        Ok(())
    }

    fn fill_contiguous<Colors>(&mut self, area: &Rectangle, colors: Colors) -> Result<(), Self::Error>
    where
        Colors: IntoIterator<Item = Self::Color>,
    {
        if area.is_zero_sized() {
            return Ok(());
        }

        let area_is_all_on_screen = self.bounding_box().intersection(area) == *area;
        if area_is_all_on_screen {
            return self.fill_area(area, colors);
        }

        let pixels = area
            .points()
            .zip(colors)
            .map(|(point, color)| Pixel(point, color));
        self.draw_iter(pixels)
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let visible_area = self.bounding_box().intersection(area);
        if visible_area.is_zero_sized() {
            return Ok(());
        }

        self.fill_area_with(&visible_area, color)
    }
}

// ============================================================================
// Backlight
// ============================================================================

/// The brightness levels the three backlight pins give on their own, darkest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BacklightLevel {
    Off,
    Low,
    Medium,
    High,
}

/// Each pin lights the screen when it is driven low, and a brighter level lights more of them.
pub struct Backlight<BacklightPin: OutputPin> {
    low_pin: BacklightPin,
    medium_pin: BacklightPin,
    high_pin: BacklightPin,
    level: BacklightLevel,
}

impl<BacklightPin: OutputPin> Backlight<BacklightPin> {
    pub fn new(
        low_pin: BacklightPin,
        medium_pin: BacklightPin,
        high_pin: BacklightPin,
        level: BacklightLevel,
    ) -> Result<Self, BacklightPin::Error> {
        let mut backlight = Backlight {
            low_pin,
            medium_pin,
            high_pin,
            level,
        };
        backlight.set_level(level)?;
        Ok(backlight)
    }

    pub fn set_level(&mut self, level: BacklightLevel) -> Result<(), BacklightPin::Error> {
        let lit_pin_count = match level {
            BacklightLevel::Off => 0,
            BacklightLevel::Low => 1,
            BacklightLevel::Medium => 2,
            BacklightLevel::High => 3,
        };

        set_lit(&mut self.low_pin, lit_pin_count >= 1)?;
        set_lit(&mut self.medium_pin, lit_pin_count >= 2)?;
        set_lit(&mut self.high_pin, lit_pin_count >= 3)?;
        self.level = level;
        Ok(())
    }

    pub fn level(&self) -> BacklightLevel {
        self.level
    }

    /// Goes one level up, staying at `High`.
    pub fn brighter(&mut self) -> Result<(), BacklightPin::Error> {
        let level = match self.level {
            BacklightLevel::Off => BacklightLevel::Low,
            BacklightLevel::Low => BacklightLevel::Medium,
            BacklightLevel::Medium | BacklightLevel::High => BacklightLevel::High,
        };
        self.set_level(level)
    }

    /// Goes one level down, staying at `Off`.
    pub fn dimmer(&mut self) -> Result<(), BacklightPin::Error> {
        let level = match self.level {
            BacklightLevel::High => BacklightLevel::Medium,
            BacklightLevel::Medium => BacklightLevel::Low,
            BacklightLevel::Low | BacklightLevel::Off => BacklightLevel::Off,
        };
        self.set_level(level)
    }

    pub fn release(self) -> [BacklightPin; 3] {
        [self.low_pin, self.medium_pin, self.high_pin]
    }
}

fn set_lit<BacklightPin: OutputPin>(pin: &mut BacklightPin, lit: bool) -> Result<(), BacklightPin::Error> {
    if lit {
        pin.set_low()
    } else {
        pin.set_high()
    }
}

#[cfg(test)]
mod tests;
