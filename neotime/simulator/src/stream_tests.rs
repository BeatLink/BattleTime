use std::{
    io::Cursor,
    sync::{atomic::AtomicBool, mpsc, Arc},
};

use embedded_graphics::prelude::*;
use neotime_app_host::demo::stopwatch_image;
use neotime_display::Display;

use super::{base64, frame_line, parse_command, read_commands, run, Command};
use crate::{
    app::{AppFiles, SimulatedSystem},
    hardware::{ThreadDelay, UnconnectedPin},
    panel::SimulatedPanel,
};

// ============================================================================
// Helpers
// ============================================================================

/// Runs the stopwatch on `input` and returns every line it wrote.
fn run_stopwatch(input: &str) -> Vec<String> {
    let image = stopwatch_image();
    let files = AppFiles {
        image: &image,
        library: &[],
        installed_apps: &[],
        installed_app_icons: &[],
        storage_folder: None,
    };
    let panel = SimulatedPanel::new();
    let display = Display::new(panel.clone(), UnconnectedPin, &mut ThreadDelay).unwrap();
    let system = SimulatedSystem::new(None, None, 1, &files);
    // Every command is waiting before the run starts, so the frames do not depend on thread timing.
    let (sender, commands) = mpsc::channel();
    for line in input.lines() {
        sender.send(line.to_string()).unwrap();
    }
    drop(sender);
    let mut output = Vec::new();

    run(
        files,
        panel,
        display,
        system,
        Arc::new(AtomicBool::new(false)),
        commands,
        &mut output,
    )
    .unwrap();

    String::from_utf8(output)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

fn frames(lines: &[String]) -> Vec<&String> {
    lines.iter().filter(|line| line.starts_with("frame ")).collect()
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn commands_are_read_with_their_points() {
    assert_eq!(
        parse_command("press 120 138"),
        Ok(Command::Press(Point::new(120, 138)))
    );
    assert_eq!(parse_command("move 1 2"), Ok(Command::Move(Point::new(1, 2))));
    assert_eq!(
        parse_command(" release 5 6 "),
        Ok(Command::Release(Point::new(5, 6)))
    );
    assert_eq!(parse_command("button"), Ok(Command::Button));
    assert_eq!(parse_command("theme"), Ok(Command::Theme));
    assert_eq!(parse_command("quit"), Ok(Command::Quit));
    assert!(parse_command("press 1").is_err());
    assert!(parse_command("press x 2").is_err());
    assert!(parse_command("dance").is_err());
}

#[test]
fn base64_pads_short_groups() {
    assert_eq!(base64(b""), "");
    assert_eq!(base64(b"f"), "Zg==");
    assert_eq!(base64(b"fo"), "Zm8=");
    assert_eq!(base64(b"foo"), "Zm9v");
    assert_eq!(base64(b"foobar"), "Zm9vYmFy");
}

#[test]
fn a_frame_line_carries_little_endian_pixels() {
    assert_eq!(
        frame_line(&[0x0102, 0xF800]),
        format!("frame {}", base64(&[0x02, 0x01, 0x00, 0xF8]))
    );
}

#[test]
fn the_first_frame_is_the_whole_screen() {
    let lines = run_stopwatch("quit\n");

    let frames = frames(&lines);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].len(), "frame ".len() + 240 * 240 * 2 / 3 * 4);
}

#[test]
fn a_click_changes_the_frame_and_the_end_of_input_quits() {
    let unclicked = run_stopwatch("quit\n");
    let clicked = run_stopwatch("press 120 138\nrelease 120 138\n");

    assert_ne!(frames(&clicked).last(), frames(&unclicked).last());
}

#[test]
fn an_unknown_command_is_reported_and_the_rest_still_run() {
    let lines = run_stopwatch("dance\nbutton\ntheme\nquit\n");

    assert!(lines.iter().any(|line| line == "unknown command \"dance\""));
    assert_eq!(frames(&lines).len(), 1);
}

#[test]
fn commands_are_read_line_by_line_until_the_input_ends() {
    let commands = read_commands(Cursor::new("press 1 2\nquit\n".to_string()));

    let lines: Vec<String> = commands.iter().collect();

    assert_eq!(lines, ["press 1 2", "quit"]);
}
