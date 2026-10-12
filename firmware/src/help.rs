//! The screen the shell shows when no watch face is installed: the time and how to install one.
//!
//! It is drawn with the firmware's own small font, so it needs nothing from the shared library.

use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::{raw::RawU16, Rgb565},
    prelude::*,
    text::{Alignment, Text},
};
use neotime_runtime::{Theme, ThemeColor, Value};

use crate::ScreenDisplay;

const MILLISECONDS_PER_MINUTE: Value = 60_000;
const MINUTES_PER_DAY: Value = 24 * 60;
const SCREEN_CENTRE_X: i32 = 120;
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
    let _ = display.clear(theme_color(theme, ThemeColor::Background));

    let minute_of_day = now
        .div_euclid(MILLISECONDS_PER_MINUTE)
        .rem_euclid(MINUTES_PER_DAY);
    let mut time = heapless::String::<5>::new();
    let _ = core::fmt::write(
        &mut time,
        format_args!("{:02}:{:02}", minute_of_day / 60, minute_of_day % 60),
    );
    draw_centred(
        display,
        &time,
        TIME_BASELINE,
        theme_color(theme, ThemeColor::Text),
    );
    for (line, text) in GUIDE.iter().enumerate() {
        let baseline = FIRST_GUIDE_BASELINE + line as i32 * GUIDE_LINE_HEIGHT;
        draw_centred(display, text, baseline, theme_color(theme, ThemeColor::Muted));
    }
}

fn draw_centred(display: &mut ScreenDisplay, text: &str, baseline: i32, color: Rgb565) {
    let style = MonoTextStyle::new(&FONT_10X20, color);
    let _ = Text::with_alignment(
        text,
        Point::new(SCREEN_CENTRE_X, baseline),
        style,
        Alignment::Center,
    )
    .draw(display);
}

fn theme_color(theme: &Theme, color: ThemeColor) -> Rgb565 {
    Rgb565::from(RawU16::new(theme.color(color)))
}

/// When the time shown next changes: the start of the next minute.
pub fn next_change(now: Value) -> Value {
    (now.div_euclid(MILLISECONDS_PER_MINUTE) + 1) * MILLISECONDS_PER_MINUTE
}
