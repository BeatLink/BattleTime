//! Format patterns for times and durations, such as `"dddd D MMMM"` or `"mm:ss.cc"`.
//!
//! A pattern is a string in the app image made of tokens, text in `[brackets]` copied as it is,
//! and any other character that is not a letter. A letter outside a token or brackets makes the
//! pattern invalid, so a typo is caught when the app is installed.

use crate::text::{BufferFull, DateTime, TextWriter};

// ============================================================================
// Names
// ============================================================================

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const WEEKDAY_NAMES: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// The length of a short name, such as `Oct` or `Sun`.
const SHORT_NAME_LETTERS: usize = 3;

/// Patterns with a name of their own, which stand for a whole pattern.
const CLOCK: &str = "clock";
const CLOCK_WITH_SECONDS: &str = "clock_with_seconds";
const DATE: &str = "date";

// ============================================================================
// Steps
// ============================================================================

const SECOND_IN_MILLISECONDS: u32 = 1000;
const MINUTE_IN_MILLISECONDS: u32 = 60 * SECOND_IN_MILLISECONDS;

/// The fastest a pattern asks to be redrawn; hundredths change faster than the screen can show.
const FASTEST_STEP_IN_MILLISECONDS: u32 = 100;

// ============================================================================
// Time tokens
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimeToken<'pattern> {
    Literal(&'pattern str),
    /// `YYYY`
    Year,
    /// `MMMM`, `MMM`
    MonthName {
        short: bool,
    },
    /// `MM`, `M`
    Month {
        padded: bool,
    },
    /// `dddd`, `ddd`
    WeekdayName {
        short: bool,
    },
    /// `DD`, `D`
    Day {
        padded: bool,
    },
    /// `HH`, `H`
    Hour24 {
        padded: bool,
    },
    /// `hh`, `h`
    Hour12 {
        padded: bool,
    },
    /// `mm`, `m`
    Minute {
        padded: bool,
    },
    /// `ss`, `s`
    Second {
        padded: bool,
    },
    /// `a` gives `am` or `pm`, and `A` gives `AM` or `PM`.
    Meridiem {
        upper_case: bool,
    },
}

/// Time tokens, longest first so that `MMMM` is not read as `MM` twice.
const TIME_TOKENS: [(&str, TimeToken); 19] = [
    ("YYYY", TimeToken::Year),
    ("MMMM", TimeToken::MonthName { short: false }),
    ("MMM", TimeToken::MonthName { short: true }),
    ("MM", TimeToken::Month { padded: true }),
    ("M", TimeToken::Month { padded: false }),
    ("dddd", TimeToken::WeekdayName { short: false }),
    ("ddd", TimeToken::WeekdayName { short: true }),
    ("DD", TimeToken::Day { padded: true }),
    ("D", TimeToken::Day { padded: false }),
    ("HH", TimeToken::Hour24 { padded: true }),
    ("H", TimeToken::Hour24 { padded: false }),
    ("hh", TimeToken::Hour12 { padded: true }),
    ("h", TimeToken::Hour12 { padded: false }),
    ("mm", TimeToken::Minute { padded: true }),
    ("m", TimeToken::Minute { padded: false }),
    ("ss", TimeToken::Second { padded: true }),
    ("s", TimeToken::Second { padded: false }),
    ("a", TimeToken::Meridiem { upper_case: false }),
    ("A", TimeToken::Meridiem { upper_case: true }),
];

/// Splits off the first token of a pattern, or returns `None` if the pattern is invalid there.
fn next_token<'pattern, Token: Copy>(
    pattern: &'pattern str,
    tokens: &[(&str, Token)],
    literal: fn(&'pattern str) -> Token,
) -> Option<(Token, &'pattern str)> {
    if let Some(inside) = pattern.strip_prefix('[') {
        let end = inside.find(']')?;
        return Some((literal(&inside[..end]), &inside[end + 1..]));
    }
    for (text, token) in tokens {
        if let Some(rest) = pattern.strip_prefix(text) {
            return Some((*token, rest));
        }
    }
    let first = pattern.chars().next()?;
    if first.is_ascii_alphabetic() {
        return None;
    }
    let length = first.len_utf8();
    Some((literal(&pattern[..length]), &pattern[length..]))
}

