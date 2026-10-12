//! Tests that the simulated panel follows each ST7789 command the way the datasheet describes.

use embedded_graphics::{pixelcolor::Rgb565, prelude::RgbColor};
use mipidsi::interface::Interface;

use super::*;

// ============================================================================
// Helpers
// ============================================================================

const RED_RAW: u16 = 0xF800;
const DARK_RED_RAW: u16 = 0x7800;
const BRIGHT_ORANGE_RAW: u16 = 0xFC00;

/// A panel that is awake, switched on and inverting colours, as the driver leaves it.
fn awake_panel() -> SimulatedPanel {
    let mut panel = SimulatedPanel::new();
    panel.send_command(SLEEP_OUT, &[]).unwrap();
    panel.send_command(DISPLAY_ON, &[]).unwrap();
    panel.send_command(INVERSION_ON, &[]).unwrap();
    panel
}

/// Sets the address window to the given inclusive columns and rows and starts a memory write.
fn start_writing(panel: &mut SimulatedPanel, columns: (u16, u16), rows: (u16, u16)) {
    let [first_column_high, first_column_low] = columns.0.to_be_bytes();
    let [last_column_high, last_column_low] = columns.1.to_be_bytes();
    let [first_row_high, first_row_low] = rows.0.to_be_bytes();
    let [last_row_high, last_row_low] = rows.1.to_be_bytes();
    panel
        .send_command(
            SET_COLUMN_ADDRESS,
            &[
                first_column_high,
                first_column_low,
                last_column_high,
                last_column_low,
            ],
        )
        .unwrap();
    panel
        .send_command(
            SET_ROW_ADDRESS,
            &[first_row_high, first_row_low, last_row_high, last_row_low],
        )
        .unwrap();
    panel.send_command(WRITE_MEMORY_START, &[]).unwrap();
}

fn send_raw_pixels(panel: &mut SimulatedPanel, raw_colours: &[u16]) {
    panel
        .send_pixels(raw_colours.iter().map(|raw_colour| raw_colour.to_be_bytes()))
        .unwrap();
}

// ============================================================================
// Power and display state
// ============================================================================

#[test]
fn a_new_panel_shows_black_because_it_is_asleep() {
    let mut panel = SimulatedPanel::new();
    panel.send_command(DISPLAY_ON, &[]).unwrap();

    start_writing(&mut panel, (0, 0), (0, 0));
    send_raw_pixels(&mut panel, &[0x1234]);

    assert_eq!(panel.shown_pixel(0, 0), Rgb565::BLACK);
}

#[test]
fn sleeping_again_hides_what_memory_holds() {
    let mut panel = awake_panel();
    start_writing(&mut panel, (0, 0), (0, 0));
    send_raw_pixels(&mut panel, &[RED_RAW]);

    panel.send_command(SLEEP_IN, &[]).unwrap();

    assert_eq!(panel.shown_pixel(0, 0), Rgb565::BLACK);
}

#[test]
fn turning_the_display_off_shows_black_and_on_again_restores_it() {
    let mut panel = awake_panel();
    start_writing(&mut panel, (0, 0), (0, 0));
    send_raw_pixels(&mut panel, &[RED_RAW]);

    panel.send_command(DISPLAY_OFF, &[]).unwrap();
    let shown_while_off = panel.shown_pixel(0, 0);
    panel.send_command(DISPLAY_ON, &[]).unwrap();

    assert_eq!(shown_while_off, Rgb565::BLACK);
    assert_eq!(panel.shown_pixel(0, 0), Rgb565::RED);
}

// ============================================================================
// Colours
// ============================================================================

#[test]
fn without_inversion_the_glass_shows_the_opposite_colour() {
    let mut panel = awake_panel();
    start_writing(&mut panel, (0, 0), (0, 0));
    send_raw_pixels(&mut panel, &[RED_RAW]);

    panel.send_command(INVERSION_OFF, &[]).unwrap();

    assert_eq!(panel.shown_pixel(0, 0), Rgb565::CYAN);
}

#[test]
fn idle_mode_rounds_each_channel_to_fully_on_or_off() {
    let mut panel = awake_panel();
    start_writing(&mut panel, (0, 1), (0, 0));
    send_raw_pixels(&mut panel, &[DARK_RED_RAW, BRIGHT_ORANGE_RAW]);

    panel.send_command(IDLE_MODE_ON, &[]).unwrap();

    assert_eq!(panel.shown_pixel(0, 0), Rgb565::BLACK);
    assert_eq!(panel.shown_pixel(1, 0), Rgb565::YELLOW);
}

