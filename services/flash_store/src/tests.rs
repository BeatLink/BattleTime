extern crate std;

use std::{string::String, vec, vec::Vec};

use embassy_futures::block_on;
use embedded_storage_async::nor_flash::{ErrorType, NorFlash, NorFlashErrorKind, ReadNorFlash};

use super::*;

// ============================================================================
// Test flash
// ============================================================================

const PROGRAM_PAGE_BYTES: usize = 256;

/// Flash in memory that programs as NOR does, and can lose power part way through a write or erase.
pub(crate) struct TestFlash {
    pub(crate) bytes: Vec<u8>,
    pub(crate) changes: u64,
    cut_at: Option<u64>,
    pub(crate) power_lost: bool,
    noise: u64,
    pub(crate) reads_fail: bool,
    pub(crate) erases_per_sector: Vec<u32>,
}

impl TestFlash {
    pub(crate) fn blank(sectors: usize) -> TestFlash {
        TestFlash::holding(vec![0xff; sectors * SECTOR_BYTES as usize])
    }

    pub(crate) fn holding(bytes: Vec<u8>) -> TestFlash {
        TestFlash {
            erases_per_sector: vec![0; bytes.len() / SECTOR_BYTES as usize],
            bytes,
            changes: 0,
            cut_at: None,
            power_lost: false,
            noise: 0x9e37_79b9_7f4a_7c15,
            reads_fail: false,
        }
    }

    /// Loses power during the write or erase numbered `change`, counted from 1, leaving it half done.
    pub(crate) fn cut_power_at(&mut self, change: u64, seed: u64) {
        self.cut_at = Some(change);
        self.noise = seed | 1;
    }

    fn next_noise(&mut self) -> u64 {
        self.noise ^= self.noise << 13;
        self.noise ^= self.noise >> 7;
        self.noise ^= self.noise << 17;
        self.noise
    }

    /// Whether this change lands whole; `None` means the power is already off.
    fn power_for_change(&mut self) -> Option<bool> {
        if self.power_lost {
            return None;
        }
        self.changes += 1;
        self.power_lost = self.cut_at == Some(self.changes);
        Some(!self.power_lost)
    }
}

impl ErrorType for TestFlash {
    type Error = NorFlashErrorKind;
}

impl ReadNorFlash for TestFlash {
    const READ_SIZE: usize = 1;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), NorFlashErrorKind> {
        if self.reads_fail {
            return Err(NorFlashErrorKind::Other);
        }
        let start = offset as usize;
        bytes.copy_from_slice(&self.bytes[start..start + bytes.len()]);
        Ok(())
    }

    fn capacity(&self) -> usize {
        self.bytes.len()
    }
}

impl NorFlash for TestFlash {
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = SECTOR_BYTES as usize;

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), NorFlashErrorKind> {
        for sector in from as usize / Self::ERASE_SIZE..to as usize / Self::ERASE_SIZE {
            let range = sector * Self::ERASE_SIZE..(sector + 1) * Self::ERASE_SIZE;
            match self.power_for_change() {
                None => return Ok(()),
                Some(true) => {
                    self.bytes[range].fill(0xff);
                    self.erases_per_sector[sector] += 1;
                }
                // An interrupted erase leaves each byte erased, as it was, or anything at all.
                Some(false) => {
                    for index in range {
                        let noise = self.next_noise();
                        self.bytes[index] = match noise % 3 {
                            0 => 0xff,
                            1 => self.bytes[index],
                            _ => (noise >> 8) as u8,
                        };
                    }
                }
            }
        }
        Ok(())
    }

    async fn write(&mut self, offset: u32, data: &[u8]) -> Result<(), NorFlashErrorKind> {
        let mut done = 0;
        while done < data.len() {
            let start = offset as usize + done;
            let length = (PROGRAM_PAGE_BYTES - start % PROGRAM_PAGE_BYTES).min(data.len() - done);
            match self.power_for_change() {
                None => return Ok(()),
                Some(true) => {
                    for index in 0..length {
                        self.bytes[start + index] &= data[done + index];
                    }
                }
                // Part of the page lands, and the byte where the power went is only partly programmed.
                Some(false) => {
                    let landed = (self.next_noise() % (length as u64 + 1)) as usize;
                    for index in 0..landed {
                        self.bytes[start + index] &= data[done + index];
                    }
                    if landed < length {
                        let partly = data[done + landed] | self.next_noise() as u8;
                        self.bytes[start + landed] &= partly;
                    }
                    return Ok(());
                }
            }
            done += length;
        }
        Ok(())
    }
}

