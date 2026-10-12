//! The Bluetooth Current Time service's two values, as phones read and write them.
//!
//! Current Time (0x2A2B) is ten bytes: the year, month, day, hour, minute and second of the local
//! time, then the day of the week, the fraction of the second in 256ths and why it last changed.
//! Local Time Information (0x2A0F) is two bytes: the time zone and the daylight saving offset, both
//! in quarter hours. The watch keeps local time, so the time zone travels beside it and never moves
//! the clock. Times here are milliseconds since 1970, as the firmware's clock counts them.

#![no_std]

// ============================================================================
// Calendar arithmetic
// ============================================================================

const MILLISECONDS_PER_SECOND: i64 = 1000;
const SECONDS_PER_MINUTE: i64 = 60;
const SECONDS_PER_HOUR: i64 = 3600;
const SECONDS_PER_DAY: i64 = 86_400;
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
// Local Time Information (0x2A0F)
// ============================================================================

pub const LOCAL_TIME_INFORMATION_BYTES: usize = 2;

/// The time zone field's value when the zone is not known.
pub const UNKNOWN_TIME_ZONE: i8 = -128;
const LOWEST_TIME_ZONE: i8 = -48;
const HIGHEST_TIME_ZONE: i8 = 56;
/// The daylight saving field's value when it is not known.
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

#[cfg(test)]
mod tests;
