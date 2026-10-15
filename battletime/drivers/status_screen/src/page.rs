//! The status page's layout: a title, four labelled lines for each part of the watch, what it is
//! doing, and how far along it is, in white 10x20 text on the background colour of the mode.

use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};

// ============================================================================
// Lines
// ============================================================================

/// The project's name, on a line of its own at the top of every page.
pub const NAME: usize = 0;
/// The firmware's version, or which of the bootloader or its updater is drawing the page.
pub const TITLE: usize = 1;
pub const BOOTLOADER: usize = 2;
pub const FIRMWARE: usize = 3;
pub const FILES: usize = 4;
pub const BLUETOOTH: usize = 5;
/// What the watch is doing: an update's step, or what the mode means.
pub const ACTION: usize = 6;
/// How much of an update has arrived, why it stopped, or what the wearer should do.
pub const PROGRESS: usize = 7;
/// The battery's charge and the charger, on the pages of the bootloader and its updater.
pub const BATTERY: usize = 8;
/// Why the watch last restarted.
pub const LAST_RUN: usize = 9;
pub const LINE_COUNT: usize = 10;

/// What the name line says.
pub const PRODUCT_NAME: &str = "BattleTime";

/// The grey name before a line's value, for the lines that have one.
pub fn label(line: usize) -> Option<&'static str> {
    match line {
        BOOTLOADER => Some("Bootloader"),
        FIRMWARE => Some("Firmware"),
        FILES => Some("Filesystem"),
        BLUETOOTH => Some("Bluetooth"),
        _ => None,
    }
}

// ============================================================================
// Sizes
// ============================================================================

pub const SCREEN_SIZE: u32 = 240;
const CHARACTER_WIDTH: i32 = 10;
const CHARACTER_HEIGHT: i32 = 20;
/// Rows of the character cell above the baseline.
const FONT_BASELINE: i32 = 15;
const LINE_HEIGHT: i32 = 22;
const FIRST_BASELINE: i32 = 25;
const LEFT_MARGIN: i32 = 12;

/// Characters that fit between the left margin and the right edge.
pub const LINE_CHARACTERS: usize = 22;
/// Characters a label takes, its space included, so every value starts in one column.
pub const LABEL_CHARACTERS: usize = 11;

pub const VALUE_COLOUR: Rgb565 = Rgb565::WHITE;
pub const LABEL_COLOUR: Rgb565 = Rgb565::new(20, 40, 20);

/// Characters left for a line's value once its label is drawn.
pub fn value_characters(line: usize) -> usize {
    match label(line) {
        Some(_) => LINE_CHARACTERS - LABEL_CHARACTERS,
        None => LINE_CHARACTERS,
    }
}

/// The value as far as it fits on its line, cut at a character, never inside one.
pub fn fitted(line: usize, value: &str) -> &str {
    value
        .char_indices()
        .nth(value_characters(line))
        .map_or(value, |(end, _)| &value[..end])
}

// ============================================================================
// Drawing
// ============================================================================

/// Draws one line, its label in grey and its value in white, and clears the rest of it to the background.
pub fn draw_line<Target: DrawTarget<Color = Rgb565>>(
    target: &mut Target,
    line: usize,
    value: &str,
    background: Rgb565,
) -> Result<(), Target::Error> {
    let top = FIRST_BASELINE + line as i32 * LINE_HEIGHT - FONT_BASELINE;
    let mut value_left = LEFT_MARGIN;
    if let Some(label) = label(line) {
        let label_end = draw_text(
            target,
            Point::new(LEFT_MARGIN, top),
            label,
            LABEL_COLOUR,
            background,
        )?;
        value_left += LABEL_CHARACTERS as i32 * CHARACTER_WIDTH;
        clear(target, label_end, value_left, top, background)?;
    }
    let value_end = draw_text(
        target,
        Point::new(value_left, top),
        fitted(line, value),
        VALUE_COLOUR,
        background,
    )?;
    clear(target, value_end, SCREEN_SIZE as i32, top, background)
}

