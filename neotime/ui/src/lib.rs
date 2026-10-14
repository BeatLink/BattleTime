//! Draws widgets onto any embedded-graphics screen, such as the PineTime display.
//!
//! The watch has no room for a whole-screen picture in memory, so every drawing call goes straight
//! to the screen. The screen cannot be read back either, so see-through colours are mixed with a
//! known background colour rather than with whatever is already on screen.

#![no_std]

pub mod geometry;
pub mod raster;
pub mod renderer;
mod scrolling;
pub mod shapes;
mod strip;
mod strip_renderer;

pub use scrolling::{BufferedScreen, FixedScreen, ScrollBand, ScrollState, ScrollingScreen};
pub use strip::Strip;
pub use strip_renderer::StripRenderer;

use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoFont, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::Rectangle,
    text::{Baseline, Text},
};
use geometry::{Color, Rect};
use renderer::Renderer;

// ============================================================================
// Screen renderer
// ============================================================================

const OPAQUE: u8 = 255;
const INVISIBLE: u8 = 0;

/// A renderer that draws onto a screen.
pub struct ScreenRenderer<'screen, Screen>
where
    Screen: DrawTarget<Color = Rgb565>,
{
    screen: &'screen mut Screen,
    background_color: Color,
    font: &'static MonoFont<'static>,
    first_error: Option<Screen::Error>,
}

impl<'screen, Screen> ScreenRenderer<'screen, Screen>
where
    Screen: DrawTarget<Color = Rgb565>,
{
    /// Draws onto `screen`, mixing see-through colours with black.
    pub fn new(screen: &'screen mut Screen) -> Self {
        Self {
            screen,
            background_color: Color(0, 0, 0, OPAQUE),
            font: &FONT_10X20,
            first_error: None,
        }
    }

    /// Mixes see-through colours with `background_color` instead of black.
    pub fn with_background_color(mut self, background_color: Color) -> Self {
        self.background_color = background_color;
        self
    }

    /// Draws plain text in `font` instead of the 10x20 font.
    pub fn with_font(mut self, font: &'static MonoFont<'static>) -> Self {
        self.font = font;
        self
    }

    /// Ends drawing and reports the first error the screen gave, if any.
    pub fn finish(self) -> Result<(), Screen::Error> {
        match self.first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn remember_error(&mut self, result: Result<(), Screen::Error>) {
        if let Err(error) = result {
            self.first_error.get_or_insert(error);
        }
    }

    /// Draws one row, leaving pixels with no colour untouched.
    fn fill_row_runs(&mut self, x: i32, y: i32, length: usize, color_at: impl Fn(usize) -> Option<Rgb565>) {
        let mut run_start = 0;
        while run_start < length {
            if color_at(run_start).is_none() {
                run_start += 1;
                continue;
            }

            let run_end = (run_start..length)
                .find(|&index| color_at(index).is_none())
                .unwrap_or(length);
            let run = Rectangle::new(
                Point::new(x + run_start as i32, y),
                Size::new((run_end - run_start) as u32, 1),
            );
            let colors = (run_start..run_end).filter_map(&color_at);
            let result = self.screen.fill_contiguous(&run, colors);
            self.remember_error(result);

            run_start = run_end;
        }
    }
}

// ============================================================================
// Drawing calls
// ============================================================================

impl<Screen> Renderer for ScreenRenderer<'_, Screen>
where
    Screen: DrawTarget<Color = Rgb565>,
{
    fn fill_rect(&mut self, rect: Rect, color: Color) {
        let Some(area) = to_rectangle(rect) else {
            return;
        };

        let result = self.screen.fill_solid(&area, to_rgb565(color));
        self.remember_error(result);
    }

    fn blend_rect(&mut self, rect: Rect, color: Color) {
        let Some(area) = to_rectangle(rect) else {
            return;
        };
        let Some(mixed_color) = mix(color, self.background_color) else {
            return;
        };

        let result = self.screen.fill_solid(&area, mixed_color);
        self.remember_error(result);
    }

    fn blend_row(&mut self, x: i32, y: i32, color: Color, coverage: &[u8]) {
        let background_color = self.background_color;

        self.fill_row_runs(x, y, coverage.len(), |index| {
            mix(with_coverage(color, coverage[index]), background_color)
        });
    }

    fn draw_pixels(&mut self, position: (i32, i32), pixels: &[Color], width: u32, height: u32) {
        let background_color = self.background_color;
        let width = width as usize;

        for (row_index, row) in pixels.chunks(width).take(height as usize).enumerate() {
            let y = position.1 + row_index as i32;
            self.fill_row_runs(position.0, y, row.len(), |index| {
                mix(row[index], background_color)
            });
        }
    }

    fn draw_text(&mut self, position: (i32, i32), text: &str, color: Color) {
        let Some(mixed_color) = mix(color, self.background_color) else {
            return;
        };

        let style = MonoTextStyle::new(self.font, mixed_color);
        // Renderer letters stand on the row above the baseline, embedded-graphics letters on it.
        let baseline_start = Point::new(position.0, position.1 - 1);
        let result = Text::with_baseline(text, baseline_start, style, Baseline::Alphabetic)
            .draw(self.screen)
            .map(|_| ());
        self.remember_error(result);
    }
}

// ============================================================================
// Colours and areas
// ============================================================================

/// The colour to draw for `color` over the background, or nothing if it cannot be seen.
fn mix(color: Color, background_color: Color) -> Option<Rgb565> {
    match color.3 {
        INVISIBLE => None,
        OPAQUE => Some(to_rgb565(color)),
        opacity => {
            let mix_channel = |front: u8, back: u8| {
                let front_share = u16::from(front) * u16::from(opacity);
                let back_share = u16::from(back) * u16::from(OPAQUE - opacity);
                ((front_share + back_share) / u16::from(OPAQUE)) as u8
            };
            Some(to_rgb565(Color(
                mix_channel(color.0, background_color.0),
                mix_channel(color.1, background_color.1),
                mix_channel(color.2, background_color.2),
                OPAQUE,
            )))
        }
    }
}

/// Scales the opacity of `color` by how much of a pixel a shape covers.
fn with_coverage(color: Color, coverage: u8) -> Color {
    let opacity = u16::from(color.3) * u16::from(coverage) / u16::from(OPAQUE);
    Color(color.0, color.1, color.2, opacity as u8)
}

fn to_rgb565(color: Color) -> Rgb565 {
    Rgb565::new(color.0 >> 3, color.1 >> 2, color.2 >> 3)
}

fn to_rectangle(rect: Rect) -> Option<Rectangle> {
    if rect.width <= 0 || rect.height <= 0 {
        return None;
    }
    Some(Rectangle::new(
        Point::new(rect.x, rect.y),
        Size::new(rect.width as u32, rect.height as u32),
    ))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "drawing_tests.rs"]
mod drawing_tests;
