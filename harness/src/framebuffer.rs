//! A 240x240 RGB565 screen in memory, which the app host draws into as it would the display.

use std::convert::Infallible;

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;

// ============================================================================
// The framebuffer
// ============================================================================

pub const SCREEN_SIDE_IN_PIXELS: usize = 240;

/// The pixels the host has drawn, kept between draws as the panel keeps them.
#[derive(Clone, PartialEq, Eq)]
pub struct Framebuffer {
    pixels: Vec<Rgb565>,
    pixels_drawn_off_screen: usize,
}

impl Framebuffer {
    pub fn new() -> Self {
        Framebuffer {
            pixels: vec![Rgb565::BLACK; SCREEN_SIDE_IN_PIXELS * SCREEN_SIDE_IN_PIXELS],
            pixels_drawn_off_screen: 0,
        }
    }

    /// Every pixel, row by row from the top left.
    pub fn pixels(&self) -> &[Rgb565] {
        &self.pixels
    }

    pub fn pixel(&self, x: usize, y: usize) -> Rgb565 {
        self.pixels[y * SCREEN_SIDE_IN_PIXELS + x]
    }

    pub fn count_of(&self, color: Rgb565) -> usize {
        self.pixels.iter().filter(|&&pixel| pixel == color).count()
    }

    /// How many pixels differ from another frame, to compare two ways of drawing the same screen.
    pub fn differing_pixels(&self, other: &Framebuffer) -> usize {
        self.pixels
            .iter()
            .zip(&other.pixels)
            .filter(|(mine, theirs)| mine != theirs)
            .count()
    }

    /// Pixels the host asked to draw outside the screen, which the panel would drop.
    pub fn pixels_drawn_off_screen(&self) -> usize {
        self.pixels_drawn_off_screen
    }
}

impl Default for Framebuffer {
    fn default() -> Self {
        Framebuffer::new()
    }
}

// ============================================================================
// Drawing into it
// ============================================================================

impl OriginDimensions for Framebuffer {
    fn size(&self) -> Size {
        Size::new(SCREEN_SIDE_IN_PIXELS as u32, SCREEN_SIDE_IN_PIXELS as u32)
    }
}

impl DrawTarget for Framebuffer {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<Pixels>(&mut self, pixels: Pixels) -> Result<(), Infallible>
    where
        Pixels: IntoIterator<Item = Pixel<Rgb565>>,
    {
        let side = SCREEN_SIDE_IN_PIXELS as i32;
        for Pixel(point, color) in pixels {
            if point.x < 0 || point.y < 0 || point.x >= side || point.y >= side {
                self.pixels_drawn_off_screen += 1;
                continue;
            }
            self.pixels[point.y as usize * SCREEN_SIDE_IN_PIXELS + point.x as usize] = color;
        }
        Ok(())
    }
}
