//! The watch's wall clock, and the ways phones read and set it.
//!
//! The clock counts UTC and keeps the time zone and daylight saving offset beside it, so it can
//! give both UTC and local time. Phones set it through the Bluetooth Current Time service or a
//! companion line. Current Time (0x2A2B) is ten bytes: the year, month, day, hour, minute and
//! second of the local time, then the day of the week, the fraction of the second in 256ths and why
//! it last changed. Local Time Information (0x2A0F) is two bytes: the time zone and the daylight
//! saving offset, both in quarter hours. Gadgetbridge's Bangle.js lines carry
//! `setTime(<UTC seconds>)` and `E.setTimeZone(<hours>)`. The wearer can also set the local time by
//! hand in the Settings app. Times here are milliseconds since 1970.

#![no_std]

// ============================================================================
// Calendar arithmetic
// ============================================================================

const MILLISECONDS_PER_SECOND: i64 = 1000;
const SECONDS_PER_MINUTE: i64 = 60;
const SECONDS_PER_HOUR: i64 = 3600;
const SECONDS_PER_DAY: i64 = 86_400;
const MILLISECONDS_PER_QUARTER_HOUR: i64 = 15 * 60 * MILLISECONDS_PER_SECOND;
const DAYS_PER_400_YEARS: i64 = 146_097;
/// Days from 0000-03-01, where the calendar arithmetic counts from, to 1970-01-01.
const DAYS_FROM_YEAR_ZERO_TO_1970: i64 = 719_468;
/// 1970-01-01 was a Thursday, the fourth day of a week that starts on Monday.
const WEEKDAY_OF_1970_FROM_MONDAY: i64 = 3;

/// A calendar date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: i64,
    pub month: i64,
    pub day: i64,
}

