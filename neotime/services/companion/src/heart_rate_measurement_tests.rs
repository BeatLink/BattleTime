use super::heart_rate_measurement::*;

#[test]
fn a_measurement_is_the_clear_flags_byte_then_the_rate() {
    assert_eq!(heart_rate_measurement(72), [0, 72]);
}

#[test]
fn no_reading_yet_is_sent_as_zero_as_infinitime_sends_it() {
    assert_eq!(heart_rate_measurement(0), [0, 0]);
}

#[test]
fn a_rate_beyond_one_byte_is_held_at_255() {
    assert_eq!(heart_rate_measurement(255), [0, 255]);
    assert_eq!(heart_rate_measurement(300), [0, 255]);
}
