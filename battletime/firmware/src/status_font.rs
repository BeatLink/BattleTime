//! The status page's text in the full firmware: the shared library's text font, in its small size.
//!
//! The recovery firmware has no library, so its status page keeps the bitmap font the page carries.
//! With no valid library the full firmware's status pages show no text; Bluetooth, updates and the
//! button's recovery hold still work.

use battletime_app_host::fonts::{self, FontName};
use battletime_runtime::AppImage;
use battletime_status_screen::page::{self, CELL_BASELINE, CELL_WIDTH};

/// Gives the app host its text font from the shared library, and the status page its text.
// Kept out of line, so the library's parse never adds to the caller's stack frame.
#[inline(never)]
pub fn start(library: Option<&'static [u8]>) {
    if let Some(Ok(library)) = library.map(AppImage::parse) {
        fonts::install_from_library(&library);
    }
    page::use_font(cell_row);
}

/// One row of a character's cell, from its top, with the small text centred in the cell's width.
fn cell_row(character: char, row: i32, ink: &mut [u8; CELL_WIDTH]) {
    FontName::Small.character_row(character, row - CELL_BASELINE, 0, ink);
}
