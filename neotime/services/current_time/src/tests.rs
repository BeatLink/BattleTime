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

#[test]
fn the_offset_adds_the_zone_and_daylight_saving() {
    let new_york_in_summer = LocalTimeInformation {
        time_zone_in_quarter_hours: -20,
        daylight_saving_in_quarter_hours: 4,
    };

    assert_eq!(new_york_in_summer.offset_in_milliseconds(), -4 * HOUR);
}

#[test]
fn unknown_parts_of_the_zone_count_as_none() {
    let unknown = LocalTimeInformation {
        time_zone_in_quarter_hours: UNKNOWN_TIME_ZONE,
        daylight_saving_in_quarter_hours: UNKNOWN_DAYLIGHT_SAVING,
    };

    assert_eq!(unknown.offset_in_milliseconds(), 0);
}

// ============================================================================
// The wall clock
// ============================================================================

const HOUR: i64 = 3_600_000;

/// UTC+1 with an hour of summer time, as central Europe has in summer.
const BERLIN_IN_SUMMER: LocalTimeInformation = LocalTimeInformation {
    time_zone_in_quarter_hours: 4,
    daylight_saving_in_quarter_hours: 4,
};

const INDIA: LocalTimeInformation = LocalTimeInformation {
    time_zone_in_quarter_hours: 22,
    daylight_saving_in_quarter_hours: 0,
};

#[test]
fn a_new_clock_counts_on_from_its_start_in_utc() {
    let clock = WallClock::starting_at(MONDAY_5_JANUARY_2026_AT_09_00);

    assert_eq!(clock.utc(5000), MONDAY_5_JANUARY_2026_AT_09_00 + 5000);
    assert_eq!(clock.local(5000), MONDAY_5_JANUARY_2026_AT_09_00 + 5000);
    assert_eq!(clock.zone(), LocalTimeInformation::UTC);
}

#[test]
fn local_time_is_utc_moved_by_the_zone() {
    let mut clock = WallClock::starting_at(MONDAY_5_JANUARY_2026_AT_09_00);

    clock.set_zone_keeping_utc(BERLIN_IN_SUMMER);

    assert_eq!(clock.utc_offset_in_milliseconds(), 2 * HOUR);
    assert_eq!(clock.local(0), MONDAY_5_JANUARY_2026_AT_09_00 + 2 * HOUR);
    assert_eq!(clock.utc(0), MONDAY_5_JANUARY_2026_AT_09_00);
}

#[test]
fn setting_utc_counts_from_the_uptime_it_was_set_at() {
    let mut clock = WallClock::starting_at(0);

    clock.set_utc(MONDAY_5_JANUARY_2026_AT_09_00, 60_000);

    assert_eq!(clock.utc(60_000), MONDAY_5_JANUARY_2026_AT_09_00);
    assert_eq!(clock.utc(61_000), MONDAY_5_JANUARY_2026_AT_09_00 + 1000);
}

#[test]
fn a_local_time_is_read_in_the_zone_the_clock_has() {
    let mut clock = WallClock::starting_at(0);
    clock.set_zone_keeping_utc(INDIA);

    clock.set_local(MONDAY_5_JANUARY_2026_AT_09_00, 0);

    assert_eq!(clock.utc(0), MONDAY_5_JANUARY_2026_AT_09_00 - 5 * HOUR - HOUR / 2);
}

#[test]
fn keeping_the_local_time_moves_utc_by_the_zone_change() {
    let mut clock = WallClock::starting_at(MONDAY_5_JANUARY_2026_AT_09_00);

    clock.set_zone_keeping_local_time(BERLIN_IN_SUMMER);

    assert_eq!(clock.local(0), MONDAY_5_JANUARY_2026_AT_09_00);
    assert_eq!(clock.utc(0), MONDAY_5_JANUARY_2026_AT_09_00 - 2 * HOUR);
}

#[test]
fn the_uptime_at_a_time_counts_from_the_start_and_never_goes_below_zero() {
    let clock = WallClock::starting_at(MONDAY_5_JANUARY_2026_AT_09_00);

    assert_eq!(clock.uptime_at(MONDAY_5_JANUARY_2026_AT_09_00 + HOUR), HOUR);
    assert_eq!(clock.uptime_at(0), 0);
}

