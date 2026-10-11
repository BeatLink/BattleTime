//! Text values, the calendar, and the formatting instructions that turn numbers into text.

use crate::instruction::TimePart;

// ============================================================================
// Text references
// ============================================================================

/// Marks a text value that lives in the interpreter's text buffer rather than the image.
const IN_BUFFER: i64 = 1 << 40;
const SIXTEEN_BITS: i64 = 0xFFFF;

/// Where a text value's bytes are: a string in the image, or a slice of the text buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextReference {
    ImageString(u16),
    Buffer { start: usize, length: usize },
}

impl TextReference {
    pub fn to_value(self) -> i64 {
        match self {
            TextReference::ImageString(index) => index as i64,
            TextReference::Buffer { start, length } => IN_BUFFER | ((start as i64) << 16) | length as i64,
        }
    }

    pub fn from_value(value: i64) -> Option<TextReference> {
        if (0..=SIXTEEN_BITS).contains(&value) {
            return Some(TextReference::ImageString(value as u16));
        }
        if value & !(IN_BUFFER | (SIXTEEN_BITS << 16) | SIXTEEN_BITS) == 0 && value & IN_BUFFER != 0 {
            let start = ((value >> 16) & SIXTEEN_BITS) as usize;
            let length = (value & SIXTEEN_BITS) as usize;
            return Some(TextReference::Buffer { start, length });
        }
        None
    }
}

// ============================================================================
// Calendar
// ============================================================================

const MILLISECONDS_PER_SECOND: i64 = 1000;
const MILLISECONDS_PER_MINUTE: i64 = 60 * MILLISECONDS_PER_SECOND;
const MILLISECONDS_PER_HOUR: i64 = 60 * MILLISECONDS_PER_MINUTE;
const MILLISECONDS_PER_DAY: i64 = 24 * MILLISECONDS_PER_HOUR;

/// A local date and time, split into its parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
    pub second: i64,
    pub weekday: i64,
}

impl DateTime {
    /// Splits milliseconds since 1970 in local time, using Howard Hinnant's days-to-civil method.
    pub fn from_local_milliseconds(milliseconds: i64) -> DateTime {
        let days = milliseconds.div_euclid(MILLISECONDS_PER_DAY);
        let time_of_day = milliseconds.rem_euclid(MILLISECONDS_PER_DAY);

        let shifted_days = days + 719_468;
        let era = shifted_days.div_euclid(146_097);
        let day_of_era = shifted_days.rem_euclid(146_097);
        let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
        let month = if shifted_month < 10 {
            shifted_month + 3
        } else {
            shifted_month - 9
        };
        let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };

        // 1 January 1970 was a Thursday, which is 3 when Monday is 0.
        let weekday = (days + 3).rem_euclid(7);

        DateTime {
            year,
            month,
            day,
            hour: time_of_day / MILLISECONDS_PER_HOUR,
            minute: time_of_day % MILLISECONDS_PER_HOUR / MILLISECONDS_PER_MINUTE,
            second: time_of_day % MILLISECONDS_PER_MINUTE / MILLISECONDS_PER_SECOND,
            weekday,
        }
    }

    pub fn part(&self, part: TimePart) -> i64 {
        match part {
            TimePart::Year => self.year,
            TimePart::Month => self.month,
            TimePart::Day => self.day,
            TimePart::Hour => self.hour,
            TimePart::Minute => self.minute,
            TimePart::Second => self.second,
            TimePart::Weekday => self.weekday,
        }
    }
}

// ============================================================================
// Writing text
// ============================================================================

/// The text buffer had no room left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferFull;

/// Appends text to the free end of a buffer.
pub struct TextWriter<'buffer> {
    buffer: &'buffer mut [u8],
    position: usize,
}

impl<'buffer> TextWriter<'buffer> {
    pub fn new(buffer: &'buffer mut [u8], position: usize) -> Self {
        TextWriter { buffer, position }
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), BufferFull> {
        let end = self.position + bytes.len();
        self.buffer
            .get_mut(self.position..end)
            .ok_or(BufferFull)?
            .copy_from_slice(bytes);
        self.position = end;
        Ok(())
    }

    /// Writes a whole number in decimal, padded with zeros to at least `minimum_digits`.
    pub fn write_number(&mut self, number: i64, minimum_digits: u8) -> Result<(), BufferFull> {
        if number < 0 {
            self.write_bytes(b"-")?;
        }
        let mut magnitude = number.unsigned_abs();
        let mut digits = [0u8; 20];
        let mut digit_count = 0;
        while magnitude > 0 || digit_count == 0 {
            digits[digit_count] = b'0' + (magnitude % 10) as u8;
            magnitude /= 10;
            digit_count += 1;
        }
        for _ in digit_count..(minimum_digits as usize).min(digits.len()) {
            self.write_bytes(b"0")?;
        }
        for index in (0..digit_count).rev() {
            self.write_bytes(&digits[index..index + 1])?;
        }
        Ok(())
    }
}
