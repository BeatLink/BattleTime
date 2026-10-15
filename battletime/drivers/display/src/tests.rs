extern crate std;

use std::{vec, vec::Vec};

use embedded_graphics::{prelude::*, primitives::PrimitiveStyle};
use embedded_hal::digital::ErrorType;

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
    /// The length of each strip send, for an interface that sends strips by DMA.
    strip_sends: Vec<usize>,
}

impl PanelInterface for FakeInterface {
    type Error = ();

    fn send_command(&mut self, code: u8, arguments: &[u8]) -> Result<(), ()> {
        self.commands.push(Command {
            code,
            arguments: arguments.to_vec(),
        });
        Ok(())
    }

    fn send_pixels(&mut self, bytes: &[u8]) -> Result<(), ()> {
        self.pixel_count += bytes.len() / 2;
        Ok(())
    }

    fn send_strip_pixels(&mut self, pixels: &[u16]) -> Result<(), ()> {
        self.strip_sends.push(pixels.len());
        self.pixel_count += pixels.len();
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

/// A delay that adds up how long it was asked to wait.
#[derive(Default)]
struct FakeDelay {
    waited_in_nanoseconds: u64,
}

impl DelayNs for FakeDelay {
    fn delay_ns(&mut self, nanoseconds: u32) {
        self.waited_in_nanoseconds += u64::from(nanoseconds);
    }
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
    Display::new(FakeInterface::default(), FakePin, &mut FakeDelay::default()).unwrap()
}

fn new_display_with_no_commands_recorded() -> Display<FakeInterface, FakePin> {
    let mut display = new_display();
    display.fake_interface().commands.clear();
    display
}

impl Display<FakeInterface, FakePin> {
    fn fake_interface(&mut self) -> &mut FakeInterface {
        self.interface_mut()
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
fn reports_the_rows_that_scroll() {
    let mut display = new_display_with_no_commands_recorded();

    display.set_scroll_area(20, 100).unwrap();

    let expected = ScrollArea {
        top_fixed_rows: 20,
        scrolling_rows: MEMORY_ROWS - 120,
    };
    assert_eq!(display.scroll_area(), expected);
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
fn a_strip_splits_into_the_fewest_equal_pieces_the_dma_can_move() {
    assert_eq!(
        Pieces::of(1920, 255),
        Pieces {
            piece_bytes: 240,
            count: 8
        }
    );
    assert_eq!(
        Pieces::of(64, 255),
        Pieces {
            piece_bytes: 64,
            count: 1
        }
    );
    assert_eq!(
        Pieces::of(2 * 251, 255),
        Pieces {
            piece_bytes: 251,
            count: 2
        }
    );
}

#[test]
fn a_length_with_no_large_divisor_still_splits_exactly() {
    let pieces = Pieces::of(2 * 257, 255);

    assert_eq!(pieces.piece_bytes * pieces.count, 2 * 257);
    assert!(pieces.piece_bytes <= 255);
}

#[test]
fn sleeping_and_waking_switch_the_display_and_its_sleep_and_wait_only_a_few_milliseconds() {
    let mut display = new_display_with_no_commands_recorded();
    let mut delay = FakeDelay::default();

    display.sleep(&mut delay).unwrap();
    display.wake(&mut delay).unwrap();

    assert_eq!(
        command_codes(&mut display),
        vec![DISPLAY_OFF, ENTER_SLEEP, EXIT_SLEEP, DISPLAY_ON]
    );
    assert_eq!(
        delay.waited_in_nanoseconds,
        2 * u64::from(SLEEP_SWITCH_SETTLE_TIME_IN_MICROSECONDS) * 1_000
    );
    assert!(delay.waited_in_nanoseconds < SLEEP_SWITCH_GAP_IN_MILLISECONDS * 1_000_000);
}

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

// ============================================================================
// Starting, strips and other interfaces
// ============================================================================

#[test]
fn starts_the_panel_as_the_st7789_needs() {
    let mut display = new_display();

    let codes = command_codes(&mut display);

    assert_eq!(&codes[..6], &[0x11, 0x36, 0x21, 0x3A, 0x13, 0x29]);
    assert_eq!(display.fake_interface().commands[3].arguments, [0x55]);
}

#[test]
fn a_strip_goes_out_split_where_memory_wraps() {
    let mut display = new_display();
    display.scroll(310).unwrap();
    display.fake_interface().commands.clear();

    display
        .send_strip(&Rectangle::new(Point::new(0, 5), Size::new(240, 10)), &[0; 2400])
        .unwrap();
    display
        .send_strip(&Rectangle::new(Point::new(0, 0), Size::new(0, 0)), &[])
        .unwrap();

    assert_eq!(display.fake_interface().strip_sends, [1200, 1200]);
}

#[test]
fn panel_pixels_hold_the_high_byte_first() {
    let pixel = panel_pixel(Rgb565::new(31, 0, 0));

    assert_eq!(pixel.to_ne_bytes(), [0xF8, 0x00]);
    assert_eq!(color_of_panel_pixel(pixel), Rgb565::new(31, 0, 0));
}

/// A mipidsi interface that counts what it is sent.
#[derive(Default)]
struct FakeMipidsi {
    commands: usize,
    pixels: usize,
}

impl mipidsi::interface::Interface for FakeMipidsi {
    type Word = u8;
    type Error = ();
    const KIND: mipidsi::interface::InterfaceKind = mipidsi::interface::InterfaceKind::Serial4Line;

    fn send_command(&mut self, _code: u8, _arguments: &[u8]) -> Result<(), ()> {
        self.commands += 1;
        Ok(())
    }

    fn send_pixels<const BYTES_PER_PIXEL: usize>(
        &mut self,
        pixels: impl IntoIterator<Item = [u8; BYTES_PER_PIXEL]>,
    ) -> Result<(), ()> {
        self.pixels += pixels.into_iter().count();
        Ok(())
    }

    fn send_repeated_pixel<const BYTES_PER_PIXEL: usize>(
        &mut self,
        _pixel: [u8; BYTES_PER_PIXEL],
        count: u32,
    ) -> Result<(), ()> {
        self.pixels += count as usize;
        Ok(())
    }
}

#[test]
fn a_mipidsi_interface_drives_the_panel_too() {
    let mut display = Display::new(
        MipidsiInterface(FakeMipidsi::default()),
        FakePin,
        &mut FakeDelay::default(),
    )
    .unwrap();

    display
        .fill_solid(
            &Rectangle::new(Point::new(0, 0), Size::new(10, 10)),
            Rgb565::new(1, 2, 3),
        )
        .unwrap();
    Pixel(Point::new(3, 3), Rgb565::new(1, 2, 3))
        .draw(&mut display)
        .unwrap();
    display
        .send_strip(&Rectangle::new(Point::new(0, 0), Size::new(2, 1)), &[1, 2])
        .unwrap();
    let (MipidsiInterface(interface), _) = display.release();

    assert_eq!(interface.pixels, 103);
    assert!(interface.commands > 6);
}

#[test]
fn the_default_repeated_pixel_goes_in_chunks() {
    let mut interface = FakeInterface::default();

    interface.send_repeated_pixel([1, 2], 70).unwrap();

    assert_eq!(interface.pixel_count, 70);
    assert_eq!(interface.send_strip_pixels(&[1, 2]), Ok(()));
}
