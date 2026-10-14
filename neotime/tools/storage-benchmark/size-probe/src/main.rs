//! A watch build that uses one store the way NeoTime would, so its size less the plain build's is the store's cost.
//!
//! Build it once with no feature and once each with `ekv`, `sequential` and `littlefs`; it is never run.

#![no_std]
#![no_main]

extern crate alloc;

use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};

use cortex_m_rt::entry;
use embassy_futures::block_on;
use embedded_alloc::LlffHeap;

// ============================================================================
// Values the compiler cannot work out, so it keeps every call
// ============================================================================

static mut INPUT: u32 = 0;
static mut OUTPUT: u32 = 0;

fn input() -> u32 {
    unsafe { read_volatile(addr_of!(INPUT)) }
}

fn output(value: u32) {
    unsafe { write_volatile(addr_of_mut!(OUTPUT), value) }
}

// ============================================================================
// A flash that is never driven, standing in for neotime-external-flash in every build alike
// ============================================================================

const SECTOR_BYTES: usize = 4096;
const SECTOR_COUNT: usize = 1024;

static mut FLASH_WINDOW: [u8; 64] = [0xff; 64];

struct StandInFlash;

impl StandInFlash {
    fn read_at(&mut self, address: usize, bytes: &mut [u8]) {
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = unsafe { read_volatile(addr_of!(FLASH_WINDOW[(address + index) % 64])) };
        }
    }

    fn write_at(&mut self, address: usize, bytes: &[u8]) {
        for (index, byte) in bytes.iter().enumerate() {
            unsafe { write_volatile(addr_of_mut!(FLASH_WINDOW[(address + index) % 64]), *byte) };
        }
    }

    fn erase_at(&mut self, address: usize) {
        unsafe { write_volatile(addr_of_mut!(FLASH_WINDOW[address % 64]), 0xff) };
    }
}

#[derive(Debug)]
struct FlashError;

impl embedded_storage::nor_flash::NorFlashError for FlashError {
    fn kind(&self) -> embedded_storage::nor_flash::NorFlashErrorKind {
        embedded_storage::nor_flash::NorFlashErrorKind::Other
    }
}

impl embedded_storage::nor_flash::ErrorType for StandInFlash {
    type Error = FlashError;
}

impl embedded_storage_async::nor_flash::ReadNorFlash for StandInFlash {
    const READ_SIZE: usize = 1;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), FlashError> {
        self.read_at(offset as usize, bytes);
        Ok(())
    }

    fn capacity(&self) -> usize {
        SECTOR_COUNT * SECTOR_BYTES
    }
}

impl embedded_storage_async::nor_flash::NorFlash for StandInFlash {
    /// The flash store needs single bytes, as the watch's driver gives; sequential-storage was measured with words.
    const WRITE_SIZE: usize = if cfg!(feature = "flash-store") { 1 } else { 4 };
    const ERASE_SIZE: usize = SECTOR_BYTES;

    async fn erase(&mut self, from: u32, _to: u32) -> Result<(), FlashError> {
        self.erase_at(from as usize);
        Ok(())
    }

    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), FlashError> {
        self.write_at(offset as usize, bytes);
        Ok(())
    }
}

impl embedded_storage_async::nor_flash::MultiwriteNorFlash for StandInFlash {}

impl embedded_storage::nor_flash::ReadNorFlash for StandInFlash {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), FlashError> {
        self.read_at(offset as usize, bytes);
        Ok(())
    }

    fn capacity(&self) -> usize {
        SECTOR_COUNT * SECTOR_BYTES
    }
}

impl embedded_storage::nor_flash::NorFlash for StandInFlash {
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = SECTOR_BYTES;

    fn erase(&mut self, from: u32, _to: u32) -> Result<(), FlashError> {
        self.erase_at(from as usize);
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), FlashError> {
        self.write_at(offset as usize, bytes);
        Ok(())
    }
}

#[cfg(feature = "ekv")]
impl ekv::flash::Flash for StandInFlash {
    type Error = FlashError;

    fn page_count(&self) -> usize {
        768
    }

    async fn erase(&mut self, page: ekv::flash::PageID) -> Result<(), FlashError> {
        self.erase_at(page.index() * SECTOR_BYTES);
        Ok(())
    }

    async fn read(&mut self, page: ekv::flash::PageID, offset: usize, data: &mut [u8]) -> Result<(), FlashError> {
        self.read_at(page.index() * SECTOR_BYTES + offset, data);
        Ok(())
    }

    async fn write(&mut self, page: ekv::flash::PageID, offset: usize, data: &[u8]) -> Result<(), FlashError> {
        self.write_at(page.index() * SECTOR_BYTES + offset, data);
        Ok(())
    }
}

// ============================================================================
// The firmware already has a heap, so every build carries one
// ============================================================================

#[global_allocator]
static HEAP: LlffHeap = LlffHeap::empty();

fn start_heap() {
    static mut HEAP_MEMORY: [u8; 4096] = [0; 4096];
    unsafe { HEAP.init(addr_of_mut!(HEAP_MEMORY) as usize, 4096) }
    let bytes = alloc::vec![input() as u8; input() as usize % 64];
    output(bytes.len() as u32);
}

// ============================================================================
// Each store, used as NeoTime would use it
// ============================================================================

static mut VALUE: [u8; 256] = [0; 256];

fn value_buffer() -> &'static mut [u8; 256] {
    unsafe { &mut *addr_of_mut!(VALUE) }
}