#[test]
fn leaving_idle_mode_brings_back_full_colour() {
    let mut panel = awake_panel();
    start_writing(&mut panel, (0, 0), (0, 0));
    send_raw_pixels(&mut panel, &[DARK_RED_RAW]);

    panel.send_command(IDLE_MODE_ON, &[]).unwrap();
    panel.send_command(IDLE_MODE_OFF, &[]).unwrap();

    assert_eq!(panel.shown_pixel(0, 0), Rgb565::new(15, 0, 0));
}

// ============================================================================
// Writing memory
// ============================================================================

#[test]
fn pixels_fill_the_window_row_by_row() {
    let mut panel = awake_panel();

    start_writing(&mut panel, (5, 6), (10, 11));
    send_raw_pixels(&mut panel, &[RED_RAW, 0xFFFF, 0x07E0, 0x001F]);

    assert_eq!(panel.shown_pixel(5, 10), Rgb565::RED);
    assert_eq!(panel.shown_pixel(6, 10), Rgb565::WHITE);
    assert_eq!(panel.shown_pixel(5, 11), Rgb565::GREEN);
    assert_eq!(panel.shown_pixel(6, 11), Rgb565::BLUE);
}

#[test]
fn pixels_past_the_end_of_the_window_are_dropped() {
    let mut panel = awake_panel();

    start_writing(&mut panel, (0, 0), (0, 0));
    send_raw_pixels(&mut panel, &[RED_RAW, 0xFFFF, 0xFFFF]);

    assert_eq!(panel.shown_pixel(0, 0), Rgb565::RED);
    assert_eq!(panel.shown_pixel(0, 1), Rgb565::BLACK);
    assert_eq!(panel.shown_pixel(1, 0), Rgb565::BLACK);
}

#[test]
fn a_repeated_pixel_fills_the_whole_window() {
    let mut panel = awake_panel();

    start_writing(&mut panel, (2, 4), (3, 4));
    panel.send_repeated_pixel(RED_RAW.to_be_bytes(), 6).unwrap();

    let mut red_pixels = 0;
    panel.for_each_shown_pixel(|_column, _screen_row, colour| {
        if colour == Rgb565::RED {
            red_pixels += 1;
        }
    });
    assert_eq!(red_pixels, 6);
    assert_eq!(panel.shown_pixel(4, 4), Rgb565::RED);
}

#[test]
fn a_command_missing_its_arguments_reads_them_as_zero() {
    let mut panel = awake_panel();

    panel.send_command(SET_COLUMN_ADDRESS, &[0, 7]).unwrap();
    panel.send_command(SET_ROW_ADDRESS, &[0, 9, 0, 9]).unwrap();
    panel.send_command(WRITE_MEMORY_START, &[]).unwrap();
    send_raw_pixels(&mut panel, &[RED_RAW, 0xFFFF]);

    assert_eq!(panel.shown_pixel(7, 9), Rgb565::RED);
    assert_eq!(panel.shown_pixel(0, 10), Rgb565::BLACK);
    assert_eq!(panel.shown_pixel(8, 9), Rgb565::BLACK);
}

#[test]
fn an_unknown_command_changes_nothing() {
    let mut panel = awake_panel();
    start_writing(&mut panel, (0, 0), (0, 0));
    send_raw_pixels(&mut panel, &[RED_RAW]);

    panel.send_command(0xFE, &[1, 2, 3, 4]).unwrap();

    assert_eq!(panel.shown_pixel(0, 0), Rgb565::RED);
}

// ============================================================================
// Scrolling
// ============================================================================

#[test]
fn fixed_rows_outside_the_scroll_area_do_not_move() {
    let mut panel = awake_panel();
    start_writing(&mut panel, (0, 0), (0, 0));
    send_raw_pixels(&mut panel, &[RED_RAW]);

    panel.send_command(SET_SCROLL_AREA, &[0, 1, 0, 100]).unwrap();
    panel.send_command(SET_SCROLL_START, &[0, 50]).unwrap();

    assert_eq!(panel.shown_pixel(0, 0), Rgb565::RED);
}

#[test]
fn the_scroll_start_row_shows_first_and_the_area_wraps_round() {
    let mut panel = awake_panel();
    start_writing(&mut panel, (0, 0), (5, 5));
    send_raw_pixels(&mut panel, &[RED_RAW]);
    start_writing(&mut panel, (0, 0), (1, 1));
    send_raw_pixels(&mut panel, &[0x07E0]);

    panel.send_command(SET_SCROLL_AREA, &[0, 1, 0, 10]).unwrap();
    panel.send_command(SET_SCROLL_START, &[0, 5]).unwrap();

    assert_eq!(panel.shown_pixel(0, 1), Rgb565::RED);
    assert_eq!(panel.shown_pixel(0, 7), Rgb565::GREEN);
    assert_eq!(panel.shown_pixel(0, 5), Rgb565::BLACK);
}