// ============================================================================
// Helpers
// ============================================================================

pub(crate) type TestStore = FlashStore<TestFlash, 16, 64, 16>;

pub(crate) const LAYOUT: Layout = Layout {
    start_address: 0,
    file_sectors: 32,
    record_sectors: 8,
    log_sectors: 6,
};

pub(crate) fn mount(flash: TestFlash) -> TestStore {
    block_on(TestStore::mount(flash, LAYOUT)).expect("mount")
}

pub(crate) fn blank_store() -> TestStore {
    mount(TestFlash::blank(LAYOUT.total_sectors()))
}

pub(crate) fn remount(store: TestStore) -> TestStore {
    mount(TestFlash::holding(store.into_flash().bytes))
}

pub(crate) fn filler(length: usize, seed: usize) -> Vec<u8> {
    (0..length)
        .map(|index| (index * 31 + seed * 17 + 5) as u8)
        .collect()
}

fn read_record(store: &mut TestStore, key: RecordKey) -> Option<Vec<u8>> {
    let mut buffer = vec![0u8; LARGEST_RECORD_BYTES];
    block_on(store.read_record(key, 0, &mut buffer))
        .unwrap()
        .map(|length| buffer[..length].to_vec())
}

pub(crate) fn read_file(store: &mut TestStore, path: &str) -> Option<Vec<u8>> {
    let mut buffer = vec![0u8; LARGEST_FILE_BYTES];
    block_on(store.read_file(path, 0, &mut buffer))
        .unwrap()
        .map(|length| buffer[..length].to_vec())
}

pub(crate) fn list(store: &mut TestStore, prefix: &str) -> Vec<String> {
    let mut paths = Vec::new();
    block_on(store.for_each_file(prefix, |path| paths.push(String::from(path)))).unwrap();
    paths.sort();
    paths
}

fn read_log(store: &mut TestStore, key: RecordKey) -> Option<Vec<u8>> {
    let mut buffer = vec![0u8; 64 * 1024];
    block_on(store.read_log(key, &mut buffer))
        .unwrap()
        .map(|length| buffer[..length].to_vec())
}

const APP: u32 = 7;
const SYSTEM: u32 = 0xfff;

// ============================================================================
// Records
// ============================================================================

#[test]
fn a_record_reads_back_and_survives_a_restart() {
    let mut store = blank_store();

    block_on(store.write_record(record_key(APP, 1), b"saved values")).unwrap();
    let mut store = remount(store);

    assert_eq!(
        read_record(&mut store, record_key(APP, 1)).unwrap(),
        b"saved values"
    );
    assert_eq!(store.record_length(record_key(APP, 1)), Some(12));
    assert_eq!(read_record(&mut store, record_key(APP, 2)), None);
}

#[test]
fn the_newest_copy_wins_while_the_ring_wraps_many_times() {
    let mut store = blank_store();

    for round in 0..300 {
        block_on(store.write_record(record_key(SYSTEM, round % 5), &filler(400, round as usize))).unwrap();
    }
    let mut store = remount(store);

    for key in 0..5 {
        let last_round = 295 + key;
        assert_eq!(
            read_record(&mut store, record_key(SYSTEM, key)).unwrap(),
            filler(400, last_round as usize)
        );
    }
}