/// Days from 1970-01-01 to a date, by Howard Hinnant's `days_from_civil`.
pub fn days_since_1970(date: Date) -> i64 {
    let year = if date.month <= 2 { date.year - 1 } else { date.year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_from_march = (date.month + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5 + date.day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * DAYS_PER_400_YEARS + day_of_era - DAYS_FROM_YEAR_ZERO_TO_1970
}

/// The date a number of days after 1970-01-01, by Howard Hinnant's `civil_from_days`.
pub fn date_from_days_since_1970(days: i64) -> Date {
    let days_from_year_zero = days + DAYS_FROM_YEAR_ZERO_TO_1970;
    let era = days_from_year_zero.div_euclid(DAYS_PER_400_YEARS);
    let day_of_era = days_from_year_zero - era * DAYS_PER_400_YEARS;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
    Date { year, month, day }
}

fn is_leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

// ============================================================================
// Current Time (0x2A2B)
// ============================================================================

pub const CURRENT_TIME_BYTES: usize = 10;

/// The earliest and latest years a phone may set, which keeps the clock's arithmetic far from overflow.
const EARLIEST_YEAR: i64 = 1970;
const LATEST_YEAR: i64 = 2200;
/// The year field's value for a year the watch cannot say.
const UNKNOWN_YEAR: u16 = 0;
/// The adjust reason the watch reports: no particular reason.
const NO_ADJUST_REASON: u8 = 0;
const FRACTIONS_PER_SECOND: i64 = 256;

/// The Current Time value for a local time.
pub fn encode_current_time(milliseconds_since_1970: i64) -> [u8; CURRENT_TIME_BYTES] {
    let seconds = milliseconds_since_1970.div_euclid(MILLISECONDS_PER_SECOND);
    let milliseconds_into_second = milliseconds_since_1970.rem_euclid(MILLISECONDS_PER_SECOND);
    let days = seconds.div_euclid(SECONDS_PER_DAY);
    let seconds_into_day = seconds.rem_euclid(SECONDS_PER_DAY);
    let date = date_from_days_since_1970(days);

    let year = u16::try_from(date.year).unwrap_or(UNKNOWN_YEAR).to_le_bytes();
    let weekday_from_monday = (days + WEEKDAY_OF_1970_FROM_MONDAY).rem_euclid(7) + 1;
    let fractions = milliseconds_into_second * FRACTIONS_PER_SECOND / MILLISECONDS_PER_SECOND;
    [
        year[0],
        year[1],
        date.month as u8,
        date.day as u8,
        (seconds_into_day / SECONDS_PER_HOUR) as u8,
        (seconds_into_day % SECONDS_PER_HOUR / SECONDS_PER_MINUTE) as u8,
        (seconds_into_day % SECONDS_PER_MINUTE) as u8,
        weekday_from_monday as u8,
        fractions as u8,
        NO_ADJUST_REASON,
    ]
}

/// The local time a written Current Time value holds, or `None` if it is short or not a real time.
pub fn decode_current_time(bytes: &[u8]) -> Option<i64> {
    let [year_low, year_high, month, day, hour, minute, second, ref rest @ ..] = *bytes else {
        return None;
    };
    let year = u16::from_le_bytes([year_low, year_high]) as i64;
    let (month, day) = (month as i64, day as i64);
    let (hour, minute, second) = (hour as i64, minute as i64, second as i64);
    if !(EARLIEST_YEAR..=LATEST_YEAR).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=days_in_month(year, month)).contains(&day)
        || hour >= 24
        || minute >= 60
        || second >= 60
    {
        return None;
    }
    let fractions = rest.get(1).copied().unwrap_or(0) as i64;

    let days = days_since_1970(Date { year, month, day });
    let seconds = days * SECONDS_PER_DAY + hour * SECONDS_PER_HOUR + minute * SECONDS_PER_MINUTE + second;
    Some(seconds * MILLISECONDS_PER_SECOND + fractions * MILLISECONDS_PER_SECOND / FRACTIONS_PER_SECOND)
}

// ============================================================================
// Setting the clock by hand
// ============================================================================

const MINUTES_PER_DAY: i64 = 1440;

/// The local time at the start of a minute of a day of a year, both counted from 1 January at 00:00, or `None` unless real.
pub fn local_time_from_day_and_minute(year: i64, day_of_year: i64, minute_of_day: i64) -> Option<i64> {
    let days_in_year = if is_leap_year(year) { 366 } else { 365 };
    if !(EARLIEST_YEAR..=LATEST_YEAR).contains(&year)
        || !(1..=days_in_year).contains(&day_of_year)
        || !(0..MINUTES_PER_DAY).contains(&minute_of_day)
    {
        return None;
    }
    let first_of_january = Date {
        year,
        month: 1,
        day: 1,
    };
    let days = days_since_1970(first_of_january) + day_of_year - 1;
    let seconds = days * SECONDS_PER_DAY + minute_of_day * SECONDS_PER_MINUTE;
    Some(seconds * MILLISECONDS_PER_SECOND)
}

// ============================================================================
// Local Time Information (0x2A0F)
// ============================================================================

pub const LOCAL_TIME_INFORMATION_BYTES: usize = 2;

/// The time zone field's value when the zone is not known, which counts as UTC.
pub const UNKNOWN_TIME_ZONE: i8 = -128;
const LOWEST_TIME_ZONE: i8 = -48;
const HIGHEST_TIME_ZONE: i8 = 56;
/// The daylight saving field's value when it is not known, which counts as none.
pub const UNKNOWN_DAYLIGHT_SAVING: u8 = 255;
/// The daylight saving offsets the field may hold: none, half an hour, one hour and two hours.
const DAYLIGHT_SAVING_OFFSETS: [u8; 4] = [0, 2, 4, 8];

/// The time zone and daylight saving offset, each in quarter hours ahead of UTC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTimeInformation {
    pub time_zone_in_quarter_hours: i8,
    pub daylight_saving_in_quarter_hours: u8,
}

impl LocalTimeInformation {
    /// What the watch serves until a phone writes its zone: UTC with no daylight saving.
    pub const UTC: Self = Self {
        time_zone_in_quarter_hours: 0,
        daylight_saving_in_quarter_hours: 0,
    };

    /// How far local time is ahead of UTC; an unknown part counts as 0.
    pub fn offset_in_milliseconds(self) -> i64 {
        let time_zone = match self.time_zone_in_quarter_hours {
            UNKNOWN_TIME_ZONE => 0,
            known => known as i64,
        };
        let daylight_saving = match self.daylight_saving_in_quarter_hours {
            UNKNOWN_DAYLIGHT_SAVING => 0,
            known => known as i64,
        };
        (time_zone + daylight_saving) * MILLISECONDS_PER_QUARTER_HOUR
    }

