use super::*;

/// Milliseconds since 1970 for a date and time, worked out by hand for each test.
const MONDAY_5_JANUARY_2026_AT_09_00: i64 = 1_767_603_600_000;

// ============================================================================
// Calendar arithmetic
// ============================================================================

#[test]
fn the_first_day_of_1970_is_day_zero() {
    assert_eq!(
        days_since_1970(Date {
            year: 1970,
            month: 1,
            day: 1
        }),
        0
    );
    assert_eq!(
        date_from_days_since_1970(0),
        Date {
            year: 1970,
            month: 1,
            day: 1
        }
    );
}

#[test]
fn dates_and_day_counts_go_both_ways_across_four_centuries() {
    let first = days_since_1970(Date {
        year: 1900,
        month: 1,
        day: 1,
    });
    let last = days_since_1970(Date {
        year: 2300,
        month: 12,
        day: 31,
    });

    for days in first..=last {
        assert_eq!(days_since_1970(date_from_days_since_1970(days)), days);
    }
}

#[test]
fn leap_days_fall_where_the_calendar_puts_them() {
    let after_february_2000 = days_since_1970(Date {
        year: 2000,
        month: 3,
        day: 1,
    });
    let after_february_2100 = days_since_1970(Date {
        year: 2100,
        month: 3,
        day: 1,
    });

    assert_eq!(
        date_from_days_since_1970(after_february_2000 - 1),
        Date {
            year: 2000,
            month: 2,
            day: 29
        }
    );
    assert_eq!(
        date_from_days_since_1970(after_february_2100 - 1),
        Date {
            year: 2100,
            month: 2,
            day: 28
        }
    );
}

// ============================================================================
// Current Time
// ============================================================================

#[test]
fn encodes_the_date_time_and_weekday() {
    let bytes = encode_current_time(MONDAY_5_JANUARY_2026_AT_09_00 + 30 * 60_000 + 15_500);

    assert_eq!(bytes, [0xEA, 0x07, 1, 5, 9, 30, 15, 1, 128, 0]);
}

#[test]
fn sunday_is_the_seventh_day() {
    let sunday = MONDAY_5_JANUARY_2026_AT_09_00 - 86_400_000;

    assert_eq!(encode_current_time(sunday)[7], 7);
}

#[test]
fn the_last_moment_of_a_year_encodes_without_rounding_up() {
    let new_year_2027 = days_since_1970(Date {
        year: 2027,
        month: 1,
        day: 1,
    }) * 86_400_000;

    assert_eq!(
        encode_current_time(new_year_2027 - 1),
        [0xEA, 0x07, 12, 31, 23, 59, 59, 4, 255, 0]
    );
}

#[test]
fn a_year_before_the_field_can_hold_is_unknown() {
    let bytes = encode_current_time(-100_000 * 365 * 86_400_000);

    assert_eq!(&bytes[..2], &[0, 0]);
}

#[test]
fn decodes_a_written_time() {
    let written = [0xEA, 0x07, 1, 5, 9, 0, 0, 1, 0, 0];

    assert_eq!(
        decode_current_time(&written),
        Some(MONDAY_5_JANUARY_2026_AT_09_00)
    );
}

#[test]
fn a_written_fraction_of_a_second_counts() {
    let written = [0xEA, 0x07, 1, 5, 9, 0, 0, 1, 64, 0];

    assert_eq!(
        decode_current_time(&written),
        Some(MONDAY_5_JANUARY_2026_AT_09_00 + 250)
    );
}

#[test]
fn seven_bytes_are_enough_to_set_the_time() {
    let written = [0xEA, 0x07, 1, 5, 9, 0, 0];

    assert_eq!(
        decode_current_time(&written),
        Some(MONDAY_5_JANUARY_2026_AT_09_00)
    );
}

#[test]
fn what_the_watch_serves_reads_back_as_the_same_time() {
    let time = MONDAY_5_JANUARY_2026_AT_09_00 + 12_345_000;

    assert_eq!(decode_current_time(&encode_current_time(time)), Some(time));
}

#[test]
fn refuses_a_short_or_impossible_time() {
    let refused: [&[u8]; 9] = [
        &[0xEA, 0x07, 1, 5, 9, 0],
        &[0xB1, 0x07, 1, 1, 0, 0, 0, 0, 0, 0],
        &[0x99, 0x08, 1, 1, 0, 0, 0, 0, 0, 0],
        &[0xEA, 0x07, 13, 1, 0, 0, 0, 0, 0, 0],
        &[0xEA, 0x07, 0, 1, 0, 0, 0, 0, 0, 0],
        &[0xEA, 0x07, 2, 29, 0, 0, 0, 0, 0, 0],
        &[0xEA, 0x07, 1, 5, 24, 0, 0, 0, 0, 0],
        &[0xEA, 0x07, 1, 5, 9, 60, 0, 0, 0, 0],
        &[0xEA, 0x07, 1, 5, 9, 0, 60, 0, 0, 0],
    ];

    for bytes in refused {
        assert_eq!(decode_current_time(bytes), None, "{bytes:?}");
    }
}

#[test]
fn accepts_the_leap_day_of_a_leap_year() {
    assert!(decode_current_time(&[0xE8, 0x07, 2, 29, 0, 0, 0]).is_some());
}

// ============================================================================
// Local Time Information
// ============================================================================

#[test]
fn utc_encodes_as_two_zeros() {
    assert_eq!(LocalTimeInformation::UTC.encode(), [0, 0]);
}

#[test]
fn a_zone_behind_utc_with_daylight_saving_goes_both_ways() {
    let new_york_in_summer = LocalTimeInformation {
        time_zone_in_quarter_hours: -20,
        daylight_saving_in_quarter_hours: 4,
    };

    assert_eq!(new_york_in_summer.encode(), [0xEC, 4]);
    assert_eq!(LocalTimeInformation::decode(&[0xEC, 4]), Some(new_york_in_summer));
}

#[test]
fn accepts_unknown_parts_and_the_ends_of_the_range() {
    for bytes in [
        [UNKNOWN_TIME_ZONE as u8, UNKNOWN_DAYLIGHT_SAVING],
        [-48i8 as u8, 8],
        [56, 2],
    ] {
        assert_eq!(LocalTimeInformation::decode(&bytes).unwrap().encode(), bytes);
    }
}

#[test]
fn refuses_a_short_or_out_of_range_zone() {
    let refused: [&[u8]; 4] = [&[0], &[57, 0], &[-49i8 as u8, 0], &[0, 3]];

    for bytes in refused {
        assert_eq!(LocalTimeInformation::decode(bytes), None, "{bytes:?}");
    }
}
