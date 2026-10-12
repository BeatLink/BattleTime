extern crate std;

use std::{vec, vec::Vec};

use super::*;

/// A whole screen in memory.
struct FrameBuffer {
    pixels: Vec<Rgb565>,
}

impl FrameBuffer {
    fn filled(colour: Rgb565) -> FrameBuffer {
        FrameBuffer {
            pixels: vec![colour; (SCREEN_SIZE * SCREEN_SIZE) as usize],
        }
    }

    fn count(&self, columns: core::ops::Range<i32>, rows: core::ops::Range<i32>, colour: Rgb565) -> usize {
        rows.flat_map(|row| columns.clone().map(move |column| (column, row)))
            .filter(|&(column, row)| self.pixels[(row * SCREEN_SIZE as i32 + column) as usize] == colour)
            .count()
    }
}

impl OriginDimensions for FrameBuffer {
    fn size(&self) -> Size {
        Size::new(SCREEN_SIZE, SCREEN_SIZE)
    }
}

impl DrawTarget for FrameBuffer {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<Pixels: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        pixels: Pixels,
    ) -> Result<(), Self::Error> {
        for Pixel(point, colour) in pixels {
            if self.bounding_box().contains(point) {
                self.pixels[(point.y * SCREEN_SIZE as i32 + point.x) as usize] = colour;
            }
        }
        Ok(())
    }
}

const BLUE: Rgb565 = Rgb565::new(0, 7, 11);

fn line_rows(line: usize) -> core::ops::Range<i32> {
    let top = FIRST_BASELINE + line as i32 * LINE_HEIGHT - FONT_BASELINE;
    top..top + CHARACTER_HEIGHT
}

const VALUE_LEFT: i32 = LEFT_MARGIN + LABEL_CHARACTERS as i32 * CHARACTER_WIDTH;

#[test]
fn a_labelled_line_has_a_grey_label_then_a_white_value() {
    let mut screen = FrameBuffer::filled(BLUE);
    draw_line(&mut screen, FIRMWARE, "On trial", BLUE).unwrap();

    let rows = line_rows(FIRMWARE);
    assert!(screen.count(LEFT_MARGIN..VALUE_LEFT, rows.clone(), LABEL_COLOUR) > 100);
    assert_eq!(
        screen.count(LEFT_MARGIN..VALUE_LEFT, rows.clone(), VALUE_COLOUR),
        0
    );
    assert!(screen.count(VALUE_LEFT..240, rows.clone(), VALUE_COLOUR) > 100);
    assert_eq!(screen.count(0..240, 0..rows.start, VALUE_COLOUR), 0);
    assert_eq!(screen.count(0..240, rows.end..240, VALUE_COLOUR), 0);
}

#[test]
fn a_line_without_a_label_starts_at_the_margin() {
    let mut screen = FrameBuffer::filled(BLUE);
    draw_line(&mut screen, ACTION, "Receiving firmware", BLUE).unwrap();
    assert!(screen.count(LEFT_MARGIN..VALUE_LEFT, line_rows(ACTION), VALUE_COLOUR) > 100);
    assert_eq!(screen.count(0..240, line_rows(ACTION), LABEL_COLOUR), 0);
}

#[test]
fn a_shorter_value_covers_what_the_longer_one_said() {
    let mut screen = FrameBuffer::filled(BLUE);
    draw_line(&mut screen, ACTION, "Installing firmware", BLUE).unwrap();
    draw_line(&mut screen, ACTION, "Done", BLUE).unwrap();
    let right_of_done = LEFT_MARGIN + 4 * CHARACTER_WIDTH;
    assert_eq!(
        screen.count(right_of_done..240, line_rows(ACTION), VALUE_COLOUR),
        0
    );
}

#[test]
fn a_new_background_replaces_the_old_one_across_the_whole_line() {
    let mut screen = FrameBuffer::filled(BLUE);
    draw_line(&mut screen, BLUETOOTH, "Connected", Rgb565::BLACK).unwrap();
    assert_eq!(screen.count(LEFT_MARGIN..240, line_rows(BLUETOOTH), BLUE), 0);
}

#[test]
fn a_value_past_its_room_is_cut_at_the_edge() {
    let mut screen = FrameBuffer::filled(BLUE);
    draw_line(
        &mut screen,
        PROGRESS,
        "A line far longer than twenty-two characters",
        BLUE,
    )
    .unwrap();
    let end_of_line = LEFT_MARGIN + LINE_CHARACTERS as i32 * CHARACTER_WIDTH;
    assert!(screen.count(LEFT_MARGIN..end_of_line, line_rows(PROGRESS), VALUE_COLOUR) > 100);
    assert_eq!(
        screen.count(end_of_line..240, line_rows(PROGRESS), VALUE_COLOUR),
        0
    );
}

#[test]
fn a_labelled_value_gets_the_room_the_label_leaves() {
    assert_eq!(fitted(FILES, "Formatted and more"), "Formatted a");
    assert_eq!(fitted(TITLE, "NeoTime 0.1.0+1b19aed*"), "NeoTime 0.1.0+1b19aed*");
    assert_eq!(value_characters(FILES) + LABEL_CHARACTERS, LINE_CHARACTERS);
}

#[test]
fn every_line_fits_on_the_screen() {
    let last_bottom = line_rows(LINE_COUNT - 1).end;
    assert!(last_bottom <= SCREEN_SIZE as i32);
    let widest = LEFT_MARGIN + LINE_CHARACTERS as i32 * CHARACTER_WIDTH;
    assert!(widest <= SCREEN_SIZE as i32);
    for line in 0..LINE_COUNT {
        assert!(label(line).map_or(0, str::len) < LABEL_CHARACTERS);
    }
}

#[test]
fn text_that_is_not_ascii_is_cut_at_a_character() {
    let value = "é€ ünïcödé line that runs on";
    assert_eq!(
        fitted(FIRMWARE, value).chars().count(),
        value_characters(FIRMWARE)
    );
    draw_line(&mut FrameBuffer::filled(BLUE), FIRMWARE, value, BLUE).unwrap();
}

#[test]
fn text_matches_embedded_graphics_drawing_the_same_font() {
    use embedded_graphics::{
        mono_font::{ascii::FONT_10X20, MonoTextStyleBuilder},
        text::{Baseline, Text},
    };

    let every_character: std::string::String = (' '..='~').collect();
    for (row, text) in every_character.as_bytes().chunks(LINE_CHARACTERS).enumerate() {
        let text = core::str::from_utf8(text).unwrap();
        let top_left = Point::new(LEFT_MARGIN, row as i32 * CHARACTER_HEIGHT);
        let mut ours = FrameBuffer::filled(Rgb565::RED);
        draw_text(&mut ours, top_left, text, VALUE_COLOUR, BLUE).unwrap();

        let mut theirs = FrameBuffer::filled(Rgb565::RED);
        let style = MonoTextStyleBuilder::new()
            .font(&FONT_10X20)
            .text_color(VALUE_COLOUR)
            .background_color(BLUE)
            .build();
        Text::with_baseline(text, top_left, style, Baseline::Top)
            .draw(&mut theirs)
            .unwrap();

        assert!(ours.pixels == theirs.pixels, "{text} differs");
    }
}

#[test]
fn a_character_the_font_lacks_is_drawn_as_a_question_mark() {
    assert!(glyph_pixels('€').eq(glyph_pixels('?')));
    assert!(glyph_pixels('\n').eq(glyph_pixels('?')));
}
