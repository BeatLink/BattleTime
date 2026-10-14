extern crate std;

use embassy_futures::block_on;
use neotime_flash_store::{ram_flash::RamFlash, FileAccess, FlashStore, Layout};

use super::log::*;
use super::*;

// ============================================================================
// Times and a store in RAM
// ============================================================================

/// Monday 5 January 2026, local midnight.
const MIDNIGHT: i64 = 1_767_571_200_000;
const DAY_OF_MIDNIGHT: i64 = MIDNIGHT / MILLISECONDS_PER_DAY;
const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;

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

fn has_any_step_file(files: &mut TestStore) -> bool {
    let mut found = false;
    block_on(FileAccess::for_each_file(files, STEPS_DIRECTORY, &mut |_| {
        found = true
    }))
    .unwrap();
    found
}

fn history_at(local_time: i64) -> StepHistory {
    let mut history = StepHistory::new();
    history.start(local_time);
    history
}

// ============================================================================
// Slots
// ============================================================================

#[test]
fn a_time_falls_in_its_day_and_six_minute_slot() {
    assert_eq!(
        SlotTime::at(MIDNIGHT),
        SlotTime {
            day: DAY_OF_MIDNIGHT,
            slot: 0
        }
    );
    assert_eq!(SlotTime::at(MIDNIGHT + 6 * MINUTE - 1).slot, 0);
    assert_eq!(SlotTime::at(MIDNIGHT + 6 * MINUTE).slot, 1);
    assert_eq!(SlotTime::at(MIDNIGHT + 9 * HOUR + 30 * MINUTE).slot, 95);
    assert_eq!(SlotTime::at(MIDNIGHT + 9 * HOUR + 30 * MINUTE).hour(), 9);
    assert_eq!(
        SlotTime::at(MIDNIGHT - 1),
        SlotTime {
            day: DAY_OF_MIDNIGHT - 1,
            slot: 239
        }
    );
}

#[test]
fn a_time_before_1970_still_has_a_day_and_slot() {
    assert_eq!(SlotTime::at(-1), SlotTime { day: -1, slot: 239 });
}

// ============================================================================
// Recording readings
// ============================================================================

#[test]
fn each_reading_adds_what_the_counter_gained() {
    let mut history = history_at(MIDNIGHT + 9 * HOUR);

    let first = history.record(MIDNIGHT + 9 * HOUR + 30_000, 40);
    let second = history.record(MIDNIGHT + 9 * HOUR + 60_000, 100);

    assert!(first.changed && second.changed);
    assert_eq!(first.finished, None);
    assert_eq!(history.steps_today(), 100);
    assert_eq!(history.steps_in_hour(0, 9), 100);
    assert_eq!(history.trip_steps(), 100);
}

#[test]
fn an_unchanged_count_changes_nothing() {
    let mut history = history_at(MIDNIGHT + 9 * HOUR);
    history.record(MIDNIGHT + 9 * HOUR + 30_000, 40);

    let recorded = history.record(MIDNIGHT + 9 * HOUR + 60_000, 40);

    assert!(!recorded.changed);
    assert_eq!(history.steps_today(), 40);
}

#[test]
fn a_counter_that_started_again_adds_all_it_holds() {
    let mut history = history_at(MIDNIGHT + 9 * HOUR);
    history.record(MIDNIGHT + 9 * HOUR + 30_000, 500);

    history.record(MIDNIGHT + 9 * HOUR + 60_000, 20);

    assert_eq!(history.steps_today(), 520);
}

#[test]
fn a_slot_that_ends_with_steps_is_handed_to_the_log() {
    let mut history = history_at(MIDNIGHT + 9 * HOUR);
    history.record(MIDNIGHT + 9 * HOUR + 3 * MINUTE, 70);

    let recorded = history.record(MIDNIGHT + 9 * HOUR + 6 * MINUTE + 10_000, 75);

    let expected = FinishedSlot {
        time: SlotTime {
            day: DAY_OF_MIDNIGHT,
            slot: 90,
        },
        steps: 75,
    };
    assert_eq!(recorded.finished, Some(expected));
    assert_eq!(recorded.day_change, DayChange::SameDay);
    assert_eq!(history.recording().slot, 91);
}

