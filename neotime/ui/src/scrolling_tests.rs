extern crate std;

use std::{vec, vec::Vec};

use embedded_hal::{delay::DelayNs, digital::ErrorType};
use neotime_display::PanelInterface;

use super::*;

// ============================================================================
// Fake hardware
// ============================================================================

struct SilentInterface;

impl PanelInterface for SilentInterface {
    type Error = ();

    fn send_command(&mut self, _command: u8, _arguments: &[u8]) -> Result<(), ()> {
        Ok(())
    }

    fn send_pixels(&mut self, _bytes: &[u8]) -> Result<(), ()> {
        Ok(())
    }
}

struct FakePin;

impl ErrorType for FakePin {
    type Error = core::convert::Infallible;
}

impl OutputPin for FakePin {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

struct FakeDelay;

impl DelayNs for FakeDelay {
    fn delay_ns(&mut self, _: u32) {}
}

fn new_display() -> Display<SilentInterface, FakePin> {
    Display::new(SilentInterface, FakePin, &mut FakeDelay).unwrap()
}

/// A 240x240 picture in memory that counts what is drawn on it.
struct Picture {
    pixels: Vec<Rgb565>,
}

impl OriginDimensions for Picture {
    fn size(&self) -> Size {
        Size::new(240, 240)
    }
}

impl DrawTarget for Picture {
    type Color = Rgb565;
    type Error = ();

    fn draw_iter<Pixels>(&mut self, pixels: Pixels) -> Result<(), ()>
    where
        Pixels: IntoIterator<Item = Pixel<Rgb565>>,
    {
        for Pixel(point, color) in pixels {
            self.pixels[point.y as usize * 240 + point.x as usize] = color;
        }
        Ok(())
    }
}

// ============================================================================
// The PineTime display
// ============================================================================

#[test]
fn a_new_display_scrolls_its_whole_memory_unturned() {
    let display = new_display();

    let state = display.scroll_state();

    assert_eq!(state.band.first_row, 0);
    assert_eq!(state.band.end_row, MEMORY_ROWS as i32);
    assert_eq!(state.turned_rows, 0);
}

#[test]
fn a_band_keeps_the_rows_around_it_still_and_starts_unturned() {
    let mut display = new_display();
    display.scroll(30).unwrap();

    let band = ScrollBand {
        first_row: 40,
        end_row: 200,
    };
    display.set_scroll_band(band).unwrap();

    assert_eq!(display.scroll_state(), ScrollState { band, turned_rows: 0 });
    let area = display.scroll_area();
    assert_eq!((area.top_fixed_rows, area.scrolling_rows), (40, 160));
}

#[test]
fn scrolling_turns_the_ring_both_ways_and_wraps_round_the_band() {
    let mut display = new_display();
    display.set_scroll_band(ScrollBand::WHOLE_SCREEN).unwrap();

    display.scroll_by(-10).unwrap();
    assert_eq!(display.scroll_state().turned_rows, 230);

    display.scroll_by(25).unwrap();
    assert_eq!(display.scroll_state().turned_rows, 15);
}

// ============================================================================
// Screens that cannot scroll
// ============================================================================

#[test]
fn a_fixed_screen_cannot_scroll_and_draws_straight_through() {
    let mut picture = Picture {
        pixels: vec![Rgb565::BLACK; 240 * 240],
    };
    let mut screen = FixedScreen::new(&mut picture);

    screen.set_scroll_band(ScrollBand::WHOLE_SCREEN).unwrap();
    screen.scroll_by(5).unwrap();
    let area = Rectangle::new(Point::new(1, 2), Size::new(2, 1));
    screen.fill_solid(&area, Rgb565::RED).unwrap();
    screen
        .fill_contiguous(&area.translate(Point::new(0, 1)), [Rgb565::GREEN; 2])
        .unwrap();
    screen.draw_iter([Pixel(Point::new(0, 0), Rgb565::BLUE)]).unwrap();

    assert!(!screen.can_scroll());
    assert_eq!(screen.scroll_state().turned_rows, 0);
    assert_eq!(screen.size(), Size::new(240, 240));
    assert_eq!(picture.pixels[2 * 240 + 1], Rgb565::RED);
    assert_eq!(picture.pixels[3 * 240 + 2], Rgb565::GREEN);
    assert_eq!(picture.pixels[0], Rgb565::BLUE);
}
