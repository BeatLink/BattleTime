//! Scripts for end-to-end runs: a small line-based format that opens an app, drives it and checks it.
//!
//! Each line holds one command, such as `tap "Start"`, `wait 3s` or `expect text "00:03"`, and `#`
//! starts a comment. `parse_script` turns the text into steps, naming the line of any mistake, and
//! `run_script` plays them on a `TestWatch`, stopping at the first step that fails. docs/testing.md
//! lists every command.

use std::fmt;
use std::fs::File;
use std::io::BufWriter;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::Duration;

use battletime_app_host::{MotionKind, PushedEvent};
use battletime_compiler::calls::effect_signature;
use battletime_compiler::expressions::system_signal;
use battletime_compiler::types::{Kind, Type};
use battletime_runtime::events::SwipeDirection;
use battletime_runtime::{EffectKind, SystemValue, Value};
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;

use crate::fake_system::RecordedEffect;
use crate::framebuffer::{Framebuffer, SCREEN_SIDE_IN_PIXELS};
use crate::sources::{package_source, PackageKind};
use crate::watch::TestWatch;

// ============================================================================
// Commands
// ============================================================================

/// A point on the screen, in pixels from the top left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,
}

/// Where a touch lands: a point, or the middle of the shown widget whose text is exactly this.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TouchTarget {
    Point(ScreenPoint),
    Text(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchKind {
    Tap,
    LongPress,
    DoubleTap,
}

/// One argument an effect check wants, as text or as a number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExpectedArgument {
    Text(String),
    Number(Value),
}

/// An effect of one kind, with the first few of its arguments if the script gave them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectCheck {
    pub kind: EffectKind,
    pub arguments: Vec<ExpectedArgument>,
}

impl EffectCheck {
    pub fn matches(&self, effect: &RecordedEffect) -> bool {
        effect.kind == self.kind
            && self
                .arguments
                .iter()
                .enumerate()
                .all(|(position, expected)| match expected {
                    ExpectedArgument::Text(text) => effect.text(position) == Some(text.as_str()),
                    ExpectedArgument::Number(number) => effect.number(position) == *number,
                })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptCommand {
    OpenApp(String),
    OpenFace(String),
    /// Leaves the open app, which goes on running closed with its state, and opens this app.
    SwitchToApp(String),
    /// Leaves the open app, which goes on running closed with its state, and opens this face.
    SwitchToFace(String),
    InstallApps(Vec<String>),
    Touch {
        kind: TouchKind,
        target: TouchTarget,
    },
    Drag {
        from: ScreenPoint,
        to: ScreenPoint,
    },
    Swipe(SwipeDirection),
    PressButton,
    Wait(Duration),
    SetSystemValue {
        value: SystemValue,
        number: Value,
    },
    Notify {
        /// The app it came from, which picks its icon; empty when the script names none.
        app: String,
        title: String,
        body: String,
    },
    Phone(String),
    Shake,
    Acceleration {
        x: Value,
        y: Value,
        z: Value,
    },
    /// A heart rate reading, as the sensor gives one while measuring.
    HeartRate(Value),
    /// Tells the app the phone's navigation changed; the turn itself is in the system values.
    Navigation,
    /// Writes a whole text file, as the phone does over Bluetooth.
    WriteFile {
        path: String,
        text: String,
    },
    /// Leaves the app and opens it again with its state kept, so it reads its files afresh.
    Reopen,
    /// Starts the open app on one of its screens with nothing to go back to, as the shell opens a notification's preview.
    StartOn(String),
    ScreenOn,
    ScreenOff,
    ExpectText(String),
    ExpectNoText(String),
    ExpectScreen(String),
    ExpectEffect(EffectCheck),
    ExpectNoEffect(EffectCheck),
    ExpectNoFailures,
    Screenshot(PathBuf),
    /// Compares the screen with the reference picture `<kind>-<id>-<name>.png` in `harness/snapshots/screens`.
    Snapshot(String),
}

impl ScriptCommand {
    /// Whether the command acts on the watch, as opposed to opening an app or checking one.
    fn drives_the_watch(&self) -> bool {
        !matches!(
            self,
            ScriptCommand::OpenApp(_)
                | ScriptCommand::OpenFace(_)
                | ScriptCommand::ExpectText(_)
                | ScriptCommand::ExpectNoText(_)
                | ScriptCommand::ExpectScreen(_)
                | ScriptCommand::ExpectEffect(_)
                | ScriptCommand::ExpectNoEffect(_)
                | ScriptCommand::ExpectNoFailures
                | ScriptCommand::Screenshot(_)
                | ScriptCommand::Snapshot(_)
        )
    }
}

/// One command, with the line it came from as the script wrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptStep {
    pub line_number: usize,
    pub text: String,
    pub command: ScriptCommand,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Script {
    pub steps: Vec<ScriptStep>,
}

// ============================================================================
// Parse errors
// ============================================================================

/// A line the parser could not read, by its number from 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptError {
    pub line_number: usize,
    pub message: String,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "line {}: {}", self.line_number, self.message)
    }
}

