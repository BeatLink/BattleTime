//! A text file shown a screen at a time, so a long file costs RAM only for the lines in view.
//!
//! When a TextFile opens, its file is wrapped once, a chunk at a time, keeping where every eighth line
//! starts. Scrolling then reads back only the lines around the view, starting from the nearest of
//! those places.

use alloc::{string::String, vec, vec::Vec};

use crate::{fonts::FontName, text::WrappedLines};

// ============================================================================
// Sizes
// ============================================================================

/// How many wrapped lines lie between the places kept for reading back.
pub const LINES_PER_CHECKPOINT: i32 = 8;

/// How much of the file is read at once; any wrapped line of a 240 pixel screen fits several times over.
const CHUNK_BYTES: usize = 512;

/// How many lines past each edge of the view are kept in RAM, so a short scroll reads nothing.
const LINES_AROUND_VIEW: i32 = LINES_PER_CHECKPOINT;

/// The most bytes read from a font file, which holds only a font's name.
const FONT_FILE_BYTES: usize = 16;

/// Reads part of the file from `offset` into the buffer, as `SystemValues::read_file` does.
pub type ReadPart<'read> = dyn FnMut(u32, &mut [u8]) -> Option<usize> + 'read;

// ============================================================================
// Wrapping the file a chunk at a time
// ============================================================================

/// Calls `visit` with each wrapped line from `start` and the byte where the line after it starts,
/// until `visit` gives false or the file ends.
pub fn for_each_line(
    read: &mut ReadPart,
    start: u32,
    font: FontName,
    width: i32,
    visit: &mut dyn FnMut(&str, u32) -> bool,
) {
    let mut buffer = vec![0u8; CHUNK_BYTES];
    let mut offset = start;
    loop {
        let Some(length) = read(offset, &mut buffer) else {
            return;
        };
        if length == 0 {
            return;
        }
        let at_end = length < buffer.len();
        let text = readable_text(&mut buffer[..length], at_end);
        if text.is_empty() {
            return;
        }
        let mut lines = WrappedLines::new(text, font, width).peekable();
        let mut is_first = true;
        while let Some(line) = lines.next() {
            let next_start = match lines.peek() {
                Some(next) => byte_position(text, next),
                None if at_end || is_first => text.len(),
                // The chunk may have cut this line short, so it is read again with the next chunk.
                None => {
                    offset += byte_position(text, line) as u32;
                    break;
                }
            };
            if !visit(line, offset + next_start as u32) {
                return;
            }
            if lines.peek().is_none() {
                if at_end {
                    return;
                }
                offset += next_start as u32;
            }
            is_first = false;
        }
    }
}

/// Where a line taken from `text` starts in it.
fn byte_position(text: &str, line: &str) -> usize {
    line.as_ptr() as usize - text.as_ptr() as usize
}

/// The chunk as text: tabs and carriage returns become spaces, and bytes that are not UTF-8 become
/// `?`; a character cut off at the chunk's end is left for the next chunk.
fn readable_text(bytes: &mut [u8], at_end: bool) -> &str {
    for byte in bytes.iter_mut() {
        if *byte == b'\t' || *byte == b'\r' {
            *byte = b' ';
        }
    }
    let mut length = bytes.len();
    loop {
        match core::str::from_utf8(&bytes[..length]) {
            Ok(_) => break,
            Err(error) => match error.error_len() {
                Some(_) => bytes[error.valid_up_to()] = b'?',
                None if at_end => bytes[error.valid_up_to()..length].fill(b'?'),
                None => length = error.valid_up_to(),
            },
        }
    }
    core::str::from_utf8(&bytes[..length]).unwrap_or("")
}

// ============================================================================
// The index kept while a TextFile shows
// ============================================================================

/// Where to start reading for each eighth wrapped line, worked out for one width and font.
#[derive(Debug, PartialEq, Eq)]
pub struct LineIndex {
    pub width: i32,
    pub font: FontName,
    pub line_count: i32,
    /// The byte where line `n * LINES_PER_CHECKPOINT` starts.
    checkpoints: Vec<u32>,
}

impl LineIndex {
    /// Wraps the whole file, up to `most_lines` lines.
    pub fn build(read: &mut ReadPart, font: FontName, width: i32, most_lines: i32) -> LineIndex {
        let mut checkpoints = vec![0];
        let mut line_count = 0;
        if most_lines > 0 {
            for_each_line(read, 0, font, width, &mut |_, next_start| {
                line_count += 1;
                if line_count % LINES_PER_CHECKPOINT == 0 {
                    checkpoints.push(next_start);
                }
                line_count < most_lines
            });
        }
        LineIndex {
            width,
            font,
            line_count,
            checkpoints,
        }
    }

    /// The lines from about `first` to `end` with some to spare around them, as the first line's
    /// number and the lines joined by line breaks.
    pub fn read_lines(&self, read: &mut ReadPart, first: i32, end: i32) -> (i32, String) {
        let wanted_first = (first - LINES_AROUND_VIEW).max(0);
        let wanted_end = (end + LINES_AROUND_VIEW).min(self.line_count);
        let checkpoint = ((wanted_first / LINES_PER_CHECKPOINT) as usize).min(self.checkpoints.len() - 1);
        let window_first = checkpoint as i32 * LINES_PER_CHECKPOINT;
        let mut text = String::new();
        let mut line_number = window_first;
        if line_number < wanted_end {
            for_each_line(
                read,
                self.checkpoints[checkpoint],
                self.font,
                self.width,
                &mut |line, _| {
                    if line_number > window_first {
                        text.push('\n');
                    }
                    text.push_str(line);
                    line_number += 1;
                    line_number < wanted_end
                },
            );
        }
        (window_first, text)
    }
}

/// Whether the lines held, from `held_first` for `held_count`, cover the lines in view.
pub fn covers(held_first: i32, held_count: i32, view_first: i32, view_end: i32, line_count: i32) -> bool {
    held_first <= view_first && held_first + held_count >= view_end.min(line_count)
}

// ============================================================================
// The font a phone chose
// ============================================================================

/// The text font a font file names, such as `small` or `large`; none for anything else.
pub fn font_from_file(read: &mut ReadPart) -> Option<FontName> {
    let mut bytes = [0u8; FONT_FILE_BYTES];
    let length = read(0, &mut bytes)?;
    let name = core::str::from_utf8(&bytes[..length]).ok()?.trim();
    Some(match name {
        "small" => FontName::Small,
        "normal" => FontName::Normal,
        "medium" => FontName::Medium,
        "large" => FontName::Large,
        _ => return None,
    })
}

#[cfg(test)]
#[path = "text_file_tests.rs"]
mod text_file_tests;
