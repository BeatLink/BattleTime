extern crate std;

use std::{vec, vec::Vec};

use embedded_graphics::{prelude::*, primitives::PrimitiveStyle};
use embedded_hal::digital::ErrorType;
use mipidsi::interface::InterfaceKind;

use super::*;

// ============================================================================
// Panel commands
// ============================================================================

const SET_ROW_ADDRESS: u8 = 0x2B;
const SET_SCROLL_AREA: u8 = 0x33;
const SET_SCROLL_START: u8 = 0x37;

// ============================================================================
// Fake hardware
// ============================================================================

#[derive(Debug, PartialEq)]
struct Command {
    code: u8,
    arguments: Vec<u8>,
}

#[derive(Default)]
struct FakeInterface {
    commands: Vec<Command>,
    pixel_count: usize,
}

impl Interface for FakeInterface {
    type Word = u8;
    type Error = ();
    const KIND: InterfaceKind = InterfaceKind::Serial4Line;

    fn send_command(&mut self, code: u8, arguments: &[u8]) -> Result<(), ()> {
        self.commands.push(Command {
            code,
            arguments: arguments.to_vec(),
        });
        Ok(())
    }

    fn send_pixels<const BYTES_PER_PIXEL: usize>(
        &mut self,
        pixels: impl IntoIterator<Item = [u8; BYTES_PER_PIXEL]>,
    ) -> Result<(), ()> {
        self.pixel_count += pixels.into_iter().count();
        Ok(())
    }

    fn send_repeated_pixel<const BYTES_PER_PIXEL: usize>(
        &mut self,
        _pixel: [u8; BYTES_PER_PIXEL],
        repeat_count: u32,
    ) -> Result<(), ()> {
        self.pixel_count += repeat_count as usize;
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

/// A backlight pin that remembers whether it was last driven low.
#[derive(Default)]
struct FakeBacklightPin {
    driven_low: bool,
}

impl ErrorType for FakeBacklightPin {
    type Error = core::convert::Infallible;
}

impl OutputPin for FakeBacklightPin {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.driven_low = true;
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.driven_low = false;
        Ok(())
    }
}

struct FakeDelay;

impl DelayNs for FakeDelay {
    fn delay_ns(&mut self, _nanoseconds: u32) {}
}

// ============================================================================
// Helpers
// ============================================================================

#[derive(Debug, PartialEq)]
struct MemoryWrite {
    first_memory_row: u16,
    last_memory_row: u16,
}

fn write(first_memory_row: u16, last_memory_row: u16) -> MemoryWrite {
    MemoryWrite {
        first_memory_row,
        last_memory_row,
    }
}

fn new_display() -> Display<FakeInterface, FakePin> {
    Display::new(FakeInterface::default(), FakePin, &mut FakeDelay).unwrap()
}

fn new_display_with_no_commands_recorded() -> Display<FakeInterface, FakePin> {
    let mut display = new_display();
    display.fake_interface().commands.clear();
    display
}

impl Display<FakeInterface, FakePin> {
    fn fake_interface(&mut self) -> &mut FakeInterface {
        // SAFETY: the tests only read and clear what the fake interface recorded.
        unsafe { self.panel.dcs() }
    }

    fn last_command(&mut self) -> Option<&Command> {
        self.fake_interface().commands.last()
    }

    fn take_memory_writes(&mut self) -> Vec<MemoryWrite> {
        let fake_interface = self.fake_interface();

        let memory_writes = fake_interface
            .commands
            .iter()
            .filter(|command| command.code == SET_ROW_ADDRESS)
            .map(|command| {
                let first_memory_row = u16::from_be_bytes([command.arguments[0], command.arguments[1]]);
                let last_memory_row = u16::from_be_bytes([command.arguments[2], command.arguments[3]]);
                write(first_memory_row, last_memory_row)
            })
            .collect();
        fake_interface.commands.clear();

        memory_writes
    }