/// Mount or format, read a key, write and delete in a transaction, and list a range of keys, as the launcher would.
#[cfg(feature = "ekv")]
async fn use_store() {
    use ekv::{Config, Database};
    use embassy_sync_for_ekv::blocking_mutex::raw::NoopRawMutex;

    let mut config = Config::default();
    config.random_seed = input();
    let database = Database::<_, NoopRawMutex>::new(StandInFlash, config);
    if database.mount().await.is_err() {
        let _ = database.format().await;
    }
    let key = input().to_be_bytes();
    let value = value_buffer();
    if let Ok(length) = database.read_transaction().await.read(&key, value).await {
        output(length as u32);
    }
    let mut transaction = database.write_transaction().await;
    let _ = transaction.write(&key, &value[..input() as usize % 256]).await;
    let _ = transaction.delete(&(input()).to_be_bytes()).await;
    let _ = transaction.commit().await;

    let start = input().to_be_bytes();
    let end = input().to_be_bytes();
    let reading = database.read_transaction().await;
    if let Ok(mut cursor) = reading.read_range(&start[..]..&end[..]).await {
        let mut found_key = [0u8; 4];
        while let Ok(Some((key_length, _))) = cursor.next(&mut found_key, value).await {
            output(key_length as u32);
        }
    }
}

/// Fetch, store and remove a key, with the page and key caches NeoTime would need.
#[cfg(feature = "sequential")]
async fn use_store() {
    use sequential_storage::{
        cache::{key_pointers::ArrayKeyPointers, page_states::ArrayPageStates, Cache, Uncached},
        map::{MapConfig, MapStorage},
    };

    let cache = Cache::new(ArrayPageStates::<768>::new(), Uncached, ArrayKeyPointers::<u32, 128>::new());
    let mut map = MapStorage::<u32, _, _>::new(StandInFlash, MapConfig::new(0x10_0000..0x40_0000), cache);
    let value = value_buffer();
    let key = input();
    if let Ok(Some(found)) = map.fetch_item::<&[u8]>(value, &key).await {
        output(found.len() as u32);
    }
    let mut stored = [0u8; 64];
    stored[0] = input() as u8;
    let _ = map.store_item(value, &key, &&stored[..input() as usize % 64]).await;
    let _ = map.remove_item(value, &input()).await;
}

/// Mount or format, read part of a file, write a file and remove one, as the firmware does today.
#[cfg(feature = "littlefs")]
async fn use_store() {
    use neotime_storage::{prepare_file_system, read_part, FileSystemFlash};

    let Ok(prepared) = prepare_file_system(FileSystemFlash::new(StandInFlash)) else {
        return;
    };
    let file_system = prepared.file_system;
    let value = value_buffer();
    if let Some(length) = read_part(&file_system, "/apps/a.ntapp", input(), value) {
        output(length as u32);
    }
    let _ = file_system.write_file("/apps/a.state", &value[..input() as usize % 256]);
    let _ = file_system.remove("/apps/b.state");
}

/// The flash store sized for the watch; a static of its exact size puts its RAM in the build's `.bss`.
#[cfg(feature = "flash-store")]
type WatchStore = neotime_flash_store::FlashStore<StandInFlash, 128, 128, 192>;

#[cfg(feature = "flash-store")]
#[used]
static mut STORE_RAM: [u8; core::mem::size_of::<WatchStore>()] = [0; core::mem::size_of::<WatchStore>()];

/// Mount, records, a whole file and an upload, reading, deleting and listing files, and a log.
#[cfg(feature = "flash-store")]
async fn use_store() {
    use neotime_flash_store::{record_key, Layout};

    let layout = Layout {
        start_address: 0x10_0000,
        file_sectors: 512,
        record_sectors: 64,
        log_sectors: 192,
    };
    let Ok(mut store) = WatchStore::mount(StandInFlash, layout).await else {
        return;
    };
    let value = value_buffer();
    let key = record_key(input(), input());
    let _ = store.write_record(key, &value[..input() as usize % 256]).await;
    if let Ok(Some(length)) = store.read_record(key, 0, value).await {
        output(length as u32);
    }
    let _ = store.delete_record(record_key(input(), input())).await;
    let _ = store.write_file("/apps/a.state", &value[..input() as usize % 256]).await;
    if store.begin_file("/apps/a.ntapp").await.is_ok() {
        let _ = store.write_file_part(&value[..input() as usize % 256]).await;
        let _ = store.finish_file().await;
    }
    if let Ok(Some(length)) = store.read_file("/apps/a.ntapp", input() as usize, value).await {
        output(length as u32);
    }
    let _ = store.delete_file("/apps/b.state").await;
    let _ = store.for_each_file("/apps/", |path| output(path.len() as u32)).await;
    let _ = store.append_log(key, &value[..input() as usize % 32]).await;
    let _ = store.flush_logs().await;
    if let Ok(Some(length)) = store.read_log(key, value).await {
        output(length as u32);
    }
}

/// The plain build: only the flash and the heap.
#[cfg(not(any(feature = "ekv", feature = "sequential", feature = "littlefs", feature = "flash-store")))]
async fn use_store() {
    let mut flash = StandInFlash;
    let value = value_buffer();
    embedded_storage_async::nor_flash::ReadNorFlash::read(&mut flash, input(), value).await.ok();
    embedded_storage::nor_flash::NorFlash::write(&mut flash, input(), &value[..4]).ok();
    output(value[0] as u32);
}

// ============================================================================
// Start
// ============================================================================

#[entry]
fn main() -> ! {
    start_heap();
    block_on(use_store());
    loop {
        cortex_m::asm::wfi();
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        cortex_m::asm::wfi();
    }
}