impl std::error::Error for ScriptError {}

// ============================================================================
// Splitting a line into words
// ============================================================================

/// A word of a line: bare, such as `tap` or `120`, or quoted, such as `"Start"`.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Word {
    Bare(String),
    Quoted(String),
}

impl Word {
    fn text(&self) -> &str {
        match self {
            Word::Bare(text) | Word::Quoted(text) => text,
        }
    }

    fn bare(&self) -> Option<&str> {
        match self {
            Word::Bare(text) => Some(text),
            Word::Quoted(_) => None,
        }
    }

    fn quoted(&self) -> Option<&str> {
        match self {
            Word::Quoted(text) => Some(text),
            Word::Bare(_) => None,
        }
    }
}

/// A line split into words, and how much of it comes before any comment.
struct SplitLine {
    words: Vec<Word>,
    command_length: usize,
}

/// Splits a line at spaces, keeping quoted text whole and stopping at a `#` outside quotes.
fn split_words(line: &str) -> Result<SplitLine, String> {
    let mut words = Vec::new();
    let mut characters = line.char_indices().peekable();
    while let Some(&(position, character)) = characters.peek() {
        if character.is_whitespace() {
            characters.next();
        } else if character == '#' {
            return Ok(SplitLine {
                words,
                command_length: position,
            });
        } else if character == '"' {
            characters.next();
            let mut text = String::new();
            loop {
                match characters.next() {
                    None => return Err("a quote is opened but never closed".to_string()),
                    Some((_, '"')) => break,
                    Some((_, '\\')) if matches!(characters.peek(), Some((_, '"' | '\\'))) => {
                        text.extend(characters.next().map(|(_, escaped)| escaped));
                    }
                    Some((_, other)) => text.push(other),
                }
            }
            words.push(Word::Quoted(text));
        } else {
            let mut text = String::new();
            while let Some(&(_, next)) = characters.peek() {
                if next.is_whitespace() || next == '"' || next == '#' {
                    break;
                }
                text.push(next);
                characters.next();
            }
            words.push(Word::Bare(text));
        }
    }
    Ok(SplitLine {
        words,
        command_length: line.len(),
    })
}

// ============================================================================
// Parsing
// ============================================================================

const COMMAND_NAMES: &str = "open, switch, install, tap, long-press, double-tap, drag, swipe, button, long-button, \
                             wait, set, notify, phone, shake, acceleration, heart, navigation, write, reopen, start, screen, expect, screenshot, snapshot";

/// A script's heart rate readings are as sure as the sensor ever reports.
const FULL_HEART_RATE_CONFIDENCE: Value = 100;

/// A snapshot name becomes part of a file name, so it keeps to lowercase letters, digits and hyphens.
fn is_snapshot_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name
            .chars()
            .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-')
}

/// Reads a whole script, or names the first line it cannot read.
pub fn parse_script(text: &str) -> Result<Script, ScriptError> {
    let mut steps = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line_number = index + 1;
        let to_error = |message| ScriptError { line_number, message };
        let split_line = split_words(line).map_err(to_error)?;
        if split_line.words.is_empty() {
            continue;
        }
        let command = parse_command(&split_line.words).map_err(to_error)?;
        steps.push(ScriptStep {
            line_number,
            text: line[..split_line.command_length].trim().to_string(),
            command,
        });
    }
    Ok(Script { steps })
}

/// Reads a script file, naming the file and line of any mistake.
pub fn load_script(path: &Path) -> Result<Script, String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    parse_script(&text).map_err(|error| format!("{}: {error}", path.display()))
}

