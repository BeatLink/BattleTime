use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
};
use neotime_display::{Display, SCREEN_HEIGHT};

use super::{slide_once, WatchDisplay};
use crate::{
    hardware::{ThreadDelay, UnconnectedPin},
    pages::{draw_page, SlideDirection, SLIDE_STEP_ROWS},
    panel::SimulatedPanel,
};

// ============================================================================
// Helpers
// ============================================================================

fn started_display() -> (SimulatedPanel, WatchDisplay) {
    let panel = SimulatedPanel::new();
    let display = Display::new(panel.clone(), UnconnectedPin, &mut ThreadDelay).unwrap();
    (panel, display)
}

fn fill(display: &mut WatchDisplay, area: Rectangle, colour: Rgb565) {
    area.into_styled(PrimitiveStyle::with_fill(colour))
        .draw(display)
        .unwrap();
}

fn shown_frame(panel: &SimulatedPanel) -> Vec<Rgb565> {
    let mut frame = Vec::new();
    panel.for_each_shown_pixel(|_column, _screen_row, colour| frame.push(colour));
    frame
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn drawn_colours_show_as_drawn() {
    let (panel, mut display) = started_display();

    fill(
        &mut display,
        Rectangle::new(Point::new(10, 20), Size::new(5, 5)),
        Rgb565::RED,
    );

    assert_eq!(panel.shown_pixel(12, 22), Rgb565::RED);
    assert_eq!(panel.shown_pixel(9, 22), Rgb565::BLACK);
}

#[test]
fn drawing_after_a_scroll_lands_at_the_screen_position() {
    let (panel, mut display) = started_display();
    fill(
        &mut display,
        Rectangle::new(Point::zero(), Size::new(240, 240)),
        Rgb565::BLACK,
    );

    display.scroll(300).unwrap();
    fill(
        &mut display,
        Rectangle::new(Point::new(0, 10), Size::new(240, 40)),
        Rgb565::GREEN,
    );

    assert_eq!(panel.shown_pixel(0, 9), Rgb565::BLACK);
    assert_eq!(panel.shown_pixel(0, 10), Rgb565::GREEN);
    assert_eq!(panel.shown_pixel(0, 49), Rgb565::GREEN);
    assert_eq!(panel.shown_pixel(0, 50), Rgb565::BLACK);
}

#[test]
fn sliding_up_shows_the_next_page_as_if_drawn_directly() {
    let (panel, mut display) = started_display();
    draw_page(&mut display, 0).unwrap();

    let mut revealed_rows = 0;
    while revealed_rows < SCREEN_HEIGHT {
        revealed_rows += SLIDE_STEP_ROWS;
        slide_once(&mut display, SlideDirection::Up, 1, revealed_rows);
    }

    let (expected_panel, mut expected_display) = started_display();
    draw_page(&mut expected_display, 1).unwrap();
    assert!(shown_frame(&panel) == shown_frame(&expected_panel));
}

#[test]
fn sliding_down_shows_the_previous_page_as_if_drawn_directly() {
    let (panel, mut display) = started_display();
    draw_page(&mut display, 1).unwrap();

    let mut revealed_rows = 0;
    while revealed_rows < SCREEN_HEIGHT {
        revealed_rows += SLIDE_STEP_ROWS;
        slide_once(&mut display, SlideDirection::Down, 0, revealed_rows);
    }

    let (expected_panel, mut expected_display) = started_display();
    draw_page(&mut expected_display, 0).unwrap();
    assert!(shown_frame(&panel) == shown_frame(&expected_panel));
}
