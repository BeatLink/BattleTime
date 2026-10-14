//! Committed snapshots of what the watch shows: the texts of each screen as JSON, which the browser
//! preview's tests read too, and the drawn screen as a PNG.
//!
//! A check compares what the test watch shows now with the committed file and gives a readable
//! difference when they part. With `NEOTIME_UPDATE_SNAPSHOTS=1` set it rewrites the file instead.
//! A screen that differs leaves the actual picture and a picture of the changed pixels under
//! `target/snapshot-failures/`. docs/testing.md says how to read and update them.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;

use crate::framebuffer::{Framebuffer, SCREEN_SIDE_IN_PIXELS};
use crate::sources::repository_root;

// ============================================================================
// Where snapshots live
// ============================================================================

/// The variable that makes every check rewrite its snapshot instead of comparing.
pub const UPDATE_SNAPSHOTS_VARIABLE: &str = "NEOTIME_UPDATE_SNAPSHOTS";

/// Whether this run rewrites snapshots rather than checking them.
pub fn is_updating_snapshots() -> bool {
    std::env::var(UPDATE_SNAPSHOTS_VARIABLE).is_ok_and(|value| !value.is_empty() && value != "0")
}

/// Where a failed screen check leaves its pictures.
pub fn failure_directory() -> PathBuf {
    repository_root().join("target/snapshot-failures")
}

fn write_creating_folders(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|error| format!("cannot create {}: {error}", folder.display()))?;
    }
    std::fs::write(path, bytes).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

// ============================================================================
// Screen texts as JSON
// ============================================================================

/// The texts one screen shows, and the gesture on the main screen that reaches it, such as `swipe up`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenTexts {
    pub name: String,
    pub reached_by: Option<String>,
    pub texts: Vec<String>,
}

/// Writes a text as a JSON string, escaping only what JSON requires.
fn json_string(text: &str) -> String {
    let mut quoted = String::from("\"");
    for character in text.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\t' => quoted.push_str("\\t"),
            control if control < ' ' => {
                let _ = write!(quoted, "\\u{:04x}", control as u32);
            }
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

/// The screens as JSON with one text to a line, sorted, so a change shows as a short diff.
pub fn screen_texts_json(screens: &[ScreenTexts]) -> String {
    let mut json = String::from("{\n  \"screens\": [");
    for (position, screen) in screens.iter().enumerate() {
        let mut texts = screen.texts.clone();
        texts.sort();
        let reached_by = match &screen.reached_by {
            Some(text) => json_string(text),
            None => "null".to_string(),
        };
        let separator = if position == 0 { "" } else { "," };
        let _ = write!(
            json,
            "{separator}\n    {{\n      \"name\": {},\n      \"reached_by\": {reached_by},\n      \"texts\": [",
            json_string(&screen.name)
        );
        let lines: Vec<String> = texts
            .iter()
            .map(|text| format!("\n        {}", json_string(text)))
            .collect();
        json.push_str(&lines.join(","));
        json.push_str(if texts.is_empty() {
            "]\n    }"
        } else {
            "\n      ]\n    }"
        });
    }
    json.push_str(if screens.is_empty() {
        "]\n}\n"
    } else {
        "\n  ]\n}\n"
    });
    json
}

/// The lines only one side has, marked `-` for the committed file and `+` for what the watch shows.
pub fn line_difference(expected: &str, actual: &str) -> String {
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let mut longest_common = vec![vec![0usize; actual_lines.len() + 1]; expected_lines.len() + 1];
    for expected_index in (0..expected_lines.len()).rev() {
        for actual_index in (0..actual_lines.len()).rev() {
            longest_common[expected_index][actual_index] =
                match expected_lines[expected_index] == actual_lines[actual_index] {
                    true => longest_common[expected_index + 1][actual_index + 1] + 1,
                    false => longest_common[expected_index + 1][actual_index]
                        .max(longest_common[expected_index][actual_index + 1]),
                };
        }
    }

    let mut difference = String::new();
    let (mut expected_index, mut actual_index) = (0, 0);
    while expected_index < expected_lines.len() || actual_index < actual_lines.len() {
        let both_left = expected_index < expected_lines.len() && actual_index < actual_lines.len();
        if both_left && expected_lines[expected_index] == actual_lines[actual_index] {
            expected_index += 1;
            actual_index += 1;
        } else if expected_index < expected_lines.len()
            && (actual_index == actual_lines.len()
                || longest_common[expected_index + 1][actual_index]
                    >= longest_common[expected_index][actual_index + 1])
        {
            let _ = writeln!(difference, "- {}", expected_lines[expected_index]);
            expected_index += 1;
        } else {
            let _ = writeln!(difference, "+ {}", actual_lines[actual_index]);
            actual_index += 1;
        }
    }
    difference
}

/// Compares text with its committed snapshot, or rewrites the snapshot when updating.
pub fn check_text_snapshot(name: &str, actual: &str) -> Result<(), String> {
    SnapshotFolders::committed().check_text(name, actual)
}

// ============================================================================
// Screens as PNG
// ============================================================================

/// The colour changed pixels are painted in the difference picture.
const CHANGED_PIXEL_COLOR: [u8; 3] = [255, 0, 255];

/// How much of the committed picture shows behind the changed pixels, out of 256.
const UNCHANGED_PIXEL_BRIGHTNESS: u16 = 64;

/// A frame as 8-bit red, green and blue bytes, widening each channel so it narrows back exactly.
pub fn frame_to_rgb_bytes(frame: &Framebuffer) -> Vec<u8> {
    let widen = |value: u8, bits: u32| (value << (8 - bits)) | (value >> (2 * bits - 8));
    frame
        .pixels()
        .iter()
        .flat_map(|pixel: &Rgb565| [widen(pixel.r(), 5), widen(pixel.g(), 6), widen(pixel.b(), 5)])
        .collect()
}

/// A 240x240 picture of red, green and blue bytes as a PNG.
pub fn encode_png(rgb_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let side = SCREEN_SIDE_IN_PIXELS as u32;
    let mut encoded = Vec::new();
    let mut encoder = png::Encoder::new(&mut encoded, side, side);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::High);
    let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
    writer
        .write_image_data(rgb_bytes)
        .map_err(|error| error.to_string())?;
    writer.finish().map_err(|error| error.to_string())?;
    Ok(encoded)
}

