//! Drawing apps onto the PineTime display driver over a model of the ST7789, to check that moving
//! rows in the panel, for scrolled lists and slides, shows exactly what drawing afresh would.

use std::{cell::RefCell, rc::Rc};

use embedded_graphics::pixelcolor::{raw::RawU16, Rgb565, RgbColor};
use embedded_hal::{delay::DelayNs, digital::ErrorType, digital::OutputPin};
use mipidsi::interface::{Interface, InterfaceKind};
use neotime_app_host::{PAINT_STEP_PIXELS, STRIP_PIXELS, TRANSITION_TIME_IN_MILLISECONDS};
use neotime_app_testing::{Framebuffer, TestWatch, Touch, Transition};
use neotime_display::Display;

// ============================================================================
// A model of the panel
// ============================================================================

const SET_COLUMNS: u8 = 0x2A;
const SET_ROWS: u8 = 0x2B;
const WRITE_MEMORY: u8 = 0x2C;
const SET_SCROLL_AREA: u8 = 0x33;
const SET_SCROLL_START: u8 = 0x37;

const SIDE: usize = 240;
const MEMORY_ROWS: usize = 320;

/// The ST7789's memory, its write window and its vertical scrolling registers.
struct PanelState {
    memory: Vec<u16>,
    first_column: usize,
    last_column: usize,
    first_row: usize,
    cursor_column: usize,
    cursor_row: usize,
    top_fixed_rows: usize,
    scrolling_rows: usize,
    scroll_start: usize,
    pixels_written: usize,
}

impl PanelState {
    fn new() -> Self {
        Self {
            memory: vec![0; SIDE * MEMORY_ROWS],
            first_column: 0,
            last_column: SIDE - 1,
            first_row: 0,
            cursor_column: 0,
            cursor_row: 0,
            top_fixed_rows: 0,
            scrolling_rows: MEMORY_ROWS,
            scroll_start: 0,
            pixels_written: 0,
        }
    }

    fn write_pixel(&mut self, bytes: [u8; 2]) {
        if let Some(cell) = self.memory.get_mut(self.cursor_row * SIDE + self.cursor_column) {
            *cell = u16::from_be_bytes(bytes);
        }
        self.pixels_written += 1;
        if self.cursor_column == self.last_column {
            self.cursor_column = self.first_column;
            self.cursor_row += 1;
        } else {
            self.cursor_column += 1;
        }
    }

    /// The memory row the panel shows on a screen row, as the datasheet's scrolling describes.
    fn memory_row_on_screen_row(&self, screen_row: usize) -> usize {
        let end_of_scrolling_rows = self.top_fixed_rows + self.scrolling_rows;
        if screen_row < self.top_fixed_rows || screen_row >= end_of_scrolling_rows {
            return screen_row;
        }
        let turned = self.scroll_start - self.top_fixed_rows;
        self.top_fixed_rows + (screen_row - self.top_fixed_rows + turned) % self.scrolling_rows
    }

    fn shown_pixel(&self, x: usize, y: usize) -> Rgb565 {
        let raw = self.memory[self.memory_row_on_screen_row(y) * SIDE + x];
        Rgb565::from(RawU16::new(raw))
    }
}

fn two_numbers(arguments: &[u8]) -> (usize, usize) {
    (
        u16::from_be_bytes([arguments[0], arguments[1]]) as usize,
        u16::from_be_bytes([arguments[2], arguments[3]]) as usize,
    )
}

struct PanelInterface {
    state: Rc<RefCell<PanelState>>,
}

impl Interface for PanelInterface {
    type Word = u8;
    type Error = ();
    const KIND: InterfaceKind = InterfaceKind::Serial4Line;

    fn send_command(&mut self, command: u8, arguments: &[u8]) -> Result<(), ()> {
        let mut state = self.state.borrow_mut();
        match command {
            SET_COLUMNS => (state.first_column, state.last_column) = two_numbers(arguments),
            SET_ROWS => (state.first_row, _) = two_numbers(arguments),
            WRITE_MEMORY => (state.cursor_column, state.cursor_row) = (state.first_column, state.first_row),
            SET_SCROLL_AREA => (state.top_fixed_rows, state.scrolling_rows) = two_numbers(arguments),
            SET_SCROLL_START => state.scroll_start = two_numbers(&[arguments[0], arguments[1], 0, 0]).0,
            _ => {}
        }
        Ok(())
    }

