extern crate std;

use core::cell::Cell;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    println, thread_local, vec,
    vec::Vec,
};

use embedded_storage::nor_flash::{ErrorType, NorFlashErrorKind, ReadNorFlash};
use littlefs_rust::{FileType, OpenFlags};
use neotime_boot::EXTERNAL_FLASH_SIZE;

use super::*;

// ============================================================================
// Fake flash
// ============================================================================

struct FakeFlash {
    memory: Vec<u8>,
    reads_fail: bool,
    erase_count: usize,
}

impl FakeFlash {
    fn blank() -> FakeFlash {
        FakeFlash {
            memory: vec![0xFF; EXTERNAL_FLASH_SIZE as usize],
            reads_fail: false,
            erase_count: 0,
        }
    }
}

impl ErrorType for FakeFlash {
    type Error = NorFlashErrorKind;
}

impl ReadNorFlash for FakeFlash {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        if self.reads_fail {
            return Err(NorFlashErrorKind::Other);
        }
        let start = offset as usize;
        bytes.copy_from_slice(&self.memory[start..start + bytes.len()]);
        Ok(())
    }

    fn capacity(&self) -> usize {
        self.memory.len()
    }
}

impl NorFlash for FakeFlash {
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = BLOCK_SIZE as usize;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        self.memory[from as usize..to as usize].fill(0xFF);
        self.erase_count += 1;
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        let start = offset as usize;
        for (memory_byte, new_byte) in self.memory[start..start + bytes.len()].iter_mut().zip(bytes) {
            *memory_byte &= new_byte;
        }
        Ok(())
    }
}

fn prepared(flash: FakeFlash) -> PreparedFileSystem<FakeFlash> {
    match prepare_file_system(FileSystemFlash::new(flash)) {
        Ok(prepared) => prepared,
        Err(failure) => panic!("preparing failed: {:?}", failure.error),
    }
}

// ============================================================================
// Counting heap use, per test thread
// ============================================================================

thread_local! {
    static HEAP_BYTES_IN_USE: Cell<isize> = const { Cell::new(0) };
}

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = HEAP_BYTES_IN_USE.try_with(|in_use| in_use.set(in_use.get() + layout.size() as isize));
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        let _ = HEAP_BYTES_IN_USE.try_with(|in_use| in_use.set(in_use.get() - layout.size() as isize));
        System.dealloc(pointer, layout)
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn heap_bytes_in_use() -> isize {
    HEAP_BYTES_IN_USE.with(Cell::get)
}

// ============================================================================
// Preparing
// ============================================================================

#[test]
fn formats_blank_flash_and_makes_the_top_level_directories() {
    let prepared = prepared(FakeFlash::blank());

    assert_eq!(prepared.preparation, Preparation::FormattedBlankFlash);
    for directory in TOP_LEVEL_DIRECTORIES {
        assert_eq!(
            prepared.file_system.stat(directory).unwrap().file_type,
            FileType::Dir
        );
    }
}

#[test]
fn keeps_files_when_mounted_again() {
    let first_start = prepared(FakeFlash::blank());
    first_start
        .file_system
        .write_file("/settings/clock", b"24h")
        .unwrap();
    let flash = first_start.file_system.unmount().unwrap().release();

    let second_start = prepared(flash);

    assert_eq!(second_start.preparation, Preparation::AlreadyFormatted);
    assert_eq!(
        second_start.file_system.read_to_vec("/settings/clock").unwrap(),
        b"24h"
    );
}

#[test]
fn never_formats_flash_that_cannot_be_read() {
    let mut flash = FakeFlash::blank();
    flash.reads_fail = true;

    let Err(failure) = prepare_file_system(FileSystemFlash::new(flash)) else {
        panic!("preparing unreadable flash succeeded");
    };

    assert_eq!(failure.error, Error::Io);
    assert_eq!(failure.storage.unwrap().release().erase_count, 0);
}

#[test]
fn mounts_a_folder_with_many_small_files() {
    let first_start = prepared(FakeFlash::blank());
    for file_number in 0..60 {
        let path = std::format!("/apps/file{file_number}");
        first_start.file_system.write_file(&path, b"small").unwrap();
    }
    let flash = first_start.file_system.unmount().unwrap().release();

    let second_start = prepared(flash);

    assert_eq!(
        second_start.file_system.list_dir(APPS_DIRECTORY).unwrap().len(),
        60
    );
}

#[test]
fn mounts_after_many_small_rewrites_of_one_setting() {
    let first_start = prepared(FakeFlash::blank());
    for rewrite in 0..300u32 {
        let paths = ["/settings/a", "/settings/b", "/settings/c"];
        first_start
            .file_system
            .write_file(paths[rewrite as usize % 3], &rewrite.to_le_bytes())
            .unwrap();
    }
    let flash = first_start.file_system.unmount().unwrap().release();

    let second_start = prepared(flash);

    assert_eq!(
        second_start.file_system.read_to_vec("/settings/c").unwrap(),
        299u32.to_le_bytes()
    );
}

