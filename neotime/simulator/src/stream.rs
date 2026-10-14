//! Runs an app without a window, for the app builder: commands come in as lines and each changed
//! frame goes out as a line, so another program can show the screen and pass on touches.
//!
//! Commands are `press X Y`, `move X Y`, `release X Y`, `button`, `theme` and `quit`; the end of
//! the input also quits. Output lines are `frame <base64>` with the 240x240 screen as little-endian
//! RGB565, `motor on` and `motor off`, and anything else is the simulator's usual log text.

use std::{
    error::Error,
    io::{BufRead, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use neotime_app_host::STRIP_PIXELS;
use neotime_runtime::DEFAULT_THEME;

use super::WatchDisplay;
use crate::{
    app::{open, report, wall_clock_in_milliseconds, AppFiles, SimulatedSystem, TouchTracker, LIGHT_THEME},
    panel::SimulatedPanel,
};

// ============================================================================
// Settings
// ============================================================================

const FRAME_PAUSE: Duration = Duration::from_millis(16);
const BASE64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

// ============================================================================
// Commands
// ============================================================================

/// One line of input from the program showing the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Press(Point),
    Move(Point),
    Release(Point),
    Button,
    Theme,
    Quit,
}

pub fn parse_command(line: &str) -> Result<Command, String> {
    let mut words = line.split_whitespace();
    let name = words.next().unwrap_or("");
    let mut point = || -> Result<Point, String> {
        let mut number = || {
            words
                .next()
                .and_then(|word| word.parse().ok())
                .ok_or(format!("{name} needs X and Y"))
        };
        Ok(Point::new(number()?, number()?))
    };
    match name {
        "press" => Ok(Command::Press(point()?)),
        "move" => Ok(Command::Move(point()?)),
        "release" => Ok(Command::Release(point()?)),
        "button" => Ok(Command::Button),
        "theme" => Ok(Command::Theme),
        "quit" => Ok(Command::Quit),
        _ => Err(format!("unknown command {line:?}")),
    }
}

/// Reads lines on their own thread, so the app keeps running while no command comes.
pub fn read_commands(input: impl BufRead + Send + 'static) -> mpsc::Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for line in input.lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    receiver
}

// ============================================================================
// Frames
// ============================================================================

/// The screen's pixels, row by row, as the panel shows them.
pub fn shown_pixels(panel: &SimulatedPanel) -> Vec<u16> {
    let mut pixels = Vec::new();
    panel.for_each_shown_pixel(|_column, _row, colour: Rgb565| pixels.push(colour.into_storage()));
    pixels
}

/// A frame line: `frame ` and the pixels as little-endian bytes in base64.
pub fn frame_line(pixels: &[u16]) -> String {
    let bytes: Vec<u8> = pixels.iter().flat_map(|pixel| pixel.to_le_bytes()).collect();
    format!("frame {}", base64(&bytes))
}

pub fn base64(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let joined = (group[0] as u32) << 16
            | (*group.get(1).unwrap_or(&0) as u32) << 8
            | *group.get(2).unwrap_or(&0) as u32;
        for place in 0..4 {
            let character = if place <= group.len() {
                BASE64_ALPHABET[(joined >> (18 - 6 * place) & 63) as usize]
            } else {
                b'='
            };
            text.push(character as char);
        }
    }
    text
}

// ============================================================================
// Running the app
// ============================================================================

/// Runs the app until `quit` or the end of the commands, writing each changed frame to `output`.
pub fn run(
    files: AppFiles,
    panel: SimulatedPanel,
    mut display: WatchDisplay,
    mut system: SimulatedSystem,
    motor_running: Arc<AtomicBool>,
    commands: mpsc::Receiver<String>,
    output: &mut impl Write,
) -> Result<(), Box<dyn Error>> {
    let mut host = open(files, wall_clock_in_milliseconds(), &mut system)?;
    let mut touch = TouchTracker::default();
    let mut is_light = false;
    let mut strip = vec![0u16; STRIP_PIXELS];
    let mut last_frame = Vec::new();
    let mut motor_was_running = false;

    loop {
        let (lines, input_ended) = waiting_commands(&commands);
        let mut quit = input_ended;
        for line in lines {
            let now = wall_clock_in_milliseconds();
            let command = match parse_command(&line) {
                Ok(command) => command,
                Err(problem) => {
                    writeln!(output, "{problem}")?;
                    continue;
                }
            };
            match command {
                Command::Quit => quit = true,
                Command::Press(point) => touch.press(point),
                Command::Move(point) => {
                    if let Some(drag) = touch.drag(point) {
                        host.touch(drag, now, &mut system);
                    }
                }
                Command::Release(point) => {
                    if let Some(gesture) = touch.release(point) {
                        report(host.touch(gesture, now, &mut system), "touch");
                    }
                }
                Command::Button => report(host.go_back_a_screen(now, &mut system), "button"),
                Command::Theme => {
                    is_light = !is_light;
                    host.set_theme(if is_light { LIGHT_THEME } else { DEFAULT_THEME });
                }
            }
            system.tell_app_about_settings(&mut host, now);
        }

        let now = wall_clock_in_milliseconds();
        host.tick(now, &mut system);
        system.take_heart_rate_updates(&mut host, now);
        if host.needs_drawing() {
            host.draw_step_at(&mut display, &mut strip, now as u64)
                .unwrap_or_else(|never| match never {});
        }

        let frame = shown_pixels(&panel);
        if frame != last_frame {
            writeln!(output, "{}", frame_line(&frame))?;
            last_frame = frame;
        }
        let motor_is_running = motor_running.load(Ordering::Relaxed);
        if motor_is_running != motor_was_running {
            writeln!(output, "motor {}", if motor_is_running { "on" } else { "off" })?;
            motor_was_running = motor_is_running;
        }
        output.flush()?;

        // The last frame goes out before quitting, so the screen shows what the final command did.
        if quit {
            return Ok(());
        }
        thread::sleep(FRAME_PAUSE);
    }
}

/// The lines that have come in since last time, and whether the input has ended.
fn waiting_commands(commands: &mpsc::Receiver<String>) -> (Vec<String>, bool) {
    let mut lines = Vec::new();
    loop {
        match commands.try_recv() {
            Ok(line) => lines.push(line),
            Err(mpsc::TryRecvError::Empty) => return (lines, false),
            Err(mpsc::TryRecvError::Disconnected) => return (lines, true),
        }
    }
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod stream_tests;