#[test]
fn reading_from_an_offset_gives_the_rest() {
    let mut store = blank_store();
    block_on(store.write_record(record_key(APP, 1), b"0123456789")).unwrap();
    let mut buffer = [0u8; 4];

    let length = block_on(store.read_record(record_key(APP, 1), 6, &mut buffer)).unwrap();

    assert_eq!(length, Some(4));
    assert_eq!(&buffer, b"6789");
}

#[test]
fn a_record_too_large_is_refused() {
    let mut store = blank_store();

    let result = block_on(store.write_record(record_key(SYSTEM, 1), &filler(LARGEST_RECORD_BYTES + 1, 0)));

    assert_eq!(result, Err(StoreError::TooLarge));
}

#[test]
fn records_past_half_the_ring_are_refused_until_some_are_deleted() {
    let mut store = blank_store();
    let mut written = 0;
    let refused = loop {
        match block_on(store.write_record(record_key(SYSTEM, written), &filler(1500, written as usize))) {
            Ok(()) => written += 1,
            Err(error) => break error,
        }
    };

    assert_eq!(refused, StoreError::Full);
    block_on(store.delete_record(record_key(SYSTEM, 0))).unwrap();
    block_on(store.write_record(record_key(SYSTEM, written), &filler(1500, 99))).unwrap();
    let mut store = remount(store);
    assert_eq!(read_record(&mut store, record_key(SYSTEM, 0)), None);
    assert_eq!(
        read_record(&mut store, record_key(SYSTEM, written)).unwrap(),
        filler(1500, 99)
    );
}

#[test]
fn an_app_past_its_record_limit_is_refused_but_the_system_is_not() {
    let mut store = blank_store();
    block_on(store.write_record(record_key(APP, 1), &filler(2000, 1))).unwrap();

    let refused = block_on(store.write_record(record_key(APP, 2), &filler(2100, 2)));
    let rewrite = block_on(store.write_record(record_key(APP, 1), &filler(2100, 3)));
    let system = block_on(store.write_record(record_key(SYSTEM, 1), &filler(2100, 4)));
    let system_more = block_on(store.write_record(record_key(SYSTEM, 2), &filler(2100, 5)));

    assert_eq!(refused, Err(StoreError::OverOwnerLimit));
    assert_eq!(rewrite, Ok(()));
    assert_eq!(system, Ok(()));
    assert_eq!(system_more, Ok(()));
}

#[test]
fn a_new_key_is_refused_when_the_key_table_is_full() {
    let mut store: FlashStore<TestFlash, 4, 3, 16> = block_on(FlashStore::mount(
        TestFlash::blank(LAYOUT.total_sectors()),
        LAYOUT,
    ))
    .unwrap();
    for item in 0..3 {
        block_on(store.write_record(record_key(SYSTEM, item), b"value")).unwrap();
    }

    let refused = block_on(store.write_record(record_key(SYSTEM, 9), b"value"));
    let rewrite = block_on(store.write_record(record_key(SYSTEM, 0), b"newer"));

    assert_eq!(refused, Err(StoreError::KeyTableFull));
    assert_eq!(rewrite, Ok(()));
}

#[test]
fn a_deleted_record_stays_deleted_and_its_tombstone_goes_once_no_copy_is_left() {
    let mut store = blank_store();
    block_on(store.write_record(record_key(APP, 1), b"gone soon")).unwrap();
    block_on(store.delete_record(record_key(APP, 1))).unwrap();

    for round in 0..300 {
        block_on(store.write_record(record_key(SYSTEM, round % 3), &filler(600, round as usize))).unwrap();
    }
    let tombstones_left = store
        .record_keys
        .iter()
        .filter(|entry| entry.is_tombstone())
        .count();
    let mut store = remount(store);

    assert_eq!(read_record(&mut store, record_key(APP, 1)), None);
    assert_eq!(tombstones_left, 0);
    assert_eq!(store.record_bytes_in_use(), 3 * ring::record_bytes(600));
}