    pub fn encode(self) -> [u8; LOCAL_TIME_INFORMATION_BYTES] {
        [
            self.time_zone_in_quarter_hours as u8,
            self.daylight_saving_in_quarter_hours,
        ]
    }

    /// The zone a written value holds, or `None` if it is short or outside what the field allows.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let [time_zone, daylight_saving, ..] = *bytes else {
            return None;
        };
        let time_zone = time_zone as i8;
        let time_zone_is_valid =
            (LOWEST_TIME_ZONE..=HIGHEST_TIME_ZONE).contains(&time_zone) || time_zone == UNKNOWN_TIME_ZONE;
        let daylight_saving_is_valid =
            DAYLIGHT_SAVING_OFFSETS.contains(&daylight_saving) || daylight_saving == UNKNOWN_DAYLIGHT_SAVING;
        if !time_zone_is_valid || !daylight_saving_is_valid {
            return None;
        }
        Some(Self {
            time_zone_in_quarter_hours: time_zone,
            daylight_saving_in_quarter_hours: daylight_saving,
        })
    }
}

// ============================================================================
// The wall clock
// ============================================================================

/// UTC counted on from a timer, with the zone it is shown in.
///
/// Uptime is the timer's reading in milliseconds, which the caller passes in, so tests need no timer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WallClock {
    utc_when_uptime_was_zero: i64,
    zone: LocalTimeInformation,
}

impl WallClock {
    /// A clock in UTC that read `utc` when the timer read 0.
    pub const fn starting_at(utc: i64) -> Self {
        Self {
            utc_when_uptime_was_zero: utc,
            zone: LocalTimeInformation::UTC,
        }
    }

    pub fn utc(&self, uptime: i64) -> i64 {
        self.utc_when_uptime_was_zero + uptime
    }

    pub fn local(&self, uptime: i64) -> i64 {
        self.utc(uptime) + self.utc_offset_in_milliseconds()
    }

    pub fn utc_offset_in_milliseconds(&self) -> i64 {
        self.zone.offset_in_milliseconds()
    }

    pub fn zone(&self) -> LocalTimeInformation {
        self.zone
    }

    pub fn set_utc(&mut self, utc: i64, uptime: i64) {
        self.utc_when_uptime_was_zero = utc - uptime;
    }

    /// Sets the clock from a local time, read in the zone the clock already has.
    pub fn set_local(&mut self, local: i64, uptime: i64) {
        self.set_utc(local - self.utc_offset_in_milliseconds(), uptime);
    }

    /// Changes the zone and keeps UTC, so the local time moves by the change.
    pub fn set_zone_keeping_utc(&mut self, zone: LocalTimeInformation) {
        self.zone = zone;
    }

    /// Changes the zone and keeps the local time, so UTC moves by the change.
    pub fn set_zone_keeping_local_time(&mut self, zone: LocalTimeInformation) {
        let change = zone.offset_in_milliseconds() - self.utc_offset_in_milliseconds();
        self.utc_when_uptime_was_zero -= change;
        self.zone = zone;
    }

    /// The timer reading at a UTC time, or 0 for a time already past at start.
    pub fn uptime_at(&self, utc: i64) -> i64 {
        (utc - self.utc_when_uptime_was_zero).max(0)
    }

    /// The Current Time (0x2A2B) value: the local time now.
    pub fn current_time(&self, uptime: i64) -> [u8; CURRENT_TIME_BYTES] {
        encode_current_time(self.local(uptime))
    }

    /// Sets the local time from a written Current Time value, unless it is not a real time.
    pub fn write_current_time(&mut self, bytes: &[u8], uptime: i64) -> bool {
        let Some(local) = decode_current_time(bytes) else {
            return false;
        };
        self.set_local(local, uptime);
        true
    }