    fn send_pixels<const N: usize>(&mut self, pixels: impl IntoIterator<Item = [u8; N]>) -> Result<(), ()> {
        let mut state = self.state.borrow_mut();
        for pixel in pixels {
            state.write_pixel([pixel[0], pixel[1]]);
        }
        Ok(())
    }

    fn send_repeated_pixel<const N: usize>(&mut self, pixel: [u8; N], count: u32) -> Result<(), ()> {
        let mut state = self.state.borrow_mut();
        for _ in 0..count {
            state.write_pixel([pixel[0], pixel[1]]);
        }
        Ok(())
    }
}

struct FakePin;

impl ErrorType for FakePin {
    type Error = core::convert::Infallible;
}

impl OutputPin for FakePin {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

struct FakeDelay;

impl DelayNs for FakeDelay {
    fn delay_ns(&mut self, _: u32) {}
}

// ============================================================================
// Helpers
// ============================================================================

/// The display, over the model, with the strip the watch draws through.
struct Watch {
    display: Display<PanelInterface, FakePin>,
    panel: Rc<RefCell<PanelState>>,
    strip: Vec<Rgb565>,
}

impl Watch {
    fn new() -> Self {
        let panel = Rc::new(RefCell::new(PanelState::new()));
        let interface = PanelInterface { state: panel.clone() };
        let display = Display::new(interface, FakePin, &mut FakeDelay).unwrap();
        Self {
            display,
            panel,
            strip: vec![Rgb565::BLACK; STRIP_PIXELS],
        }
    }

    /// Draws one step and says how many pixels it sent to the panel.
    fn draw_step(&mut self, app: &mut TestWatch) -> usize {
        let before = self.panel.borrow().pixels_written;
        app.host_mut()
            .draw_step(&mut self.display, &mut self.strip)
            .unwrap();
        self.panel.borrow().pixels_written - before
    }

    /// Draws one step paced by the clock at `now_in_milliseconds`, and says how many pixels it sent.
    fn draw_step_at(&mut self, app: &mut TestWatch, now_in_milliseconds: u64) -> usize {
        let before = self.panel.borrow().pixels_written;
        app.host_mut()
            .draw_step_at(&mut self.display, &mut self.strip, now_in_milliseconds)
            .unwrap();
        self.panel.borrow().pixels_written - before
    }

    /// Draws until nothing is left, and says how many steps that took.
    fn draw_everything(&mut self, app: &mut TestWatch) -> usize {
        let mut steps = 0;
        while app.host().needs_drawing() {
            self.draw_step(app);
            steps += 1;
        }
        steps
    }

    fn shown_row(&self, y: usize) -> Vec<Rgb565> {
        let panel = self.panel.borrow();
        (0..SIDE).map(|x| panel.shown_pixel(x, y)).collect()
    }

    fn shown_column(&self, x: usize) -> Vec<Rgb565> {
        let panel = self.panel.borrow();
        (0..SIDE).map(|y| panel.shown_pixel(x, y)).collect()
    }

    fn pixels_unlike(&self, frame: &Framebuffer) -> usize {
        let panel = self.panel.borrow();
        (0..SIDE)
            .flat_map(|y| (0..SIDE).map(move |x| (x, y)))
            .filter(|&(x, y)| panel.shown_pixel(x, y) != frame.pixel(x, y))
            .count()
    }

    fn turned_rows(&self) -> usize {
        let panel = self.panel.borrow();
        panel.scroll_start - panel.top_fixed_rows
    }