#[test]
fn deleting_a_missing_record_writes_nothing() {
    let mut store = blank_store();
    let changes = store.flash().changes;

    block_on(store.delete_record(record_key(APP, 1))).unwrap();

    assert_eq!(store.flash().changes, changes);
}

#[test]
fn wear_spreads_over_the_whole_records_ring() {
    let mut store = blank_store();
    block_on(store.write_record(record_key(SYSTEM, 1), b"rarely changes")).unwrap();

    for round in 0..2000 {
        block_on(store.write_record(record_key(SYSTEM, 2), &filler(300, round))).unwrap();
    }

    let first = LAYOUT.file_sectors;
    let erases = &store.flash().erases_per_sector[first..first + LAYOUT.record_sectors];
    let most = *erases.iter().max().unwrap();
    let least = *erases.iter().min().unwrap();
    assert!(least * 2 >= most, "erases per sector {erases:?}");
    let mut store = remount(store);
    assert_eq!(
        read_record(&mut store, record_key(SYSTEM, 1)).unwrap(),
        b"rarely changes"
    );
}

// ============================================================================
// Files
// ============================================================================

#[test]
fn a_small_file_is_a_record_and_a_large_one_takes_sectors() {
    let mut store = blank_store();
    let free = store.free_file_sectors();

    block_on(store.write_file("/remote/buttons", b"11 bytes ok")).unwrap();
    assert_eq!(store.free_file_sectors(), free);
    block_on(store.write_file("/notes/note.txt", &filler(9000, 1))).unwrap();
    assert_eq!(store.free_file_sectors(), free - 3);
    let mut store = remount(store);

    assert_eq!(read_file(&mut store, "/remote/buttons").unwrap(), b"11 bytes ok");
    assert_eq!(read_file(&mut store, "/notes/note.txt").unwrap(), filler(9000, 1));
    assert_eq!(
        block_on(store.file_length("/notes/note.txt")).unwrap(),
        Some(9000)
    );
    assert_eq!(block_on(store.file_length("/remote/buttons")).unwrap(), Some(11));
    assert_eq!(read_file(&mut store, "/missing"), None);
}

#[test]
fn a_file_moves_between_small_and_large_as_it_is_rewritten() {
    let mut store = blank_store();
    let free = store.free_file_sectors();

    for (round, length) in [10, 5000, 20, 7000, 1024, 1025].into_iter().enumerate() {
        block_on(store.write_file("/settings/theme", &filler(length, round))).unwrap();
        assert_eq!(
            read_file(&mut store, "/settings/theme").unwrap(),
            filler(length, round)
        );
    }
    let mut store = remount(store);

    assert_eq!(read_file(&mut store, "/settings/theme").unwrap(), filler(1025, 5));
    assert_eq!(list(&mut store, "/"), ["/settings/theme"]);
    assert_eq!(store.free_file_sectors(), free - 1);
}

#[test]
fn reading_a_file_from_an_offset_crosses_sectors() {
    let mut store = blank_store();
    let data = filler(12_000, 3);
    block_on(store.write_file("/resources/font.bin", &data)).unwrap();
    let mut buffer = vec![0u8; 5000];

    let length = block_on(store.read_file("/resources/font.bin", 3900, &mut buffer)).unwrap();

    assert_eq!(length, Some(5000));
    assert_eq!(buffer, data[3900..8900]);
}

