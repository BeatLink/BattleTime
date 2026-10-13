//! Cuts the power at every write and erase of a scripted run of the custom store, and checks what survives.
//!
//! After each cut the store is mounted again and every value and file must be as it was before the interrupted
//! operation or as that operation left it. The run then carries on, with a second cut somewhere in the recovery,
//! and is checked again. The flash is small, so both rings wrap many times and the file area reuses its sectors.

use std::collections::{BTreeSet, HashMap};

use embassy_futures::block_on;

use crate::{
    custom_store::{app_path, CustomStore, Geometry},
    flash::{SharedCounters, SimulatedFlash, Timing},
    stores::Store,
    workload::{key, Key, Kind, Shuffle},
};

/// Live data fills under half of each ring, as on the watch, where it fills about a tenth.
const GEOMETRY: Geometry = Geometry {
    file_sectors: 128,
    record_sectors: 16,
    log_sectors: 8,
};
const OPERATION_COUNT: usize = 700;
/// How many operations the run carries on for after a restart before it is checked again.
const OPERATIONS_AFTER_RESTART: usize = 20;
/// Named files, written small and large in turn, so they move between records and sectors.
const PATHS: [&str; 6] = [
    "/notes/note.txt",
    "/remote/buttons",
    "/settings/theme",
    "/resources/icons.bin",
    "/resources/font.bin",
    "/apps/007/data",
];

#[derive(Clone)]
enum Operation {
    Put(Key, Vec<u8>),
    File(String, Vec<u8>),
    Delete(String),
}

/// What the store should hold: values by key, and files by path, app images included.
#[derive(Clone, Default)]
struct State {
    values: HashMap<Key, Vec<u8>>,
    files: HashMap<String, Vec<u8>>,
}

// ============================================================================
// The scripted run
// ============================================================================

/// Each operation changes one value or file; the expected state after each is worked out alongside.
fn script() -> (Vec<Operation>, Vec<State>) {
    let mut shuffle = Shuffle::new(7);
    let mut operations = Vec::new();
    let mut expected = vec![State::default()];
    for index in 0..OPERATION_COUNT {
        let mut state = expected.last().unwrap().clone();
        let choice = shuffle.next() % 100;
        let operation = if choice < 6 {
            let app = 1 + (shuffle.next() % 5) as u32;
            let length = 200 + (shuffle.next() % 14_000) as usize;
            Operation::Put(key(app, Kind::WholeImage, 0), filler(length, index))
        } else if choice < 21 {
            let path = PATHS[(shuffle.next() % PATHS.len() as u64) as usize];
            let length = if shuffle.next() % 2 == 0 {
                1 + (shuffle.next() % 1024) as usize
            } else {
                1025 + (shuffle.next() % 40_000) as usize
            };
            Operation::File(path.to_string(), filler(length, index))
        } else if choice < 24 {
            Operation::Delete(PATHS[(shuffle.next() % PATHS.len() as u64) as usize].to_string())
        } else if choice < 70 {
            let app = 1 + (shuffle.next() % 8) as u32;
            let length = 20 + (shuffle.next() % 1200) as usize;
            Operation::Put(key(app, Kind::SavedState, 0), filler(length, index))
        } else {
            // A new day every 40 operations keeps each day well inside the log ring.
            let day = index as u32 / 40;
            let day_key = key(0xfff, Kind::Steps, day);
            let mut log = state.values.get(&day_key).cloned().unwrap_or_default();
            log.extend(filler(2 + (shuffle.next() % 600) as usize, index));
            Operation::Put(day_key, log)
        };
        match &operation {
            Operation::Put(key, value) if (key >> 12 & 0xf) == Kind::WholeImage as u32 => {
                state.files.insert(app_path(*key), value.clone());
            }
            Operation::Put(key, value) => {
                state.values.insert(*key, value.clone());
            }
            Operation::File(path, data) => {
                state.files.insert(path.clone(), data.clone());
            }
            Operation::Delete(path) => {
                state.files.remove(path);
            }
        }
        operations.push(operation);
        expected.push(state);
    }
    (operations, expected)
}

