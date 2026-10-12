//! A small ST7789 driver for status screens: a coloured background and lines of text.
//!
//! The firmware's display driver is built on mipidsi, which does not fit in the bootloader's 16 KB, so
//! this one sends the few commands a status screen needs and nothing else. The bootloader and the
//! bootloader updater both use it. Text uses the small 6x10 font drawn at double size, which looks
//! like the firmware's 12x20 text for a third of the flash.

#![no_std]

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::{raw::RawU16, Rgb565},
    prelude::*,
    primitives::Rectangle,
    text::Text,
};
use embedded_hal::{delay::DelayNs, digital::OutputPin, spi::SpiDevice};

// ============================================================================
// Panel commands
// ============================================================================

const SCREEN_SIZE: u32 = 240;
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

/// The 6x10 font is 12x20 on screen, so 19 characters fit between the margins.
pub const LINE_CHARACTERS: usize = 19;
/// Each font pixel becomes a square this many screen pixels wide.
const TEXT_SCALE: u32 = 2;
/// The most font pixels one glyph call hands over: a 6x10 character.
const LARGEST_GLYPH_PIXELS: usize = 60;

// ============================================================================
// Screen
// ============================================================================

pub struct Screen<Spi, DataOrCommand> {
    spi: Spi,
    data_or_command: DataOrCommand,
    background: Rgb565,
}

impl<Spi: SpiDevice, DataOrCommand: OutputPin> Screen<Spi, DataOrCommand> {
    const LINE_HEIGHT: i32 = 26;
    const FIRST_BASELINE: i32 = 50;
    const LEFT_MARGIN: i32 = 12;

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

    /// Draws up to `LINE_CHARACTERS` of the text, then fills the rest of the line, so it covers whatever the line said before.
    pub fn write_line(&mut self, line: i32, text: &str) {
        let style = MonoTextStyleBuilder::new()
            .font(&FONT_6X10)
            .text_color(Rgb565::WHITE)
            .background_color(self.background)
            .build();
        let shown = text
            .char_indices()
            .nth(LINE_CHARACTERS)
            .map_or(text, |(end, _)| &text[..end]);
        let baseline = (Self::FIRST_BASELINE + line * Self::LINE_HEIGHT) / TEXT_SCALE as i32;
        let position = Point::new(Self::LEFT_MARGIN / TEXT_SCALE as i32, baseline);
        let Ok(text_end) = Text::new(shown, position, style).draw(&mut DoubleSize { screen: self }) else {
            return;
        };

        let glyph_top = baseline - FONT_6X10.baseline as i32;
        let rest_of_line = Rectangle::new(
            Point::new(text_end.x, glyph_top) * TEXT_SCALE as i32,
            Size::new(
                SCREEN_SIZE.saturating_sub(text_end.x as u32 * TEXT_SCALE),
                FONT_6X10.character_size.height * TEXT_SCALE,
            ),
        );
        let background = self.background;
        let _ = self.fill_area(&rest_of_line, core::iter::repeat(background));
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

// ============================================================================
// Double-size text
// ============================================================================

/// Draws everything twice as wide and twice as tall, for the 6x10 font.
struct DoubleSize<'screen, Spi, DataOrCommand> {
    screen: &'screen mut Screen<Spi, DataOrCommand>,
}

impl<Spi, DataOrCommand> OriginDimensions for DoubleSize<'_, Spi, DataOrCommand> {
    fn size(&self) -> Size {
        Size::new(SCREEN_SIZE / TEXT_SCALE, SCREEN_SIZE / TEXT_SCALE)
    }
}

impl<Spi: SpiDevice, DataOrCommand: OutputPin> DrawTarget for DoubleSize<'_, Spi, DataOrCommand> {
    type Color = Rgb565;
    type Error = Spi::Error;

    fn draw_iter<Pixels>(&mut self, pixels: Pixels) -> Result<(), Self::Error>
    where
        Pixels: IntoIterator<Item = Pixel<Rgb565>>,
    {
        for Pixel(point, colour) in pixels {
            let square = Rectangle::new(point * TEXT_SCALE as i32, Size::new(TEXT_SCALE, TEXT_SCALE));
            self.screen.fill_contiguous(&square, core::iter::repeat(colour))?;
        }
        Ok(())
    }

    /// Repeats each font pixel across its square: every pixel twice along a row, and every row twice.
    fn fill_contiguous<Colours>(&mut self, area: &Rectangle, colours: Colours) -> Result<(), Self::Error>
    where
        Colours: IntoIterator<Item = Rgb565>,
    {
        let width = area.size.width as usize;
        let pixel_count = width * area.size.height as usize;
        if pixel_count > LARGEST_GLYPH_PIXELS || width == 0 {
            return self.draw_iter(
                area.points()
                    .zip(colours)
                    .map(|(point, colour)| Pixel(point, colour)),
            );
        }
        let mut glyph = [Rgb565::BLACK; LARGEST_GLYPH_PIXELS];
        for (slot, colour) in glyph.iter_mut().zip(colours) {
            *slot = colour;
        }
        let scale = TEXT_SCALE as usize;
        let scaled = Rectangle::new(area.top_left * TEXT_SCALE as i32, area.size * TEXT_SCALE);
        let scaled_colours = (0..pixel_count * scale * scale).map(|index| {
            let scaled_row = index / (width * scale);
            let scaled_column = index % (width * scale);
            glyph[(scaled_row / scale) * width + scaled_column / scale]
        });
        self.screen.fill_contiguous(&scaled, scaled_colours)
    }
}