#[test]
fn an_upload_replaces_the_file_only_when_finished() {
    let mut store = blank_store();
    block_on(store.write_file("/apps/app.ntapp", &filler(6000, 1))).unwrap();

    block_on(store.begin_file("/apps/app.ntapp")).unwrap();
    for part in filler(10_000, 2).chunks(101) {
        block_on(store.write_file_part(part)).unwrap();
    }
    assert_eq!(read_file(&mut store, "/apps/app.ntapp").unwrap(), filler(6000, 1));
    let mut unfinished = remount_keeping_upload(&mut store);
    assert_eq!(
        read_file(&mut unfinished, "/apps/app.ntapp").unwrap(),
        filler(6000, 1)
    );
    block_on(store.finish_file()).unwrap();
    let mut store = remount(store);

    assert_eq!(
        read_file(&mut store, "/apps/app.ntapp").unwrap(),
        filler(10_000, 2)
    );
}

/// A store mounted from a copy of the flash, as after a power cut, leaving the original running.
fn remount_keeping_upload(store: &mut TestStore) -> TestStore {
    mount(TestFlash::holding(store.flash().bytes.clone()))
}

#[test]
fn an_abandoned_upload_gives_its_sectors_back() {
    let mut store = blank_store();
    let free = store.free_file_sectors();

    block_on(store.begin_file("/resources/icons.bin")).unwrap();
    block_on(store.write_file_part(&filler(20_000, 1))).unwrap();
    store.abandon_file();

    assert_eq!(store.free_file_sectors(), free);
    assert_eq!(read_file(&mut store, "/resources/icons.bin"), None);
}

#[test]
fn uploads_are_one_at_a_time() {
    let mut store = blank_store();

    assert_eq!(
        block_on(store.write_file_part(b"data")),
        Err(StoreError::NoUpload)
    );
    assert_eq!(block_on(store.finish_file()), Err(StoreError::NoUpload));
    block_on(store.begin_file("/a")).unwrap();
    assert_eq!(
        block_on(store.begin_file("/b")),
        Err(StoreError::UploadInProgress)
    );
}

#[test]
fn files_past_the_limits_are_refused() {
    let mut store = blank_store();
    let long_path: String = "/".repeat(LONGEST_PATH_BYTES + 1);

    assert_eq!(
        block_on(store.write_file(&long_path, b"data")),
        Err(StoreError::PathTooLong)
    );
    assert_eq!(
        block_on(store.begin_file(&long_path)),
        Err(StoreError::PathTooLong)
    );
    assert_eq!(
        block_on(store.write_file("/too/large", &filler(LARGEST_FILE_BYTES + 1, 0))),
        Err(StoreError::TooLarge)
    );
    assert_eq!(read_file(&mut store, "/too/large"), None);
}

#[test]
fn the_file_area_and_table_refuse_more_when_full() {
    let mut store = blank_store();
    let mut area_full = None;
    for index in 0..8 {
        if let Err(error) = block_on(store.write_file(&std::format!("/big/{index}"), &filler(16_500, index)))
        {
            area_full = Some(error);
            break;
        }
    }
    let mut store = blank_store();
    let mut table_full = None;
    for index in 0..20 {
        if let Err(error) = block_on(store.write_file(&std::format!("/file/{index}"), &filler(2000, index))) {
            table_full = Some(error);
            break;
        }
    }

    assert_eq!(area_full, Some(StoreError::FileAreaFull));
    assert_eq!(table_full, Some(StoreError::FileTableFull));
}

#[test]
fn deleting_removes_a_file_in_either_form() {
    let mut store = blank_store();
    block_on(store.write_file("/small", b"tiny")).unwrap();
    block_on(store.write_file("/large", &filler(5000, 0))).unwrap();

    block_on(store.delete_file("/small")).unwrap();
    block_on(store.delete_file("/large")).unwrap();
    block_on(store.delete_file("/never")).unwrap();
    let mut store = remount(store);

    assert_eq!(read_file(&mut store, "/small"), None);
    assert_eq!(read_file(&mut store, "/large"), None);
    assert!(list(&mut store, "/").is_empty());
}

