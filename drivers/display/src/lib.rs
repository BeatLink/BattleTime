//! Driver for the PineTime's 240x240 ST7789 display, with hardware vertical scrolling.
//!
//! The ST7789 holds 320 rows of memory but the panel shows only 240, and scrolling changes which
//! memory row appears at the top of the screen. Drawing takes screen positions and lands on whichever
//! memory rows are on screen, split in two where the memory wraps.
//!
//! The panel also has a low-power mode for an always-on screen, and `Backlight` drives the three
//! backlight pins that light it.

#![no_std]

use embedded_graphics_core::{
    draw_target::DrawTarget,
    geometry::{Dimensions, OriginDimensions, Size},
    pixelcolor::Rgb565,
    prelude::PointsIter,
    primitives::Rectangle,
    Pixel,
};
use embedded_hal::{delay::DelayNs, digital::OutputPin};
use mipidsi::{interface::Interface, models::ST7789, options::ColorInversion, Builder, InitError};

// ============================================================================
// Panel size
// ============================================================================

pub const SCREEN_WIDTH: u16 = 240;
pub const SCREEN_HEIGHT: u16 = 240;
pub const MEMORY_ROWS: u16 = 320;

// ============================================================================
// Panel commands
// ============================================================================

const IDLE_MODE_OFF: u8 = 0x38;
const IDLE_MODE_ON: u8 = 0x39;
const SET_PORCHES: u8 = 0xB2;
const SET_IDLE_FRAME_RATE: u8 = 0xB3;

/// Normal porches as the panel starts, with long porches in idle mode to slow its refresh.
const LOW_POWER_PORCHES: [u8; 5] = [0x0C, 0x0C, 0x01, 0xED, 0xED];
const DEFAULT_PORCHES: [u8; 5] = [0x0C, 0x0C, 0x00, 0x33, 0x33];

/// Divides the idle frame rate by four.
const LOW_POWER_IDLE_FRAME_RATE: [u8; 3] = [0x12, 0x1E, 0x1E];
const DEFAULT_IDLE_FRAME_RATE: [u8; 3] = [0x00, 0x0F, 0x0F];

// ============================================================================
// Display
// ============================================================================

/// Which memory rows scroll: those after the top fixed rows, `scrolling_rows` of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollArea {
    pub top_fixed_rows: u16,
    pub scrolling_rows: u16,
}

pub struct Display<DisplayInterface: Interface<Word = u8>, ResetPin: OutputPin> {
    panel: mipidsi::Display<DisplayInterface, ST7789, ResetPin>,
    top_fixed_rows: u16,
    scrolling_rows: u16,
    scroll_offset: u16,
    in_low_power_mode: bool,
}

