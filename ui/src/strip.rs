//! A few rows of the screen held in memory, so a part of the screen is drawn there first and then
//! sent to the panel in one go.
//!
//! Drawing straight onto the panel paints the background, then each widget on top, and the panel
//! shows every step; it also sends every short run of pixels on its own. Drawing into a strip and
//! sending the strip once shows each pixel only in its final colour, and sends it once. Pixels are
//! kept in the panel's own order, high byte first, so a strip can go to the panel as it lies.

use core::convert::Infallible;

use crate::geometry::Rect;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
use neotime_display::{color_of_panel_pixel, panel_pixel};

// ============================================================================
// Strip
// ============================================================================

/// A rectangle of the screen held in a buffer of panel pixels; drawing outside it is dropped.
pub struct Strip<'buffer> {
    pixels: &'buffer mut [u16],
    area: Rect,
}

impl<'buffer> Strip<'buffer> {
    /// Holds `area` in the front of `buffer`, which must have room for all of it.
    pub fn new(buffer: &'buffer mut [u16], area: Rect) -> Self {
        let pixel_count = (area.width.max(0) * area.height.max(0)) as usize;
        Self {
            pixels: &mut buffer[..pixel_count],
            area,
        }
    }

    /// How many whole rows of `width` pixels a buffer of `buffer_pixels` holds, at least one.
    pub fn rows_that_fit(buffer_pixels: usize, width: i32) -> i32 {
        (buffer_pixels / width.max(1) as usize).max(1) as i32
    }

    pub fn area(&self) -> Rect {
        self.area
    }

    /// The pixels of row `y` across `columns`, which must lie inside the strip.
    pub(crate) fn row_mut(&mut self, y: i32, columns: core::ops::Range<i32>) -> &mut [u16] {
        let start = ((y - self.area.y) * self.area.width + (columns.start - self.area.x)) as usize;
        &mut self.pixels[start..start + columns.len()]
    }

    /// The pixel at a screen point, if it is inside the strip.
    pub(crate) fn pixel_at(&self, x: i32, y: i32) -> Option<Rgb565> {
        self.index_of(x, y)
            .map(|index| color_of_panel_pixel(self.pixels[index]))
    }

    /// The strip's colours row by row.
    pub fn pixels(&self) -> impl Iterator<Item = Rgb565> + '_ {
        self.pixels.iter().map(|&pixel| color_of_panel_pixel(pixel))
    }

    /// Where a screen point sits in the buffer, if it is inside the strip.
    fn index_of(&self, x: i32, y: i32) -> Option<usize> {
        let column = x - self.area.x;
        let row = y - self.area.y;
        let is_inside = (0..self.area.width).contains(&column) && (0..self.area.height).contains(&row);
        is_inside.then(|| (row * self.area.width + column) as usize)
    }
}

// ============================================================================
// Drawing with embedded-graphics
// ============================================================================

impl OriginDimensions for Strip<'_> {
    /// The whole screen, so callers place shapes in screen positions.
    fn size(&self) -> Size {
        let right = (self.area.x + self.area.width).max(0) as u32;
        let bottom = (self.area.y + self.area.height).max(0) as u32;
        Size::new(right, bottom)
    }
}

impl DrawTarget for Strip<'_> {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<Pixels>(&mut self, pixels: Pixels) -> Result<(), Infallible>
    where
        Pixels: IntoIterator<Item = Pixel<Rgb565>>,
    {
        for Pixel(point, color) in pixels {
            if let Some(index) = self.index_of(point.x, point.y) {
                self.pixels[index] = panel_pixel(color);
            }
        }
        Ok(())
    }

    fn fill_contiguous<Colors>(&mut self, area: &Rectangle, colors: Colors) -> Result<(), Infallible>
    where
        Colors: IntoIterator<Item = Rgb565>,
    {
        let width = area.size.width as i32;
        let height = area.size.height as i32;
        let mut colors = colors.into_iter();
        let first_column = area.top_left.x.max(self.area.x);
        let end_column = (area.top_left.x + width).min(self.area.x + self.area.width);
        let skipped_before = (first_column - area.top_left.x).max(0) as usize;

        for row in area.top_left.y..area.top_left.y + height {
            let mut row_colors = colors.by_ref().take(width as usize);
            let row_is_inside = (self.area.y..self.area.y + self.area.height).contains(&row);
            if !row_is_inside || first_column >= end_column {
                row_colors.for_each(drop);
                continue;
            }
            let start = (row - self.area.y) * self.area.width + (first_column - self.area.x);
            let destination = &mut self.pixels[start as usize..(start + end_column - first_column) as usize];
            for (place, color) in destination
                .iter_mut()
                .zip(row_colors.by_ref().skip(skipped_before))
            {
                *place = panel_pixel(color);
            }
            row_colors.for_each(drop);
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Rgb565) -> Result<(), Infallible> {
        let first_column = area.top_left.x.max(self.area.x);
        let end_column = (area.top_left.x + area.size.width as i32).min(self.area.x + self.area.width);
        let first_row = area.top_left.y.max(self.area.y);
        let end_row = (area.top_left.y + area.size.height as i32).min(self.area.y + self.area.height);
        if first_column >= end_column {
            return Ok(());
        }
        for row in first_row..end_row {
            let start = (row - self.area.y) * self.area.width + (first_column - self.area.x);
            self.pixels[start as usize..(start + end_column - first_column) as usize]
                .fill(panel_pixel(color));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "strip_tests.rs"]
mod strip_tests;
