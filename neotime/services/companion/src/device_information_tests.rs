use super::device_information::*;

#[test]
fn serial_number_writes_every_digit_most_significant_first() {
    assert_eq!(&serial_number(0x0123_4567_89AB_CDEF), b"0123456789ABCDEF");
}

#[test]
fn serial_number_keeps_leading_zeros() {
    assert_eq!(&serial_number(0xA), b"000000000000000A");
}