fn filler(length: usize, seed: usize) -> Vec<u8> {
    (0..length).map(|index| (index * 31 + seed * 17 + 5) as u8).collect()
}

// ============================================================================
// Running and checking
// ============================================================================

fn apply(store: &mut CustomStore, operation: &Operation) {
    match operation {
        Operation::Put(key, value) => block_on(store.write_all(&[(*key, value.clone())])),
        Operation::File(path, data) => block_on(store.write_file(path, data)),
        Operation::Delete(path) => block_on(store.delete_file(path)),
    }
}

/// Runs operations from `first` until `last` or until the power goes; gives the store and the operation cut short.
fn run_until_cut(
    mut store: CustomStore,
    counters: &SharedCounters,
    operations: &[Operation],
    first: usize,
    last: usize,
) -> (CustomStore, Option<usize>) {
    for (index, operation) in operations.iter().enumerate().take(last).skip(first) {
        apply(&mut store, operation);
        if counters.get().power_lost {
            return (store, Some(index));
        }
    }
    (store, None)
}

fn restart(store: &CustomStore, second_cut: Option<(u64, u64)>) -> (CustomStore, SharedCounters) {
    let (mut flash, counters) = SimulatedFlash::from_bytes(store.flash().bytes().to_vec(), Timing::ASSUMED);
    if let Some((change, seed)) = second_cut {
        flash.cut_power_at(change, seed);
    }
    let store = block_on(CustomStore::mount(flash, GEOMETRY));
    (store, counters)
}

/// Every value and file must match one of the two allowed states, and the listing must hold no stray path.
///
/// The step log drops its oldest sectors when it wraps, so an older day may have lost its start: what is left
/// must be an exact tail of the day. The newest day must be whole.
fn check(store: &mut CustomStore, before: &State, after: &State, context: &str) -> Result<(), String> {
    let mut buffer = vec![0u8; 256 * 1024];
    let newest_day = after
        .values
        .keys()
        .filter(|key| (*key >> 12 & 0xf) == Kind::Steps as u32)
        .map(|key| key & 0xfff)
        .max()
        .unwrap_or(0);
    let keys: BTreeSet<Key> = before.values.keys().chain(after.values.keys()).copied().collect();
    for key in keys {
        let is_older_day = (key >> 12 & 0xf) == Kind::Steps as u32 && (key & 0xfff) < newest_day;
        let found = block_on(store.read(key, &mut buffer)).map(|length| buffer[..length].to_vec());
        let is_tail_of = |expected: Option<&Vec<u8>>| match (&found, expected) {
            (None, _) => true,
            (Some(found), Some(expected)) => expected.ends_with(found),
            (Some(_), None) => false,
        };
        if is_older_day && (is_tail_of(before.values.get(&key)) || is_tail_of(after.values.get(&key))) {
            continue;
        }
        if found.as_ref() != before.values.get(&key) && found.as_ref() != after.values.get(&key) {
            return Err(format!(
                "{context}: key {key:08x} holds {:?} B, expected {:?} or {:?} B",
                found.as_ref().map(Vec::len),
                before.values.get(&key).map(Vec::len),
                after.values.get(&key).map(Vec::len)
            ));
        }
    }

    let paths: BTreeSet<&String> = before.files.keys().chain(after.files.keys()).collect();
    for path in &paths {
        let found = block_on(store.read_file(path, 0, &mut buffer)).map(|length| buffer[..length].to_vec());
        if found.as_ref() != before.files.get(*path) && found.as_ref() != after.files.get(*path) {
            return Err(format!(
                "{context}: {path} holds {:?} B, expected {:?} or {:?} B",
                found.as_ref().map(Vec::len),
                before.files.get(*path).map(Vec::len),
                after.files.get(*path).map(Vec::len)
            ));
        }
    }
    let listed: BTreeSet<String> = block_on(store.list_files("/")).into_iter().collect();
    for path in &listed {
        if !paths.contains(path) {
            return Err(format!("{context}: lists {path}, which neither state holds"));
        }
    }
    for path in before.files.keys().filter(|path| after.files.contains_key(*path)) {
        if !listed.contains(path) {
            return Err(format!("{context}: leaves {path} out of the listing"));
        }
    }
    Ok(())
}

