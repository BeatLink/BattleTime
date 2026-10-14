extern crate std;

use embassy_futures::block_on;
use neotime_flash_store::{ram_flash::RamFlash, FileAccess, FlashStore, Layout};

use super::*;

// ============================================================================
// Times and a store in RAM
// ============================================================================

/// Monday 5 January 2026, local midnight.
const MIDNIGHT: i64 = 1_767_571_200_000;
const DAY_OF_MIDNIGHT: i64 = MIDNIGHT / MILLISECONDS_PER_DAY;
const MINUTE: i64 = 60_000;

type TestStore = FlashStore<RamFlash, 8, 32, 8>;

const LAYOUT: Layout = Layout {
    start_address: 0,
    file_sectors: 8,
    record_sectors: 8,
    log_sectors: 4,
};

fn file_system() -> TestStore {
    block_on(TestStore::mount(RamFlash::blank(LAYOUT.total_sectors()), LAYOUT)).unwrap()
}

fn read_whole(files: &mut TestStore, path: &str) -> Option<std::vec::Vec<u8>> {
    let length = block_on(FileAccess::file_length(files, path)).unwrap()?;
    let mut bytes = std::vec![0; length];
    block_on(FileAccess::read_file(files, path, 0, &mut bytes)).unwrap();
    Some(bytes)
}

fn slot(day: i64, slot: usize) -> SlotTime {
    SlotTime { day, slot }
}

fn ended(day: i64, slot: usize, today: i64) -> EndedSlot {
    EndedSlot {
        time: SlotTime { day, slot },
        today,
    }
}

// ============================================================================
// Slots and the clock
// ============================================================================

#[test]
fn a_time_falls_in_its_day_and_six_minute_slot() {
    assert_eq!(SlotTime::at(MIDNIGHT), slot(DAY_OF_MIDNIGHT, 0));
    assert_eq!(SlotTime::at(MIDNIGHT + 6 * MINUTE - 1), slot(DAY_OF_MIDNIGHT, 0));
    assert_eq!(SlotTime::at(MIDNIGHT + 6 * MINUTE), slot(DAY_OF_MIDNIGHT, 1));
    assert_eq!(
        SlotTime::at(MIDNIGHT - 1),
        slot(DAY_OF_MIDNIGHT - 1, SLOTS_PER_DAY - 1)
    );
}

#[test]
fn the_clock_says_when_a_slot_ends_and_on_which_day_the_next_falls() {
    let mut clock = SlotClock::new();
    assert_eq!(clock.look(MIDNIGHT - 5 * MINUTE), None);
    assert_eq!(clock.look(MIDNIGHT - 4 * MINUTE), None);
    assert_eq!(
        clock.look(MIDNIGHT + MINUTE),
        Some(ended(DAY_OF_MIDNIGHT - 1, SLOTS_PER_DAY - 1, DAY_OF_MIDNIGHT))
    );
    assert_eq!(clock.look(MIDNIGHT + 2 * MINUTE), None);
    assert_eq!(
        clock.look(MIDNIGHT + 6 * MINUTE),
        Some(ended(DAY_OF_MIDNIGHT, 0, DAY_OF_MIDNIGHT))
    );
}

#[test]
fn a_clock_set_back_ends_the_slot_being_recorded_too() {
    let mut clock = SlotClock::new();
    clock.look(MIDNIGHT);
    assert_eq!(
        clock.look(MIDNIGHT - 3 * MILLISECONDS_PER_DAY),
        Some(ended(DAY_OF_MIDNIGHT, 0, DAY_OF_MIDNIGHT - 3))
    );
}

#[test]
fn an_average_rounds_and_starts_again_once_taken() {
    let mut average = Average::new();
    assert_eq!(average.take(), 0);
    average.add(70);
    average.add(73);
    assert_eq!(average.take(), 72);
    assert_eq!(average.take(), 0);
    average.add(400);
    assert_eq!(average.take(), 255);
}

#[test]
fn a_battery_byte_holds_the_percent_and_whether_it_charges() {
    assert_eq!(battery_byte(57, false), 57);
    assert_eq!(battery_byte(57, true), 57 | CHARGING_BIT);
    assert_eq!(battery_byte(130, false), 100);
}

// ============================================================================
// Paths
// ============================================================================

#[test]
fn each_day_has_its_own_file_in_a_folder_for_its_year() {
    assert_eq!(
        day_path(HEART_RATE_DIRECTORY, DAY_OF_MIDNIGHT).as_str(),
        "/logs/heart/2026/01-05"
    );
    assert_eq!(
        day_path(BATTERY_DIRECTORY, DAY_OF_MIDNIGHT).as_str(),
        "/logs/battery/2026/01-05"
    );
}

