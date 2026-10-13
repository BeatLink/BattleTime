//! The screen the shell shows when no watch face is installed: the time and how to install one.
//!
//! It is drawn in the status page's font, so it needs nothing from the shared library.

use embedded_graphics::{
    pixelcolor::{raw::RawU16, Rgb565},
    prelude::*,
};
use neotime_runtime::{Theme, ThemeColor, Value};
use neotime_status_screen::page::{self, TextColours};

use crate::ScreenDisplay;

const MILLISECONDS_PER_MINUTE: Value = 60_000;
const MINUTES_PER_DAY: Value = 24 * 60;
/// Rows of the font's cell above its baseline.
const FONT_BASELINE: i32 = 15;
const TIME_BASELINE: i32 = 70;
const FIRST_GUIDE_BASELINE: i32 = 120;
const GUIDE_LINE_HEIGHT: i32 = 24;

const GUIDE: [&str; 4] = [
    "No watch face",
    "installed. Install",
    "one with the NeoTime",
    "companion app.",
];

/// Paints the whole screen: the time as hours and minutes, then the guide.
pub fn draw(display: &mut ScreenDisplay, now: Value, theme: &Theme) {
    let background = theme_color(theme, ThemeColor::Background);
    let _ = display.clear(background);

    let minute_of_day = now
        .div_euclid(MILLISECONDS_PER_MINUTE)
        .rem_euclid(MINUTES_PER_DAY);
    let mut time = heapless::String::<5>::new();
    let _ = core::fmt::write(
        &mut time,
        format_args!("{:02}:{:02}", minute_of_day / 60, minute_of_day % 60),
    );
    let time_colours = TextColours {
        text: theme_color(theme, ThemeColor::Text),
        background,
    };
    draw_centred(display, &time, TIME_BASELINE, time_colours);
    let guide_colours = TextColours {
        text: theme_color(theme, ThemeColor::Muted),
        background,
    };
    for (line, text) in GUIDE.iter().enumerate() {
        let baseline = FIRST_GUIDE_BASELINE + line as i32 * GUIDE_LINE_HEIGHT;
        draw_centred(display, text, baseline, guide_colours);
    }
}

fn draw_centred(display: &mut ScreenDisplay, text: &str, baseline: i32, colours: TextColours) {
    let screen_width = page::SCREEN_SIZE as i32;
    let _ = page::draw_centred(display, 0, screen_width, baseline - FONT_BASELINE, text, colours);
}

fn theme_color(theme: &Theme, color: ThemeColor) -> Rgb565 {
    Rgb565::from(RawU16::new(theme.color(color)))
}

/// When the time shown next changes: the start of the next minute.
pub fn next_change(now: Value) -> Value {
    (now.div_euclid(MILLISECONDS_PER_MINUTE) + 1) * MILLISECONDS_PER_MINUTE
}
