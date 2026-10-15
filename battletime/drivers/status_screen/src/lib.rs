//! The status page every part of BattleTime shows, and a small ST7789 driver to show it from the bootloader.
//!
//! `page` sets out the page: which line says what, its labels, font and colours. The firmware, the
//! recovery firmware, the bootloader and the bootloader updater all draw through it, so the page
//! looks the same whichever of them is running. The firmware's DMA display driver is too big for the
//! bootloader's 20 KB, so `Screen` sends the few commands a status page needs and nothing else.

#![no_std]

use embedded_graphics::{
    pixelcolor::{raw::RawU16, Rgb565},
    prelude::*,
    primitives::Rectangle,
};
use embedded_hal::{delay::DelayNs, digital::OutputPin, spi::SpiDevice};

pub mod page;

// ============================================================================
// Panel commands
// ============================================================================

use page::SCREEN_SIZE;
const LEAVE_SLEEP: u8 = 0x11;
const NORMAL_MODE: u8 = 0x13;
const INVERT_COLOURS: u8 = 0x21;
const DISPLAY_ON: u8 = 0x29;
const COLUMN_RANGE: u8 = 0x2A;
const ROW_RANGE: u8 = 0x2B;
const WRITE_MEMORY: u8 = 0x2C;
const MEMORY_ORDER: u8 = 0x36;
const PIXEL_FORMAT: u8 = 0x3A;
const SIXTEEN_BITS_PER_PIXEL: u8 = 0x55;

/// Pixels sent per SPI write, kept in RAM since the SPI driver cannot send more than 255 bytes from flash.
const PIXELS_PER_WRITE: usize = 64;

// ============================================================================
// Screen
// ============================================================================

pub struct Screen<Spi, DataOrCommand> {
    spi: Spi,
    data_or_command: DataOrCommand,
    background: Rgb565,
}

impl<Spi: SpiDevice, DataOrCommand: OutputPin> Screen<Spi, DataOrCommand> {
    /// Resets the panel, wakes it and fills it with the background colour.
    pub fn start(
        spi: Spi,
        data_or_command: DataOrCommand,
        mut reset: impl OutputPin,
        background: Rgb565,
        delay: &mut impl DelayNs,
    ) -> Option<Self> {
        let _ = reset.set_low();
        delay.delay_ms(10);
        let _ = reset.set_high();
        delay.delay_ms(120);

        let mut screen = Screen {
            spi,
            data_or_command,
            background,
        };
        screen.command(LEAVE_SLEEP, &[]).ok()?;
        delay.delay_ms(10);
        screen.command(PIXEL_FORMAT, &[SIXTEEN_BITS_PER_PIXEL]).ok()?;
        screen.command(MEMORY_ORDER, &[0]).ok()?;
        screen.command(INVERT_COLOURS, &[]).ok()?;
        screen.command(NORMAL_MODE, &[]).ok()?;
        screen.fill_background().ok()?;
        screen.command(DISPLAY_ON, &[]).ok()?;
        Some(screen)
    }

    /// Shows a line of the page, its label included, covering whatever the line said before.
    pub fn write_line(&mut self, line: usize, text: &str) {
        let background = self.background;
        let _ = page::draw_line(self, line, text, background);
    }

    fn fill_background(&mut self) -> Result<(), Spi::Error> {
        let whole_screen = Rectangle::new(Point::zero(), Size::new(SCREEN_SIZE, SCREEN_SIZE));
        let background = self.background;
        self.fill_area(&whole_screen, core::iter::repeat(background))
    }

    fn command(&mut self, command: u8, parameters: &[u8]) -> Result<(), Spi::Error> {
        let _ = self.data_or_command.set_low();
        self.spi.write(&[command])?;
        let _ = self.data_or_command.set_high();
        if !parameters.is_empty() {
            self.spi.write(parameters)?;
        }
        Ok(())
    }

    /// Sends the colours for every pixel of an on-screen area, row by row.
    fn fill_area(
        &mut self,
        area: &Rectangle,
        colours: impl IntoIterator<Item = Rgb565>,
    ) -> Result<(), Spi::Error> {
        let Some(bottom_right) = area.bottom_right() else {
            return Ok(());
        };
        let [left, top, right, bottom] =
            [area.top_left.x, area.top_left.y, bottom_right.x, bottom_right.y].map(|value| value as u16);
        self.command(COLUMN_RANGE, &range_bytes(left, right))?;
        self.command(ROW_RANGE, &range_bytes(top, bottom))?;
        self.command(WRITE_MEMORY, &[])?;

        let mut bytes = [0; PIXELS_PER_WRITE * 2];
        let mut count = 0;
        for colour in colours
            .into_iter()
            .take(area.size.width as usize * area.size.height as usize)
        {
            let raw = RawU16::from(colour).into_inner().to_be_bytes();
            bytes[count * 2..count * 2 + 2].copy_from_slice(&raw);
            count += 1;
            if count == PIXELS_PER_WRITE {
                self.spi.write(&bytes)?;
                count = 0;
            }
        }
        if count > 0 {
            self.spi.write(&bytes[..count * 2])?;
        }
        Ok(())
    }
}

fn range_bytes(start: u16, end: u16) -> [u8; 4] {
    let [start_high, start_low] = start.to_be_bytes();
    let [end_high, end_low] = end.to_be_bytes();
    [start_high, start_low, end_high, end_low]
}

// ============================================================================
// Drawing
// ============================================================================

impl<Spi, DataOrCommand> OriginDimensions for Screen<Spi, DataOrCommand> {
    fn size(&self) -> Size {
        Size::new(SCREEN_SIZE, SCREEN_SIZE)
    }
}

impl<Spi: SpiDevice, DataOrCommand: OutputPin> DrawTarget for Screen<Spi, DataOrCommand> {
    type Color = Rgb565;
    type Error = Spi::Error;

    fn draw_iter<Pixels>(&mut self, pixels: Pixels) -> Result<(), Self::Error>
    where
        Pixels: IntoIterator<Item = Pixel<Rgb565>>,
    {
        for Pixel(point, colour) in pixels {
            if self.bounding_box().contains(point) {
                self.fill_area(&Rectangle::new(point, Size::new(1, 1)), [colour])?;
            }
        }
        Ok(())
    }

    fn fill_contiguous<Colours>(&mut self, area: &Rectangle, colours: Colours) -> Result<(), Self::Error>
    where
        Colours: IntoIterator<Item = Rgb565>,
    {
        if self.bounding_box().intersection(area) != *area {
            return self.draw_iter(
                area.points()
                    .zip(colours)
                    .map(|(point, colour)| Pixel(point, colour)),
            );
        }
        self.fill_area(area, colours)
    }
}
