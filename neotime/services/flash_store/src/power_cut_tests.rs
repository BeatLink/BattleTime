extern crate std;

use std::{
    collections::{BTreeSet, HashMap},
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use embassy_futures::block_on;

use super::tests::{filler, list, mount, read_file, TestFlash, TestStore, LAYOUT};
use super::*;

// ============================================================================
// The scripted run
// ============================================================================

const OPERATION_COUNT: usize = 160;
/// How many operations the run carries on for after a restart before it is checked again.
const OPERATIONS_AFTER_RESTART: usize = 12;
const PATHS: [&str; 4] = [
    "/notes/note.txt",
    "/remote/buttons",
    "/resources/font.bin",
    "/apps/1.ntapp",
];
const LOG_KEY: RecordKey = record_key(0xfff, 40);

#[derive(Clone, Debug)]
enum Operation {
    Record(RecordKey, Vec<u8>),
    File(String, Vec<u8>),
    /// A file sent a part at a time, as over Bluetooth.
    Upload(String, Vec<u8>),
    Delete(String),
    /// Log bytes, flushed at once so the write is part of the operation.
    Log(Vec<u8>),
}

/// What the store should hold after each operation.
#[derive(Clone, Default)]
struct State {
    records: HashMap<RecordKey, Vec<u8>>,
    files: HashMap<String, Vec<u8>>,
    log: Vec<u8>,
}

fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

fn script() -> (Vec<Operation>, Vec<State>) {
    let mut seed = 7u64;
    let mut operations = Vec::new();
    let mut states = vec![State::default()];
    for index in 0..OPERATION_COUNT {
        let mut state = states.last().unwrap().clone();
        let path = PATHS[(next(&mut seed) % PATHS.len() as u64) as usize].to_string();
        let operation = match next(&mut seed) % 100 {
            0..=44 => {
                let key = record_key(1 + (next(&mut seed) % 4) as u32, 1);
                Operation::Record(key, filler(20 + (next(&mut seed) % 900) as usize, index))
            }
            45..=59 => {
                let length = if next(&mut seed).is_multiple_of(2) {
                    1 + next(&mut seed) % 1024
                } else {
                    1025 + next(&mut seed) % 9000
                };
                Operation::File(path, filler(length as usize, index))
            }
            60..=67 => Operation::Upload(path, filler(500 + (next(&mut seed) % 9000) as usize, index)),
            68..=71 => Operation::Delete(path),
            _ => Operation::Log(filler(2 + (next(&mut seed) % 1500) as usize, index)),
        };
        match &operation {
            Operation::Record(key, value) => {
                state.records.insert(*key, value.clone());
            }
            Operation::File(path, data) | Operation::Upload(path, data) => {
                state.files.insert(path.clone(), data.clone());
            }
            Operation::Delete(path) => {
                state.files.remove(path);
            }
            Operation::Log(bytes) => state.log.extend_from_slice(bytes),
        }
        operations.push(operation);
        states.push(state);
    }
    (operations, states)
}

// ============================================================================
// Running and checking
// ============================================================================

fn apply(store: &mut TestStore, operation: &Operation) {
    let result = match operation {
        Operation::Record(key, value) => block_on(store.write_record(*key, value)),
        Operation::File(path, data) => block_on(store.write_file(path, data)),
        Operation::Upload(path, data) => block_on(async {
            store.begin_file(path).await?;
            for part in data.chunks(101) {
                store.write_file_part(part).await?;
            }
            store.finish_file().await
        }),
        Operation::Delete(path) => block_on(store.delete_file(path)),
        Operation::Log(bytes) => block_on(async {
            store.append_log(LOG_KEY, bytes).await?;
            store.flush_logs().await
        }),
    };
    if !store.flash().power_lost {
        result.unwrap_or_else(|error| panic!("{operation:?} failed: {error:?}"));
    }
}

/// Runs operations from `first` until `last` or until the power goes, and says which one it cut short.
fn run_until_cut(
    store: &mut TestStore,
    operations: &[Operation],
    first: usize,
    last: usize,
) -> Option<usize> {
    for (index, operation) in operations.iter().enumerate().take(last).skip(first) {
        apply(store, operation);
        if store.flash().power_lost {
            return Some(index);
        }
    }
    None
}

fn restart(store: &TestStore, second_cut: Option<u64>) -> TestStore {
    let mut flash = TestFlash::holding(store.flash().bytes.clone());
    if let Some(change) = second_cut {
        flash.cut_power_at(change, change.wrapping_mul(0x85eb_ca6b));
    }
    block_on(TestStore::mount(flash, LAYOUT)).unwrap_or_else(|error| panic!("mount after a cut: {error:?}"))
}

/// Every record and file must be as before the interrupted operation or as it left it; the listing holds no stray.
fn check(store: &mut TestStore, before: &State, after: &State, context: &str) -> Result<(), String> {
    let mut buffer = vec![0u8; LARGEST_RECORD_BYTES];
    let keys: BTreeSet<RecordKey> = before
        .records
        .keys()
        .chain(after.records.keys())
        .copied()
        .collect();
    for key in keys {
        let found = block_on(store.read_record(key, 0, &mut buffer))
            .unwrap()
            .map(|length| buffer[..length].to_vec());
        if found.as_ref() != before.records.get(&key) && found.as_ref() != after.records.get(&key) {
            return Err(format!(
                "{context}: record {key:08x} holds {:?} B",
                found.map(|value| value.len())
            ));
        }
    }
    let paths: BTreeSet<&String> = before.files.keys().chain(after.files.keys()).collect();
    for path in &paths {
        let found = read_file(store, path);
        if found.as_ref() != before.files.get(*path) && found.as_ref() != after.files.get(*path) {
            return Err(format!(
                "{context}: {path} holds {:?} B",
                found.map(|data| data.len())
            ));
        }
    }
    for path in list(store, "/") {
        if !paths.contains(&path) {
            return Err(format!("{context}: lists {path}, which neither state holds"));
        }
    }
    let mut log = vec![0u8; 64 * 1024];
    let log_length = block_on(store.read_log(LOG_KEY, &mut log)).unwrap().unwrap_or(0);
    // The log ring drops its oldest sectors when it wraps, so what is left must be an exact tail of the log.
    let log = &log[..log_length];
    if !before.log.ends_with(log) && !after.log.ends_with(log) {
        return Err(format!(
            "{context}: the log holds {log_length} B that are not its newest"
        ));
    }
    Ok(())
}

/// Whether the interrupted operation was a log append that had already landed when the power went.
fn appended_before_the_cut(store: &mut TestStore, operation: &Operation, before: &State) -> bool {
    if !matches!(operation, Operation::Log(_)) {
        return false;
    }
    let mut log = vec![0u8; 64 * 1024];
    let length = block_on(store.read_log(LOG_KEY, &mut log)).unwrap().unwrap_or(0);
    !before.log.ends_with(&log[..length])
}

fn try_cut(
    operations: &[Operation],
    states: &[State],
    cut: u64,
    second_cut: Option<u64>,
) -> Result<(), String> {
    let mut flash = TestFlash::blank(LAYOUT.total_sectors());
    flash.cut_power_at(cut, cut.wrapping_mul(0x9e37_79b9));
    let mut store = mount(flash);
    let interrupted =
        run_until_cut(&mut store, operations, 0, operations.len()).ok_or("the cut came after the run")?;

    let mut restarted = restart(&store, None);
    check(
        &mut restarted,
        &states[interrupted],
        &states[interrupted + 1],
        &format!("cut {cut}"),
    )?;

    // Records and files can be written again, but a log append that landed must not be appended twice.
    let carry_on_from =
        if appended_before_the_cut(&mut restarted, &operations[interrupted], &states[interrupted]) {
            interrupted + 1
        } else {
            interrupted
        };
    let carry_on_until = (interrupted + OPERATIONS_AFTER_RESTART).min(operations.len());
    let mut carried_on = restart(&store, second_cut);
    let (before, after) = if carried_on.flash().power_lost {
        (&states[interrupted], &states[interrupted + 1])
    } else {
        match run_until_cut(&mut carried_on, operations, carry_on_from, carry_on_until) {
            Some(index) => (&states[index], &states[index + 1]),
            None => (&states[carry_on_until], &states[carry_on_until]),
        }
    };
    let mut last = restart(&carried_on, None);
    check(
        &mut last,
        before,
        after,
        &format!("cut {cut}, then {second_cut:?} while recovering"),
    )
}

// ============================================================================
// The test
// ============================================================================

#[test]
fn every_value_survives_a_power_cut_at_any_write_or_erase() {
    let (operations, states) = script();
    let mut store = mount(TestFlash::blank(LAYOUT.total_sectors()));
    assert_eq!(run_until_cut(&mut store, &operations, 0, operations.len()), None);
    check(
        &mut store,
        &states[OPERATION_COUNT],
        &states[OPERATION_COUNT],
        "without a cut",
    )
    .unwrap();
    let total = store.flash().changes;
    let record_start = LAYOUT.file_sectors;
    let log_start = record_start + LAYOUT.record_sectors;
    let erases = &store.flash().erases_per_sector;
    let records_ring = &erases[record_start..log_start];
    let records_lapped =
        records_ring.iter().all(|count| *count >= 1) && records_ring.iter().any(|count| *count >= 2);
    assert!(records_lapped, "the records ring wraps: {erases:?}");
    assert!(
        erases[log_start..].iter().all(|count| *count >= 1),
        "the log ring wraps: {erases:?}"
    );

    let mut seed = 11u64;
    let mut failures = Vec::new();
    for cut in 1..=total {
        let second = 1 + next(&mut seed) % 300;
        for second_cut in [None, Some(second)] {
            if let Err(problem) = try_cut(&operations, &states, cut, second_cut) {
                failures.push(problem);
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} runs failed, first: {:?}",
        failures.len(),
        total * 2,
        failures.first()
    );
}

// ============================================================================
// Programming a blank file in place
// ============================================================================

const BLANK_PATH: &str = "/blocks/7";
const SLOT_COUNT: usize = 40;

/// Two bytes a slot, every slot different, none of them erased.
fn slot_value(slot: usize) -> [u8; 2] {
    [slot as u8, 0x80 | slot as u8]
}

/// Programs one slot at a time into a blank file the first makes; gives how many were written before the power went.
fn program_slots(store: &mut TestStore) -> usize {
    for slot in 0..SLOT_COUNT {
        let _ = block_on(store.program_file(BLANK_PATH, 1024 + 2 * slot, &slot_value(slot)));
        if store.flash().power_lost {
            return slot;
        }
    }
    SLOT_COUNT
}

#[test]
fn a_power_cut_while_programming_a_blank_file_spoils_at_most_the_slot_being_written() {
    let mut store = mount(TestFlash::blank(LAYOUT.total_sectors()));
    assert_eq!(program_slots(&mut store), SLOT_COUNT);
    let total = store.flash().changes;

    for cut in 1..=total {
        let mut flash = TestFlash::blank(LAYOUT.total_sectors());
        flash.cut_power_at(cut, cut.wrapping_mul(0x9e37_79b9));
        let mut store = mount(flash);
        let written = program_slots(&mut store);
        let mut store = restart(&store, None);
        let Some(bytes) = read_file(&mut store, BLANK_PATH) else {
            assert_eq!(written, 0, "cut {cut}: the file is gone after {written} slots");
            continue;
        };
        assert_eq!(bytes.len(), BLANK_FILE_BYTES, "cut {cut}");
        let slots = &bytes[1024..];
        for slot in 0..SLOT_COUNT {
            let found = [slots[2 * slot], slots[2 * slot + 1]];
            let as_expected = match slot.cmp(&written) {
                core::cmp::Ordering::Less => found == slot_value(slot),
                core::cmp::Ordering::Equal => true,
                core::cmp::Ordering::Greater => found == [0xff, 0xff],
            };
            assert!(
                as_expected,
                "cut {cut}: slot {slot} holds {found:?} after {written} slots"
            );
        }
        assert!(bytes[..1024].iter().all(|byte| *byte == 0xff), "cut {cut}");
    }
}