#[test]
fn listing_gives_each_path_under_a_prefix_once() {
    let mut store = blank_store();
    for (path, length) in [
        ("/apps/a", 5000),
        ("/apps/b", 10),
        ("/notes/note.txt", 30),
        ("/apps/c", 2000),
    ] {
        block_on(store.write_file(path, &filler(length, 0))).unwrap();
    }

    assert_eq!(list(&mut store, "/apps/"), ["/apps/a", "/apps/b", "/apps/c"]);
    assert_eq!(
        list(&mut store, "/"),
        ["/apps/a", "/apps/b", "/apps/c", "/notes/note.txt"]
    );
}

#[test]
fn a_small_file_whose_key_another_path_holds_goes_into_sectors() {
    let mut store = blank_store();
    let (first, second) = colliding_paths();
    let free = store.free_file_sectors();

    block_on(store.write_file(&first, b"first")).unwrap();
    block_on(store.write_file(&second, b"second")).unwrap();

    assert_eq!(read_file(&mut store, &first).unwrap(), b"first");
    assert_eq!(read_file(&mut store, &second).unwrap(), b"second");
    assert_eq!(store.free_file_sectors(), free - 1);
}

/// Two paths whose hashes share their low 20 bits, so they want the same small-file key.
fn colliding_paths() -> (String, String) {
    let mut seen = std::collections::HashMap::new();
    for index in 0.. {
        let path = std::format!("/p/{index}");
        let key = checksum::path_hash(&path) & 0x000f_ffff;
        if let Some(earlier) = seen.insert(key, path.clone()) {
            return (earlier, path);
        }
    }
    unreachable!()
}

// ============================================================================
// Logs
// ============================================================================

#[test]
fn log_appends_are_batched_and_read_with_what_is_waiting() {
    let mut store = blank_store();
    let steps = record_key(SYSTEM, 1);
    let heart = record_key(SYSTEM, 2);
    let changes = store.flash().changes;

    for slot in 0..10u8 {
        block_on(store.append_log(steps, &[slot, slot])).unwrap();
        block_on(store.append_log(heart, &[60 + slot])).unwrap();
    }
    assert_eq!(store.flash().changes, changes);
    let expected_steps: Vec<u8> = (0..10u8).flat_map(|slot| [slot, slot]).collect();
    assert_eq!(read_log(&mut store, steps).unwrap(), expected_steps);
    block_on(store.flush_logs()).unwrap();
    let mut store = remount(store);

    assert_eq!(read_log(&mut store, steps).unwrap(), expected_steps);
    assert_eq!(
        read_log(&mut store, heart).unwrap(),
        (60..70).collect::<Vec<u8>>()
    );
    assert_eq!(read_log(&mut store, record_key(SYSTEM, 3)), None);
}

#[test]
fn a_full_batch_and_a_fifth_stream_are_written_out() {
    let mut store = blank_store();

    for stream in 0..5 {
        for _ in 0..40 {
            block_on(store.append_log(record_key(SYSTEM, stream), &[stream as u8; 3])).unwrap();
        }
    }
    block_on(store.append_log(record_key(SYSTEM, 9), &filler(500, 1))).unwrap();
    let mut store = remount(store);

    assert_eq!(
        read_log(&mut store, record_key(SYSTEM, 0)).unwrap(),
        vec![0u8; 120]
    );
    assert_eq!(
        read_log(&mut store, record_key(SYSTEM, 9)).unwrap(),
        filler(500, 1)
    );
}

#[test]
fn the_log_drops_its_oldest_entries_when_it_wraps() {
    let mut store = blank_store();

    for day in 0..40 {
        block_on(store.append_log(record_key(SYSTEM, day), &filler(2000, day as usize))).unwrap();
    }

    assert_eq!(read_log(&mut store, record_key(SYSTEM, 0)), None);
    assert_eq!(
        read_log(&mut store, record_key(SYSTEM, 39)).unwrap(),
        filler(2000, 39)
    );
}

