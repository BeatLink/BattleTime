//! Screens whose panel can move a band of rows by itself, as the ST7789's vertical scrolling does.
//!
//! Moving rows in the panel costs one command, where drawing them again costs every pixel, so
//! scrolling lists and sliding screens use it. The panel keeps the band's rows in a ring: moving
//! the band turns the ring, and drawing still takes screen positions, which the driver maps onto
//! the ring. A new band can only be set while the ring is unturned, or the picture must be drawn
//! again, because the panel's memory stays where it was.

use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle, Pixel};
use embedded_hal::digital::OutputPin;
use neotime_display::{Display, PanelInterface, MEMORY_ROWS, SCREEN_HEIGHT};

// ============================================================================
// Scroll bands
// ============================================================================

/// The screen rows from `first_row` up to `end_row` that the panel moves together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollBand {
    pub first_row: i32,
    pub end_row: i32,
}

impl ScrollBand {
    pub const WHOLE_SCREEN: ScrollBand = ScrollBand {
        first_row: 0,
        end_row: SCREEN_HEIGHT as i32,
    };

    pub fn height(&self) -> i32 {
        self.end_row - self.first_row
    }
}

/// The band that scrolls now, and how many rows its ring is turned, from 0 up to its height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollState {
    pub band: ScrollBand,
    pub turned_rows: i32,
}

/// A screen that may be able to move a band of rows by itself.
pub trait ScrollingScreen: DrawTarget<Color = Rgb565> {
    /// Whether the panel can move rows at all; when it cannot, nothing else here is called.
    fn can_scroll(&self) -> bool {
        true
    }

    fn scroll_state(&self) -> ScrollState;

    /// Makes `band` scroll and every other row stay, with the ring unturned.
    fn set_scroll_band(&mut self, band: ScrollBand) -> Result<(), Self::Error>;

    /// Moves the band's content up by `rows`, or down when it is negative, wrapping round the band.
    fn scroll_by(&mut self, rows: i32) -> Result<(), Self::Error>;

    /// A buffer of panel pixels the screen sends from by itself, to draw the next strip into.
    fn strip_buffer(&mut self) -> Option<&mut [u16]> {
        None
    }

    /// Sends the strip buffer's first pixels to `area`, which must be all on screen.
    fn send_strip(&mut self, _area: &Rectangle) -> Result<(), Self::Error> {
        Ok(())
    }
}

// ============================================================================
// The PineTime display
// ============================================================================

impl<Interface, ResetPin> ScrollingScreen for Display<Interface, ResetPin>
where
    Interface: PanelInterface,
    ResetPin: OutputPin,
{
    fn scroll_state(&self) -> ScrollState {
        let area = self.scroll_area();
        let first_row = i32::from(area.top_fixed_rows);
        ScrollState {
            band: ScrollBand {
                first_row,
                end_row: first_row + i32::from(area.scrolling_rows),
            },
            turned_rows: i32::from(self.scroll_offset()),
        }
    }

    fn set_scroll_band(&mut self, band: ScrollBand) -> Result<(), Self::Error> {
        let bottom_fixed_rows = MEMORY_ROWS as i32 - band.end_row;
        self.set_scroll_area(band.first_row as u16, bottom_fixed_rows as u16)
    }

    fn scroll_by(&mut self, rows: i32) -> Result<(), Self::Error> {
        let state = self.scroll_state();
        let turned_rows = (state.turned_rows + rows).rem_euclid(state.band.height());
        self.scroll(turned_rows as u16)
    }

    fn strip_buffer(&mut self) -> Option<&mut [u16]> {
        Display::strip_buffer(self)
    }

    fn send_strip(&mut self, area: &Rectangle) -> Result<(), Self::Error> {
        Display::send_strip(self, area)
    }
}

// ============================================================================
// Screens that cannot scroll
// ============================================================================

/// Any screen, such as a picture in memory, used where a scrolling screen is wanted.
pub struct FixedScreen<'screen, Screen> {
    screen: &'screen mut Screen,
}

impl<'screen, Screen> FixedScreen<'screen, Screen> {
    pub fn new(screen: &'screen mut Screen) -> Self {
        Self { screen }
    }
}

impl<Screen: DrawTarget<Color = Rgb565>> OriginDimensions for FixedScreen<'_, Screen> {
    fn size(&self) -> Size {
        self.screen.bounding_box().size
    }
}

impl<Screen: DrawTarget<Color = Rgb565>> DrawTarget for FixedScreen<'_, Screen> {
    type Color = Rgb565;
    type Error = Screen::Error;

    fn draw_iter<Pixels>(&mut self, pixels: Pixels) -> Result<(), Self::Error>
    where
        Pixels: IntoIterator<Item = Pixel<Rgb565>>,
    {
        self.screen.draw_iter(pixels)
    }

    fn fill_contiguous<Colors>(&mut self, area: &Rectangle, colors: Colors) -> Result<(), Self::Error>
    where
        Colors: IntoIterator<Item = Rgb565>,
    {
        self.screen.fill_contiguous(area, colors)
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Rgb565) -> Result<(), Self::Error> {
        self.screen.fill_solid(area, color)
    }
}

impl<Screen: DrawTarget<Color = Rgb565>> ScrollingScreen for FixedScreen<'_, Screen> {
    fn can_scroll(&self) -> bool {
        false
    }

    fn scroll_state(&self) -> ScrollState {
        ScrollState {
            band: ScrollBand::WHOLE_SCREEN,
            turned_rows: 0,
        }
    }

    fn set_scroll_band(&mut self, _band: ScrollBand) -> Result<(), Self::Error> {
        Ok(())
    }

    fn scroll_by(&mut self, _rows: i32) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "scrolling_tests.rs"]
mod scrolling_tests;