#[test]
fn a_day_path_reads_back_as_its_day_only_in_its_own_folder() {
    let path = day_path(BATTERY_DIRECTORY, DAY_OF_MIDNIGHT);
    assert_eq!(day_of_path(BATTERY_DIRECTORY, &path), Some(DAY_OF_MIDNIGHT));
    assert_eq!(day_of_path(HEART_RATE_DIRECTORY, &path), None);
    assert_eq!(day_of_path(BATTERY_DIRECTORY, "/logs/battery/20x6/01-05"), None);
    assert_eq!(day_of_path(BATTERY_DIRECTORY, "/logs/batteryx/2026/01-05"), None);
}

// ============================================================================
// Writing
// ============================================================================

#[test]
fn a_slot_is_written_after_empty_slots_before_it_and_keeps_the_others() {
    let mut files = file_system();
    let path = day_path(BATTERY_DIRECTORY, DAY_OF_MIDNIGHT);
    block_on(write_byte_slot(
        &mut files,
        BATTERY_DIRECTORY,
        slot(DAY_OF_MIDNIGHT, 3),
        80,
        0xff,
    ))
    .unwrap();
    assert_eq!(read_whole(&mut files, &path).unwrap(), [0xff, 0xff, 0xff, 80]);
    block_on(write_byte_slot(
        &mut files,
        BATTERY_DIRECTORY,
        slot(DAY_OF_MIDNIGHT, 1),
        81,
        0xff,
    ))
    .unwrap();
    assert_eq!(read_whole(&mut files, &path).unwrap(), [0xff, 81, 0xff, 80]);
    block_on(write_byte_slot(
        &mut files,
        BATTERY_DIRECTORY,
        slot(DAY_OF_MIDNIGHT, 5),
        79,
        0xff,
    ))
    .unwrap();
    assert_eq!(
        read_whole(&mut files, &path).unwrap(),
        [0xff, 81, 0xff, 80, 0xff, 79]
    );
}

#[test]
fn the_last_slot_of_the_day_fills_the_whole_file() {
    let mut files = file_system();
    let last = slot(DAY_OF_MIDNIGHT, SLOTS_PER_DAY - 1);
    block_on(write_byte_slot(&mut files, HEART_RATE_DIRECTORY, last, 61, 0)).unwrap();
    let bytes = read_whole(&mut files, &day_path(HEART_RATE_DIRECTORY, DAY_OF_MIDNIGHT)).unwrap();
    assert_eq!(bytes.len(), SLOTS_PER_DAY);
    assert_eq!(bytes[SLOTS_PER_DAY - 1], 61);
    assert!(bytes[..SLOTS_PER_DAY - 1].iter().all(|&byte| byte == 0));
}

#[test]
fn an_ended_slot_writes_its_battery_always_and_its_heart_rate_only_when_measured() {
    let mut files = file_system();
    block_on(store_ended_slot(
        &mut files,
        ended(DAY_OF_MIDNIGHT, 0, DAY_OF_MIDNIGHT),
        0,
        90,
    ))
    .unwrap();
    assert_eq!(
        read_whole(&mut files, &day_path(HEART_RATE_DIRECTORY, DAY_OF_MIDNIGHT)),
        None
    );
    block_on(store_ended_slot(
        &mut files,
        ended(DAY_OF_MIDNIGHT, 1, DAY_OF_MIDNIGHT),
        64,
        89,
    ))
    .unwrap();
    assert_eq!(
        read_whole(&mut files, &day_path(HEART_RATE_DIRECTORY, DAY_OF_MIDNIGHT)).unwrap(),
        [0, 64]
    );
    assert_eq!(
        read_whole(&mut files, &day_path(BATTERY_DIRECTORY, DAY_OF_MIDNIGHT)).unwrap(),
        [90, 89]
    );
}

#[test]
fn days_older_than_the_log_keeps_are_forgotten_when_a_new_day_starts() {
    let mut files = file_system();
    for days_back in [1, DAYS_KEPT - 1, DAYS_KEPT, DAYS_KEPT + 30] {
        let day = DAY_OF_MIDNIGHT - days_back;
        block_on(store_ended_slot(&mut files, ended(day, 3, day), 70, 50)).unwrap();
    }

    block_on(store_ended_slot(
        &mut files,
        ended(DAY_OF_MIDNIGHT - 1, SLOTS_PER_DAY - 1, DAY_OF_MIDNIGHT),
        70,
        50,
    ))
    .unwrap();

    for directory in [HEART_RATE_DIRECTORY, BATTERY_DIRECTORY] {
        let mut kept = std::vec::Vec::new();
        block_on(FileAccess::for_each_file(&mut files, directory, &mut |path| {
            kept.push(day_of_path(directory, path))
        }))
        .unwrap();
        kept.sort();
        assert_eq!(
            kept,
            [Some(DAY_OF_MIDNIGHT - DAYS_KEPT + 1), Some(DAY_OF_MIDNIGHT - 1)]
        );
    }
}