#[test]
fn a_log_read_into_a_short_buffer_gives_the_whole_length() {
    let mut store = blank_store();
    block_on(store.append_log(record_key(SYSTEM, 1), &filler(300, 1))).unwrap();
    block_on(store.append_log(record_key(SYSTEM, 1), &[1, 2, 3])).unwrap();
    let mut buffer = [0u8; 100];

    let length = block_on(store.read_log(record_key(SYSTEM, 1), &mut buffer)).unwrap();

    assert_eq!(length, Some(303));
    assert_eq!(buffer[..], filler(300, 1)[..100]);
}

// ============================================================================
// Mounting
// ============================================================================

#[test]
fn a_store_of_another_format_version_is_refused() {
    let mut store = blank_store();
    block_on(store.write_record(record_key(APP, 1), b"value")).unwrap();
    let mut bytes = store.into_flash().bytes;
    let ring_start = LAYOUT.file_sectors * SECTOR_BYTES as usize;
    bytes[ring_start + 11] = FORMAT_VERSION + 1;
    let checksum = checksum::Checksum::of(&bytes[ring_start..ring_start + 12]);
    bytes[ring_start + 12..ring_start + 16].copy_from_slice(&checksum.to_le_bytes());

    let result = block_on(TestStore::mount(TestFlash::holding(bytes), LAYOUT));

    assert!(matches!(result, Err(StoreError::OtherFormatVersion)));
}

#[test]
fn a_file_of_another_format_version_is_refused() {
    let mut store = blank_store();
    block_on(store.write_file("/large", &filler(5000, 0))).unwrap();
    let mut bytes = store.into_flash().bytes;
    let first = (0..LAYOUT.file_sectors)
        .find(|sector| bytes[sector * SECTOR_BYTES as usize] == b'N')
        .unwrap();
    let header = first * SECTOR_BYTES as usize;
    bytes[header + 17] = FORMAT_VERSION + 1;
    let checksum = checksum::Checksum::of(&bytes[header..header + 212]);
    bytes[header + 212..header + 216].copy_from_slice(&checksum.to_le_bytes());

    let result = block_on(TestStore::mount(TestFlash::holding(bytes), LAYOUT));

    assert!(matches!(result, Err(StoreError::OtherFormatVersion)));
}

#[test]
fn a_layout_larger_than_the_tables_is_refused() {
    let layout = Layout {
        record_sectors: 17,
        ..LAYOUT
    };

    let result = block_on(TestStore::mount(TestFlash::blank(layout.total_sectors()), layout));

    assert!(matches!(result, Err(StoreError::LayoutDoesNotFit)));
}

#[test]
fn a_flash_that_cannot_be_read_is_reported_not_formatted() {
    let mut flash = TestFlash::blank(LAYOUT.total_sectors());
    flash.reads_fail = true;

    let result = block_on(TestStore::mount(flash, LAYOUT));

    assert!(matches!(result, Err(StoreError::Flash(NorFlashErrorKind::Other))));
}

#[test]
fn a_corrupted_file_is_reported() {
    let mut store = blank_store();
    block_on(store.write_file("/large", &filler(5000, 0))).unwrap();
    let mut flash = store.into_flash();
    let first = (0..LAYOUT.file_sectors)
        .find(|sector| flash.bytes[sector * SECTOR_BYTES as usize] == b'N')
        .unwrap();
    flash.bytes[first * SECTOR_BYTES as usize + 1000] ^= 0x01;
    let mut store = mount(flash);
    let mut buffer = vec![0u8; 5000];

    let result = block_on(store.read_file("/large", 0, &mut buffer));

    assert_eq!(result, Err(StoreError::Corrupted));
}

#[test]
fn the_ram_it_keeps_is_known_when_built() {
    type WatchStore = FlashStore<TestFlash, 128, 128, 192>;

    let ram = WatchStore::RAM_BYTES;

    assert!(ram < 3 * 1024, "{ram} B");
}