#[test]
fn a_slot_without_steps_is_not_logged() {
    let mut history = history_at(MIDNIGHT + 9 * HOUR);

    let recorded = history.record(MIDNIGHT + 9 * HOUR + 7 * MINUTE, 0);

    assert_eq!(recorded.finished, None);
    assert!(!recorded.changed);
}

#[test]
fn midnight_moves_today_into_yesterday() {
    let mut history = history_at(MIDNIGHT - 2 * MINUTE);
    history.record(MIDNIGHT - MINUTE, 300);

    let recorded = history.record(MIDNIGHT + 30_000, 310);

    assert_eq!(recorded.day_change, DayChange::NextDay);
    assert!(recorded.changed);
    assert_eq!(
        recorded.finished.unwrap().time,
        SlotTime {
            day: DAY_OF_MIDNIGHT - 1,
            slot: 239
        }
    );
    assert_eq!(history.steps_today(), 0);
    assert_eq!(history.steps_on_day(1), 310);
    assert_eq!(history.steps_in_hour(1, 23), 310);
    assert_eq!(history.trip_steps(), 310);
}

#[test]
fn the_oldest_day_is_forgotten_after_a_week() {
    let mut history = history_at(MIDNIGHT + HOUR);
    history.record(MIDNIGHT + HOUR + MINUTE, 50);

    for day in 1..DAYS_REMEMBERED as i64 {
        history.record(MIDNIGHT + day * MILLISECONDS_PER_DAY + HOUR, 50);
    }
    assert_eq!(history.steps_on_day(DAYS_REMEMBERED - 1), 50);
    history.record(
        MIDNIGHT + DAYS_REMEMBERED as i64 * MILLISECONDS_PER_DAY + HOUR,
        50,
    );

    assert_eq!(history.steps_on_day(DAYS_REMEMBERED - 1), 0);
}

#[test]
fn a_clock_set_to_another_day_forgets_the_remembered_days() {
    let mut history = history_at(MIDNIGHT + HOUR);
    history.record(MIDNIGHT + HOUR + MINUTE, 80);

    let recorded = history.record(MIDNIGHT + 30 * MILLISECONDS_PER_DAY, 90);

    assert_eq!(recorded.day_change, DayChange::Jump);
    assert_eq!(history.steps_on_day(0), 0);
    assert_eq!(history.steps_on_day(1), 0);
    assert_eq!(history.trip_steps(), 90);
}

#[test]
fn a_clock_set_back_a_day_is_a_jump_too() {
    let mut history = history_at(MIDNIGHT + HOUR);

    let recorded = history.record(MIDNIGHT - HOUR, 0);

    assert_eq!(recorded.day_change, DayChange::Jump);
}

#[test]
fn days_and_hours_outside_the_history_hold_nothing() {
    let history = history_at(MIDNIGHT);

    assert_eq!(history.steps_in_hour(DAYS_REMEMBERED, 0), 0);
    assert_eq!(history.steps_in_hour(0, HOURS_PER_DAY), 0);
    assert_eq!(history.steps_on_day(DAYS_REMEMBERED), 0);
}

#[test]
fn starting_again_keeps_the_trip_count() {
    let mut history = history_at(MIDNIGHT + HOUR);
    history.record(MIDNIGHT + HOUR + MINUTE, 120);

    history.start(MIDNIGHT + 2 * HOUR);

    assert_eq!(history.trip_steps(), 120);
    assert_eq!(history.steps_today(), 0);
}

#[test]
fn the_trip_count_can_be_set_and_counts_on_from_there() {
    let mut history = history_at(MIDNIGHT + HOUR);
    history.record(MIDNIGHT + HOUR + MINUTE, 120);

    history.set_trip_steps(0);
    history.record(MIDNIGHT + HOUR + 2 * MINUTE, 150);

    assert_eq!(history.trip_steps(), 30);
    assert_eq!(history.steps_today(), 150);
}

