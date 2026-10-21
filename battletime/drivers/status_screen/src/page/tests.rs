extern crate std;

use std::{vec, vec::Vec};

use super::*;

/// A whole screen in memory.
struct FrameBuffer {
    pixels: Vec<Rgb565>,
}

impl FrameBuffer {
    /// A screen of one colour; without the bitmap font, the page is given the block font first.
    fn filled(colour: Rgb565) -> FrameBuffer {
        #[cfg(not(feature = "bitmap-font"))]
        use_font(block_font);
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
    assert_eq!(fitted(TITLE, "0.1.0+1b19aed*"), "0.1.0+1b19aed*");
    assert_eq!(label(NAME), None);
    assert_eq!(fitted(NAME, PRODUCT_NAME), PRODUCT_NAME);
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
fn the_longest_battery_line_fits_whole() {
    let longest = "Battery 100% unplugged";
    assert_eq!(longest.len(), battletime_battery::BATTERY_LINE_LENGTH);
    assert_eq!(fitted(BATTERY, longest), longest);
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

#[cfg(feature = "bitmap-font")]
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

#[cfg(feature = "bitmap-font")]
#[test]
fn a_character_the_font_lacks_is_drawn_as_a_question_mark() {
    assert!(glyph_pixels('€').eq(glyph_pixels('?')));
    assert!(glyph_pixels('\n').eq(glyph_pixels('?')));
}

#[test]
fn centred_text_sits_in_the_middle_of_its_columns() {
    let mut screen = FrameBuffer::filled(BLUE);
    let colours = TextColours {
        text: VALUE_COLOUR,
        background: BLUE,
    };

    draw_centred(&mut screen, 20, 120, 0, "OK", colours).unwrap();

    assert!(screen.count(60..80, 0..CHARACTER_HEIGHT, VALUE_COLOUR) > 20);
    assert_eq!(screen.count(0..60, 0..240, VALUE_COLOUR), 0);
    assert_eq!(screen.count(80..240, 0..240, VALUE_COLOUR), 0);
}

#[test]
fn centred_text_wider_than_its_columns_is_cut() {
    let mut screen = FrameBuffer::filled(BLUE);
    let colours = TextColours {
        text: VALUE_COLOUR,
        background: BLUE,
    };

    draw_centred(&mut screen, 0, 40, 0, "Too wide here", colours).unwrap();

    assert!(screen.count(0..40, 0..CHARACTER_HEIGHT, VALUE_COLOUR) > 20);
    assert_eq!(screen.count(40..240, 0..240, VALUE_COLOUR), 0);
    assert_eq!(SCREEN_CHARACTERS, 24);
}

// ============================================================================
// A font given from outside
// ============================================================================

/// A font for the tests: every character but the space a solid block, with a half-inked left column.
#[cfg(not(feature = "bitmap-font"))]
fn block_font(character: char, row: i32, ink: &mut [u8; CELL_WIDTH]) {
    if character != ' ' && (3..CELL_BASELINE + 2).contains(&row) {
        ink.fill(255);
        ink[0] = 128;
    }
}

#[cfg(not(feature = "bitmap-font"))]
#[test]
fn a_given_font_is_blended_into_the_background() {
    let mut screen = FrameBuffer::filled(BLUE);
    draw_text(&mut screen, Point::new(0, 0), "A ", VALUE_COLOUR, BLUE).unwrap();

    assert_eq!(screen.count(1..10, 3..17, VALUE_COLOUR), 9 * 14);
    assert_eq!(screen.count(0..1, 3..17, blend(BLUE, VALUE_COLOUR, 128)), 14);
    assert_eq!(screen.count(0..20, 0..20, VALUE_COLOUR), 9 * 14);
    assert_eq!(screen.count(10..20, 0..20, BLUE), 200);
}

#[cfg(not(feature = "bitmap-font"))]
#[test]
fn blending_keeps_both_ends_exact() {
    assert_eq!(blend(BLUE, VALUE_COLOUR, 0), BLUE);
    assert_eq!(blend(BLUE, VALUE_COLOUR, 255), VALUE_COLOUR);
    assert_eq!(blend(Rgb565::BLACK, Rgb565::WHITE, 128), Rgb565::new(16, 32, 16));
}

// ============================================================================
// Without a font
// ============================================================================

#[test]
fn without_a_font_an_update_shows_its_steps_and_how_much_arrived() {
    let mut screen = FrameBuffer::filled(Rgb565::BLACK);
    let update = UpdateProgress {
        step: 2,
        failed: false,
        bytes_received: 50,
        bytes_in_total: 100,
    };

    draw_without_text(&mut screen, Rgb565::BLUE, Some(update)).unwrap();

    let left = steps_left();
    let step_rows = STEPS_TOP..STEPS_TOP + STEP_SIZE;
    let step =
        |index: i32| left + index * (STEP_SIZE + STEP_GAP)..left + index * (STEP_SIZE + STEP_GAP) + STEP_SIZE;
    let square = (STEP_SIZE * STEP_SIZE) as usize;
    assert_eq!(screen.count(step(1), step_rows.clone(), STEP_LIT), square);
    assert_eq!(screen.count(step(2), step_rows, STEP_TO_COME), square);
    let bar_rows = BAR_TOP + BAR_EDGE..BAR_TOP + BAR_HEIGHT - BAR_EDGE;
    let lit = screen.count(0..SCREEN_SIZE as i32, bar_rows.clone(), STEP_LIT);
    let inner = (SCREEN_SIZE as i32 - 2 * left - 2 * BAR_EDGE) as usize;
    assert_eq!(lit, inner / 2 * bar_rows.len());
}

#[test]
fn without_a_font_a_failed_update_turns_every_step_red() {
    let mut screen = FrameBuffer::filled(Rgb565::BLACK);
    let update = UpdateProgress {
        step: 3,
        failed: true,
        ..UpdateProgress::default()
    };

    draw_without_text(&mut screen, Rgb565::BLUE, Some(update)).unwrap();

    let rows = STEPS_TOP..STEPS_TOP + STEP_SIZE;
    let all_steps = (STEP_SIZE * STEP_SIZE) as usize * UPDATE_STEPS;
    assert_eq!(screen.count(0..SCREEN_SIZE as i32, rows, STEP_FAILED), all_steps);
}

#[test]
fn without_a_font_and_no_update_the_page_is_its_colour() {
    let mut screen = FrameBuffer::filled(Rgb565::BLACK);

    draw_without_text(&mut screen, Rgb565::BLUE, None).unwrap();

    let whole = (SCREEN_SIZE * SCREEN_SIZE) as usize;
    assert_eq!(
        screen.count(0..SCREEN_SIZE as i32, 0..SCREEN_SIZE as i32, Rgb565::BLUE),
        whole
    );
}
