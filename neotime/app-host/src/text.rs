//! Lines of text: wrapping a label on spaces to fit its width, and drawing a line glyph by glyph.

use neotime_ui::{geometry::Color, renderer::Renderer};

use crate::fonts::{character_advance, line_height, text_width, FontName};

// ============================================================================
// Wrapping
// ============================================================================

/// Splits text into lines no wider than `width`, breaking at spaces and at line breaks.
///
/// A word wider than the whole line is broken between characters.
pub struct WrappedLines<'text> {
    rest: &'text str,
    font: FontName,
    width: i32,
}

impl<'text> WrappedLines<'text> {
    pub fn new(text: &'text str, font: FontName, width: i32) -> Self {
        WrappedLines {
            rest: text,
            font,
            width: width.max(1),
        }
    }
}

impl<'text> Iterator for WrappedLines<'text> {
    type Item = &'text str;

    fn next(&mut self) -> Option<&'text str> {
        if self.rest.is_empty() {
            return None;
        }
        let metrics = self.font.metrics();
        let mut used = 0;
        let mut last_space: Option<usize> = None;
        let mut line_end = self.rest.len();
        let mut next_start = self.rest.len();
        let mut is_line_break = false;

        for (position, character) in self.rest.char_indices() {
            if character == '\n' {
                line_end = position;
                next_start = position + 1;
                is_line_break = true;
                break;
            }
            let advance = character_advance(metrics, character);
            if character == ' ' {
                last_space = Some(position);
            } else if used + advance > self.width && position > 0 {
                match last_space {
                    Some(space) => {
                        line_end = space;
                        next_start = space + 1;
                    }
                    None => {
                        line_end = position;
                        next_start = position;
                    }
                }
                break;
            }
            used += advance;
        }

        let line = self.rest[..line_end].trim_end_matches(' ');
        self.rest = &self.rest[next_start..];
        if !is_line_break {
            self.rest = self.rest.trim_start_matches(' ');
        }
        Some(line)
    }
}

/// How many lines text takes when wrapped to `width`, at least one.
pub fn wrapped_line_count(text: &str, font: FontName, width: i32) -> i32 {
    (WrappedLines::new(text, font, width).count() as i32).max(1)
}

/// The width of the widest line once text is wrapped to `width`.
pub fn wrapped_width(text: &str, font: FontName, width: i32) -> i32 {
    WrappedLines::new(text, font, width)
        .map(|line| text_width(font.metrics(), line))
        .max()
        .unwrap_or(0)
}

// ============================================================================
// Drawing
// ============================================================================

/// Draws one line with its left end at `x` and its letters standing on `baseline`.
pub fn draw_line(
    renderer: &mut dyn Renderer,
    font: FontName,
    line: &str,
    x: i32,
    baseline: i32,
    color: Color,
) {
    // A line height above and below the baseline holds every glyph, so a line outside that is skipped.
    let metrics = font.metrics();
    let reach = line_height(metrics);
    let visible = renderer.visible_area();
    if let Some(visible) = visible {
        if baseline + reach < visible.y || baseline - 2 * reach > visible.bottom() {
            return;
        }
    }
    let mut pen = x;
    for character in line.chars() {
        let advance = character_advance(metrics, character);
        // A glyph's ink stays within a line height of its advance, so one further outside is skipped.
        let is_outside =
            visible.is_some_and(|visible| pen + advance + reach < visible.x || pen - reach > visible.right());
        if !is_outside {
            font.draw_character(renderer, character, (pen, baseline), color);
        }
        pen += advance;
    }
}