#[test]
fn an_hour_stops_at_the_largest_count_it_holds() {
    let mut history = history_at(MIDNIGHT + HOUR);

    history.record(MIDNIGHT + HOUR + MINUTE, 70_000);

    assert_eq!(history.steps_in_hour(0, 1), u16::MAX as u32);
    assert_eq!(history.trip_steps(), 70_000);
}

#[test]
fn a_default_history_is_empty() {
    let history = StepHistory::default();

    assert_eq!(history.steps_today(), 0);
    assert_eq!(history.recording(), SlotTime { day: 0, slot: 0 });
}

// ============================================================================
// The log's format
// ============================================================================

#[test]
fn each_day_has_its_own_file_in_a_folder_for_its_year() {
    assert_eq!(day_path(DAY_OF_MIDNIGHT).as_str(), "/steps/2026/01-05");
    assert_eq!(day_path(0).as_str(), "/steps/1970/01-01");
    assert_eq!(day_path(DAY_OF_MIDNIGHT - 5).as_str(), "/steps/2025/12-31");
}

#[test]
fn a_file_holds_each_slot_as_two_little_endian_bytes() {
    let bytes = [0x10, 0x00, 0x00, 0x01, 0x05];

    assert_eq!(slot_steps(&bytes, 0), 16);
    assert_eq!(slot_steps(&bytes, 1), 256);
    assert_eq!(slot_steps(&bytes, 2), 0);
    assert_eq!(slot_steps(&bytes, SLOTS_PER_DAY + 1), 0);
}

#[test]
fn ten_slots_make_an_hour() {
    let mut bytes = [0u8; LARGEST_DAY_FILE_BYTES];
    bytes[0] = 5;
    bytes[18] = 7;
    bytes[20] = 1;
    bytes[LARGEST_DAY_FILE_BYTES - 2] = 9;

    let hours = hours_from_file(&bytes);

    assert_eq!(hours[0], 12);
    assert_eq!(hours[1], 1);
    assert_eq!(hours[23], 9);
}

// ============================================================================
// Writing and reading the log
// ============================================================================

fn finished(day: i64, slot: usize, steps: u32) -> FinishedSlot {
    FinishedSlot {
        time: SlotTime { day, slot },
        steps,
    }
}

#[test]
fn a_slot_is_written_after_zeros_for_the_slots_before_it() {
    let mut files = file_system();

    block_on(write_slot(&mut files, finished(DAY_OF_MIDNIGHT, 3, 42))).unwrap();

    let bytes = read_whole(&mut files, "/steps/2026/01-05").unwrap();
    assert_eq!(bytes, [0, 0, 0, 0, 0, 0, 42, 0]);
}

#[test]
fn a_later_slot_lengthens_the_file_and_an_earlier_one_adds_to_its_place() {
    let mut files = file_system();
    block_on(write_slot(&mut files, finished(DAY_OF_MIDNIGHT, 1, 10))).unwrap();

    block_on(write_slot(&mut files, finished(DAY_OF_MIDNIGHT, 4, 300))).unwrap();
    block_on(write_slot(&mut files, finished(DAY_OF_MIDNIGHT, 1, 5))).unwrap();

    let bytes = read_whole(&mut files, &day_path(DAY_OF_MIDNIGHT)).unwrap();
    assert_eq!(bytes.len(), 10);
    assert_eq!(slot_steps(&bytes, 1), 15);
    assert_eq!(slot_steps(&bytes, 4), 300);
}

#[test]
fn the_last_slot_of_the_day_fills_the_whole_file() {
    let mut files = file_system();

    block_on(write_slot(
        &mut files,
        finished(DAY_OF_MIDNIGHT, SLOTS_PER_DAY - 1, 1),
    ))
    .unwrap();

    let bytes = read_whole(&mut files, &day_path(DAY_OF_MIDNIGHT)).unwrap();
    assert_eq!(bytes.len(), LARGEST_DAY_FILE_BYTES);
}