fn parse_command(words: &[Word]) -> Result<ScriptCommand, String> {
    let Some(name) = words[0].bare() else {
        return Err(format!(
            "a line starts with a command, not the text \"{}\"",
            words[0].text()
        ));
    };
    let arguments = &words[1..];
    match name {
        "open" => parse_open(arguments),
        "install" => parse_install(arguments),
        "switch" => parse_switch(arguments),
        "tap" => parse_touch(TouchKind::Tap, name, arguments),
        "long-press" => parse_touch(TouchKind::LongPress, name, arguments),
        "double-tap" => parse_touch(TouchKind::DoubleTap, name, arguments),
        "drag" => parse_drag(arguments),
        "swipe" => parse_swipe(arguments),
        "button" => expect_no_arguments(name, arguments, ScriptCommand::PressButton),
        "wait" => match arguments {
            [duration] => parse_duration(duration.text()).map(ScriptCommand::Wait),
            _ => Err("write `wait <duration>`, such as `wait 3s`".to_string()),
        },
        "set" => parse_set(arguments),
        "notify" => match arguments {
            [Word::Quoted(title), Word::Quoted(body)] => Ok(ScriptCommand::Notify {
                app: String::new(),
                title: title.clone(),
                body: body.clone(),
            }),
            [Word::Quoted(app), Word::Quoted(title), Word::Quoted(body)] => Ok(ScriptCommand::Notify {
                app: app.clone(),
                title: title.clone(),
                body: body.clone(),
            }),
            _ => Err("write `notify \"title\" \"body\"`, or `notify \"app\" \"title\" \"body\"`".to_string()),
        },
        "phone" => match arguments {
            [Word::Quoted(line)] => Ok(ScriptCommand::Phone(line.clone())),
            _ => Err("write `phone \"<line>\"`, with each quote in the line written \\\"".to_string()),
        },
        "shake" => expect_no_arguments(name, arguments, ScriptCommand::Shake),
        "heart" => match arguments {
            [bpm] => bpm
                .bare()
                .and_then(parse_number)
                .map(ScriptCommand::HeartRate)
                .ok_or_else(|| "write `heart <beats per minute>`, such as `heart 72`".to_string()),
            _ => Err("write `heart <beats per minute>`, such as `heart 72`".to_string()),
        },
        "navigation" => expect_no_arguments(name, arguments, ScriptCommand::Navigation),
        "reopen" => expect_no_arguments(name, arguments, ScriptCommand::Reopen),
        "start" => match arguments {
            [on, screen] if on.bare() == Some("on") => Ok(ScriptCommand::StartOn(screen.text().to_string())),
            _ => Err("write `start on <screen>`".to_string()),
        },
        "write" => match arguments {
            [Word::Quoted(path), Word::Quoted(text)] => Ok(ScriptCommand::WriteFile {
                path: path.clone(),
                text: text.clone(),
            }),
            _ => Err("write `write \"/path\" \"text\"`".to_string()),
        },
        "acceleration" => parse_acceleration(arguments),
        "screen" => match arguments.first().and_then(Word::bare) {
            Some("on") if arguments.len() == 1 => Ok(ScriptCommand::ScreenOn),
            Some("off") if arguments.len() == 1 => Ok(ScriptCommand::ScreenOff),
            _ => Err("write `screen on` or `screen off`".to_string()),
        },
        "expect" => parse_expect(arguments),
        "screenshot" => match arguments {
            [path] => Ok(ScriptCommand::Screenshot(PathBuf::from(path.text()))),
            _ => Err("write `screenshot <path.png>`".to_string()),
        },
        "snapshot" => match arguments {
            [name] if is_snapshot_name(name.text()) => Ok(ScriptCommand::Snapshot(name.text().to_string())),
            _ => {
                Err("write `snapshot <name>`, the name in lowercase letters, digits and hyphens".to_string())
            }
        },
        unknown => Err(format!(
            "unknown command `{unknown}`; the commands are {COMMAND_NAMES}"
        )),
    }
}

fn expect_no_arguments(
    name: &str,
    arguments: &[Word],
    command: ScriptCommand,
) -> Result<ScriptCommand, String> {
    match arguments.is_empty() {
        true => Ok(command),
        false => Err(format!("`{name}` takes nothing after it")),
    }
}

fn parse_open(arguments: &[Word]) -> Result<ScriptCommand, String> {
    match arguments {
        [kind, id] if kind.bare() == Some("app") => Ok(ScriptCommand::OpenApp(id.text().to_string())),
        [kind, id] if kind.bare() == Some("face") => Ok(ScriptCommand::OpenFace(id.text().to_string())),
        _ => Err("write `open app <id>` or `open face <id>`".to_string()),
    }
}