impl<DisplayInterface, ResetPin> Display<DisplayInterface, ResetPin>
where
    DisplayInterface: Interface<Word = u8>,
    ResetPin: OutputPin,
{
    /// Resets and starts the panel, with the whole of memory as one scrolling area.
    pub fn new(
        display_interface: DisplayInterface,
        reset_pin: ResetPin,
        delay: &mut impl DelayNs,
    ) -> Result<Self, InitError<DisplayInterface::Error, ResetPin::Error>> {
        let panel = Builder::new(ST7789, display_interface)
            .display_size(SCREEN_WIDTH, SCREEN_HEIGHT)
            .invert_colors(ColorInversion::Inverted)
            .reset_pin(reset_pin)
            .init(delay)?;

        let mut display = Display {
            panel,
            top_fixed_rows: 0,
            scrolling_rows: MEMORY_ROWS,
            scroll_offset: 0,
            in_low_power_mode: false,
        };
        display.set_scroll_area(0, 0).map_err(InitError::Interface)?;

        Ok(display)
    }

    /// The bottom fixed rows count from the end of memory, so they are on screen only beyond 80.
    pub fn set_scroll_area(
        &mut self,
        top_fixed_rows: u16,
        bottom_fixed_rows: u16,
    ) -> Result<(), DisplayInterface::Error> {
        assert!(
            top_fixed_rows + bottom_fixed_rows < MEMORY_ROWS,
            "fixed areas leave no rows to scroll"
        );

        self.panel
            .set_vertical_scroll_region(top_fixed_rows, bottom_fixed_rows)?;
        self.top_fixed_rows = top_fixed_rows;
        self.scrolling_rows = MEMORY_ROWS - top_fixed_rows - bottom_fixed_rows;

        self.scroll(0)
    }

    /// Moves the scrolling area up, wrapping round its memory.
    pub fn scroll(&mut self, scroll_offset: u16) -> Result<(), DisplayInterface::Error> {
        self.scroll_offset = scroll_offset % self.scrolling_rows;
        self.panel
            .set_vertical_scroll_offset(self.top_fixed_rows + self.scroll_offset)
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

    /// Turns the panel off to save power; its memory is kept.
    pub fn sleep(&mut self, delay: &mut impl DelayNs) -> Result<(), DisplayInterface::Error> {
        self.panel.sleep(delay)
    }

    pub fn wake(&mut self, delay: &mut impl DelayNs) -> Result<(), DisplayInterface::Error> {
        self.panel.wake(delay)
    }

    /// Shows only 8 colours at a slow refresh, for an always-on screen; memory keeps every colour.
    pub fn enter_low_power_mode(&mut self) -> Result<(), DisplayInterface::Error> {
        self.send_command(SET_PORCHES, &LOW_POWER_PORCHES)?;
        self.send_command(SET_IDLE_FRAME_RATE, &LOW_POWER_IDLE_FRAME_RATE)?;
        self.send_command(IDLE_MODE_ON, &[])?;
        self.in_low_power_mode = true;
        Ok(())
    }

    pub fn leave_low_power_mode(&mut self) -> Result<(), DisplayInterface::Error> {
        self.send_command(IDLE_MODE_OFF, &[])?;
        self.send_command(SET_IDLE_FRAME_RATE, &DEFAULT_IDLE_FRAME_RATE)?;
        self.send_command(SET_PORCHES, &DEFAULT_PORCHES)?;
        self.in_low_power_mode = false;
        Ok(())
    }

    pub fn is_in_low_power_mode(&self) -> bool {
        self.in_low_power_mode
    }

    pub fn release(self) -> (DisplayInterface, Option<ResetPin>) {
        let (display_interface, _model, reset_pin) = self.panel.release();
        (display_interface, reset_pin)
    }

    fn send_command(&mut self, code: u8, arguments: &[u8]) -> Result<(), DisplayInterface::Error> {
        // SAFETY: these commands change only how the panel shows its memory, which mipidsi does not track.
        unsafe { self.panel.dcs() }.send_command(code, arguments)
    }

    fn fill_area<Colors>(&mut self, area: &Rectangle, colors: Colors) -> Result<(), DisplayInterface::Error>
    where
        Colors: IntoIterator<Item = Rgb565>,
    {
        let mut colors = colors.into_iter();
        let first_column = area.top_left.x as u16;
        let last_column = first_column + area.size.width as u16 - 1;
        let first_screen_row = area.top_left.y as u16;
        let screen_row_count = area.size.height as u16;

        for run in self.memory_runs(first_screen_row, screen_row_count).iter() {
            let pixel_count = area.size.width as usize * run.row_count as usize;
            let last_memory_row = run.first_memory_row + run.row_count - 1;
            let run_colors = colors.by_ref().take(pixel_count);

            self.panel.set_pixels(
                first_column,
                run.first_memory_row,
                last_column,
                last_memory_row,
                run_colors,
            )?;
        }

        Ok(())
    }
}

// ============================================================================
// Mapping screen rows to memory rows
// ============================================================================

impl<DisplayInterface, ResetPin> Display<DisplayInterface, ResetPin>
where
    DisplayInterface: Interface<Word = u8>,
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

impl<DisplayInterface, ResetPin> OriginDimensions for Display<DisplayInterface, ResetPin>
where
    DisplayInterface: Interface<Word = u8>,
    ResetPin: OutputPin,
{
    fn size(&self) -> Size {
        Size::new(SCREEN_WIDTH.into(), SCREEN_HEIGHT.into())
    }
}

impl<DisplayInterface, ResetPin> DrawTarget for Display<DisplayInterface, ResetPin>
where
    DisplayInterface: Interface<Word = u8>,
    ResetPin: OutputPin,
{
    type Color = Rgb565;
    type Error = DisplayInterface::Error;

    fn draw_iter<Pixels>(&mut self, pixels: Pixels) -> Result<(), Self::Error>
    where
        Pixels: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let screen = self.bounding_box();

        for Pixel(point, color) in pixels {
            if screen.contains(point) {
                let memory_row = self.memory_row_for_screen_row(point.y as u16);
                self.panel.set_pixel(point.x as u16, memory_row, color)?;
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

        self.fill_area(&visible_area, core::iter::repeat(color))
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
