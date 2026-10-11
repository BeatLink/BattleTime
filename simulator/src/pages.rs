//! Demo pages that exercise the display driver until NeoTime has a real UI.
//!
//! A swipe slides the next page in with the panel's hardware scroll: each step scrolls a band of
//! rows and draws only the band that has just come on screen, as the watch will do.

use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
};
use neotime_display::{SCREEN_HEIGHT, SCREEN_WIDTH};

// ============================================================================
// Pages
// ============================================================================

pub const PAGE_COUNT: usize = 3;

const PAGE_BACKGROUNDS: [Rgb565; PAGE_COUNT] = [
    Rgb565::new(2, 6, 12),
    Rgb565::new(12, 4, 2),
    Rgb565::new(2, 14, 4),
];

pub fn draw_page<Target>(target: &mut Target, page_number: usize) -> Result<(), Target::Error>
where
    Target: DrawTarget<Color = Rgb565>,
{
    let screen = Rectangle::new(
        Point::zero(),
        Size::new(SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32),
    );
    screen
        .into_styled(PrimitiveStyle::with_fill(PAGE_BACKGROUNDS[page_number]))
        .draw(target)?;

    let text_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
    Text::with_alignment("NeoTime", Point::new(120, 40), text_style, Alignment::Center).draw(target)?;

    let page_label = format!("Page {} of {}", page_number + 1, PAGE_COUNT);
    Text::with_alignment(&page_label, Point::new(120, 220), text_style, Alignment::Center).draw(target)?;

    for dot in 0..=page_number {
        let dot_centre_x = 120 - 30 * page_number as i32 + 60 * dot as i32;
        Circle::with_center(Point::new(dot_centre_x, 125), 40)
            .into_styled(PrimitiveStyle::with_fill(Rgb565::YELLOW))
            .draw(target)?;
    }

    Ok(())
}

// ============================================================================
// Sliding between pages
// ============================================================================

pub const SLIDE_STEP_ROWS: u16 = 16;

#[derive(Clone, Copy, PartialEq)]
pub enum SlideDirection {
    /// The next page comes up from the bottom.
    Up,
    /// The previous page comes down from the top.
    Down,
}

/// Which rows of the incoming page are on screen after a step, and where they are.
pub struct SlideStep {
    pub band: Rectangle,
    pub page_top_row: i32,
}

pub fn slide_step(direction: SlideDirection, revealed_rows: u16) -> SlideStep {
    let band_height = SLIDE_STEP_ROWS as u32;
    let screen_height = SCREEN_HEIGHT as i32;

    match direction {
        SlideDirection::Up => SlideStep {
            band: Rectangle::new(
                Point::new(0, screen_height - SLIDE_STEP_ROWS as i32),
                Size::new(SCREEN_WIDTH as u32, band_height),
            ),
            page_top_row: screen_height - revealed_rows as i32,
        },
        SlideDirection::Down => SlideStep {
            band: Rectangle::new(Point::zero(), Size::new(SCREEN_WIDTH as u32, band_height)),
            page_top_row: revealed_rows as i32 - screen_height,
        },
    }
}