#[test]
fn current_time_and_zone_written_in_either_order_give_the_same_clock() {
    let nine_in_berlin = [0xEA, 0x07, 1, 5, 9, 0, 0, 1, 0, 0];
    let berlin = BERLIN_IN_SUMMER.encode();
    let mut time_first = WallClock::starting_at(0);
    let mut zone_first = WallClock::starting_at(0);

    assert!(time_first.write_current_time(&nine_in_berlin, 0));
    assert!(time_first.write_local_time_information(&berlin));
    assert!(zone_first.write_local_time_information(&berlin));
    assert!(zone_first.write_current_time(&nine_in_berlin, 0));

    assert_eq!(time_first, zone_first);
    assert_eq!(time_first.local(0), MONDAY_5_JANUARY_2026_AT_09_00);
    assert_eq!(time_first.utc(0), MONDAY_5_JANUARY_2026_AT_09_00 - 2 * HOUR);
    assert_eq!(time_first.current_time(0), nine_in_berlin);
}

#[test]
fn bad_writes_leave_the_clock_alone() {
    let mut clock = WallClock::starting_at(MONDAY_5_JANUARY_2026_AT_09_00);
    let before = clock;

    assert!(!clock.write_current_time(&[0xEA, 0x07, 13, 1, 0, 0, 0], 0));
    assert!(!clock.write_local_time_information(&[57, 0]));

    assert_eq!(clock, before);
}

#[test]
fn a_time_line_sets_utc_then_the_zone_keeping_utc() {
    let mut clock = WallClock::starting_at(0);
    let line = TimeLine {
        utc_seconds: Some(MONDAY_5_JANUARY_2026_AT_09_00 / 1000),
        zone: Some(INDIA),
    };

    clock.apply_time_line(line, 1000);

    assert_eq!(clock.utc(1000), MONDAY_5_JANUARY_2026_AT_09_00);
    assert_eq!(
        clock.local(1000),
        MONDAY_5_JANUARY_2026_AT_09_00 + 5 * HOUR + HOUR / 2
    );
}

#[test]
fn an_empty_time_line_changes_nothing() {
    let mut clock = WallClock::starting_at(MONDAY_5_JANUARY_2026_AT_09_00);
    let before = clock;

    clock.apply_time_line(TimeLine::default(), 0);

    assert_eq!(clock, before);
}

// ============================================================================
// Companion time lines
// ============================================================================

fn zone_of(quarter_hours: i8) -> Option<LocalTimeInformation> {
    Some(LocalTimeInformation {
        time_zone_in_quarter_hours: quarter_hours,
        daylight_saving_in_quarter_hours: 0,
    })
}

#[test]
fn reads_gadgetbridges_time_line() {
    let line = b"\x10setTime(1767603600);E.setTimeZone(1.0);(s=>s&&(s.timezone=1.0,require('Storage').write('setting.json',s)))(require('Storage').readJSON('setting.json',1))";

    assert_eq!(
        parse_time_line(line),
        Some(TimeLine {
            utc_seconds: Some(1_767_603_600),
            zone: zone_of(4),
        })
    );
}

#[test]
fn reads_each_call_alone() {
    assert_eq!(
        parse_time_line(b"setTime(1767603600.75)"),
        Some(TimeLine {
            utc_seconds: Some(1_767_603_600),
            zone: None,
        })
    );
    assert_eq!(
        parse_time_line(b"E.setTimeZone(-3.5)"),
        Some(TimeLine {
            utc_seconds: None,
            zone: zone_of(-14),
        })
    );
}

#[test]
fn rounds_a_zone_to_the_nearest_quarter_hour() {
    assert_eq!(parse_time_line(b"E.setTimeZone(5.75)").unwrap().zone, zone_of(23));
    assert_eq!(parse_time_line(b"E.setTimeZone(5.8)").unwrap().zone, zone_of(23));
    assert_eq!(
        parse_time_line(b"E.setTimeZone(-9.9)").unwrap().zone,
        zone_of(-40)
    );
    assert_eq!(parse_time_line(b"E.setTimeZone(0)").unwrap().zone, zone_of(0));
}

#[test]
fn leaves_out_a_malformed_or_out_of_range_call() {
    let refused: [&[u8]; 8] = [
        b"setTime();E.setTimeZone(15)",
        b"setTime(abc);E.setTimeZone(1.x)",
        b"setTime(-5);E.setTimeZone(-)",
        b"setTime(99999999999);E.setTimeZone(.5)",
        b"setTime(1234567890123456);E.setTimeZone(-12.5)",
        b"setTime(17676036;E.setTimeZone(1",
        b"setTime(1.2.3);E.setTimeZone(99999999999999)",
        b"setTime( );x.E.setTimeZone(1)",
    ];

    for line in refused {
        assert_eq!(parse_time_line(line), Some(TimeLine::default()), "{line:?}");
    }
}

#[test]
fn ignores_other_lines() {
    let ignored: [&[u8]; 4] = [
        br#"GB({"t":"notify","id":1})"#,
        b"{\"t\":\"settings\"}",
        b"print(setTime(5))",
        &[0xFF, 0xFE],
    ];

    for line in ignored {
        assert_eq!(parse_time_line(line), None, "{line:?}");
    }
}