#[test]
fn a_day_without_a_file_had_no_steps() {
    let mut files = file_system();

    assert_eq!(
        block_on(read_hours(&mut files, DAY_OF_MIDNIGHT)),
        [0; HOURS_PER_DAY]
    );
}

#[test]
fn hours_read_back_from_a_written_day() {
    let mut files = file_system();
    block_on(write_slot(&mut files, finished(DAY_OF_MIDNIGHT, 90, 400))).unwrap();
    block_on(write_slot(&mut files, finished(DAY_OF_MIDNIGHT, 99, 100))).unwrap();

    let hours = block_on(read_hours(&mut files, DAY_OF_MIDNIGHT));

    assert_eq!(hours[9], 500);
    assert_eq!(hours.iter().map(|&steps| steps as u32).sum::<u32>(), 500);
}

#[test]
fn the_trip_count_is_saved_and_read_back() {
    let mut files = file_system();
    assert_eq!(block_on(load_trip(&mut files)), None);

    block_on(save_trip(&mut files, 123_456)).unwrap();

    assert_eq!(block_on(load_trip(&mut files)), Some(123_456));
    assert_eq!(
        read_whole(&mut files, TRIP_PATH).unwrap(),
        123_456u32.to_le_bytes()
    );
}

#[test]
fn a_trip_file_of_the_wrong_size_is_ignored() {
    let mut files = file_system();
    block_on(FileAccess::write_file(&mut files, TRIP_PATH, &[1, 2])).unwrap();

    assert_eq!(block_on(load_trip(&mut files)), None);
}

// ============================================================================
// The history and the log together
// ============================================================================

#[test]
fn storing_a_finished_slot_writes_it_and_the_trip_count() {
    let mut files = file_system();
    let mut history = history_at(MIDNIGHT + 9 * HOUR);
    history.record(MIDNIGHT + 9 * HOUR + MINUTE, 64);

    let recorded = history.record(MIDNIGHT + 9 * HOUR + 7 * MINUTE, 64);
    block_on(store(&mut files, &mut history, recorded)).unwrap();

    assert_eq!(block_on(read_hours(&mut files, DAY_OF_MIDNIGHT))[9], 64);
    assert_eq!(block_on(load_trip(&mut files)), Some(64));
}

#[test]
fn storing_a_reading_without_a_finished_slot_writes_nothing() {
    let mut files = file_system();
    let mut history = history_at(MIDNIGHT + 9 * HOUR);

    let recorded = history.record(MIDNIGHT + 9 * HOUR + MINUTE, 64);
    block_on(store(&mut files, &mut history, recorded)).unwrap();

    assert!(!has_any_step_file(&mut files));
}

#[test]
fn a_restart_reads_the_week_and_the_trip_back_from_the_log() {
    let mut files = file_system();
    let mut history = history_at(MIDNIGHT - 30 * MINUTE);
    for (minutes, count) in [(0, 100), (31, 250), (32, 400), (40, 400)] {
        let recorded = history.record(MIDNIGHT - 30 * MINUTE + minutes * MINUTE, count);
        block_on(store(&mut files, &mut history, recorded)).unwrap();
    }

    let mut restarted = StepHistory::new();
    restarted.start(MIDNIGHT + 30 * MINUTE);
    block_on(load_history(&mut files, &mut restarted));

    assert_eq!(restarted.steps_on_day(1), 250);
    assert_eq!(restarted.steps_today(), 150);
    assert_eq!(restarted.trip_steps(), 400);
}