fn parse_switch(arguments: &[Word]) -> Result<ScriptCommand, String> {
    match arguments {
        [to, kind, id] if to.bare() == Some("to") && kind.bare() == Some("app") => {
            Ok(ScriptCommand::SwitchToApp(id.text().to_string()))
        }
        [to, kind, id] if to.bare() == Some("to") && kind.bare() == Some("face") => {
            Ok(ScriptCommand::SwitchToFace(id.text().to_string()))
        }
        _ => Err("write `switch to app <id>` or `switch to face <id>`".to_string()),
    }
}

fn parse_install(arguments: &[Word]) -> Result<ScriptCommand, String> {
    match arguments.is_empty() {
        true => Err("write `install <app id> ...`, such as `install stopwatch timer`".to_string()),
        false => Ok(ScriptCommand::InstallApps(
            arguments.iter().map(|id| id.text().to_string()).collect(),
        )),
    }
}

fn parse_touch(kind: TouchKind, name: &str, arguments: &[Word]) -> Result<ScriptCommand, String> {
    let target = match arguments {
        [Word::Quoted(text)] => TouchTarget::Text(text.clone()),
        [x, y] => TouchTarget::Point(parse_point(x, y)?),
        _ => return Err(format!("write `{name} <x> <y>` or `{name} \"text\"`")),
    };
    Ok(ScriptCommand::Touch { kind, target })
}

fn parse_drag(arguments: &[Word]) -> Result<ScriptCommand, String> {
    match arguments {
        [start_x, start_y, end_x, end_y] => Ok(ScriptCommand::Drag {
            from: parse_point(start_x, start_y)?,
            to: parse_point(end_x, end_y)?,
        }),
        _ => Err("write `drag <x1> <y1> <x2> <y2>`".to_string()),
    }
}

fn parse_point(x: &Word, y: &Word) -> Result<ScreenPoint, String> {
    let coordinate = |word: &Word| {
        word.bare()
            .and_then(|text| text.parse::<i32>().ok())
            .ok_or(format!("`{}` is not a pixel position", word.text()))
    };
    Ok(ScreenPoint {
        x: coordinate(x)?,
        y: coordinate(y)?,
    })
}

fn parse_acceleration(arguments: &[Word]) -> Result<ScriptCommand, String> {
    let usage = || {
        "write `acceleration <x> <y> <z>` in thousandths of g, such as `acceleration 0 0 1000`".to_string()
    };
    let [x, y, z] = arguments else {
        return Err(usage());
    };
    let axis = |word: &Word| {
        word.bare()
            .and_then(|text| text.parse::<Value>().ok())
            .ok_or_else(usage)
    };
    Ok(ScriptCommand::Acceleration {
        x: axis(x)?,
        y: axis(y)?,
        z: axis(z)?,
    })
}

fn parse_swipe(arguments: &[Word]) -> Result<ScriptCommand, String> {
    let direction = match arguments {
        [word] => match word.bare() {
            Some("up") => SwipeDirection::Up,
            Some("down") => SwipeDirection::Down,
            Some("left") => SwipeDirection::Left,
            Some("right") => SwipeDirection::Right,
            _ => return Err(format!("`{}` is not up, down, left or right", word.text())),
        },
        _ => return Err("write `swipe up`, `swipe down`, `swipe left` or `swipe right`".to_string()),
    };
    Ok(ScriptCommand::Swipe(direction))
}

/// Reads a duration such as `250ms`, `3s`, `10min` or `2h`.
fn parse_duration(text: &str) -> Result<Duration, String> {
    let digits_end = text
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(text.len());
    let (digits, unit) = text.split_at(digits_end);
    let bad_duration =
        || format!("`{text}` is not a duration; write a whole number and ms, s, min or h, such as 3s");
    let amount: u64 = digits.parse().map_err(|_| bad_duration())?;
    let unit_in_milliseconds: u64 = match unit {
        "ms" => 1,
        "s" => 1_000,
        "min" => 60_000,
        "h" => 3_600_000,
        _ => return Err(bad_duration()),
    };
    let milliseconds = amount
        .checked_mul(unit_in_milliseconds)
        .ok_or_else(bad_duration)?;
    Ok(Duration::from_millis(milliseconds))
}

/// Reads a number, or `true` and `false` as 1 and 0.
fn parse_number(text: &str) -> Option<Value> {
    match text {
        "true" => Some(1),
        "false" => Some(0),
        _ => text.parse().ok(),
    }
}