#[test]
fn stays_inside_the_file_system_region() {
    let prepared = prepared(FakeFlash::blank());

    prepared
        .file_system
        .write_file("/resources/picture", &[0; 3 * BLOCK_SIZE as usize])
        .unwrap();
    let flash = prepared.file_system.unmount().unwrap().release();

    let before_the_region = &flash.memory[..FILE_SYSTEM.start as usize];
    assert!(before_the_region.iter().all(|byte| *byte == 0xFF));
}

// ============================================================================
// Memory use
// ============================================================================

#[test]
fn mounted_file_system_and_one_open_file_fit_in_two_kilobytes_of_heap() {
    let flash = FakeFlash::blank();
    let heap_before = heap_bytes_in_use();

    let prepared = prepared(flash);
    let heap_while_mounted = heap_bytes_in_use() - heap_before;
    let file = prepared
        .file_system
        .open("/settings/clock", OpenFlags::WRITE | OpenFlags::CREATE)
        .unwrap();
    let heap_for_one_open_file = heap_bytes_in_use() - heap_before - heap_while_mounted;
    file.close().unwrap();

    println!("heap while mounted: {heap_while_mounted} bytes, per open file: {heap_for_one_open_file} bytes");
    assert!(heap_while_mounted + heap_for_one_open_file <= 2048);
}

// ============================================================================
// Free space
// ============================================================================

#[test]
fn free_space_shrinks_by_the_blocks_a_file_takes() {
    let prepared = prepared(FakeFlash::blank());
    let free_when_empty = free_kilobytes(&prepared.file_system).unwrap();

    prepared
        .file_system
        .write_file("/apps/big", &[7; 3 * BLOCK_SIZE as usize])
        .unwrap();
    let free_with_the_file = free_kilobytes(&prepared.file_system).unwrap();

    assert!(free_when_empty < BLOCK_COUNT * 4);
    assert!(free_when_empty > (BLOCK_COUNT - 16) * 4);
    assert!(free_when_empty - free_with_the_file >= 3 * 4);
    assert_eq!(free_with_the_file % 4, 0);
}

#[test]
fn reads_any_part_of_a_file_and_nothing_past_its_end() {
    let prepared = prepared(FakeFlash::blank());
    let bytes: Vec<u8> = (0..5000u32).map(|place| (place % 251) as u8).collect();
    prepared.file_system.mkdir("/notes").unwrap();
    prepared
        .file_system
        .write_file("/notes/note.txt", &bytes)
        .unwrap();
    let mut out = [0u8; 512];

    assert_eq!(
        read_part(&prepared.file_system, "/notes/note.txt", 4096, &mut out),
        Some(512)
    );
    assert_eq!(&out[..], &bytes[4096..4608]);
    assert_eq!(
        read_part(&prepared.file_system, "/notes/note.txt", 4800, &mut out),
        Some(200)
    );
    assert_eq!(&out[..200], &bytes[4800..]);
    assert_eq!(
        read_part(&prepared.file_system, "/notes/note.txt", 5000, &mut out),
        Some(0)
    );
    assert_eq!(
        read_part(&prepared.file_system, "/notes/note.txt", 9000, &mut out),
        Some(0)
    );
    assert_eq!(
        read_part(&prepared.file_system, "/notes/missing.txt", 0, &mut out),
        None
    );
}

// ============================================================================
// Erasing ahead
// ============================================================================

/// A flash shared with the test, which refuses to program a byte that is not erased, as NOR flash cannot.
#[derive(Clone)]
struct StrictFlash(std::rc::Rc<core::cell::RefCell<FakeFlash>>);

impl StrictFlash {
    fn blank() -> StrictFlash {
        StrictFlash(std::rc::Rc::new(core::cell::RefCell::new(FakeFlash::blank())))
    }

    fn erase_count(&self) -> usize {
        self.0.borrow().erase_count
    }

    /// Erases a block as the firmware's background erase does, outside littlefs.
    fn erase_ahead(&self, file_system: &FileSystem<StrictFlash>, erased_ahead: &ErasedBlocks) -> usize {
        let mut erased = 0;
        while let Some(block) = next_block_to_erase_ahead(file_system, erased_ahead) {
            erased_ahead.begin_erasing(block);
            let start = block_address(block);
            self.0.borrow_mut().erase(start, start + BLOCK_SIZE).unwrap();
            erased_ahead.finish_erasing(block);
            erased += 1;
        }
        erased
    }
}

impl ErrorType for StrictFlash {
    type Error = NorFlashErrorKind;
}

impl ReadNorFlash for StrictFlash {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        self.0.borrow_mut().read(offset, bytes)
    }

    fn capacity(&self) -> usize {
        self.0.borrow().capacity()
    }
}