#[test]
fn after_the_clock_jumps_the_days_are_read_from_the_log() {
    let mut files = file_system();
    block_on(write_slot(&mut files, finished(DAY_OF_MIDNIGHT - 1, 100, 900))).unwrap();
    block_on(write_slot(&mut files, finished(DAY_OF_MIDNIGHT, 10, 30))).unwrap();
    let mut history = history_at(0);

    let recorded = history.record(MIDNIGHT + 2 * HOUR, 0);
    block_on(store(&mut files, &mut history, recorded)).unwrap();

    assert_eq!(history.steps_today(), 30);
    assert_eq!(history.steps_on_day(1), 900);
}

#[test]
fn a_whole_day_of_readings_lands_in_the_log_slot_by_slot() {
    let mut files = file_system();
    let mut history = history_at(MIDNIGHT);
    let mut count = 0;
    let mut time = MIDNIGHT;
    while time <= MIDNIGHT + MILLISECONDS_PER_DAY {
        let since_midnight = time - MIDNIGHT;
        let walking = since_midnight > 8 * HOUR && since_midnight <= 18 * HOUR;
        if walking {
            count += 1;
        }
        let recorded = history.record(time, count);
        block_on(store(&mut files, &mut history, recorded)).unwrap();
        time += 30_000;
    }

    let bytes = read_whole(&mut files, &day_path(DAY_OF_MIDNIGHT)).unwrap();
    assert_eq!(bytes.len(), 180 * BYTES_PER_SLOT);
    assert_eq!(slot_steps(&bytes, 79), 0);
    assert_eq!(slot_steps(&bytes, 80), 12);
    assert_eq!(slot_steps(&bytes, 179), 12);
    assert_eq!(history.steps_on_day(1), 1_200);
    assert_eq!(block_on(read_hours(&mut files, DAY_OF_MIDNIGHT))[8], 120);
}

// ============================================================================
// Keeping the log within the store's records
// ============================================================================

#[test]
fn a_day_path_reads_back_as_its_day() {
    assert_eq!(day_of_path(&day_path(DAY_OF_MIDNIGHT)), Some(DAY_OF_MIDNIGHT));
    assert_eq!(day_of_path(TRIP_PATH), None);
    assert_eq!(day_of_path("/steps/20x6/01-05"), None);
    assert_eq!(day_of_path("/notes/2026/01-05"), None);
}

#[test]
fn days_older_than_the_log_keeps_are_forgotten() {
    let mut files = file_system();
    for days_back in [0, 1, DAYS_KEPT - 1, DAYS_KEPT, DAYS_KEPT + 30] {
        block_on(write_slot(
            &mut files,
            finished(DAY_OF_MIDNIGHT - days_back, 3, 10),
        ))
        .unwrap();
    }
    block_on(save_trip(&mut files, 5)).unwrap();

    block_on(forget_old_days(&mut files, DAY_OF_MIDNIGHT)).unwrap();

    let mut kept = std::vec::Vec::new();
    block_on(FileAccess::for_each_file(
        &mut files,
        STEPS_DIRECTORY,
        &mut |path| kept.push(day_of_path(path)),
    ))
    .unwrap();
    kept.sort();
    let expected = [
        None,
        Some(DAY_OF_MIDNIGHT - DAYS_KEPT + 1),
        Some(DAY_OF_MIDNIGHT - 1),
        Some(DAY_OF_MIDNIGHT),
    ];
    assert_eq!(kept, expected);
}

#[test]
fn a_new_day_forgets_the_oldest_as_it_is_stored() {
    let mut files = file_system();
    block_on(write_slot(
        &mut files,
        finished(DAY_OF_MIDNIGHT - DAYS_KEPT, 3, 10),
    ))
    .unwrap();
    let mut history = history_at(MIDNIGHT - 30 * MINUTE);
    history.record(MIDNIGHT - 29 * MINUTE, 50);

    let recorded = history.record(MIDNIGHT + 10 * MINUTE, 80);
    block_on(store(&mut files, &mut history, recorded)).unwrap();

    assert_eq!(
        read_whole(&mut files, &day_path(DAY_OF_MIDNIGHT - DAYS_KEPT)),
        None
    );
}
