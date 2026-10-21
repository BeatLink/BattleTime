use super::*;

const TEST_CRC: u32 = 0x1234_5678;

#[test]
fn a_module_stops_after_its_second_crash() {
    let mut counts = CrashCounts::default();

    counts.count("test", TEST_CRC, 1);
    let after_one = counts.is_stopped("test", TEST_CRC);
    counts.count("test", TEST_CRC, 2);

    assert!(!after_one);
    assert!(counts.is_stopped("test", TEST_CRC));
    assert!(!counts.is_stopped("verifier", TEST_CRC));
}

#[test]
fn the_same_crash_record_is_counted_once() {
    let mut counts = CrashCounts::default();

    assert!(counts.count("test", TEST_CRC, 7));
    assert!(!counts.count("test", TEST_CRC, 7));

    assert!(!counts.is_stopped("test", TEST_CRC));
}

#[test]
fn a_new_copy_of_the_module_starts_from_no_crashes() {
    let mut counts = CrashCounts::default();
    counts.count("test", TEST_CRC, 1);
    counts.count("test", TEST_CRC, 2);

    counts.count("test", TEST_CRC + 1, 3);

    assert!(!counts.is_stopped("test", TEST_CRC + 1));
}

#[test]
fn survives_being_written_and_read_back() {
    let mut counts = CrashCounts::default();
    counts.count("test", TEST_CRC, 1);
    counts.count("verifier", 9, 2);

    let read_back = CrashCounts::parse(&counts.encode());

    assert_eq!(read_back, counts);
}

#[test]
fn forgets_the_oldest_module_to_make_room() {
    let mut counts = CrashCounts::default();
    let names = ["a", "b", "c", "d", "e", "f", "g", "h", "i"];
    for (record, name) in names.iter().enumerate() {
        counts.count(name, TEST_CRC, record as u32 + 1);
        counts.count(name, TEST_CRC, record as u32 + 100);
    }

    assert!(!counts.is_stopped("a", TEST_CRC));
    assert!(counts.is_stopped("i", TEST_CRC));
}

#[test]
fn a_missing_file_counts_no_crashes() {
    assert_eq!(CrashCounts::parse(&[]), CrashCounts::default());
}

#[test]
fn a_crash_tag_names_the_module_it_was_made_for() {
    let tag = crash_tag("verifier");
    let length = tag.iter().position(|&byte| byte == 0).unwrap();

    let id = core::str::from_utf8(&tag[..length]).unwrap();

    assert_eq!(id, "mod:verifier");
    assert_eq!(module_of_crash(id), Some("verifier"));
    assert_eq!(module_of_crash("settings"), None);
}