fn next_time_token(pattern: &str) -> Option<(TimeToken<'_>, &str)> {
    next_token(pattern, &TIME_TOKENS, TimeToken::Literal)
}

/// Turns a named pattern into its tokens, and leaves any other pattern as it is.
fn expand_named_time_pattern(pattern: &str, uses_24_hour_clock: bool) -> &str {
    match (pattern, uses_24_hour_clock) {
        (CLOCK, true) => "HH:mm",
        (CLOCK, false) => "h:mm",
        (CLOCK_WITH_SECONDS, true) => "HH:mm:ss",
        (CLOCK_WITH_SECONDS, false) => "h:mm:ss",
        (DATE, _) => "YYYY-MM-DD",
        (other, _) => other,
    }
}

/// Whether a time pattern is a named pattern or made only of tokens and literal text.
pub fn is_valid_time_pattern(pattern: &str) -> bool {
    let mut rest = expand_named_time_pattern(pattern, true);
    while !rest.is_empty() {
        match next_time_token(rest) {
            Some((_, after)) => rest = after,
            None => return false,
        }
    }
    true
}

/// How often text in this pattern can change as the clock runs.
pub fn time_pattern_step_in_milliseconds(pattern: &str) -> u32 {
    let mut rest = expand_named_time_pattern(pattern, true);
    let mut step = MINUTE_IN_MILLISECONDS;
    while let Some((token, after)) = next_time_token(rest) {
        if matches!(token, TimeToken::Second { .. }) {
            step = SECOND_IN_MILLISECONDS;
        }
        rest = after;
    }
    step
}

fn write_name(writer: &mut TextWriter, name: &str, short: bool) -> Result<(), BufferFull> {
    let length = if short {
        SHORT_NAME_LETTERS.min(name.len())
    } else {
        name.len()
    };
    writer.write_bytes(&name.as_bytes()[..length])
}

/// Writes a local time in a pattern. An invalid pattern writes what comes before the problem.
pub fn write_time(
    writer: &mut TextWriter,
    time: &DateTime,
    pattern: &str,
    uses_24_hour_clock: bool,
) -> Result<(), BufferFull> {
    let digits = |padded: bool| if padded { 2 } else { 1 };
    let twelve_hour = if time.hour % 12 == 0 { 12 } else { time.hour % 12 };
    let month_index = (time.month - 1).clamp(0, 11) as usize;
    let weekday_index = time.weekday.clamp(0, 6) as usize;

    let mut rest = expand_named_time_pattern(pattern, uses_24_hour_clock);
    while let Some((token, after)) = next_time_token(rest) {
        match token {
            TimeToken::Literal(text) => writer.write_bytes(text.as_bytes())?,
            TimeToken::Year => writer.write_number(time.year, 4)?,
            TimeToken::MonthName { short } => write_name(writer, MONTH_NAMES[month_index], short)?,
            TimeToken::Month { padded } => writer.write_number(time.month, digits(padded))?,
            TimeToken::WeekdayName { short } => write_name(writer, WEEKDAY_NAMES[weekday_index], short)?,
            TimeToken::Day { padded } => writer.write_number(time.day, digits(padded))?,
            TimeToken::Hour24 { padded } => writer.write_number(time.hour, digits(padded))?,
            TimeToken::Hour12 { padded } => writer.write_number(twelve_hour, digits(padded))?,
            TimeToken::Minute { padded } => writer.write_number(time.minute, digits(padded))?,
            TimeToken::Second { padded } => writer.write_number(time.second, digits(padded))?,
            TimeToken::Meridiem { upper_case } => {
                let text = match (time.hour < 12, upper_case) {
                    (true, false) => "am",
                    (false, false) => "pm",
                    (true, true) => "AM",
                    (false, true) => "PM",
                };
                writer.write_bytes(text.as_bytes())?
            }
        }
        rest = after;
    }
    Ok(())
}

