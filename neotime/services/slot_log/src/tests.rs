extern crate std;

use embassy_futures::block_on;
use neotime_flash_store::{ram_flash::RamFlash, FlashStore, Layout};

use super::*;

// ============================================================================
// Times and a store in RAM
// ============================================================================

/// Monday 5 January 2026, local midnight.
const MIDNIGHT: i64 = 1_767_571_200_000;
const DAY_OF_MIDNIGHT: i64 = MIDNIGHT / MILLISECONDS_PER_DAY;
const MINUTE: i64 = 60_000;

type TestStore = FlashStore<RamFlash, 32, 32, 8>;

const LAYOUT: Layout = Layout {
    start_address: 0,
    file_sectors: 32,
    record_sectors: 8,
    log_sectors: 4,
};

fn file_system() -> TestStore {
    block_on(TestStore::mount(RamFlash::blank(LAYOUT.total_sectors()), LAYOUT)).unwrap()
}

/// A day's bytes as its block holds them, every slot, or nothing when its block is missing.
fn block_day(files: &mut TestStore, log: &SlotLog, day: i64) -> Option<std::vec::Vec<u8>> {
    let mut bytes = std::vec![0; log.day_bytes()];
    block_on(read_block_day(files, log, day, &mut bytes)).unwrap()?;
    Some(bytes)
}

