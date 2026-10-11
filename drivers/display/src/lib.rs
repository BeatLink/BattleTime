//! Driver for the PineTime's 240x240 ST7789 display, with hardware vertical scrolling.
//!
//! The ST7789 holds 320 rows of memory but the panel shows only 240, and scrolling changes which
//! memory row appears at the top of the screen. This driver takes screen coordinates and writes
//! to whichever memory rows are on screen at that moment, splitting a drawing where the memory wraps.

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

/// Width of the panel in pixels.
pub const WIDTH: u16 = 240;
/// Height of the panel in pixels.
pub const HEIGHT: u16 = 240;
/// Rows of display memory, whatever the panel shows.
pub const MEMORY_ROWS: u16 = 320;

/// The PineTime display.
pub struct Display<DI: Interface<Word = u8>, RST: OutputPin> {
    inner: mipidsi::Display<DI, ST7789, RST>,
    top_fixed: u16,
    scroll_rows: u16,
    offset: u16,
}

impl<DI, RST> Display<DI, RST>
where
    DI: Interface<Word = u8>,
    RST: OutputPin,
{
    /// Resets and starts the display, with the whole of memory as one scrolling area.
    pub fn new(di: DI, rst: RST, delay: &mut impl DelayNs) -> Result<Self, InitError<DI::Error, RST::Error>> {
        let inner = Builder::new(ST7789, di)
            .display_size(WIDTH, HEIGHT)
            .invert_colors(ColorInversion::Inverted)
            .reset_pin(rst)
            .init(delay)?;
        let mut display = Self {
            inner,
            top_fixed: 0,
            scroll_rows: MEMORY_ROWS,
            offset: 0,
        };
        display.set_scroll_area(0, 0).map_err(InitError::Interface)?;
        Ok(display)
    }

    /// Splits memory into a fixed top, a scrolling middle and a fixed bottom, and resets the scroll.
    ///
    /// The bottom area counts from the end of memory, so it is on screen only beyond 80 rows.
    pub fn set_scroll_area(&mut self, top_fixed: u16, bottom_fixed: u16) -> Result<(), DI::Error> {
        assert!(
            top_fixed + bottom_fixed < MEMORY_ROWS,
            "fixed areas leave no rows to scroll"
        );
        self.inner.set_vertical_scroll_region(top_fixed, bottom_fixed)?;
        self.top_fixed = top_fixed;
        self.scroll_rows = MEMORY_ROWS - top_fixed - bottom_fixed;
        self.scroll(0)
    }

    /// Moves the scrolling area up by `offset` rows, wrapping round its memory.
    pub fn scroll(&mut self, offset: u16) -> Result<(), DI::Error> {
        self.offset = offset % self.scroll_rows;
        self.inner
            .set_vertical_scroll_offset(self.top_fixed + self.offset)
    }

    /// How far the scrolling area is moved up, in rows.
    pub fn scroll_offset(&self) -> u16 {
        self.offset
    }

    /// Turns the panel off to save power; memory is kept.
    pub fn sleep(&mut self, delay: &mut impl DelayNs) -> Result<(), DI::Error> {
        self.inner.sleep(delay)
    }

    /// Turns the panel back on after [`sleep`](Self::sleep).
    pub fn wake(&mut self, delay: &mut impl DelayNs) -> Result<(), DI::Error> {
        self.inner.wake(delay)
    }

    /// Gives back the interface and the reset pin.
    pub fn release(self) -> (DI, Option<RST>) {
        let (di, _, rst) = self.inner.release();
        (di, rst)
    }

    /// The memory row shown on screen row `y`.
    fn memory_row(&self, y: u16) -> u16 {
        let top = self.top_fixed;
        if y < top || y >= top + self.scroll_rows {
            y
        } else {
            top + (y - top + self.offset) % self.scroll_rows
        }
    }

    /// Splits screen rows `y..y + rows` into runs that are contiguous in memory.
    fn memory_runs(&self, mut y: u16, rows: u16) -> Runs {
        let mut runs = Runs::default();
        let end = y + rows;
        let scroll_end = self.top_fixed + self.scroll_rows;
        while y < end {
            let memory = self.memory_row(y);
            let mut stop = if y < self.top_fixed {
                self.top_fixed
            } else if y < scroll_end {
                y + (scroll_end - memory)
            } else {
                end
            };
            stop = stop.min(end);
            runs.push(memory, stop - y);
            y = stop;
        }
        runs
    }

    fn fill_runs<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), DI::Error>
    where
        I: IntoIterator<Item = Rgb565>,
    {
        let mut colors = colors.into_iter();
        let x = area.top_left.x as u16;
        let width = area.size.width as u16;
        for &(memory, rows) in self
            .memory_runs(area.top_left.y as u16, area.size.height as u16)
            .iter()
        {
            let pixels = colors.by_ref().take(usize::from(width) * usize::from(rows));
            self.inner
                .set_pixels(x, memory, x + width - 1, memory + rows - 1, pixels)?;
        }
        Ok(())
    }
}

/// Up to four memory runs, each a start row and a row count.
#[derive(Default)]
struct Runs {
    runs: [(u16, u16); 4],
    len: usize,
}

impl Runs {
    fn push(&mut self, memory: u16, rows: u16) {
        self.runs[self.len] = (memory, rows);
        self.len += 1;
    }

    fn iter(&self) -> core::slice::Iter<'_, (u16, u16)> {
        self.runs[..self.len].iter()
    }
}

impl<DI, RST> OriginDimensions for Display<DI, RST>
where
    DI: Interface<Word = u8>,
    RST: OutputPin,
{
    fn size(&self) -> Size {
        Size::new(WIDTH.into(), HEIGHT.into())
    }
}

impl<DI, RST> DrawTarget for Display<DI, RST>
where
    DI: Interface<Word = u8>,
    RST: OutputPin,
{
    type Color = Rgb565;
    type Error = DI::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let bounds = self.bounding_box();
        for Pixel(point, color) in pixels {
            if bounds.contains(point) {
                let memory = self.memory_row(point.y as u16);
                self.inner.set_pixel(point.x as u16, memory, color)?;
            }
        }
        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        if area.is_zero_sized() {
            return Ok(());
        }
        if self.bounding_box().intersection(area) == *area {
            return self.fill_runs(area, colors);
        }
        let pixels = area
            .points()
            .zip(colors)
            .map(|(point, color)| Pixel(point, color));
        self.draw_iter(pixels)
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let area = self.bounding_box().intersection(area);
        if area.is_zero_sized() {
            return Ok(());
        }
        self.fill_runs(&area, core::iter::repeat(color))
    }
}

#[cfg(test)]
mod tests;
