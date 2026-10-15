extern crate std;

use std::{vec, vec::Vec};

use embedded_graphics::{
    pixelcolor::Rgb565,
    primitives::{PrimitiveStyle, Rectangle},
};

use super::*;

// ============================================================================
// Helpers
// ============================================================================

const BLACK: Rgb565 = Rgb565::BLACK;
const RED: Rgb565 = Rgb565::RED;

fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
    Rect { x, y, width, height }
}

fn rectangle(x: i32, y: i32, width: u32, height: u32) -> Rectangle {
    Rectangle::new(Point::new(x, y), Size::new(width, height))
}

/// The strip's rows as text, `#` for red and `.` for anything else.
fn rows_of(strip: &Strip) -> Vec<std::string::String> {
    let pixels: Vec<Rgb565> = strip.pixels().collect();
    pixels
        .chunks(strip.area().width as usize)
        .map(|row| {
            row.iter()
                .map(|&pixel| if pixel == RED { '#' } else { '.' })
                .collect()
        })
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn counts_the_whole_rows_a_buffer_holds() {
    assert_eq!(Strip::rows_that_fit(960, 240), 4);
    assert_eq!(Strip::rows_that_fit(960, 100), 9);
    assert_eq!(Strip::rows_that_fit(100, 240), 1);
}

#[test]
fn keeps_only_the_part_of_a_fill_inside_it() {
    let mut buffer = vec![0u16; 64];
    let mut strip = Strip::new(&mut buffer, rect(10, 20, 4, 3));

    strip.fill_solid(&rectangle(12, 18, 10, 4), RED).unwrap();

    assert_eq!(rows_of(&strip), ["..##", "..##", "...."]);
}

#[test]
fn skips_the_colours_of_rows_and_columns_outside_it() {
    let mut buffer = vec![0u16; 64];
    let mut strip = Strip::new(&mut buffer, rect(10, 20, 4, 2));
    let pattern = [BLACK, RED, BLACK, RED, RED, BLACK];

    let area = rectangle(9, 19, 6, 3);
    let colors = pattern.iter().cycle().copied().take(18);
    strip.fill_contiguous(&area, colors).unwrap();

    assert_eq!(rows_of(&strip), ["#.##", "#.##"]);
}

#[test]
fn draws_single_pixels_and_drops_those_outside() {
    let mut buffer = vec![0u16; 64];
    let mut strip = Strip::new(&mut buffer, rect(0, 0, 3, 2));

    let pixels = [Pixel(Point::new(1, 1), RED), Pixel(Point::new(5, 5), RED)];
    strip.draw_iter(pixels).unwrap();

    assert_eq!(rows_of(&strip), ["...", ".#."]);
}

#[test]
fn shapes_are_placed_in_screen_positions() {
    let mut buffer = vec![0u16; 64];
    let mut strip = Strip::new(&mut buffer, rect(100, 50, 4, 2));

    rectangle(101, 0, 2, 51)
        .into_styled(PrimitiveStyle::with_fill(RED))
        .draw(&mut strip)
        .unwrap();

    assert_eq!(rows_of(&strip), [".##.", "...."]);
}
