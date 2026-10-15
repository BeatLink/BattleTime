//! The screen the app host draws on: the display, locked for each command, and two strip buffers.
//!
//! The buffers live outside the display's lock, so drawing never holds the display while it waits
//! for a strip to go out, and other tasks, such as the status page's, can use it meanwhile.

use battletime_ui::{ScrollBand, ScrollState, ScrollingScreen};
use embassy_nrf::spim;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle, Pixel};

use crate::{display_dma, SharedDisplay};

// ============================================================================
// Strip buffers
// ============================================================================

/// The pixels a strip buffer holds: four rows of the screen, as the app host draws them.
pub const STRIP_BUFFER_PIXELS: usize = 960;

pub type StripBuffers = [[u16; STRIP_BUFFER_PIXELS]; 2];

// ============================================================================
// The screen
// ============================================================================

pub struct DrawingScreen {
    display: &'static SharedDisplay,
    buffers: &'static mut StripBuffers,
    /// The buffer to draw the next strip into; the other may still be going out.
    filling: usize,
}

impl DrawingScreen {
    pub fn new(display: &'static SharedDisplay, buffers: &'static mut StripBuffers) -> DrawingScreen {
        DrawingScreen {
            display,
            buffers,
            filling: 0,
        }
    }
}

impl OriginDimensions for DrawingScreen {
    fn size(&self) -> Size {
        self.display.lock(|display| display.borrow().size())
    }
}

impl DrawTarget for DrawingScreen {
    type Color = Rgb565;
    type Error = spim::Error;

    fn draw_iter<Pixels>(&mut self, pixels: Pixels) -> Result<(), Self::Error>
    where
        Pixels: IntoIterator<Item = Pixel<Rgb565>>,
    {
        self.display
            .lock(|display| display.borrow_mut().draw_iter(pixels))
    }

    fn fill_contiguous<Colors>(&mut self, area: &Rectangle, colors: Colors) -> Result<(), Self::Error>
    where
        Colors: IntoIterator<Item = Rgb565>,
    {
        self.display
            .lock(|display| display.borrow_mut().fill_contiguous(area, colors))
    }
}

impl ScrollingScreen for DrawingScreen {
    fn scroll_state(&self) -> ScrollState {
        self.display.lock(|display| display.borrow().scroll_state())
    }

    fn set_scroll_band(&mut self, band: ScrollBand) -> Result<(), Self::Error> {
        self.display
            .lock(|display| display.borrow_mut().set_scroll_band(band))
    }

    fn scroll_by(&mut self, rows: i32) -> Result<(), Self::Error> {
        self.display.lock(|display| display.borrow_mut().scroll_by(rows))
    }

    fn strip_buffer(&mut self) -> Option<&mut [u16]> {
        Some(&mut self.buffers[self.filling])
    }

    /// Starts the strip and draws the next into the other buffer, which went out before this one started.
    fn send_strip(&mut self, area: &Rectangle) -> Result<(), Self::Error> {
        let pixel_count = (area.size.width * area.size.height) as usize;
        let pixels = &self.buffers[self.filling][..pixel_count];
        self.display
            .lock(|display| display.borrow_mut().send_strip(area, pixels))?;
        self.filling = 1 - self.filling;
        Ok(())
    }

    async fn ready(&mut self) {
        display_dma::bus_is_free().await;
    }
}