/// Characters that fit across the whole screen.
pub const SCREEN_CHARACTERS: usize = SCREEN_SIZE as usize / CHARACTER_WIDTH as usize;

/// Height of one line of text in pixels.
pub const TEXT_HEIGHT: u32 = CHARACTER_HEIGHT as u32;

/// Draws text centred in the columns from `left` to `right`, with its top at `top`, as far as it fits.
pub fn draw_centred<Target: DrawTarget<Color = Rgb565>>(
    target: &mut Target,
    left: i32,
    right: i32,
    top: i32,
    text: &str,
    colours: TextColours,
) -> Result<(), Target::Error> {
    let fitting_characters = ((right - left) / CHARACTER_WIDTH).max(0) as usize;
    let shown = text
        .char_indices()
        .nth(fitting_characters)
        .map_or(text, |(end, _)| &text[..end]);
    let width = shown.chars().count() as i32 * CHARACTER_WIDTH;
    let start = left + (right - left - width) / 2;
    draw_text(
        target,
        Point::new(start, top),
        shown,
        colours.text,
        colours.background,
    )?;
    Ok(())
}

/// The colour of a text's lit pixels and of the rest of its cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextColours {
    pub text: Rgb565,
    pub background: Rgb565,
}

/// Fills the line's height between two columns with the background.
fn clear<Target: DrawTarget<Color = Rgb565>>(
    target: &mut Target,
    left: i32,
    right: i32,
    top: i32,
    background: Rgb565,
) -> Result<(), Target::Error> {
    if right <= left {
        return Ok(());
    }
    let area = Rectangle::new(
        Point::new(left, top),
        Size::new((right - left) as u32, CHARACTER_HEIGHT as u32),
    );
    target.fill_solid(&area, background)
}

// ============================================================================
// The font
// ============================================================================

/// The printable ASCII characters, 16 to a row of the image, one bit a pixel; see fonts/README.md.
const FONT_IMAGE: &[u8] = include_bytes!("../fonts/font_10x20.raw");
const IMAGE_ROW_BYTES: usize = 20;
const CHARACTERS_PER_IMAGE_ROW: usize = 16;
const FIRST_CHARACTER: char = ' ';
const LAST_CHARACTER: char = '~';
/// Shown for any character the font does not have.
const MISSING_CHARACTER: char = '?';

/// Draws each character as one block of pixels with its top left at `top_left`, and gives the column after the last.
fn draw_text<Target: DrawTarget<Color = Rgb565>>(
    target: &mut Target,
    top_left: Point,
    text: &str,
    colour: Rgb565,
    background: Rgb565,
) -> Result<i32, Target::Error> {
    let mut left = top_left.x;
    for character in text.chars() {
        let cell = Rectangle::new(
            Point::new(left, top_left.y),
            Size::new(CHARACTER_WIDTH as u32, CHARACTER_HEIGHT as u32),
        );
        let pixels = glyph_pixels(character).map(|lit| if lit { colour } else { background });
        target.fill_contiguous(&cell, pixels)?;
        left += CHARACTER_WIDTH;
    }
    Ok(left)
}

/// Whether each pixel of a character's cell is lit, row by row.
fn glyph_pixels(character: char) -> impl Iterator<Item = bool> {
    let character = if (FIRST_CHARACTER..=LAST_CHARACTER).contains(&character) {
        character
    } else {
        MISSING_CHARACTER
    };
    let index = character as usize - FIRST_CHARACTER as usize;
    let first_column = (index % CHARACTERS_PER_IMAGE_ROW) * CHARACTER_WIDTH as usize;
    let first_row = (index / CHARACTERS_PER_IMAGE_ROW) * CHARACTER_HEIGHT as usize;
    (0..CHARACTER_HEIGHT as usize).flat_map(move |row| {
        (0..CHARACTER_WIDTH as usize).map(move |column| {
            let image_column = first_column + column;
            let byte = FONT_IMAGE[(first_row + row) * IMAGE_ROW_BYTES + image_column / 8];
            byte & (0x80 >> (image_column % 8)) != 0
        })
    })
}

#[cfg(test)]
mod tests;