    fn take_pixel_count(&mut self) -> usize {
        core::mem::take(&mut self.fake_interface().pixel_count)
    }
}

fn new_backlight(level: BacklightLevel) -> Backlight<FakeBacklightPin> {
    Backlight::new(
        FakeBacklightPin::default(),
        FakeBacklightPin::default(),
        FakeBacklightPin::default(),
        level,
    )
    .unwrap()
}

/// Which of the low, medium and high pins light the screen.
fn lit_pins(backlight: Backlight<FakeBacklightPin>) -> [bool; 3] {
    backlight.release().map(|pin| pin.driven_low)
}

fn command_codes(display: &mut Display<FakeInterface, FakePin>) -> Vec<u8> {
    display
        .fake_interface()
        .commands
        .iter()
        .map(|command| command.code)
        .collect()
}

fn fill_rectangle(display: &mut Display<FakeInterface, FakePin>, top_left: Point, size: Size) {
    Rectangle::new(top_left, size)
        .into_styled(PrimitiveStyle::with_fill(Rgb565::RED))
        .draw(display)
        .unwrap();
}

// ============================================================================
// Scroll area
// ============================================================================

#[test]
fn starts_with_all_memory_scrolling() {
    let mut display = new_display();

    let commands = &display.fake_interface().commands;

    let whole_memory_scrolls = Command {
        code: SET_SCROLL_AREA,
        arguments: vec![0, 0, 1, 64, 0, 0],
    };
    let scroll_starts_at_row_zero = Command {
        code: SET_SCROLL_START,
        arguments: vec![0, 0],
    };
    assert!(commands.contains(&whole_memory_scrolls));
    assert_eq!(commands.last(), Some(&scroll_starts_at_row_zero));
}

#[test]
fn scroll_wraps_round_the_area() {
    let mut display = new_display_with_no_commands_recorded();

    display.scroll(MEMORY_ROWS + 5).unwrap();

    assert_eq!(display.scroll_offset(), 5);
}

#[test]
fn keeps_the_fixed_area_still() {
    let mut display = new_display_with_no_commands_recorded();
    display.set_scroll_area(20, 0).unwrap();

    display.scroll(10).unwrap();
    let scroll_command = display.last_command();
    assert_eq!(
        scroll_command,
        Some(&Command {
            code: SET_SCROLL_START,
            arguments: vec![0, 30]
        })
    );

    display.take_memory_writes();
    fill_rectangle(&mut display, Point::new(0, 15), Size::new(1, 10));

    assert_eq!(display.take_memory_writes(), vec![write(15, 19), write(30, 34)]);
    assert_eq!(display.take_pixel_count(), 10);
}

// ============================================================================
// Drawing
// ============================================================================

#[test]
fn draws_in_place_when_not_scrolled() {
    let mut display = new_display_with_no_commands_recorded();

    fill_rectangle(&mut display, Point::new(10, 10), Size::new(20, 5));

    assert_eq!(display.take_memory_writes(), vec![write(10, 14)]);
    assert_eq!(display.take_pixel_count(), 100);
}

#[test]
fn follows_the_scroll() {
    let mut display = new_display_with_no_commands_recorded();
    display.scroll(100).unwrap();
    display.take_memory_writes();

    fill_rectangle(&mut display, Point::new(0, 0), Size::new(240, 10));

    assert_eq!(display.take_memory_writes(), vec![write(100, 109)]);
    assert_eq!(display.take_pixel_count(), 2400);
}

#[test]
fn splits_where_memory_wraps() {
    let mut display = new_display_with_no_commands_recorded();
    display.scroll(300).unwrap();
    display.take_memory_writes();

    fill_rectangle(&mut display, Point::new(0, 0), Size::new(10, 40));

    assert_eq!(display.take_memory_writes(), vec![write(300, 319), write(0, 19)]);
    assert_eq!(display.take_pixel_count(), 400);
}

#[test]
fn clips_to_the_screen() {
    let mut display = new_display_with_no_commands_recorded();

    fill_rectangle(&mut display, Point::new(230, 235), Size::new(20, 20));

    assert_eq!(display.take_memory_writes(), vec![write(235, 239)]);
    assert_eq!(display.take_pixel_count(), 50);
}

#[test]
fn single_pixels_follow_the_scroll() {
    let mut display = new_display_with_no_commands_recorded();
    display.scroll(310).unwrap();
    display.take_memory_writes();

    Pixel(Point::new(3, 20), Rgb565::BLUE).draw(&mut display).unwrap();

    assert_eq!(display.take_memory_writes(), vec![write(10, 10)]);
    assert_eq!(display.take_pixel_count(), 1);
}

// ============================================================================
// Low-power mode
// ============================================================================

#[test]
fn low_power_mode_slows_the_idle_refresh_then_turns_idle_on() {
    let mut display = new_display_with_no_commands_recorded();

    display.enter_low_power_mode().unwrap();

    assert_eq!(
        command_codes(&mut display),
        vec![SET_PORCHES, SET_IDLE_FRAME_RATE, IDLE_MODE_ON]
    );
    assert!(display.is_in_low_power_mode());
}

#[test]
fn leaving_low_power_mode_puts_back_the_panel_defaults() {
    let mut display = new_display();
    display.enter_low_power_mode().unwrap();
    display.fake_interface().commands.clear();

    display.leave_low_power_mode().unwrap();

    let commands = &display.fake_interface().commands;
    assert_eq!(
        *commands,
        vec![
            Command {
                code: IDLE_MODE_OFF,
                arguments: vec![]
            },
            Command {
                code: SET_IDLE_FRAME_RATE,
                arguments: DEFAULT_IDLE_FRAME_RATE.to_vec()
            },
            Command {
                code: SET_PORCHES,
                arguments: DEFAULT_PORCHES.to_vec()
            },
        ]
    );
    assert!(!display.is_in_low_power_mode());
}

#[test]
fn drawing_is_unchanged_in_low_power_mode() {
    let mut display = new_display_with_no_commands_recorded();
    display.scroll(300).unwrap();
    display.enter_low_power_mode().unwrap();
    display.take_memory_writes();

    fill_rectangle(&mut display, Point::new(0, 0), Size::new(10, 40));

    assert_eq!(display.take_memory_writes(), vec![write(300, 319), write(0, 19)]);
}

// ============================================================================
// Backlight
// ============================================================================

#[test]
fn each_level_lights_one_more_pin() {
    assert_eq!(
        lit_pins(new_backlight(BacklightLevel::Off)),
        [false, false, false]
    );
    assert_eq!(lit_pins(new_backlight(BacklightLevel::Low)), [true, false, false]);
    assert_eq!(
        lit_pins(new_backlight(BacklightLevel::Medium)),
        [true, true, false]
    );
    assert_eq!(lit_pins(new_backlight(BacklightLevel::High)), [true, true, true]);
}

#[test]
fn dimming_turns_off_the_brightest_pin() {
    let mut backlight = new_backlight(BacklightLevel::High);

    backlight.dimmer().unwrap();

    assert_eq!(backlight.level(), BacklightLevel::Medium);
    assert_eq!(lit_pins(backlight), [true, true, false]);
}

#[test]
fn brighter_stops_at_high() {
    let mut backlight = new_backlight(BacklightLevel::Medium);

    backlight.brighter().unwrap();
    backlight.brighter().unwrap();

    assert_eq!(backlight.level(), BacklightLevel::High);
}

#[test]
fn dimmer_stops_at_off() {
    let mut backlight = new_backlight(BacklightLevel::Low);

    backlight.dimmer().unwrap();
    backlight.dimmer().unwrap();

    assert_eq!(backlight.level(), BacklightLevel::Off);
    assert_eq!(lit_pins(backlight), [false, false, false]);
}
