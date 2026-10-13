extern crate alloc;
extern crate std;

use std::{vec, vec::Vec};

use crate::shapes::{draw_widget_bg, Style};
use embedded_graphics::geometry::OriginDimensions;
use embedded_hal::{delay::DelayNs, digital::ErrorType, digital::OutputPin};
use mipidsi::interface::{Interface, InterfaceKind};
use neotime_display::{Display, MEMORY_ROWS, SCREEN_HEIGHT, SCREEN_WIDTH};

use super::*;

// ============================================================================
// Fake screen
// ============================================================================

const BLACK: Rgb565 = Rgb565::BLACK;
const WHITE: Color = Color(255, 255, 255, OPAQUE);
const RED: Color = Color(255, 0, 0, OPAQUE);

/// A 240x240 screen held in memory.
struct FakeScreen {
    pixels: Vec<Rgb565>,
    fails: bool,
}

impl FakeScreen {
    fn new() -> Self {
        Self {
            pixels: vec![BLACK; usize::from(SCREEN_WIDTH) * usize::from(SCREEN_HEIGHT)],
            fails: false,
        }
    }

    fn pixel(&self, x: u16, y: u16) -> Rgb565 {
        self.pixels[usize::from(y) * usize::from(SCREEN_WIDTH) + usize::from(x)]
    }

    fn painted_pixels(&self) -> usize {
        self.pixels.iter().filter(|&&pixel| pixel != BLACK).count()
    }
}

impl OriginDimensions for FakeScreen {
    fn size(&self) -> Size {
        Size::new(SCREEN_WIDTH.into(), SCREEN_HEIGHT.into())
    }
}

impl DrawTarget for FakeScreen {
    type Color = Rgb565;
    type Error = ();

    fn draw_iter<Pixels>(&mut self, pixels: Pixels) -> Result<(), ()>
    where
        Pixels: IntoIterator<Item = Pixel<Rgb565>>,
    {
        if self.fails {
            return Err(());
        }
        let bounds = self.bounding_box();
        for Pixel(point, color) in pixels {
            if bounds.contains(point) {
                self.pixels[point.y as usize * usize::from(SCREEN_WIDTH) + point.x as usize] = color;
            }
        }
        Ok(())
    }
}

fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
    Rect { x, y, width, height }
}

// ============================================================================
// Rectangles
// ============================================================================

#[test]
fn fill_rect_paints_the_area() {
    let mut screen = FakeScreen::new();

    let mut renderer = ScreenRenderer::new(&mut screen);
    renderer.fill_rect(rect(10, 20, 30, 40), RED);
    renderer.finish().unwrap();

    assert_eq!(screen.pixel(10, 20), Rgb565::RED);
    assert_eq!(screen.pixel(39, 59), Rgb565::RED);
    assert_eq!(screen.pixel(40, 59), BLACK);
    assert_eq!(screen.painted_pixels(), 30 * 40);
}

#[test]
fn rect_off_the_edge_is_clipped() {
    let mut screen = FakeScreen::new();

    let mut renderer = ScreenRenderer::new(&mut screen);
    renderer.fill_rect(rect(-10, 230, 20, 20), RED);
    renderer.finish().unwrap();

    assert_eq!(screen.painted_pixels(), 10 * 10);
}

#[test]
fn empty_rect_draws_nothing() {
    let mut screen = FakeScreen::new();

    let mut renderer = ScreenRenderer::new(&mut screen);
    renderer.fill_rect(rect(10, 10, 0, 5), RED);
    renderer.fill_rect(rect(10, 10, 5, -1), RED);
    renderer.finish().unwrap();

    assert_eq!(screen.painted_pixels(), 0);
}

#[test]
fn see_through_rect_is_mixed_with_the_background() {
    let mut screen = FakeScreen::new();

    let mut renderer = ScreenRenderer::new(&mut screen).with_background_color(Color(0, 0, 255, OPAQUE));
    renderer.blend_rect(rect(0, 0, 1, 1), Color(255, 0, 0, 128));
    renderer.finish().unwrap();

    assert_eq!(screen.pixel(0, 0), to_rgb565(Color(128, 0, 127, OPAQUE)));
}

#[test]
fn invisible_rect_draws_nothing() {
    let mut screen = FakeScreen::new();

    let mut renderer = ScreenRenderer::new(&mut screen);
    renderer.blend_rect(rect(0, 0, 50, 50), Color(255, 0, 0, INVISIBLE));
    renderer.finish().unwrap();

    assert_eq!(screen.painted_pixels(), 0);
}

// ============================================================================
// Rows and pixels
// ============================================================================

#[test]
fn coverage_row_leaves_uncovered_pixels_untouched() {
    let mut screen = FakeScreen::new();
    screen.pixels[2] = Rgb565::GREEN;

    let mut renderer = ScreenRenderer::new(&mut screen);
    renderer.blend_row(0, 0, WHITE, &[255, 0, 0, 128, 255]);
    renderer.finish().unwrap();

    assert_eq!(screen.pixel(0, 0), Rgb565::WHITE);
    assert_eq!(screen.pixel(1, 0), BLACK);
    assert_eq!(screen.pixel(2, 0), Rgb565::GREEN);
    assert_eq!(screen.pixel(3, 0), to_rgb565(Color(128, 128, 128, OPAQUE)));
    assert_eq!(screen.pixel(4, 0), Rgb565::WHITE);
}

#[test]
fn draw_pixels_skips_transparent_pixels() {
    let mut screen = FakeScreen::new();
    let clear = Color(0, 0, 0, INVISIBLE);
    let image = [RED, clear, clear, RED];

    let mut renderer = ScreenRenderer::new(&mut screen);
    renderer.draw_pixels((5, 6), &image, 2, 2);
    renderer.finish().unwrap();

    assert_eq!(screen.pixel(5, 6), Rgb565::RED);
    assert_eq!(screen.pixel(6, 7), Rgb565::RED);
    assert_eq!(screen.painted_pixels(), 2);
}