    /// Takes the zone from a written Local Time Information value, keeping the local time.
    ///
    /// Keeping the local time means a phone may write Current Time and Local Time Information in
    /// either order and the watch shows the same time, as InfiniTime does.
    pub fn write_local_time_information(&mut self, bytes: &[u8]) -> bool {
        let Some(zone) = LocalTimeInformation::decode(bytes) else {
            return false;
        };
        self.set_zone_keeping_local_time(zone);
        true
    }

    /// Sets the local time from a day of a year and a minute of that day, chosen on the watch, keeping the zone.
    pub fn set_by_hand(&mut self, year: i64, day_of_year: i64, minute_of_day: i64, uptime: i64) -> bool {
        let Some(local) = local_time_from_day_and_minute(year, day_of_year, minute_of_day) else {
            return false;
        };
        self.set_local(local, uptime);
        true
    }

    /// Applies a companion time line: UTC first, then the zone, keeping UTC.
    pub fn apply_time_line(&mut self, line: TimeLine, uptime: i64) {
        if let Some(utc_seconds) = line.utc_seconds {
            self.set_utc(utc_seconds * MILLISECONDS_PER_SECOND, uptime);
        }
        if let Some(zone) = line.zone {
            self.set_zone_keeping_utc(zone);
        }
    }
}

// ============================================================================
// The clock kept across a reset
// ============================================================================

/// Words in a kept clock: a marker, UTC in two halves, the zone, and a check over the four.
pub const KEPT_CLOCK_WORDS: usize = 5;

/// "TIME" in ASCII, so a kept clock is easy to find in a dump of RAM.
pub const KEPT_CLOCK_MARKER: u32 = 0x454D_4954;

impl WallClock {
    /// The clock as words for RAM a reset keeps: UTC at `uptime` and the zone, with a marker and a check.
    pub fn kept_words(&self, uptime: i64) -> [u32; KEPT_CLOCK_WORDS] {
        let utc = self.utc(uptime) as u64;
        let zone = u16::from_le_bytes(self.zone.encode()) as u32;
        let [marker, utc_low, utc_high, zone] = [KEPT_CLOCK_MARKER, utc as u32, (utc >> 32) as u32, zone];
        let check = kept_clock_check([marker, utc_low, utc_high, zone]);
        [marker, utc_low, utc_high, zone, check]
    }

    /// The clock kept words hold, counted on from `uptime`, or `None` unless they are a whole kept clock.
    ///
    /// A kept time before `earliest_utc`, the firmware's build time, is moved up to it.
    pub fn from_kept_words(words: [u32; KEPT_CLOCK_WORDS], uptime: i64, earliest_utc: i64) -> Option<Self> {
        let [marker, utc_low, utc_high, zone, check] = words;
        if marker != KEPT_CLOCK_MARKER || check != kept_clock_check([marker, utc_low, utc_high, zone]) {
            return None;
        }
        let zone = LocalTimeInformation::decode(&u16::try_from(zone).ok()?.to_le_bytes())?;
        let utc = ((utc_high as u64) << 32 | utc_low as u64) as i64;
        let latest_year_starts = Date {
            year: LATEST_YEAR,
            month: 1,
            day: 1,
        };
        let latest_utc = days_since_1970(latest_year_starts) * SECONDS_PER_DAY * MILLISECONDS_PER_SECOND;
        if !(0..latest_utc).contains(&utc) {
            return None;
        }
        let mut clock = WallClock::starting_at(0);
        clock.set_utc(utc.max(earliest_utc), uptime);
        clock.zone = zone;
        Some(clock)
    }
}

/// A check over a kept clock's words, so RAM left by a power cut or another build reads as no clock.
fn kept_clock_check(words: [u32; KEPT_CLOCK_WORDS - 1]) -> u32 {
    words.iter().fold(0x811C_9DC5, |check, word| {
        (check ^ word).rotate_left(5).wrapping_mul(0x0100_0193)
    })
}

// ============================================================================
// Companion time lines
// ============================================================================

/// What a Gadgetbridge time line sets: UTC, the zone, or both.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimeLine {
    pub utc_seconds: Option<i64>,
    /// The whole offset from `E.setTimeZone`, held as a time zone with no daylight saving.
    pub zone: Option<LocalTimeInformation>,
}