    fn scrolling_band(&self) -> (usize, usize) {
        let panel = self.panel.borrow();
        (panel.top_fixed_rows, panel.top_fixed_rows + panel.scrolling_rows)
    }
}

fn frame_row(frame: &Framebuffer, y: usize) -> Vec<Rgb565> {
    (0..SIDE).map(|x| frame.pixel(x, y)).collect()
}

fn frame_column(frame: &Framebuffer, x: usize) -> Vec<Rgb565> {
    (0..SIDE).map(|y| frame.pixel(x, y)).collect()
}

/// Drags a finger from one row to another a few pixels at a time, drawing after each move.
fn drag_drawing_each_move(app: &mut TestWatch, watch: &mut Watch, from_y: i32, to_y: i32) -> Vec<usize> {
    let x = 120;
    let step: i32 = if to_y > from_y { 6 } else { -6 };
    let mut sent = Vec::new();
    app.touch(Touch::Press { x, y: from_y });
    let mut y = from_y;
    while (to_y - y).abs() >= step.abs() {
        y += step;
        app.touch(Touch::Drag {
            x,
            y,
            start_x: x,
            start_y: from_y,
        });
        let before = watch.panel.borrow().pixels_written;
        watch.draw_everything(app);
        sent.push(watch.panel.borrow().pixels_written - before);
        assert_eq!(
            watch.pixels_unlike(&app.fresh_frame()),
            0,
            "after the drag reached row {y}"
        );
    }
    app.touch(Touch::Release { x, y });
    sent
}

fn launcher_with_many_apps() -> TestWatch {
    let mut app = TestWatch::open_app("launcher").unwrap();
    let apps = [
        "stopwatch",
        "timer",
        "alarm",
        "settings",
        "dice",
        "heart",
        "music",
        "snake",
        "paddle",
        "twos",
    ];
    app.install_apps(&apps).unwrap();
    app
}

// ============================================================================
// Scrolling lists
// ============================================================================

#[test]
fn the_settings_list_scrolls_with_the_panel_and_shows_what_drawing_afresh_would() {
    let mut app = TestWatch::open_app("settings").unwrap();
    let mut watch = Watch::new();
    watch.draw_everything(&mut app);

    let mut sent_going_down = drag_drawing_each_move(&mut app, &mut watch, 220, 50);
    sent_going_down.extend(drag_drawing_each_move(&mut app, &mut watch, 220, 50));
    sent_going_down.extend(drag_drawing_each_move(&mut app, &mut watch, 220, 50));
    assert!(app.shows_text("Updates"), "{:?}", app.visible_texts());
    let sent_going_up = drag_drawing_each_move(&mut app, &mut watch, 60, 160);

    // Only the band of the list scrolls, and each move draws a handful of rows rather than the list.
    assert_eq!(watch.scrolling_band(), (42, 232));
    let most_sent = sent_going_down
        .iter()
        .chain(&sent_going_up)
        .max()
        .copied()
        .unwrap();
    assert!(most_sent <= 6 * SIDE, "a move sent {most_sent} pixels");
}

#[test]
fn the_launcher_scrolls_the_whole_screen_with_the_panel() {
    let mut app = launcher_with_many_apps();
    let mut watch = Watch::new();
    watch.draw_everything(&mut app);

    let sent = drag_drawing_each_move(&mut app, &mut watch, 220, 20);
    drag_drawing_each_move(&mut app, &mut watch, 20, 200);

    assert_eq!(watch.scrolling_band(), (0, SIDE));
    assert!(sent.iter().all(|&pixels| pixels <= 6 * SIDE), "{sent:?}");
}

#[test]
fn a_slide_after_a_lift_of_the_finger_does_not_scroll_twice() {
    let mut app = TestWatch::open_app("settings").unwrap();
    app.touch(Touch::Press { x: 120, y: 200 });
    app.touch(Touch::Drag {
        x: 120,
        y: 180,
        start_x: 120,
        start_y: 200,
    });
    let first_place = app.visible_texts();

    app.touch(Touch::Swipe(neotime_app_testing::Direction::Up));
    app.touch(Touch::Drag {
        x: 120,
        y: 180,
        start_x: 120,
        start_y: 200,
    });

    assert_eq!(app.visible_texts(), first_place);
}

// ============================================================================
// Painting in slices
// ============================================================================

#[test]
fn a_whole_screen_drawn_in_place_comes_a_slice_per_step() {
    let mut app = TestWatch::open_app("settings").unwrap();
    let mut watch = Watch::new();

    let mut sent = Vec::new();
    while app.host().needs_drawing() {
        sent.push(watch.draw_step(&mut app));
    }

    assert_eq!(sent.len(), SIDE * SIDE / PAINT_STEP_PIXELS as usize);
    assert!(
        sent.iter().all(|&pixels| pixels <= PAINT_STEP_PIXELS as usize),
        "{sent:?}"
    );
    assert_eq!(watch.pixels_unlike(&app.fresh_frame()), 0);
}

#[test]
fn a_scroll_begun_while_slices_are_left_waits_for_them_and_ends_exact() {
    let mut app = launcher_with_many_apps();
    let mut watch = Watch::new();
    watch.draw_step(&mut app);

    drag_drawing_each_move(&mut app, &mut watch, 220, 100);

    assert_eq!(watch.pixels_unlike(&app.fresh_frame()), 0);
}

#[test]
fn a_change_while_slices_are_left_is_drawn_once_they_are_done() {
    let mut app = TestWatch::open_app("settings").unwrap();
    let mut watch = Watch::new();
    watch.draw_everything(&mut app);
    let theme = *app.host().theme();
    app.host_mut().set_theme(theme);
    watch.draw_step(&mut app);

    drag_drawing_each_move(&mut app, &mut watch, 220, 120);

    assert_eq!(watch.pixels_unlike(&app.fresh_frame()), 0);
}

// ============================================================================
// Transitions
// ============================================================================

#[test]
fn a_new_screen_slides_up_over_the_old_one_and_leaves_the_panel_unturned() {
    let mut launcher = launcher_with_many_apps();
    let mut watch = Watch::new();
    watch.draw_everything(&mut launcher);
    let old = launcher.fresh_frame();
    let mut settings = TestWatch::open_app("settings").unwrap();
    let new = settings.fresh_frame();

    settings.host_mut().set_next_transition(Transition::Up);
    let theme = *settings.host().theme();
    settings.host_mut().set_theme(theme);
    for _ in 0..10 {
        watch.draw_step(&mut settings);
    }

    let moved_rows = 80;
    assert_eq!(watch.shown_row(0), frame_row(&old, moved_rows));
    assert_eq!(watch.shown_row(SIDE - moved_rows - 1), frame_row(&old, SIDE - 1));
    assert_eq!(watch.shown_row(SIDE - moved_rows), frame_row(&new, 0));
    assert_eq!(watch.shown_row(SIDE - 1), frame_row(&new, moved_rows - 1));
    watch.draw_everything(&mut settings);
    assert_eq!(watch.pixels_unlike(&new), 0);
    assert_eq!(watch.turned_rows(), 0);
}

#[test]
fn a_new_screen_slides_down_over_the_old_one() {
    let mut settings = TestWatch::open_app("settings").unwrap();
    let mut watch = Watch::new();
    watch.draw_everything(&mut settings);
    let old = settings.fresh_frame();
    let mut launcher = launcher_with_many_apps();
    let new = launcher.fresh_frame();

    launcher.host_mut().set_next_transition(Transition::Down);
    let theme = *launcher.host().theme();
    launcher.host_mut().set_theme(theme);
    for _ in 0..5 {
        watch.draw_step(&mut launcher);
    }

    let moved_rows = 40;
    assert_eq!(watch.shown_row(0), frame_row(&new, SIDE - moved_rows));
    assert_eq!(watch.shown_row(moved_rows), frame_row(&old, 0));
    let steps_left = watch.draw_everything(&mut launcher);
    assert_eq!(steps_left, 25);
    assert_eq!(watch.pixels_unlike(&new), 0);
}

#[test]
fn a_slide_paced_by_the_clock_catches_up_after_a_late_step() {
    let mut launcher = launcher_with_many_apps();
    let mut watch = Watch::new();
    watch.draw_everything(&mut launcher);
    let old = launcher.fresh_frame();
    let mut settings = TestWatch::open_app("settings").unwrap();
    let new = settings.fresh_frame();
    settings.host_mut().set_next_transition(Transition::Up);
    let theme = *settings.host().theme();
    settings.host_mut().set_theme(theme);

    let started_at = 1_000;
    watch.draw_step_at(&mut settings, started_at);
    watch.draw_step_at(&mut settings, started_at + TRANSITION_TIME_IN_MILLISECONDS / 2);

    let moved_rows = SIDE / 2;
    assert_eq!(watch.shown_row(0), frame_row(&old, moved_rows));
    assert_eq!(watch.shown_row(SIDE - moved_rows), frame_row(&new, 0));
    assert_eq!(watch.shown_row(SIDE - 1), frame_row(&new, moved_rows - 1));
    watch.draw_step_at(&mut settings, started_at + TRANSITION_TIME_IN_MILLISECONDS);
    assert!(!settings.host().is_in_transition());
    assert_eq!(watch.pixels_unlike(&new), 0);
    assert_eq!(watch.turned_rows(), 0);
}

#[test]
fn a_transition_paced_by_the_clock_still_moves_when_steps_come_quickly() {
    let mut settings = TestWatch::open_app("settings").unwrap();
    let mut watch = Watch::new();
    watch.draw_everything(&mut settings);
    settings.tap_text("Brightness");

    let mut steps = 0;
    while settings.host().needs_drawing() {
        watch.draw_step_at(&mut settings, 5_000);
        steps += 1;
    }

    assert_eq!(steps, 15);
    assert_eq!(watch.pixels_unlike(&settings.fresh_frame()), 0);
}

#[test]
fn a_wipe_that_falls_far_behind_the_clock_finishes_in_one_step() {
    let mut settings = TestWatch::open_app("settings").unwrap();
    let mut watch = Watch::new();
    watch.draw_everything(&mut settings);
    settings.tap_text("Brightness");

    watch.draw_step_at(&mut settings, 0);
    watch.draw_step_at(&mut settings, 10 * TRANSITION_TIME_IN_MILLISECONDS);

    assert!(!settings.host().needs_drawing());
    assert_eq!(watch.pixels_unlike(&settings.fresh_frame()), 0);
}

#[test]
fn going_to_another_screen_wipes_it_in_from_the_right_and_back_from_the_left() {
    let mut settings = TestWatch::open_app("settings").unwrap();
    let mut watch = Watch::new();
    watch.draw_everything(&mut settings);
    let list = settings.fresh_frame();

    settings.tap_text("Brightness");
    watch.draw_step(&mut settings);
    watch.draw_step(&mut settings);
    let brightness = {
        let mut copy = TestWatch::open_app("settings").unwrap();
        copy.tap_text("Brightness");
        copy.fresh_frame()
    };
    assert_eq!(watch.shown_column(SIDE - 1), frame_column(&brightness, SIDE - 1));
    assert_eq!(
        watch.shown_column(SIDE - 32),
        frame_column(&brightness, SIDE - 32)
    );
    assert_eq!(watch.shown_column(SIDE - 33), frame_column(&list, SIDE - 33));
    watch.draw_everything(&mut settings);
    assert_eq!(watch.pixels_unlike(&settings.fresh_frame()), 0);

    settings.press_button();
    watch.draw_step(&mut settings);
    assert_eq!(watch.shown_column(0), frame_column(&list, 0));
    assert_eq!(watch.shown_column(16), frame_column(&brightness, 16));
    watch.draw_everything(&mut settings);
    assert_eq!(watch.pixels_unlike(&list), 0);
}

#[test]
fn a_slide_from_a_scrolled_list_ends_showing_the_new_screen_exactly() {
    let mut settings = TestWatch::open_app("settings").unwrap();
    let mut watch = Watch::new();
    watch.draw_everything(&mut settings);
    drag_drawing_each_move(&mut settings, &mut watch, 220, 100);
    assert_ne!(watch.turned_rows(), 0);
    let mut launcher = launcher_with_many_apps();

    launcher.host_mut().set_next_transition(Transition::Down);
    let theme = *launcher.host().theme();
    launcher.host_mut().set_theme(theme);
    watch.draw_everything(&mut launcher);

    assert_eq!(watch.pixels_unlike(&launcher.fresh_frame()), 0);
}

#[test]
fn a_list_opened_after_a_scrolled_launcher_still_scrolls_with_the_panel() {
    let mut launcher = launcher_with_many_apps();
    let mut watch = Watch::new();
    watch.draw_everything(&mut launcher);
    drag_drawing_each_move(&mut launcher, &mut watch, 220, 150);
    assert_ne!(watch.turned_rows(), 0);
    let mut settings = TestWatch::open_app("settings").unwrap();

    settings.host_mut().set_next_transition(Transition::Up);
    let theme = *settings.host().theme();
    settings.host_mut().set_theme(theme);
    watch.draw_everything(&mut settings);
    assert_eq!(watch.pixels_unlike(&settings.fresh_frame()), 0);
    let sent = drag_drawing_each_move(&mut settings, &mut watch, 220, 120);

    assert_eq!(watch.scrolling_band(), (42, 232));
    assert!(sent.iter().all(|&pixels| pixels <= 6 * SIDE), "{sent:?}");
}

#[test]
fn a_list_of_repeated_rows_scrolls_with_the_panel_while_rows_are_built_and_dropped() {
    let source = r#"
import { define_app, range } from "neotime";
import { Button, List } from "neotime/widgets";

function Rows() {
  return (
    <List padding={8} gap={8}>
      {range(20).map((place) => (
        <Button height={44}>{`Row ${place}`}</Button>
      ))}
    </List>
  );
}

export default define_app({ id: "rows", name: "Rows", screens: { main: Rows } });
"#;
    let mut app = TestWatch::open_source(source).unwrap();
    let mut watch = Watch::new();
    watch.draw_everything(&mut app);
    let nodes_at_the_top = app.host().node_count();

    let mut sent = drag_drawing_each_move(&mut app, &mut watch, 220, 30);
    sent.extend(drag_drawing_each_move(&mut app, &mut watch, 220, 30));

    assert!(app.shows_text("Row 9"), "{:?}", app.visible_texts());
    assert!(!app.shows_text("Row 0"));
    assert!(app.host().node_count() <= nodes_at_the_top + 1);
    let most_sent = sent.iter().max().copied().unwrap();
    assert!(most_sent <= 8 * SIDE, "a move sent {most_sent} pixels");
}

#[test]
fn a_cell_grid_draws_its_numbers_as_one_widget() {
    let source = r##"
import { signal } from "@preact/signals";
import { define_app } from "neotime";
import { CellGrid, Screen } from "neotime/widgets";

const board = signal([0, 1, 2, 3]);

function Tiles() {
  return (
    <Screen>
      <CellGrid
        x={0}
        y={0}
        columns={2}
        rows={2}
        cell_size={60}
        cells={board.value}
        palette={["background", "surface", "surface", "surface"]}
        show_numbers
        number_map={[0, 2, 4, 8]}
        number_colors={["text", "text", "text", "#ff0000"]}
      />
    </Screen>
  );
}

export default define_app({ id: "tiles", name: "Tiles", screens: { main: Tiles } });
"##;
    let mut app = TestWatch::open_source(source).unwrap();
    let mut watch = Watch::new();
    watch.draw_everything(&mut app);

    let mut texts = app.visible_texts();
    texts.sort();
    assert_eq!(texts, ["2", "4", "8"]);
    assert_eq!(app.host().node_count(), 2);
    assert!(app
        .area_of_text("8")
        .is_some_and(|cell| cell.x >= 60 && cell.y >= 60));
}

/// An app showing `text` in a TextLines three lines tall, as the morse app keeps its text.
fn text_lines_showing(text: &str) -> TestWatch {
    let codes: Vec<String> = text
        .chars()
        .map(|character| (character as u32).to_string())
        .collect();
    let source = format!(
        r##"
import {{ signal }} from "@preact/signals";
import {{ define_app }} from "neotime";
import {{ Screen, TextLines }} from "neotime/widgets";

const codes = signal([{codes}, 32, 32]);

function Text() {{
  return (
    <Screen>
      <TextLines x={{0}} y={{0}} width={{100}} height={{92}} codes={{codes.value}} length={{{length}}} />
    </Screen>
  );
}}

export default define_app({{ id: "text", name: "Text", screens: {{ main: Text }} }});
"##,
        codes = codes.join(", "),
        length = codes.len(),
    );
    let mut app = TestWatch::open_source(&source).unwrap();
    Watch::new().draw_everything(&mut app);
    app
}

#[test]
fn text_lines_wrap_break_at_new_lines_and_keep_the_last_that_fit() {
    let app = text_lines_showing("one two three four five\nsix\n\nseven");

    // The last three lines are "six", an empty one and "seven".
    assert_eq!(app.visible_texts(), ["six", "seven"]);
    assert_eq!(app.host().node_count(), 2);
}

#[test]
fn text_lines_start_an_empty_line_after_a_last_new_line() {
    let app = text_lines_showing("a\nb\nc\nd\n");

    // The empty line after the last new line is the third of the three that fit.
    assert_eq!(app.visible_texts(), ["c", "d"]);
}