fn parse_set(arguments: &[Word]) -> Result<ScriptCommand, String> {
    let [name, number] = arguments else {
        return Err("write `set <system value> <number>`, such as `set battery_percent 15`".to_string());
    };
    let name = name.text().replace('-', "_");
    let Some(signal) = system_signal(&name) else {
        return Err(format!(
            "`{name}` is not a system value; use the name apps read, such as battery_percent"
        ));
    };
    if signal.value == SystemValue::Now {
        return Err("the clock cannot be set; move it on with `wait`".to_string());
    }
    if signal.value.is_text() {
        return Err(format!(
            "`{name}` is text, and a script can only set number values"
        ));
    }
    let number = number
        .bare()
        .and_then(parse_number)
        .ok_or(format!("`{}` is not a number", number.text()))?;
    Ok(ScriptCommand::SetSystemValue {
        value: signal.value,
        number,
    })
}

fn parse_expect(arguments: &[Word]) -> Result<ScriptCommand, String> {
    let words: Vec<Option<&str>> = arguments.iter().map(Word::bare).collect();
    let quoted_text = |word: &Word, form: &str| {
        word.quoted()
            .map(str::to_string)
            .ok_or(format!("write `{form}`, with the text in quotes"))
    };
    match (words.as_slice(), arguments) {
        ([Some("text"), _], [_, text]) => {
            quoted_text(text, "expect text \"…\"").map(ScriptCommand::ExpectText)
        }
        ([Some("no"), Some("text"), _], [_, _, text]) => {
            quoted_text(text, "expect no text \"…\"").map(ScriptCommand::ExpectNoText)
        }
        ([Some("screen"), _], [_, name]) => Ok(ScriptCommand::ExpectScreen(name.text().to_string())),
        ([Some("effect"), ..], [_, rest @ ..]) => parse_effect_check(rest).map(ScriptCommand::ExpectEffect),
        ([Some("no"), Some("effect"), ..], [_, _, rest @ ..]) => {
            parse_effect_check(rest).map(ScriptCommand::ExpectNoEffect)
        }
        ([Some("no"), Some("failures")], _) => Ok(ScriptCommand::ExpectNoFailures),
        _ => Err(
            "write `expect text \"…\"`, `expect no text \"…\"`, `expect screen <name>`, \
                  `expect effect <name> ...`, `expect no effect <name> ...` or `expect no failures`"
                .to_string(),
        ),
    }
}

/// Reads an effect name as apps call it, with hyphens or underscores, and the arguments to match.
fn parse_effect_check(words: &[Word]) -> Result<EffectCheck, String> {
    let Some((name, arguments)) = words.split_first() else {
        return Err("name the effect, such as `expect effect vibrate`".to_string());
    };
    let name = name.text().replace('-', "_");
    let Some((kind, argument_types)) = effect_signature(&name) else {
        return Err(format!(
            "`{name}` is not an effect; use the name apps call, such as vibrate or open_app"
        ));
    };
    if arguments.len() > argument_types.len() {
        return Err(format!(
            "`{name}` has {} arguments, not {}",
            argument_types.len(),
            arguments.len()
        ));
    }
    let arguments = arguments
        .iter()
        .zip(argument_types)
        .map(|(word, argument_type)| parse_expected_argument(word.text(), *argument_type))
        .collect::<Result<_, _>>()?;
    Ok(EffectCheck { kind, arguments })
}

fn parse_expected_argument(text: &str, argument_type: Type) -> Result<ExpectedArgument, String> {
    let number = match argument_type.kind {
        Kind::Text | Kind::Name => return Ok(ExpectedArgument::Text(text.to_string())),
        Kind::Choice(choices) => choices
            .number_of(text)
            .or_else(|| text.parse().ok())
            .ok_or(format!("`{text}` is not one of {}", choices.names().join(", ")))?,
        _ => parse_number(text).ok_or(format!("`{text}` is not a number"))?,
    };
    Ok(ExpectedArgument::Number(number))
}

// ============================================================================
// Results
// ============================================================================

/// The step that did not pass: its line, the command as written, what it wanted and what was there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepFailure {
    pub line_number: usize,
    pub command: String,
    pub expected: String,
    pub found: String,
}

impl fmt::Display for StepFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(
            formatter,
            "line {}: {}\n  expected {}\n  found {}",
            self.line_number, self.command, self.expected, self.found
        )
    }
}

/// What a whole run did: the steps that passed, in order, and the step that failed, if one did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptRun {
    pub passed_steps: Vec<ScriptStep>,
    pub failure: Option<StepFailure>,
}