fn erases(files: &TestStore) -> u32 {
    files.flash().erases
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

#[test]
fn a_day_lies_in_a_block_of_whole_days() {
    assert_eq!(STEPS.days_per_block(), 8);
    assert_eq!(HEART_RATE.days_per_block(), 16);
    let (path, start) = BATTERY.place_of_day(16 * 1273 + 2);
    assert_eq!(
        (path.as_str(), start),
        ("/day_blocks/battery/1273", 2 * SLOTS_PER_DAY)
    );
    assert_eq!(BATTERY.block_of_path(&path), Some(1273));
    assert_eq!(HEART_RATE.block_of_path(&path), None);
    assert_eq!(STEPS.place_of_day(-1).0.as_str(), "/day_blocks/steps/-1");
}

// ============================================================================
// Writing and reading
// ============================================================================

#[test]
fn a_slot_is_written_in_its_place_and_unwritten_slots_read_as_empty() {
    let mut files = file_system();
    block_on(write_slot(&mut files, &BATTERY, slot(DAY_OF_MIDNIGHT, 3), 80)).unwrap();
    block_on(write_slot(
        &mut files,
        &BATTERY,
        slot(DAY_OF_MIDNIGHT, 1),
        81 | CHARGING_BIT as u16,
    ))
    .unwrap();
    block_on(write_slot(&mut files, &HEART_RATE, slot(DAY_OF_MIDNIGHT, 2), 255)).unwrap();

    let battery = block_day(&mut files, &BATTERY, DAY_OF_MIDNIGHT).unwrap();
    assert_eq!(&battery[..5], [0xff, 81 | CHARGING_BIT, 0xff, 80, 0xff]);
    assert!(battery[5..].iter().all(|&byte| byte == NO_BATTERY_READING));
    let heart = block_day(&mut files, &HEART_RATE, DAY_OF_MIDNIGHT).unwrap();
    assert_eq!(&heart[..4], [0, 0, 255, 0]);
    assert_eq!(block_day(&mut files, &STEPS, DAY_OF_MIDNIGHT), None);
}

#[test]
fn a_slot_keeps_the_first_value_written_to_it() {
    let mut files = file_system();
    block_on(write_slot(&mut files, &HEART_RATE, slot(DAY_OF_MIDNIGHT, 2), 70)).unwrap();
    block_on(write_slot(&mut files, &HEART_RATE, slot(DAY_OF_MIDNIGHT, 2), 90)).unwrap();
    assert_eq!(
        block_day(&mut files, &HEART_RATE, DAY_OF_MIDNIGHT).unwrap()[2],
        70
    );
}

#[test]
fn neighbouring_days_share_a_block_without_touching_each_other() {
    let mut files = file_system();
    let first = 16 * 1273;
    block_on(write_slot(
        &mut files,
        &HEART_RATE,
        slot(first, SLOTS_PER_DAY - 1),
        61,
    ))
    .unwrap();
    block_on(write_slot(&mut files, &HEART_RATE, slot(first + 15, 0), 62)).unwrap();

    let first_day = block_day(&mut files, &HEART_RATE, first).unwrap();
    assert_eq!(first_day[SLOTS_PER_DAY - 1], 61);
    assert!(first_day[..SLOTS_PER_DAY - 1].iter().all(|&byte| byte == 0));
    assert_eq!(block_day(&mut files, &HEART_RATE, first + 15).unwrap()[0], 62);
    assert_eq!(
        block_day(&mut files, &HEART_RATE, first + 1).unwrap(),
        [0; SLOTS_PER_DAY]
    );
    assert_eq!(block_day(&mut files, &HEART_RATE, first + 16), None);
}

#[test]
fn a_whole_day_of_slots_costs_no_erase_once_its_blocks_are_made() {
    let mut files = file_system();
    let first = ended(DAY_OF_MIDNIGHT, 0, DAY_OF_MIDNIGHT);
    block_on(store_ended_slot(&mut files, first, 60, 90)).unwrap();
    block_on(write_slot(&mut files, &STEPS, first.time, 1000)).unwrap();
    let erases_with_the_blocks = erases(&files);

    for slot in 1..SLOTS_PER_DAY {
        let time = ended(DAY_OF_MIDNIGHT, slot, DAY_OF_MIDNIGHT);
        block_on(store_ended_slot(&mut files, time, 60 + slot as u8 % 50, 90)).unwrap();
        block_on(write_slot(&mut files, &STEPS, time.time, 1000 + slot as u16)).unwrap();
    }

    assert_eq!(erases(&files), erases_with_the_blocks);
    let steps = block_day(&mut files, &STEPS, DAY_OF_MIDNIGHT).unwrap();
    assert_eq!(&steps[2..4], &1001u16.to_le_bytes());
    assert_eq!(&steps[478..], &1239u16.to_le_bytes());
    let heart = block_day(&mut files, &HEART_RATE, DAY_OF_MIDNIGHT).unwrap();
    assert_eq!(heart[239], 60 + 239 % 50);
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
    assert_eq!(block_day(&mut files, &HEART_RATE, DAY_OF_MIDNIGHT), None);
    block_on(store_ended_slot(
        &mut files,
        ended(DAY_OF_MIDNIGHT, 1, DAY_OF_MIDNIGHT),
        64,
        89,
    ))
    .unwrap();
    assert_eq!(
        block_day(&mut files, &HEART_RATE, DAY_OF_MIDNIGHT).unwrap()[..3],
        [0, 64, 0]
    );
    assert_eq!(
        block_day(&mut files, &BATTERY, DAY_OF_MIDNIGHT).unwrap()[..3],
        [90, 89, 0xff]
    );
}

// ============================================================================
// Forgetting
// ============================================================================

#[test]
fn blocks_whose_days_are_all_older_than_the_log_keeps_are_forgotten_when_a_new_day_starts() {
    let mut files = file_system();
    let oldest_kept_block = (DAY_OF_MIDNIGHT - DAYS_KEPT + 1).div_euclid(16);
    for days_back in [1, DAYS_KEPT - 1, DAYS_KEPT + 16, DAYS_KEPT + 40] {
        let day = DAY_OF_MIDNIGHT - days_back;
        block_on(store_ended_slot(&mut files, ended(day, 3, day), 70, 50)).unwrap();
    }
    let old_file = day_path(HEART_RATE_DIRECTORY, DAY_OF_MIDNIGHT - DAYS_KEPT);
    let kept_file = day_path(HEART_RATE_DIRECTORY, DAY_OF_MIDNIGHT - DAYS_KEPT + 1);
    block_on(files.write_file(&old_file, &[70])).unwrap();
    block_on(files.write_file(&kept_file, &[71])).unwrap();

    block_on(store_ended_slot(
        &mut files,
        ended(DAY_OF_MIDNIGHT - 1, SLOTS_PER_DAY - 1, DAY_OF_MIDNIGHT),
        70,
        50,
    ))
    .unwrap();

    for log in [HEART_RATE, BATTERY] {
        let mut blocks = std::vec::Vec::new();
        block_on(files.for_each_file(log.block_directory, |path| {
            blocks.push(log.block_of_path(path).unwrap())
        }))
        .unwrap();
        blocks.sort();
        assert_eq!(blocks, [oldest_kept_block, (DAY_OF_MIDNIGHT - 1).div_euclid(16)]);
    }
    assert_eq!(block_on(files.file_length(&old_file)).unwrap(), None);
    assert_eq!(block_on(files.file_length(&kept_file)).unwrap(), Some(1));
}