/// A PNG's pixels as red, green and blue bytes, or why it cannot be read as a 240x240 screen.
pub fn decode_png(encoded: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(encoded));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
    let mut pixels = vec![0; reader.output_buffer_size().unwrap_or_default()];
    let frame = reader
        .next_frame(&mut pixels)
        .map_err(|error| error.to_string())?;
    let side = SCREEN_SIDE_IN_PIXELS as u32;
    if frame.width != side || frame.height != side || frame.color_type != png::ColorType::Rgb {
        return Err(format!(
            "it is {}x{} {:?}, not a {side}x{side} RGB screen",
            frame.width, frame.height, frame.color_type
        ));
    }
    pixels.truncate(frame.buffer_size());
    Ok(pixels)
}

/// The committed picture dimmed, with every pixel that differs painted in a bright colour.
pub fn difference_picture(expected: &[u8], actual: &[u8]) -> (Vec<u8>, usize) {
    let mut changed_pixels = 0;
    let (expected_pixels, _) = expected.as_chunks::<3>();
    let (actual_pixels, _) = actual.as_chunks::<3>();
    let picture = expected_pixels
        .iter()
        .zip(actual_pixels)
        .flat_map(|(expected_pixel, actual_pixel)| {
            if expected_pixel == actual_pixel {
                expected_pixel.map(|channel| (channel as u16 * UNCHANGED_PIXEL_BRIGHTNESS / 256) as u8)
            } else {
                changed_pixels += 1;
                CHANGED_PIXEL_COLOR
            }
        })
        .collect();
    (picture, changed_pixels)
}

/// Compares a frame with its committed PNG, or rewrites the PNG when updating.
pub fn check_screen_snapshot(name: &str, frame: &Framebuffer) -> Result<(), String> {
    SnapshotFolders::committed().check_screen(name, frame)
}

// ============================================================================
// Checking against the files
// ============================================================================

/// Where the snapshots are read from and failures written to, and whether this run rewrites them.
pub struct SnapshotFolders {
    pub texts: PathBuf,
    pub screens: PathBuf,
    pub failures: PathBuf,
    pub is_updating: bool,
}

impl SnapshotFolders {
    /// The folders in the repository, updating when `NEOTIME_UPDATE_SNAPSHOTS` asks to.
    pub fn committed() -> Self {
        SnapshotFolders {
            texts: repository_root().join("harness/snapshots/texts"),
            screens: repository_root().join("harness/snapshots/screens"),
            failures: failure_directory(),
            is_updating: is_updating_snapshots(),
        }
    }

    pub fn check_text(&self, name: &str, actual: &str) -> Result<(), String> {
        let path = self.texts.join(format!("{name}.json"));
        if self.is_updating {
            return write_creating_folders(&path, actual.as_bytes());
        }
        let expected = std::fs::read_to_string(&path).unwrap_or_default();
        if expected == actual {
            return Ok(());
        }
        Err(format!(
            "{name}: the watch no longer shows what {} holds (- committed, + now):\n{}",
            path.display(),
            line_difference(&expected, actual)
        ))
    }

    pub fn check_screen(&self, name: &str, frame: &Framebuffer) -> Result<(), String> {
        let path = self.screens.join(format!("{name}.png"));
        let actual = frame_to_rgb_bytes(frame);
        if self.is_updating {
            return write_creating_folders(&path, &encode_png(&actual)?);
        }
        let expected = std::fs::read(&path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| decode_png(&bytes));
        let actual_path = self.failures.join(format!("{name}.actual.png"));
        let expected = match expected {
            Ok(expected) => expected,
            Err(problem) => {
                write_creating_folders(&actual_path, &encode_png(&actual)?)?;
                return Err(format!(
                    "{name}: cannot read {}: {problem}; the drawn screen is in {}",
                    path.display(),
                    actual_path.display()
                ));
            }
        };
        let (picture, changed_pixels) = difference_picture(&expected, &actual);
        let difference_path = self.failures.join(format!("{name}.difference.png"));
        if changed_pixels == 0 {
            // Pictures left by an earlier failure would only mislead once the screen matches again.
            let _ = std::fs::remove_file(&actual_path);
            let _ = std::fs::remove_file(&difference_path);
            return Ok(());
        }
        write_creating_folders(&actual_path, &encode_png(&actual)?)?;
        write_creating_folders(&difference_path, &encode_png(&picture)?)?;
        Err(format!(
            "{name}: {changed_pixels} pixels differ from {}; see {} and {}",
            path.display(),
            actual_path.display(),
            difference_path.display()
        ))
    }
}