impl ScriptRun {
    pub fn passed(&self) -> bool {
        self.failure.is_none()
    }
}

/// One step's result, handed out as the script runs so a caller can print it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepOutcome<'run> {
    Passed(&'run ScriptStep),
    Failed(&'run StepFailure),
}

impl fmt::Display for StepOutcome<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StepOutcome::Passed(step) => write!(formatter, "ok    line {}: {}", step.line_number, step.text),
            StepOutcome::Failed(failure) => write!(formatter, "FAIL  {failure}"),
        }
    }
}

/// What a step wanted and what it found instead.
struct Mismatch {
    expected: String,
    found: String,
}

fn mismatch(expected: impl Into<String>, found: impl Into<String>) -> Mismatch {
    Mismatch {
        expected: expected.into(),
        found: found.into(),
    }
}

// ============================================================================
// Running
// ============================================================================

/// Runs a script from nothing open, saving relative `screenshot` paths under `output_folder`.
pub fn run_script(script: &Script, output_folder: &Path, on_step: impl FnMut(StepOutcome)) -> ScriptRun {
    run_steps(None, script, output_folder, on_step)
}

/// Runs a script on a watch that is already open, such as one with an app written in a test.
pub fn run_script_on(
    watch: TestWatch,
    script: &Script,
    output_folder: &Path,
    on_step: impl FnMut(StepOutcome),
) -> ScriptRun {
    run_steps(Some(watch), script, output_folder, on_step)
}

fn run_steps(
    watch: Option<TestWatch>,
    script: &Script,
    output_folder: &Path,
    mut on_step: impl FnMut(StepOutcome),
) -> ScriptRun {
    let mut runner = ScriptRunner {
        watch,
        output_folder,
        first_effect_to_check: 0,
        has_checked_effects_since_driving: false,
        next_notification_id: 1,
        opened_name: String::new(),
    };
    let mut passed_steps = Vec::new();
    for step in &script.steps {
        let result = catch_unwind(AssertUnwindSafe(|| runner.carry_out(&step.command)))
            .unwrap_or_else(|panic| Err(mismatch("the step to finish", panic_message(panic.as_ref()))));
        match result {
            Ok(()) => {
                on_step(StepOutcome::Passed(step));
                passed_steps.push(step.clone());
            }
            Err(Mismatch { expected, found }) => {
                let failure = StepFailure {
                    line_number: step.line_number,
                    command: step.text.clone(),
                    expected,
                    found,
                };
                on_step(StepOutcome::Failed(&failure));
                return ScriptRun {
                    passed_steps,
                    failure: Some(failure),
                };
            }
        }
    }
    ScriptRun {
        passed_steps,
        failure: None,
    }
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("no message");
    format!("a panic: {message}")
}

/// The watch a script drives, and which of its effects the next effect check looks at.
struct ScriptRunner<'folder> {
    watch: Option<TestWatch>,
    output_folder: &'folder Path,
    /// Effect checks look only at effects from here on, so a later check is not met by an earlier step.
    first_effect_to_check: usize,
    has_checked_effects_since_driving: bool,
    next_notification_id: Value,
    /// The open package as reference pictures name it, such as `app-stopwatch`.
    opened_name: String,
}