const SET_TIME: &[u8] = b"setTime(";
const SET_TIME_ZONE: &[u8] = b"E.setTimeZone(";
/// The control character Gadgetbridge puts first, which turns off the Espruino console echo.
const ECHO_OFF: u8 = 0x10;
const LATEST_UTC_SECONDS: i64 = 7_258_118_400;
const THOUSANDTHS_PER_WHOLE: i64 = 1000;
const QUARTER_HOURS_PER_HOUR: i64 = 4;
/// The most whole digits a number may have, which keeps its thousandths far from overflow.
const MOST_WHOLE_DIGITS: usize = 12;

/// Reads a line that starts with `setTime(...)` or `E.setTimeZone(...)`, or `None` for any other line.
///
/// A call may start the line or follow a `;`, and one that is malformed or out of range is left out.
pub fn parse_time_line(line: &[u8]) -> Option<TimeLine> {
    let mut text = line.trim_ascii();
    while let [ECHO_OFF, rest @ ..] = text {
        text = rest;
    }
    if !text.starts_with(SET_TIME) && !text.starts_with(SET_TIME_ZONE) {
        return None;
    }
    let utc_seconds = argument_of(text, SET_TIME)
        .and_then(parse_thousandths)
        .map(|thousandths| thousandths.div_euclid(THOUSANDTHS_PER_WHOLE))
        .filter(|seconds| (0..=LATEST_UTC_SECONDS).contains(seconds));
    let zone = argument_of(text, SET_TIME_ZONE)
        .and_then(parse_thousandths)
        .and_then(zone_from_thousandths_of_an_hour);
    Some(TimeLine { utc_seconds, zone })
}

/// The text between a call's opening and closing brackets, where the call starts the line or follows a `;`.
fn argument_of<'text>(text: &'text [u8], call: &[u8]) -> Option<&'text [u8]> {
    let start = (0..text.len())
        .find(|&start| (start == 0 || text[start - 1] == b';') && text[start..].starts_with(call))?;
    let after = &text[start + call.len()..];
    let end = after.iter().position(|&byte| byte == b')')?;
    Some(&after[..end])
}

/// A decimal number such as `-3.5`, in thousandths, with digits past the third decimal dropped.
fn parse_thousandths(text: &[u8]) -> Option<i64> {
    let text = text.trim_ascii();
    let (is_negative, digits) = match text {
        [b'-', rest @ ..] => (true, rest),
        _ => (false, text),
    };
    let point = digits
        .iter()
        .position(|&byte| byte == b'.')
        .unwrap_or(digits.len());
    let whole = &digits[..point];
    let fraction = digits.get(point + 1..).unwrap_or(&[]);
    if whole.is_empty()
        || whole.len() > MOST_WHOLE_DIGITS
        || !whole.iter().chain(fraction).all(u8::is_ascii_digit)
    {
        return None;
    }
    let digit_value = |byte: &u8| (byte - b'0') as i64;
    let whole_value = whole.iter().fold(0, |value, byte| value * 10 + digit_value(byte));
    let mut thousandths = whole_value * THOUSANDTHS_PER_WHOLE;
    for (place, digit) in [100, 10, 1].into_iter().zip(fraction) {
        thousandths += place * digit_value(digit);
    }
    Some(if is_negative { -thousandths } else { thousandths })
}

/// The zone for an offset in thousandths of an hour, to the nearest quarter hour, if the field can hold it.
fn zone_from_thousandths_of_an_hour(thousandths: i64) -> Option<LocalTimeInformation> {
    let thousandths_of_a_quarter_hour = thousandths * QUARTER_HOURS_PER_HOUR;
    let rounding = thousandths_of_a_quarter_hour.signum() * THOUSANDTHS_PER_WHOLE / 2;
    let quarter_hours = (thousandths_of_a_quarter_hour + rounding) / THOUSANDTHS_PER_WHOLE;
    let time_zone = i8::try_from(quarter_hours)
        .ok()
        .filter(|zone| *zone != UNKNOWN_TIME_ZONE)?;
    LocalTimeInformation::decode(&[time_zone as u8, 0])
}

#[cfg(test)]
mod tests;