// ============================================================================
// Text and widgets
// ============================================================================

#[test]
fn text_sits_on_its_baseline() {
    let mut screen = FakeScreen::new();

    let mut renderer = ScreenRenderer::new(&mut screen);
    renderer.draw_text((0, 100), "H", WHITE);
    renderer.finish().unwrap();

    let painted_rows: Vec<u16> = (0..SCREEN_HEIGHT)
        .filter(|&y| (0..20).any(|x| screen.pixel(x, y) != BLACK))
        .collect();
    assert!(!painted_rows.is_empty());
    assert!(painted_rows.iter().all(|&y| (80..100).contains(&y)));
}

#[test]
fn a_rounded_box_draws_onto_the_screen() {
    let mut screen = FakeScreen::new();
    let style = Style {
        radius: 8,
        ..Style::default()
    };

    let mut renderer = ScreenRenderer::new(&mut screen);
    draw_widget_bg(&mut renderer, rect(20, 20, 100, 40), &style);
    renderer.finish().unwrap();

    assert!(screen.painted_pixels() > 0);
}

#[test]
fn reports_the_first_screen_error() {
    let mut screen = FakeScreen::new();
    screen.fails = true;

    let mut renderer = ScreenRenderer::new(&mut screen);
    renderer.fill_rect(rect(0, 0, 10, 10), RED);

    assert_eq!(renderer.finish(), Err(()));
}

// ============================================================================
// The PineTime display
// ============================================================================

const SET_COLUMNS: u8 = 0x2A;
const SET_ROWS: u8 = 0x2B;
const WRITE_MEMORY: u8 = 0x2C;
const BYTES_PER_PIXEL: usize = 2;

/// The panel's 240x320 memory, filled in the way the ST7789 fills it.
struct FakePanel {
    memory: Vec<u16>,
    first_column: u16,
    last_column: u16,
    first_row: u16,
    cursor_column: u16,
    cursor_row: u16,
}

impl FakePanel {
    fn new() -> Self {
        Self {
            memory: vec![0; usize::from(SCREEN_WIDTH) * usize::from(MEMORY_ROWS)],
            first_column: 0,
            last_column: SCREEN_WIDTH - 1,
            first_row: 0,
            cursor_column: 0,
            cursor_row: 0,
        }
    }

    fn write_pixel(&mut self, pixel: [u8; BYTES_PER_PIXEL]) {
        let index =
            usize::from(self.cursor_row) * usize::from(SCREEN_WIDTH) + usize::from(self.cursor_column);
        if let Some(cell) = self.memory.get_mut(index) {
            *cell = u16::from_be_bytes(pixel);
        }

        if self.cursor_column == self.last_column {
            self.cursor_column = self.first_column;
            self.cursor_row += 1;
        } else {
            self.cursor_column += 1;
        }
    }

    fn row_is_painted(&self, memory_row: u16) -> bool {
        let start = usize::from(memory_row) * usize::from(SCREEN_WIDTH);
        self.memory[start..start + usize::from(SCREEN_WIDTH)]
            .iter()
            .any(|&pixel| pixel != 0)
    }
}

fn start_and_end(arguments: &[u8]) -> (u16, u16) {
    (
        u16::from_be_bytes([arguments[0], arguments[1]]),
        u16::from_be_bytes([arguments[2], arguments[3]]),
    )
}

impl Interface for FakePanel {
    type Word = u8;
    type Error = ();
    const KIND: InterfaceKind = InterfaceKind::Serial4Line;

    fn send_command(&mut self, command: u8, arguments: &[u8]) -> Result<(), ()> {
        match command {
            SET_COLUMNS => (self.first_column, self.last_column) = start_and_end(arguments),
            SET_ROWS => (self.first_row, _) = start_and_end(arguments),
            WRITE_MEMORY => (self.cursor_column, self.cursor_row) = (self.first_column, self.first_row),
            _ => {}
        }
        Ok(())
    }

    fn send_pixels<const N: usize>(&mut self, pixels: impl IntoIterator<Item = [u8; N]>) -> Result<(), ()> {
        for pixel in pixels {
            self.write_pixel([pixel[0], pixel[1]]);
        }
        Ok(())
    }

    fn send_repeated_pixel<const N: usize>(&mut self, pixel: [u8; N], count: u32) -> Result<(), ()> {
        for _ in 0..count {
            self.write_pixel([pixel[0], pixel[1]]);
        }
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

#[test]
fn a_box_reaches_the_pinetime_display_memory() {
    let mut display = Display::new(FakePanel::new(), FakePin, &mut FakeDelay).unwrap();

    let mut renderer = ScreenRenderer::new(&mut display);
    renderer.fill_rect(rect(0, 0, 100, 40), WHITE);
    draw_widget_bg(&mut renderer, rect(0, 0, 100, 40), &Style::default());
    renderer.finish().unwrap();

    let (panel, _) = display.release();
    assert!(panel.row_is_painted(0));
    assert!(panel.row_is_painted(39));
    assert!(!panel.row_is_painted(40));
}

#[test]
fn drawing_follows_the_display_scroll() {
    let mut display = Display::new(FakePanel::new(), FakePin, &mut FakeDelay).unwrap();
    display.scroll(100).unwrap();

    let mut renderer = ScreenRenderer::new(&mut display);
    renderer.fill_rect(rect(0, 0, 10, 1), WHITE);
    renderer.finish().unwrap();

    let (panel, _) = display.release();
    assert!(!panel.row_is_painted(0));
    assert!(panel.row_is_painted(100));
}