impl ScriptRunner<'_> {
    fn carry_out(&mut self, command: &ScriptCommand) -> Result<(), Mismatch> {
        match command {
            ScriptCommand::OpenApp(id) => {
                // The shell's screens open as apps too, but their pictures are named `shell-<id>`.
                let is_shell = !package_source(PackageKind::App, id).is_file()
                    && package_source(PackageKind::Shell, id).is_file();
                let kind = if is_shell { "shell" } else { "app" };
                return self.open(TestWatch::open_app(id), kind, id);
            }
            ScriptCommand::OpenFace(id) => return self.open(TestWatch::open_face(id), "face", id),
            _ => {}
        }
        if command.drives_the_watch() {
            self.start_new_effect_window();
        }
        let output_folder = self.output_folder;
        let watch = self.watch.as_mut().ok_or_else(|| {
            mismatch(
                "an app or face opened first, with `open app <id>` or `open face <id>`",
                "nothing open",
            )
        })?;
        match command {
            ScriptCommand::OpenApp(_) | ScriptCommand::OpenFace(_) => {}
            ScriptCommand::SwitchToApp(id) => watch
                .switch_to_app(id)
                .map_err(|failure| mismatch(format!("the app {id} to open"), failure.to_string()))?,
            ScriptCommand::SwitchToFace(id) => watch
                .switch_to_face(id)
                .map_err(|failure| mismatch(format!("the face {id} to open"), failure.to_string()))?,
            ScriptCommand::InstallApps(ids) => {
                let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
                watch
                    .install_apps(&ids)
                    .map_err(|failure| mismatch("the apps to install", failure.to_string()))?;
            }
            ScriptCommand::Touch { kind, target } => {
                let point = touch_point(watch, target)?;
                match kind {
                    TouchKind::Tap => watch.tap(point.x, point.y),
                    TouchKind::LongPress => watch.long_press(point.x, point.y),
                    TouchKind::DoubleTap => watch.double_tap(point.x, point.y),
                };
            }
            ScriptCommand::Drag { from, to } => {
                watch.drag((from.x, from.y), (to.x, to.y));
            }
            ScriptCommand::Swipe(direction) => {
                watch.swipe(*direction);
            }
            ScriptCommand::PressButton => {
                watch.press_button();
            }
            ScriptCommand::Wait(duration) => watch.advance(*duration),
            ScriptCommand::SetSystemValue { value, number } => watch.set_system_value(*value, *number),
            ScriptCommand::Notify { app, title, body } => {
                let id = self.next_notification_id;
                self.next_notification_id += 1;
                let time = watch.now();
                watch.deliver(PushedEvent::Notification {
                    id,
                    app,
                    title,
                    body,
                    time,
                });
            }
            ScriptCommand::Phone(line) => {
                watch
                    .receive_from_phone(line)
                    .map_err(|failure| mismatch("a line the watch takes", failure))?;
            }
            ScriptCommand::Shake => {
                watch.deliver(PushedEvent::Motion(MotionKind::Shake));
            }
            ScriptCommand::Acceleration { x, y, z } => {
                watch.deliver(PushedEvent::Acceleration { x: *x, y: *y, z: *z });
            }
            ScriptCommand::HeartRate(beats_per_minute) => {
                watch.deliver(PushedEvent::HeartRate {
                    beats_per_minute: *beats_per_minute,
                    confidence: FULL_HEART_RATE_CONFIDENCE,
                });
            }
            ScriptCommand::Navigation => {
                watch.deliver(PushedEvent::Navigation);
            }
            ScriptCommand::WriteFile { path, text } => {
                watch.write_file(path, text.as_bytes());
            }
            ScriptCommand::Reopen => {
                watch.reopen();
            }
            ScriptCommand::StartOn(screen) => {
                if !watch.start_on_screen(screen) {
                    return Err(mismatch(format!("a screen named {screen}"), "no such screen"));
                }
            }
            ScriptCommand::ScreenOn => {
                watch.screen_on();
            }
            ScriptCommand::ScreenOff => {
                watch.screen_off();
            }
            ScriptCommand::ExpectText(text) => {
                if !watch.shows_text(text) {
                    return Err(mismatch(format!("text {text:?} shown"), shown_texts(watch)));
                }
            }
            ScriptCommand::ExpectNoText(text) => {
                if watch.shows_text(text) {
                    return Err(mismatch(format!("no text {text:?} shown"), shown_texts(watch)));
                }
            }
            ScriptCommand::ExpectScreen(name) => {
                if watch.screen_name() != Some(name.as_str()) {
                    let found = match watch.screen_name() {
                        Some(shown) => format!("screen {shown}"),
                        None => "a screen with no name".to_string(),
                    };
                    return Err(mismatch(format!("screen {name}"), found));
                }
            }
            ScriptCommand::ExpectEffect(check) => {
                let effects = &watch.effects()[self.first_effect_to_check.min(watch.effects().len())..];
                self.has_checked_effects_since_driving = true;
                if !effects.iter().any(|effect| check.matches(effect)) {
                    return Err(mismatch(describe_check(check), describe_effects(effects)));
                }
            }
            ScriptCommand::ExpectNoEffect(check) => {
                let effects = &watch.effects()[self.first_effect_to_check.min(watch.effects().len())..];
                self.has_checked_effects_since_driving = true;
                if effects.iter().any(|effect| check.matches(effect)) {
                    return Err(mismatch(
                        format!("no {}", describe_check(check)),
                        describe_effects(effects),
                    ));
                }
            }
            ScriptCommand::ExpectNoFailures => {
                if !watch.failures().is_empty() {
                    return Err(mismatch("no failures", format!("{:?}", watch.failures())));
                }
            }
            ScriptCommand::Screenshot(path) => {
                let path = output_folder.join(path);
                save_png(watch.frame(), &path)
                    .map_err(|problem| mismatch(format!("to save {}", path.display()), problem))?;
            }
            ScriptCommand::Snapshot(_) if self.opened_name.is_empty() => {
                return Err(mismatch(
                    "an app or face opened by the script, which names the picture",
                    "a watch opened before the script",
                ));
            }
            ScriptCommand::Snapshot(name) => {
                // A switch, or a closed app that woke, may have opened another package since the script opened one.
                let picture = format!("{}-{name}", watch.package_name());
                crate::snapshots::check_screen_snapshot(&picture, watch.frame())
                    .map_err(|problem| mismatch(format!("the screen to match {picture}.png"), problem))?;
            }
        }
        Ok(())
    }

    fn open(
        &mut self,
        opened: Result<TestWatch, crate::sources::OpenFailure>,
        kind: &str,
        id: &str,
    ) -> Result<(), Mismatch> {
        let watch =
            opened.map_err(|failure| mismatch(format!("the {kind} {id} to open"), failure.to_string()))?;
        self.watch = Some(watch);
        self.opened_name = format!("{kind}-{id}");
        self.first_effect_to_check = 0;
        self.has_checked_effects_since_driving = false;
        Ok(())
    }

    /// After an effect check, the next step that drives the watch starts a fresh set of effects to check.
    fn start_new_effect_window(&mut self) {
        if !self.has_checked_effects_since_driving {
            return;
        }
        self.has_checked_effects_since_driving = false;
        if let Some(watch) = &self.watch {
            self.first_effect_to_check = watch.effects().len();
        }
    }
}