// ============================================================================
// Duration tokens
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DurationToken<'pattern> {
    Literal(&'pattern str),
    /// `hh`, `h`
    Hours {
        padded: bool,
    },
    /// `mm`, `m`
    Minutes {
        padded: bool,
    },
    /// `ss`, `s`
    Seconds {
        padded: bool,
    },
    /// `cc`, hundredths of a second, always two digits.
    Hundredths,
}

const DURATION_TOKENS: [(&str, DurationToken); 7] = [
    ("hh", DurationToken::Hours { padded: true }),
    ("h", DurationToken::Hours { padded: false }),
    ("mm", DurationToken::Minutes { padded: true }),
    ("m", DurationToken::Minutes { padded: false }),
    ("ss", DurationToken::Seconds { padded: true }),
    ("s", DurationToken::Seconds { padded: false }),
    ("cc", DurationToken::Hundredths),
];

fn next_duration_token(pattern: &str) -> Option<(DurationToken<'_>, &str)> {
    next_token(pattern, &DURATION_TOKENS, DurationToken::Literal)
}

pub fn is_valid_duration_pattern(pattern: &str) -> bool {
    let mut rest = pattern;
    while !rest.is_empty() {
        match next_duration_token(rest) {
            Some((_, after)) => rest = after,
            None => return false,
        }
    }
    true
}

/// The smallest unit a duration pattern shows, which is how often it can change.
pub fn duration_pattern_step_in_milliseconds(pattern: &str) -> u32 {
    let mut rest = pattern;
    let mut step = MINUTE_IN_MILLISECONDS;
    while let Some((token, after)) = next_duration_token(rest) {
        let token_step = match token {
            DurationToken::Hundredths => FASTEST_STEP_IN_MILLISECONDS,
            DurationToken::Seconds { .. } => SECOND_IN_MILLISECONDS,
            _ => MINUTE_IN_MILLISECONDS,
        };
        step = step.min(token_step);
        rest = after;
    }
    step
}

/// Writes a duration in a pattern; a negative duration is shown as zero.
///
/// The largest unit in the pattern shows the whole amount, so `mm:ss` shows 75 minutes as `75:00`.
pub fn write_duration(writer: &mut TextWriter, milliseconds: i64, pattern: &str) -> Result<(), BufferFull> {
    const MILLISECONDS_PER_SECOND: i64 = 1000;
    const MILLISECONDS_PER_MINUTE: i64 = 60 * MILLISECONDS_PER_SECOND;
    const MILLISECONDS_PER_HOUR: i64 = 60 * MILLISECONDS_PER_MINUTE;

    let milliseconds = milliseconds.max(0);
    let mut shows_hours = false;
    let mut shows_minutes = false;
    let mut rest = pattern;
    while let Some((token, after)) = next_duration_token(rest) {
        shows_hours |= matches!(token, DurationToken::Hours { .. });
        shows_minutes |= matches!(token, DurationToken::Minutes { .. });
        rest = after;
    }

    let hours = milliseconds / MILLISECONDS_PER_HOUR;
    let minutes = match shows_hours {
        true => milliseconds % MILLISECONDS_PER_HOUR / MILLISECONDS_PER_MINUTE,
        false => milliseconds / MILLISECONDS_PER_MINUTE,
    };
    let seconds = match shows_hours || shows_minutes {
        true => milliseconds % MILLISECONDS_PER_MINUTE / MILLISECONDS_PER_SECOND,
        false => milliseconds / MILLISECONDS_PER_SECOND,
    };
    let hundredths = milliseconds % MILLISECONDS_PER_SECOND / 10;
    let digits = |padded: bool| if padded { 2 } else { 1 };

    let mut rest = pattern;
    while let Some((token, after)) = next_duration_token(rest) {
        match token {
            DurationToken::Literal(text) => writer.write_bytes(text.as_bytes())?,
            DurationToken::Hours { padded } => writer.write_number(hours, digits(padded))?,
            DurationToken::Minutes { padded } => writer.write_number(minutes, digits(padded))?,
            DurationToken::Seconds { padded } => writer.write_number(seconds, digits(padded))?,
            DurationToken::Hundredths => writer.write_number(hundredths, 2)?,
        }
        rest = after;
    }
    Ok(())
}
