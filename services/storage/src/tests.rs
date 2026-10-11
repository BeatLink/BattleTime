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
