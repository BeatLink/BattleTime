extern crate std;

use std::vec::Vec;

use embedded_graphics::{prelude::*, primitives::PrimitiveStyle};
use mipidsi::interface::InterfaceKind;

use super::*;

const RASET: u8 = 0x2B;
const VSCRDEF: u8 = 0x33;
const VSCSAD: u8 = 0x37;

/// Records what the driver sends to the panel.
#[derive(Default)]
struct Recorder {
    commands: Vec<(u8, Vec<u8>)>,
    pixels: usize,
}

impl Interface for Recorder {
    type Word = u8;
    type Error = ();
    const KIND: InterfaceKind = InterfaceKind::Serial4Line;

    fn send_command(&mut self, command: u8, args: &[u8]) -> Result<(), ()> {
        self.commands.push((command, args.to_vec()));
        Ok(())
    }

    fn send_pixels<const N: usize>(&mut self, pixels: impl IntoIterator<Item = [u8; N]>) -> Result<(), ()> {
        self.pixels += pixels.into_iter().count();
        Ok(())
    }

    fn send_repeated_pixel<const N: usize>(&mut self, _: [u8; N], count: u32) -> Result<(), ()> {
        self.pixels += count as usize;
        Ok(())
    }
}

struct Pin;

impl embedded_hal::digital::ErrorType for Pin {
    type Error = core::convert::Infallible;
}

impl OutputPin for Pin {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn set_high(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

struct Delay;

impl DelayNs for Delay {
    fn delay_ns(&mut self, _: u32) {}
}

fn display() -> Display<Recorder, Pin> {
    let mut display = Display::new(Recorder::default(), Pin, &mut Delay).unwrap();
    display.inner_interface().commands.clear();
    display
}

impl Display<Recorder, Pin> {
    fn inner_interface(&mut self) -> &mut Recorder {
        // SAFETY: the tests only read and clear the recording.
        unsafe { self.inner.dcs() }
    }

    /// The row ranges of every memory write, and the pixels sent.
    fn writes(&mut self) -> (Vec<(u16, u16)>, usize) {
        let recorder = self.inner_interface();
        let rows = recorder
            .commands
            .iter()
            .filter(|(command, _)| *command == RASET)
            .map(|(_, a)| (u16::from_be_bytes([a[0], a[1]]), u16::from_be_bytes([a[2], a[3]])))
            .collect();
        let pixels = recorder.pixels;
        recorder.commands.clear();
        recorder.pixels = 0;
        (rows, pixels)
    }
}

fn fill(display: &mut Display<Recorder, Pin>, x: i32, y: i32, w: u32, h: u32) {
    Rectangle::new(Point::new(x, y), Size::new(w, h))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::RED))
        .draw(display)
        .unwrap();
}

#[test]
fn starts_with_all_memory_scrolling() {
    let mut display = Display::new(Recorder::default(), Pin, &mut Delay).unwrap();
    let commands = &display.inner_interface().commands;
    assert!(commands.contains(&(VSCRDEF, std::vec![0, 0, 1, 64, 0, 0])));
    assert_eq!(commands.last(), Some(&(VSCSAD, std::vec![0, 0])));
}

#[test]
fn draws_in_place_when_not_scrolled() {
    let mut display = display();
    fill(&mut display, 10, 10, 20, 5);
    assert_eq!(display.writes(), (std::vec![(10, 14)], 100));
}

#[test]
fn follows_the_scroll() {
    let mut display = display();
    display.scroll(100).unwrap();
    fill(&mut display, 0, 0, 240, 10);
    assert_eq!(display.writes(), (std::vec![(100, 109)], 2400));
}

#[test]
fn splits_where_memory_wraps() {
    let mut display = display();
    display.scroll(300).unwrap();
    fill(&mut display, 0, 0, 10, 40);
    assert_eq!(display.writes(), (std::vec![(300, 319), (0, 19)], 400));
}

#[test]
fn keeps_the_fixed_area_still() {
    let mut display = display();
    display.set_scroll_area(20, 0).unwrap();
    display.scroll(10).unwrap();
    assert_eq!(
        display.inner_interface().commands.last(),
        Some(&(VSCSAD, std::vec![0, 30]))
    );
    fill(&mut display, 0, 15, 1, 10);
    assert_eq!(display.writes(), (std::vec![(15, 19), (30, 34)], 10));
}

#[test]
fn scroll_wraps_round_the_area() {
    let mut display = display();
    display.scroll(MEMORY_ROWS + 5).unwrap();
    assert_eq!(display.scroll_offset(), 5);
}

#[test]
fn clips_to_the_screen() {
    let mut display = display();
    fill(&mut display, 230, 235, 20, 20);
    assert_eq!(display.writes(), (std::vec![(235, 239)], 50));
}

#[test]
fn single_pixels_follow_the_scroll() {
    let mut display = display();
    display.scroll(310).unwrap();
    Pixel(Point::new(3, 20), Rgb565::BLUE).draw(&mut display).unwrap();
    assert_eq!(display.writes(), (std::vec![(10, 10)], 1));
}