/// One cut at `cut`, a restart, and a carried-on run with a second cut at `second_cut` when given.
fn try_cut(operations: &[Operation], expected: &[State], cut: u64, second_cut: Option<u64>) -> Result<(), String> {
    let (mut flash, counters) = SimulatedFlash::new(GEOMETRY.sector_count(), Timing::ASSUMED);
    flash.cut_power_at(cut, cut.wrapping_mul(0x9e37_79b9));
    let store = block_on(CustomStore::mount(flash, GEOMETRY));
    let (store, cut_short) = if counters.get().power_lost {
        (store, Some(0))
    } else {
        run_until_cut(store, &counters, operations, 0, operations.len())
    };
    let Some(interrupted) = cut_short else {
        return Err(format!("cut {cut} came after the run ended"));
    };

    let (mut restarted, _) = restart(&store, None);
    check(&mut restarted, &expected[interrupted], &expected[interrupted + 1], &format!("cut {cut} at operation {interrupted}"))?;

    // Carry on from the interrupted operation, with a second cut when asked, then restart and check again.
    let carry_on_until = (interrupted + OPERATIONS_AFTER_RESTART).min(operations.len());
    let (store, counters) = restart(&store, second_cut.map(|change| (change, change.wrapping_mul(0x85eb_ca6b))));
    let (store, second_interrupted) = if counters.get().power_lost {
        (store, Some(interrupted))
    } else {
        run_until_cut(store, &counters, operations, interrupted, carry_on_until)
    };
    let (allowed_before, allowed_after) = match second_interrupted {
        Some(index) => (&expected[index], &expected[index + 1]),
        None => (&expected[carry_on_until], &expected[carry_on_until]),
    };
    let (mut final_store, _) = restart(&store, None);
    check(&mut final_store, allowed_before, allowed_after, &format!("cut {cut}, then {second_cut:?} while recovering"))
}

pub fn run() {
    let (operations, expected) = script();
    let (flash, counters) = SimulatedFlash::new(GEOMETRY.sector_count(), Timing::ASSUMED);
    let store = block_on(CustomStore::mount(flash, GEOMETRY));
    let (mut store, _) = run_until_cut(store, &counters, &operations, 0, operations.len());
    if let Err(problem) = check(&mut store, &expected[OPERATION_COUNT], &expected[OPERATION_COUNT], "without a cut") {
        panic!("{problem}");
    }
    let total = counters.get().changes;
    let flash = store.flash();
    println!(
        "A run of {OPERATION_COUNT} operations makes {total} writes and erases; most erases of one sector: files {}, records {}, logs {}.",
        flash.most_erases_of_one_sector(GEOMETRY.file_range()),
        flash.most_erases_of_one_sector(GEOMETRY.record_range()),
        flash.most_erases_of_one_sector(GEOMETRY.log_range()),
    );

    let mut shuffle = Shuffle::new(11);
    let mut failures = 0;
    for cut in 1..=total {
        // The second cut lands somewhere in the recovery: the mount and the operations carried on after it.
        let second = 1 + shuffle.next() % 400;
        for second_cut in [None, Some(second)] {
            if let Err(problem) = try_cut(&operations, &expected, cut, second_cut) {
                failures += 1;
                if failures <= 10 {
                    println!("FAILED {problem}");
                }
            }
        }
    }
    println!(
        "Cut the power at each of the {total} writes and erases, each also with a second cut while recovering: {} runs, {failures} failed.",
        total * 2
    );
    if failures > 0 {
        std::process::exit(1);
    }
}
