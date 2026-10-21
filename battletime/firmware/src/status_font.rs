//! The text of the full firmware's own pages: the text font read from the external flash, in its small size.
//!
//! The firmware carries no font. Once the store is up, `start` has the app host read the text font
//! from `/system/font` a glyph at a time, and every firmware page draws through `draw_with_glyphs`,
//! which reads what a drawing missed and draws again. While the font cannot be read, as before the
//! store mounts or with the file missing, the pages draw without text: the status page as coloured
//! steps and a progress bar, the help screen as a clock of bars. The recovery firmware has no shell,
//! so its status page keeps the bitmap font the page carries.

use battletime_app_host::font_file::{self, TEXT_FONT_PATH};
use battletime_app_host::fonts::FontName;
use battletime_status_screen::page::{self, CELL_BASELINE, CELL_WIDTH};
use core::sync::atomic::{AtomicPtr, Ordering};

use crate::storage::Files;

/// The files the font is read through, once `start` has them.
static FILES: AtomicPtr<Files> = AtomicPtr::new(core::ptr::null_mut());

/// Gives the status page its text, read through the store from now on.
#[inline(never)]
pub fn start(files: &'static Files) {
    FILES.store(files as *const Files as *mut Files, Ordering::Release);
    page::use_font(cell_row);
    font_file::read_text_font_with(read_font_file);
}

fn read_font_file(offset: u32, out: &mut [u8]) -> Option<usize> {
    // Only `start` stores it, from a `'static` reference.
    let files = unsafe { FILES.load(Ordering::Acquire).as_ref()? };
    files.read_part_now(TEXT_FONT_PATH, offset, out)
}

/// Draws, then, if the drawing met glyphs not yet read, reads them and draws again.
pub fn draw_with_glyphs(mut draw: impl FnMut()) {
    draw();
    for _ in 0..font_file::MOST_PASSES {
        if !font_file::load_missing_glyphs() {
            break;
        }
        draw();
    }
}

/// Whether the pages can draw text now.
pub fn has_text() -> bool {
    font_file::text_font_readable()
}

/// One row of a character's cell, from its top, with the small text centred in the cell's width.
fn cell_row(character: char, row: i32, ink: &mut [u8; CELL_WIDTH]) {
    FontName::Small.character_row(character, row - CELL_BASELINE, 0, ink);
}
