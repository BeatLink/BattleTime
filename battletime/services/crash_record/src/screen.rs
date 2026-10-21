//! The pages the firmware draws itself when something went wrong, in the status page's font.
//!
//! After a crash the wearer sees what stopped BattleTime and picks Restart or Send debug info; while
//! the debug info waits for the companion app a second page says so. When an app needs more memory
//! than is free, a page says so instead of opening it, and while the verifier checks an app's code
//! after an install, or before its first open, a page without buttons says that. These pages need no app, so they work
//! whatever app broke; their words are in the status page's font, which in the full firmware is the
//! shared library's text font.

use core::fmt::Write;

use battletime_status_screen::page::{draw_centred, TextColours, SCREEN_SIZE, TEXT_HEIGHT};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};

use crate::Crash;

// ============================================================================
// Pages and choices
// ============================================================================

/// What one of the firmware's own pages shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page<'text> {
    /// The previous run crashed, while `app_name` was open, or with an empty name when none was.
    Crash {
        crash: &'text Crash,
        app_name: &'text str,
    },
    /// The crash waits for the companion app to read it.
    WaitingForCompanion,
    /// An app was not opened because it needs more memory than is free.
    NotEnoughMemory {
        app_name: &'text str,
        needed_bytes: u32,
        free_bytes: u32,
    },
    /// The verifier checks `app_name`'s code, just installed or about to open for the first time.
    Checking { app_name: &'text str, installing: bool },
}

/// What the wearer picked on a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    /// Clear the crash and go on to the face.
    Restart,
    /// Keep the crash for the companion app and wait for it.
    SendDebugInfo,
    /// Leave the page, keeping the crash readable for the companion app.
    Dismiss,
}

impl Page<'_> {
    /// The page's buttons, top to bottom, each with its label and what it picks.
    fn buttons(&self) -> &'static [(&'static str, Choice)] {
        match self {
            Page::Crash { .. } => &[
                ("Restart", Choice::Restart),
                ("Send debug info", Choice::SendDebugInfo),
            ],
            Page::WaitingForCompanion => &[("Skip", Choice::Dismiss)],
            Page::NotEnoughMemory { .. } => &[("OK", Choice::Dismiss)],
            Page::Checking { .. } => &[],
        }
    }

    /// What a click of the side button picks: the page's first button, or leaving a page without one.
    pub fn button_press_choice(&self) -> Choice {
        self.buttons()
            .first()
            .map_or(Choice::Dismiss, |(_, choice)| *choice)
    }

    /// What a tap at the point picks, if it lands on a button.
    pub fn choice_at(&self, point: Point) -> Option<Choice> {
        let buttons = self.buttons();
        buttons
            .iter()
            .enumerate()
            .find(|(place, _)| button_area(*place, buttons.len()).contains(point))
            .map(|(_, (_, choice))| *choice)
    }

    fn background(&self) -> Rgb565 {
        match self {
            Page::Crash { .. } | Page::WaitingForCompanion => CRASH_BACKGROUND,
            Page::NotEnoughMemory { .. } | Page::Checking { .. } => NOTICE_BACKGROUND,
        }
    }
}

// ============================================================================
// Sizes and colours
// ============================================================================

const CRASH_BACKGROUND: Rgb565 = Rgb565::new(14, 4, 2);
const NOTICE_BACKGROUND: Rgb565 = Rgb565::new(2, 6, 10);
const BUTTON_COLOUR: Rgb565 = Rgb565::new(8, 16, 8);
const TEXT_COLOUR: Rgb565 = Rgb565::WHITE;

const FIRST_LINE_TOP: i32 = 14;
const LINE_HEIGHT: i32 = 26;
const MOST_LINES: usize = 5;
const SIDE_MARGIN: i32 = 12;
const BUTTON_HEIGHT: i32 = 44;
const BUTTON_GAP: i32 = 8;
const BOTTOM_MARGIN: i32 = 6;

/// Where a page's button sits, the last one at the bottom of the screen.
fn button_area(place: usize, button_count: usize) -> Rectangle {
    let from_bottom = (button_count - place) as i32;
    let top = SCREEN_SIZE as i32 - BOTTOM_MARGIN - from_bottom * (BUTTON_HEIGHT + BUTTON_GAP) + BUTTON_GAP;
    Rectangle::new(
        Point::new(SIDE_MARGIN, top),
        Size::new(SCREEN_SIZE - 2 * SIDE_MARGIN as u32, BUTTON_HEIGHT as u32),
    )
}

// ============================================================================
// Words
// ============================================================================

type Line = heapless::String<32>;

/// The lines of text above the buttons.
fn lines(page: &Page) -> heapless::Vec<Line, MOST_LINES> {
    let mut lines = heapless::Vec::new();
    let mut add = |text: core::fmt::Arguments| {
        let mut line = Line::new();
        let _ = line.write_fmt(text);
        let _ = lines.push(line);
    };
    match page {
        Page::Crash { crash, app_name } => {
            add(format_args!("BattleTime stopped:"));
            add(format_args!("{}", crash.cause()));
            if !app_name.is_empty() {
                add(format_args!("while {app_name}"));
                add(format_args!("was open."));
            }
        }
        Page::WaitingForCompanion => {
            add(format_args!("Connect the"));
            add(format_args!("companion app and"));
            add(format_args!("save a debug report."));
            add(format_args!("Waiting..."));
        }
        Page::NotEnoughMemory {
            app_name,
            needed_bytes,
            free_bytes,
        } => {
            add(format_args!("Not enough memory"));
            add(format_args!("to open {app_name}."));
            add(format_args!(
                "It needs {} KB,",
                kilobytes_rounded_up(*needed_bytes)
            ));
            add(format_args!("{} KB is free.", free_bytes / 1024));
        }
        Page::Checking { app_name, installing } => {
            add(format_args!(""));
            add(format_args!(
                "{}",
                if *installing { "Installing" } else { "Checking" }
            ));
            add(format_args!("{app_name}..."));
        }
    }
    lines
}

fn kilobytes_rounded_up(bytes: u32) -> u32 {
    bytes.div_ceil(1024)
}

// ============================================================================
// Drawing
// ============================================================================

/// Paints the whole screen: the page's lines, then its buttons.
pub fn draw<Target: DrawTarget<Color = Rgb565>>(
    target: &mut Target,
    page: &Page,
) -> Result<(), Target::Error> {
    let background = page.background();
    target.clear(background)?;

    let on_background = TextColours {
        text: TEXT_COLOUR,
        background,
    };
    for (place, line) in lines(page).iter().enumerate() {
        let top = FIRST_LINE_TOP + place as i32 * LINE_HEIGHT;
        draw_centred(target, 0, SCREEN_SIZE as i32, top, line, on_background)?;
    }

    let on_button = TextColours {
        text: TEXT_COLOUR,
        background: BUTTON_COLOUR,
    };
    let buttons = page.buttons();
    for (place, (label, _)) in buttons.iter().enumerate() {
        let area = button_area(place, buttons.len());
        target.fill_solid(&area, BUTTON_COLOUR)?;
        let text_top = area.top_left.y + (BUTTON_HEIGHT - TEXT_HEIGHT as i32) / 2;
        let right = area.top_left.x + area.size.width as i32;
        draw_centred(target, area.top_left.x, right, text_top, label, on_button)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