// ============================================================================
// Finding and describing
// ============================================================================

fn touch_point(watch: &TestWatch, target: &TouchTarget) -> Result<ScreenPoint, Mismatch> {
    let text = match target {
        TouchTarget::Point(point) => return Ok(*point),
        TouchTarget::Text(text) => text,
    };
    let area = watch.area_of_text(text);
    match area {
        Some(area) => Ok(ScreenPoint {
            x: area.x + area.width / 2,
            y: area.y + area.height / 2,
        }),
        None => Err(mismatch(
            format!("a shown widget reading exactly {text:?}"),
            shown_texts(watch),
        )),
    }
}

fn shown_texts(watch: &TestWatch) -> String {
    format!("the screen shows {:?}", watch.visible_texts())
}

fn describe_check(check: &EffectCheck) -> String {
    let arguments: Vec<String> = check
        .arguments
        .iter()
        .map(|argument| match argument {
            ExpectedArgument::Text(text) => format!("{text:?}"),
            ExpectedArgument::Number(number) => number.to_string(),
        })
        .collect();
    match arguments.is_empty() {
        true => format!("effect {:?}", check.kind),
        false => format!("effect {:?}({}, ...)", check.kind, arguments.join(", ")),
    }
}

/// Lists effects as `Vibrate(30)` or `OpenApp("timer")`, as the simulator prints them.
fn describe_effects(effects: &[RecordedEffect]) -> String {
    if effects.is_empty() {
        return "no effects in the steps it covers".to_string();
    }
    let described: Vec<String> = effects
        .iter()
        .map(|effect| {
            let arguments: Vec<String> = (0..effect.numbers.len())
                .map(|position| match effect.text(position) {
                    Some(text) => format!("{text:?}"),
                    None => effect.number(position).to_string(),
                })
                .collect();
            format!("{:?}({})", effect.kind, arguments.join(", "))
        })
        .collect();
    format!("the effects {}", described.join(", "))
}

// ============================================================================
// Screenshots
// ============================================================================

/// Saves the screen as a 240x240 RGB PNG, making the folder if it is missing.
fn save_png(framebuffer: &Framebuffer, path: &Path) -> Result<(), String> {
    if let Some(folder) = path.parent().filter(|folder| !folder.as_os_str().is_empty()) {
        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    let file = File::create(path).map_err(|error| error.to_string())?;
    let side = SCREEN_SIDE_IN_PIXELS as u32;
    let mut encoder = png::Encoder::new(BufWriter::new(file), side, side);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let bytes: Vec<u8> = framebuffer
        .pixels()
        .iter()
        .flat_map(|&pixel| {
            let color = Rgb888::from(pixel);
            [color.r(), color.g(), color.b()]
        })
        .collect();
    let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
    writer
        .write_image_data(&bytes)
        .map_err(|error| error.to_string())?;
    writer.finish().map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "script_tests.rs"]
mod script_tests;