impl NorFlash for StrictFlash {
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = BLOCK_SIZE as usize;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        self.0.borrow_mut().erase(from, to)
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        let start = offset as usize;
        let mut flash = self.0.borrow_mut();
        assert!(
            flash.memory[start..start + bytes.len()]
                .iter()
                .all(|byte| *byte == 0xFF),
            "programmed flash at {offset:#x} that was not erased"
        );
        flash.write(offset, bytes)
    }
}

fn leaked_erased_blocks() -> &'static ErasedBlocks {
    std::boxed::Box::leak(std::boxed::Box::new(ErasedBlocks::new()))
}

fn prepared_erasing_ahead(
    flash: &StrictFlash,
    erased_ahead: &'static ErasedBlocks,
) -> FileSystem<StrictFlash> {
    match prepare_file_system(FileSystemFlash::with_blocks_erased_ahead(
        flash.clone(),
        erased_ahead,
    )) {
        Ok(prepared) => prepared.file_system,
        Err(failure) => panic!("preparing failed: {:?}", failure.error),
    }
}

fn contents(number: usize, length: usize) -> Vec<u8> {
    (0..length).map(|place| (place * 7 + number * 13) as u8).collect()
}

#[test]
fn files_written_between_erases_ahead_read_back_whole_and_never_program_unerased_flash() {
    let flash = StrictFlash::blank();
    let erased_ahead = leaked_erased_blocks();
    let file_system = prepared_erasing_ahead(&flash, erased_ahead);
    file_system.mkdir("/work").unwrap();
    let mut expected: Vec<Option<Vec<u8>>> = vec![None; 7];
    let mut erased_ahead_count = 0;

    for round in 0..80 {
        let number = (round * 5) % expected.len();
        let path = std::format!("/work/{number}");
        if round % 9 == 8 && expected[number].is_some() {
            file_system.remove(&path).unwrap();
            expected[number] = None;
        } else {
            let bytes = contents(round, 1 + (round * 977) % 9000);
            file_system.write_file(&path, &bytes).unwrap();
            expected[number] = Some(bytes);
        }
        erased_ahead_count += flash.erase_ahead(&file_system, erased_ahead);
    }

    assert!(erased_ahead_count > 0);
    let file_system = prepared_erasing_ahead(&flash, leaked_erased_blocks());
    for (number, bytes) in expected.iter().enumerate() {
        let path = std::format!("/work/{number}");
        match bytes {
            Some(bytes) => assert_eq!(&file_system.read_to_vec(&path).unwrap(), bytes, "{path}"),
            None => assert!(!file_system.exists(&path), "{path}"),
        }
    }
}

#[test]
fn an_erase_littlefs_asks_for_is_skipped_once_the_block_was_erased_ahead() {
    let flash = StrictFlash::blank();
    let erased_ahead = leaked_erased_blocks();
    let file_system = prepared_erasing_ahead(&flash, erased_ahead);
    file_system
        .write_file("/settings/first", &contents(1, 6000))
        .unwrap();
    flash.erase_ahead(&file_system, erased_ahead);
    let erases_before = flash.erase_count();

    file_system
        .write_file("/settings/second", &contents(2, 6000))
        .unwrap();

    let erases_by_littlefs = flash.erase_count() - erases_before;
    assert!(erased_ahead.erases_skipped() > 0);
    let plain_flash = StrictFlash::blank();
    let plain = prepared_erasing_ahead(&plain_flash, leaked_erased_blocks());
    plain.write_file("/settings/first", &contents(1, 6000)).unwrap();
    let plain_before = plain_flash.erase_count();
    plain.write_file("/settings/second", &contents(2, 6000)).unwrap();
    assert!(erases_by_littlefs < plain_flash.erase_count() - plain_before);
}

#[test]
fn a_block_littlefs_uses_while_it_is_erased_ahead_does_not_count_as_erased() {
    let erased_ahead = leaked_erased_blocks();
    let mut storage = FileSystemFlash::with_blocks_erased_ahead(StrictFlash::blank(), erased_ahead);

    erased_ahead.begin_erasing(40);
    Storage::erase(&mut storage, 40).unwrap();
    Storage::write(&mut storage, 40, 0, &[0; 16]).unwrap();
    erased_ahead.finish_erasing(40);

    assert!(!erased_ahead.is_erased(40));
    erased_ahead.begin_erasing(41);
    erased_ahead.finish_erasing(41);
    assert!(erased_ahead.is_erased(41));
}

#[test]
fn nothing_is_erased_ahead_until_littlefs_has_erased_since_it_mounted() {
    let flash = StrictFlash::blank();
    let file_system = prepared_erasing_ahead(&flash, leaked_erased_blocks());
    file_system.write_file("/settings/first", b"one").unwrap();
    let flash_after = file_system.unmount().unwrap().release();
    let erased_ahead = leaked_erased_blocks();
    let file_system = prepared_erasing_ahead(&flash_after, erased_ahead);

    assert_eq!(next_block_to_erase_ahead(&file_system, erased_ahead), None);
    file_system.write_file("/settings/second", b"two").unwrap();
    assert!(next_block_to_erase_ahead(&file_system, erased_ahead).is_some());
}
