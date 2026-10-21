//! The screen the shell shows when no watch face is installed: the time and how to install one.
//!
//! It is drawn in the status page's font, the text font's small size, so it needs no app. While
//! that font cannot be read from the external flash, the time is drawn as a clock of bars instead.

use battletime_runtime::{Theme, ThemeColor, Value};
use battletime_status_screen::page::{self, TextColours};
use embedded_graphics::{
    pixelcolor::{raw::RawU16, Rgb565},
    prelude::*,
    primitives::Rectangle,
};

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
    "one with the BattleTime",
    "companion app.",
];

/// Paints the whole screen: the time as hours and minutes, then the guide.
pub fn draw(display: &mut ScreenDisplay, now: Value, theme: &Theme) {
    let background = theme_color(theme, ThemeColor::Background);
    let _ = display.clear(background);

    let minute_of_day = now
        .div_euclid(MILLISECONDS_PER_MINUTE)
        .rem_euclid(MINUTES_PER_DAY);
    if !crate::status_font::has_text() {
        draw_bar_clock(display, minute_of_day, theme_color(theme, ThemeColor::Text));
        return;
    }
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

// ============================================================================
// The clock of bars
// ============================================================================

/// Which of a digit's seven bars are lit, top, top right, bottom right, bottom, bottom left, top left, middle.
const DIGIT_BARS: [u8; 10] = [
    0b0111111, 0b0000110, 0b1011011, 0b1001111, 0b1100110, 0b1101101, 0b1111101, 0b0000111, 0b1111111,
    0b1101111,
];
const DIGIT_WIDTH: i32 = 36;
const DIGIT_HEIGHT: i32 = 64;
const BAR: i32 = 8;
const DIGIT_GAP: i32 = 12;
const COLON_GAP: i32 = 24;
const CLOCK_TOP: i32 = 88;

/// The time as four seven-bar digits, for a watch whose font cannot be read.
fn draw_bar_clock(display: &mut ScreenDisplay, minute_of_day: Value, colour: Rgb565) {
    let digits = [
        minute_of_day / 600,
        minute_of_day / 60 % 10,
        minute_of_day % 60 / 10,
        minute_of_day % 10,
    ];
    let width = 4 * DIGIT_WIDTH + 3 * DIGIT_GAP + COLON_GAP;
    let mut left = (page::SCREEN_SIZE as i32 - width) / 2;
    for (place, digit) in digits.into_iter().enumerate() {
        draw_digit(display, left, DIGIT_BARS[digit as usize], colour);
        left += DIGIT_WIDTH
            + if place == 1 {
                COLON_GAP + DIGIT_GAP
            } else {
                DIGIT_GAP
            };
    }
    let colon_left = (page::SCREEN_SIZE as i32 - BAR) / 2;
    for top in [
        CLOCK_TOP + DIGIT_HEIGHT / 3,
        CLOCK_TOP + 2 * DIGIT_HEIGHT / 3 - BAR,
    ] {
        fill(display, colon_left, top, BAR, BAR, colour);
    }
}

fn draw_digit(display: &mut ScreenDisplay, left: i32, bars: u8, colour: Rgb565) {
    let half = DIGIT_HEIGHT / 2;
    let right = left + DIGIT_WIDTH - BAR;
    let places = [
        (left, CLOCK_TOP, DIGIT_WIDTH, BAR),
        (right, CLOCK_TOP, BAR, half),
        (right, CLOCK_TOP + half, BAR, half),
        (left, CLOCK_TOP + DIGIT_HEIGHT - BAR, DIGIT_WIDTH, BAR),
        (left, CLOCK_TOP + half, BAR, half),
        (left, CLOCK_TOP, BAR, half),
        (left, CLOCK_TOP + half - BAR / 2, DIGIT_WIDTH, BAR),
    ];
    for (bar, (x, y, width, height)) in places.into_iter().enumerate() {
        if bars & (1 << bar) != 0 {
            fill(display, x, y, width, height, colour);
        }
    }
}

fn fill(display: &mut ScreenDisplay, left: i32, top: i32, width: i32, height: i32, colour: Rgb565) {
    let area = Rectangle::new(Point::new(left, top), Size::new(width as u32, height as u32));
    let _ = display.fill_solid(&area, colour);
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
