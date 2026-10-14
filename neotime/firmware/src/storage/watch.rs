//! Storage on a watch running NeoTime's own bootloader: the flash store, the update slots and the boot state.

use embassy_embedded_hal::{adapter::BlockingAsync, flash::partition::BlockingPartition};
use embassy_futures::block_on;
use embassy_nrf::{
    gpio::{Level, Output, OutputDrive},
    nvmc::Nvmc,
    peripherals::{NVMC, P0_05},
    wdt::HaltConfig,
    Peri,
};
use embassy_sync::blocking_mutex::{raw::NoopRawMutex, Mutex};
use embassy_sync::mutex::Mutex as AsyncMutex;

#[cfg(with_shell)]
use crate::motion;
use embassy_futures::select::{select, Either};
use embedded_storage::nor_flash::ReadNorFlash;
use neotime_blefs::FileTransfer;
use neotime_boot::{
    bootloader_identity, image_header::read_image_header, slot_table::read_slots, Slots, UpdaterNote,
    BOOT_STATE, DEFAULT_SLOTS, FIRMWARE, SLOT_TABLE,
};
use neotime_external_flash::{BusyWait, ExternalFlash};
use neotime_flash_store::{Extent, FileAccess, FlashStore, Layout, StoreError};
use neotime_packages::{
    commands::{refuse_command, Change},
    InstalledApp,
};
#[cfg(with_shell)]
use neotime_steps::{log, DayChange, DAYS_REMEMBERED};
use neotime_updates::{
    confirm_running_firmware, forget_rollback, running_firmware_is_on_trial,
    running_firmware_was_rolled_back, Receivers, RecoveryReceiver, UpdateReceiver,
};
use static_cell::StaticCell;

use alloc::boxed::Box;
use core::{
    cell::RefCell,
    future::Future,
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
};

use crate::display_dma::{start_flash_read, FlashReadInProgress, MOST_READ_EXTENTS};

use crate::{
    foreground, line, packages,
    status::{self, Mode},
    storage_requests::{self, StorageRequest},
    Screen, SpiBus,
};

// ============================================================================
// What this build is
// ============================================================================

pub const WATCHDOG_HALT: HaltConfig = HaltConfig::Pause;
/// The bootloader takes over at the next start, swapping in an update if one was marked.
pub async fn reset(_screen: &mut Screen) -> ! {
    embassy_time::Timer::after(embassy_time::Duration::from_millis(200)).await;
    cortex_m::peripheral::SCB::sys_reset()
}

/// The flash store keeps its tables in static RAM, so storage needs little of the heap.
pub const HEAP_SIZE: usize = 10 * 1024;

// ============================================================================
// Starting the storage
// ============================================================================

/// Waits between busy polls of the flash chip by spinning the 64 MHz CPU.
const BUSY_WAIT: BusyWait = BusyWait {
    cpu_frequency_in_hertz: 64_000_000,
};

type FlashOnBus = ExternalFlash<crate::display_dma::FlashOnBus, BusyWait>;
type SharedExternalFlash = Mutex<NoopRawMutex, RefCell<FlashOnBus>>;
type SharedInternalFlash = Mutex<NoopRawMutex, RefCell<Nvmc<'static>>>;
type ExternalSlot = BlockingAsync<BlockingPartition<'static, NoopRawMutex, FlashOnBus>>;
type BootStatePage = BlockingAsync<BlockingPartition<'static, NoopRawMutex, Nvmc<'static>>>;

pub type Targets = Receivers<'static, ExternalSlot, BootStatePage, ExternalSlot>;

static EXTERNAL_FLASH: StaticCell<SharedExternalFlash> = StaticCell::new();
static INTERNAL_FLASH: StaticCell<SharedInternalFlash> = StaticCell::new();
static STATE_ALIGNMENT: StaticCell<[u8; 4]> = StaticCell::new();
static CONFIRM_ALIGNMENT: StaticCell<[u8; 4]> = StaticCell::new();
static FILES: StaticCell<Files> = StaticCell::new();

pub struct StoragePeripherals {
    pub flash_chip_select: Peri<'static, P0_05>,
    pub flash_controller: Peri<'static, NVMC>,
}

pub async fn start(
    spi_bus: &'static SpiBus,
    peripherals: StoragePeripherals,
    screen: &mut Screen,
) -> Option<(Targets, Confirmation, Mode, &'static Files)> {
    let flash_chip_select = Output::new(peripherals.flash_chip_select, Level::High, OutputDrive::Standard);
    let external_flash = match ExternalFlash::new(
        crate::display_dma::FlashOnBus::new(spi_bus, flash_chip_select),
        BUSY_WAIT,
    ) {
        Ok(mut flash) => {
            if let Ok(id) = flash.read_chip_id() {
                crate::about::note_flash_id(id.manufacturer, id.memory_type, id.capacity);
            }
            EXTERNAL_FLASH.init(Mutex::new(RefCell::new(flash)))
        }
        Err(_) => {
            screen.write_line(line::FILES, "No chip");
            return None;
        }
    };
    let internal_flash =
        INTERNAL_FLASH.init(Mutex::new(RefCell::new(Nvmc::new(peripherals.flash_controller))));
    let slots = read_slots(&mut BlockingPartition::new(
        internal_flash,
        SLOT_TABLE.start,
        SLOT_TABLE.size,
    ))
    .unwrap_or(DEFAULT_SLOTS);

    let files = make_files(external_flash, internal_flash);
    let mounted = mount_store(files);
    screen.write_line(
        line::FILES,
        match &mounted {
            Ok(()) => "OK",
            Err(StoreError::OtherFormatVersion) => "Other format",
            Err(_) => "Damaged",
        },
    );
    if mounted.is_err() {
        forget_store(files);
    }

    let targets = Receivers {
        firmware: UpdateReceiver::new(
            BlockingAsync::new(BlockingPartition::new(
                external_flash,
                slots.update.start,
                slots.update.size,
            )),
            BlockingAsync::new(BlockingPartition::new(
                internal_flash,
                BOOT_STATE.start,
                BOOT_STATE.size,
            )),
            STATE_ALIGNMENT.init([0; 4]),
        ),
        recovery: RecoveryReceiver::new(BlockingAsync::new(BlockingPartition::new(
            external_flash,
            slots.recovery.start,
            slots.recovery.size,
        ))),
    };
    let mut confirmation = Confirmation {
        boot_state: BlockingAsync::new(BlockingPartition::new(
            internal_flash,
            BOOT_STATE.start,
            BOOT_STATE.size,
        )),
        alignment: CONFIRM_ALIGNMENT.init([0; 4]),
    };
    let mut mode = confirmation.mode().await;
    if mode == Mode::OnTrial
        && !rolling_back_would_help(external_flash, slots)
        && confirmation.confirm().await
    {
        mode = Mode::Confirmed;
    }
    if mode == Mode::RolledBack {
        match status::take_updater_note() {
            Some(UpdaterNote::BootloaderWritten) if confirmation.forget_rollback().await => {
                mode = Mode::BootloaderUpdated;
            }
            Some(UpdaterNote::BootloaderAlreadyCurrent) if confirmation.forget_rollback().await => {
                mode = Mode::BootloaderAlreadyCurrent;
            }
            _ => {}
        }
    }
    if status::recovery_was_just_installed() || status::IS_RECOVERY_BUILD {
        mode = Mode::Recovery;
    }
    FIRMWARE_STATE.store(
        match mode {
            Mode::OnTrial => firmware_state::ON_TRIAL,
            Mode::RolledBack => firmware_state::ROLLED_BACK,
            _ => firmware_state::CONFIRMED,
        },
        Ordering::Relaxed,
    );
    let identity = bootloader_identity(&mut status::BootloaderPages).map_err(|_| ());
    if let Ok(Some(crc)) = identity {
        crate::about::note_bootloader_crc(crc);
    }
    screen.write_line(line::BOOTLOADER, &status::bootloader_words(identity, mode));
    Some((targets, confirmation, mode, files))
}

/// A rollback restores the update slot, which after a migration holds the old system's leftovers and after
/// a recovery holds the broken firmware; in both cases this firmware must stay.
fn rolling_back_would_help(external_flash: &'static SharedExternalFlash, slots: Slots) -> bool {
    let previous = slots.previous_firmware();
    let mut previous_firmware = BlockingPartition::new(external_flash, previous.start, previous.size);
    let previous_is_firmware = read_image_header(&mut previous_firmware, 0)
        .is_ok_and(|header| header.is_some_and(|header| header.runs_as_firmware()));
    if !previous_is_firmware {
        return false;
    }

    let mut recovery = BlockingPartition::new(external_flash, slots.recovery.start, slots.recovery.size);
    !running_firmware_is(&mut recovery)
}

/// Compares the running firmware with an image in the external flash, 256 bytes at a time.
fn running_firmware_is(image: &mut impl ReadNorFlash) -> bool {
    const CHUNK_SIZE: usize = 256;
    let running = unsafe { core::slice::from_raw_parts(FIRMWARE.start as *const u8, FIRMWARE.size as usize) };
    let mut chunk = [0; CHUNK_SIZE];
    running
        .chunks(CHUNK_SIZE)
        .enumerate()
        .all(|(index, running_chunk)| {
            image.read((index * CHUNK_SIZE) as u32, &mut chunk).is_ok() && chunk[..] == *running_chunk
        })
}

// ============================================================================
// Telling the bootloader this firmware works
// ============================================================================

pub struct Confirmation {
    boot_state: BootStatePage,
    alignment: &'static mut [u8; 4],
}

impl Confirmation {
    /// What the boot state says: on trial after a swap, rolled back after a revert, confirmed otherwise.
    async fn mode(&mut self) -> Mode {
        if running_firmware_is_on_trial(&mut self.boot_state, self.alignment).await == Ok(true) {
            Mode::OnTrial
        } else if running_firmware_was_rolled_back(&mut self.boot_state, self.alignment).await == Ok(true) {
            Mode::RolledBack
        } else {
            Mode::Confirmed
        }
    }

    /// True once the rollback's mark is gone, so the page no longer says the update failed.
    async fn forget_rollback(&mut self) -> bool {
        forget_rollback(&mut self.boot_state, self.alignment)
            .await
            .is_ok()
    }

    /// True once the bootloader will keep this firmware.
    pub async fn confirm(&mut self) -> bool {
        let kept = confirm_running_firmware(&mut self.boot_state, self.alignment)
            .await
            .is_ok();
        if kept {
            note_firmware_confirmed();
        }
        kept
    }
}

/// What apps read as `firmware_state`.
mod firmware_state {
    pub const CONFIRMED: u8 = 0;
    pub const ON_TRIAL: u8 = 1;
    pub const ROLLED_BACK: u8 = 2;
}

/// The running firmware's state, set at start and changed only when a firmware on trial is confirmed.
static FIRMWARE_STATE: AtomicU8 = AtomicU8::new(firmware_state::CONFIRMED);

/// Whether the bootloader keeps the running firmware, as `firmware_state` numbers it.
pub fn firmware_state() -> u8 {
    FIRMWARE_STATE.load(Ordering::Relaxed)
}

/// Marks a firmware on trial as kept, and tells the open app.
fn note_firmware_confirmed() {
    let was = FIRMWARE_STATE.compare_exchange(
        firmware_state::ON_TRIAL,
        firmware_state::CONFIRMED,
        Ordering::Relaxed,
        Ordering::Relaxed,
    );
    if was.is_ok() {
        crate::system_values::changed(&[neotime_runtime::SystemValue::FirmwareState]);
    }
}

#[cfg_attr(not(with_shell), allow(dead_code))]
impl Files {
    /// The boot state page, for the shell's own confirm or rollback.
    fn boot_state(&self) -> BootStatePage {
        BlockingAsync::new(BlockingPartition::new(
            self.internal_flash,
            BOOT_STATE.start,
            BOOT_STATE.size,
        ))
    }

    /// Tells the bootloader to keep a firmware on trial, as the wearer asked; true once it will.
    pub fn confirm_firmware_now(&self) -> bool {
        let mut alignment = [0; 4];
        let kept = block_on(confirm_running_firmware(&mut self.boot_state(), &mut alignment)).is_ok();
        if kept {
            note_firmware_confirmed();
        }
        kept
    }

    /// Restarts a firmware the boot state still says is on trial, so the bootloader brings back the one before.
    pub fn roll_back_firmware_now(&self) {
        let mut alignment = [0; 4];
        if block_on(running_firmware_is_on_trial(
            &mut self.boot_state(),
            &mut alignment,
        )) == Ok(true)
        {
            cortex_m::peripheral::SCB::sys_reset();
        }
    }
}

// ============================================================================
// Where the update and recovery slots sit
// ============================================================================

/// Only the full firmware moves slots: the recovery build has no room to spare in the reloaders.
#[cfg(with_shell)]
pub use slots::{answer_slots, slots_answer_line};

#[cfg(with_shell)]
mod slots {
    use embassy_embedded_hal::flash::partition::BlockingPartition;
    use neotime_boot::{
        image_header::read_image_header, move_recovery, slot_table::read_slots, RecoveryMove, Region,
        SlotProblem, Slots, DEFAULT_SLOTS, FIRMWARE, SLOT_AREA, SLOT_TABLE,
    };
    use neotime_companion::{slots_refusal, slots_report, SlotRange, SlotsReport, SlotsRequest};
    use neotime_updates::{change_slots, UpdateError};

    use super::{Confirmation, Files};

    /// Answers the companion's `slots`: the slots as they now are, and whether they moved; or why not.
    ///
    /// A moved recovery slot gets the recovery image first, so the table never names an empty slot when
    /// there was an image to keep. The receivers were built from the old slots, so after a move the
    /// caller restarts.
    pub async fn answer_slots(
        files: &'static Files,
        confirmation: &mut Confirmation,
        request: SlotsRequest,
    ) -> Result<(Slots, bool), &'static str> {
        let mut slot_table = BlockingPartition::new(files.internal_flash, SLOT_TABLE.start, SLOT_TABLE.size);
        let current = read_slots(&mut slot_table).unwrap_or(DEFAULT_SLOTS);
        if !request.moves_anything() {
            return Ok((current, false));
        }

        let region = |range: SlotRange| Region {
            start: range.start,
            size: range.size,
        };
        let wanted = Slots {
            update: request.update.map(region).unwrap_or(current.update),
            recovery: request.recovery.map(region).unwrap_or(current.recovery),
        };
        wanted.check().map_err(slot_problem_text)?;
        if wanted.recovery != current.recovery {
            let mut external_flash = BlockingPartition::new(files.external_flash, 0, SLOT_AREA.size);
            match move_recovery(&mut external_flash, current.recovery, wanted.recovery) {
                Ok(moved) if moved.may_switch() => {}
                Ok(RecoveryMove::ImageTooLarge) => {
                    return Err("the recovery image is larger than the new slot")
                }
                Ok(RecoveryMove::Overlapping) => {
                    return Err("the new recovery slot overlaps the old one at another start")
                }
                _ => return Err("the recovery image could not be copied"),
            }
        }

        match change_slots(
            &mut slot_table,
            &mut confirmation.boot_state,
            confirmation.alignment,
            wanted,
        )
        .await
        {
            Ok(()) => Ok((wanted, true)),
            Err(UpdateError::RunningFirmwareNotConfirmed) => {
                Err("an update is waiting or on trial; finish it first")
            }
            Err(UpdateError::UnsoundSlots(problem)) => Err(slot_problem_text(problem)),
            Err(_) => Err("the slot table could not be written"),
        }
    }

    /// The reply line for `answer_slots`' answer, with the recovery image's length read from its header.
    pub fn slots_answer_line(
        files: &'static Files,
        answer: Result<(Slots, bool), &'static str>,
    ) -> alloc::boxed::Box<str> {
        let line = match answer {
            Ok((slots, restarting)) => {
                let range = |region: Region| SlotRange {
                    start: region.start,
                    size: region.size,
                };
                let mut external_flash = BlockingPartition::new(files.external_flash, 0, SLOT_AREA.size);
                let recovery_image_size = read_image_header(&mut external_flash, slots.recovery.start)
                    .ok()
                    .flatten()
                    .map_or(0, |header| header.total_size_in_bytes());
                slots_report(&SlotsReport {
                    update: range(slots.update),
                    recovery: range(slots.recovery),
                    area_end: SLOT_AREA.end(),
                    firmware_size: FIRMWARE.size,
                    recovery_image_size,
                    restarting,
                })
            }
            Err(reason) => slots_refusal(reason),
        };
        alloc::boxed::Box::from(line.as_str())
    }

    fn slot_problem_text(problem: SlotProblem) -> &'static str {
        match problem {
            SlotProblem::NotWholePages => "slots must start and end on 4096-byte pages",
            SlotProblem::OutsideSlotArea => "slots must end before the file system",
            SlotProblem::UpdateSlotNotFirmwareSizePlusOnePage => {
                "the update slot must be the firmware's size plus one page"
            }
            SlotProblem::RecoverySlotTooSmall => "the recovery slot must be at least 64 KB",
            SlotProblem::RecoverySlotTooLarge => "the recovery slot must be no larger than the firmware's",
            SlotProblem::Overlapping => "the slots overlap",
        }
    }
}

// ============================================================================
// The flash store, reached a page at a time without spinning
// ============================================================================

/// The 3 MB file system region: 2 MB of files, 256 KB of records and 768 KB of logs.
const STORE_LAYOUT: Layout = Layout {
    start_address: neotime_boot::FILE_SYSTEM.start,
    file_sectors: 512,
    record_sectors: 64,
    log_sectors: 192,
};

/// 128 files, 128 record keys and rings of up to 192 sectors: 2.4 KB of static RAM.
type WatchStore = FlashStore<StoreFlash, 128, 128, 192>;

/// How often a running erase or page program is checked while other tasks run.
const ERASE_POLL_INTERVAL: embassy_time::Duration = embassy_time::Duration::from_millis(2);
const PROGRAM_POLL_INTERVAL: embassy_time::Duration = embassy_time::Duration::from_micros(100);

/// The external flash as the store sees it: reads move by EasyDMA while the CPU sleeps, and erases and page programs
/// are started and then waited on with the timer, so the CPU and other tasks run while the chip works.
///
/// It shares the chip with the update slots, which use it blocking; the driver makes either wait for the other.
struct StoreFlash {
    flash: &'static SharedExternalFlash,
    /// Polls the chip without the timer, for the shell's calls, which run to their end without yielding.
    polls_without_yielding: bool,
}

impl embedded_storage_async::nor_flash::ErrorType for StoreFlash {
    type Error = <FlashOnBus as embedded_storage::nor_flash::ErrorType>::Error;
}

impl embedded_storage_async::nor_flash::ReadNorFlash for StoreFlash {
    const READ_SIZE: usize = 1;

    /// Moves the bytes by EasyDMA in the moments the display leaves the bus free, so a strip going out is not slowed.
    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        self.wake_chip()?;
        self.flash
            .lock(|flash| flash.borrow_mut().prepare_read(offset, bytes.len()))?;
        crate::display_dma::read_flash(offset, bytes);
        Ok(())
    }

    fn capacity(&self) -> usize {
        neotime_external_flash::CAPACITY_IN_BYTES
    }
}

impl embedded_storage_async::nor_flash::NorFlash for StoreFlash {
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = neotime_flash_store::SECTOR_BYTES as usize;

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        self.wake_chip()?;
        for sector in (from..to).step_by(Self::ERASE_SIZE) {
            self.wait_for_free_bus().await;
            self.flash
                .lock(|flash| flash.borrow_mut().start_erasing_sector(sector))?;
            self.wait_until_done(ERASE_POLL_INTERVAL).await?;
            crate::diagnostics::add_one(crate::diagnostics::Counter::StoreSectorsErased);
        }
        Ok(())
    }

    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        self.wake_chip()?;
        let page = neotime_external_flash::PROGRAM_PAGE_SIZE;
        let mut done = 0;
        while done < bytes.len() {
            let address = offset as usize + done;
            let length = (page - address % page).min(bytes.len() - done);
            let piece = &bytes[done..done + length];
            self.wait_for_free_bus().await;
            self.flash
                .lock(|flash| flash.borrow_mut().start_programming_page(address as u32, piece))?;
            self.wait_until_done(PROGRAM_POLL_INTERVAL).await?;
            done += length;
        }
        Ok(())
    }
}

impl StoreFlash {
    /// Brings the chip out of deep power-down before the store reaches it, by the driver or any other way.
    fn wake_chip(&self) -> Result<(), <Self as embedded_storage_async::nor_flash::ErrorType>::Error> {
        self.flash.lock(|flash| flash.borrow_mut().wake_if_asleep())
    }

    /// Waits for a strip going out to the display, letting other tasks run, unless this call may not yield.
    async fn wait_for_free_bus(&self) {
        if !self.polls_without_yielding {
            crate::display_dma::bus_is_free().await;
        }
    }

    async fn wait_until_done(
        &mut self,
        interval: embassy_time::Duration,
    ) -> Result<(), <Self as embedded_storage_async::nor_flash::ErrorType>::Error> {
        loop {
            self.wait_for_free_bus().await;
            if self
                .flash
                .lock(|flash| flash.borrow_mut().operation_has_finished())?
            {
                break;
            }
            if !self.polls_without_yielding {
                embassy_time::Timer::after(interval).await;
            }
        }
        Ok(())
    }
}

// ============================================================================
// Parts of files read ahead
// ============================================================================

/// Part of a file on its way into RAM by DMA, or already there.
pub enum FilePart {
    Moving(FlashReadInProgress),
    Arrived(Box<[u8]>),
}

impl FilePart {
    /// How many bytes it holds, or will once they have all arrived.
    pub fn len(&self) -> usize {
        match self {
            FilePart::Moving(read) => read.len(),
            FilePart::Arrived(bytes) => bytes.len(),
        }
    }

    /// The bytes, once every one has arrived; sleeps until then if some are still moving.
    pub fn bytes(&mut self) -> &[u8] {
        if let FilePart::Moving(_) = self {
            if let FilePart::Moving(read) = core::mem::replace(self, FilePart::Arrived(Box::default())) {
                *self = FilePart::Arrived(read.finish());
            }
        }
        match self {
            FilePart::Arrived(bytes) => bytes,
            FilePart::Moving(_) => &[],
        }
    }
}

// ============================================================================
// Files for apps, settings and the step log
// ============================================================================

/// While updates are off the phone may not install, change or remove apps, nor run one from the temporary folder.
fn folders_locked_by_settings() -> &'static [&'static str] {
    const APP_FOLDERS: &[&str] = &[
        neotime_packages::APPS_DIRECTORY,
        neotime_packages::TEMPORARY_DIRECTORY,
    ];
    match crate::settings::current().is_on(neotime_settings::Setting::AllowsUpdates) {
        true => &[],
        false => APP_FOLDERS,
    }
}

/// Runs a store call to its end with its state on the heap, since a call's state can be 1.8 KB and the shell calls
/// from deep in its stack.
fn run_on_heap<Call: Future>(call: Call) -> Call::Output {
    block_on(alloc::boxed::Box::pin(call))
}

/// The mounted store, or nothing when it would not mount; the shell, the Bluetooth task and the
/// accelerometer task share it, each holding it only for one call.
pub struct Files {
    store: AsyncMutex<NoopRawMutex, Option<WatchStore>>,
    /// For copying the shared library into its region of the internal flash.
    #[cfg_attr(not(with_shell), allow(dead_code))]
    internal_flash: &'static SharedInternalFlash,
    /// For moving the update and recovery slots, outside the store, and for putting the chip to sleep.
    external_flash: &'static SharedExternalFlash,
    /// Set while the screen is off, when the chip goes into deep power-down whenever the store is idle.
    flash_may_sleep: AtomicBool,
}

#[cfg_attr(not(with_shell), allow(dead_code))]
impl Files {
    /// Waits until no other task is in the middle of a store call.
    ///
    /// Tasks only switch where one waits, so after this the shell's calls below find the store free until it
    /// next waits.
    pub async fn wait_until_free(&self) {
        drop(self.store.lock().await);
    }

    /// Runs a store call to its end at once, for the shell, which does not wait; nothing if another task holds it.
    fn now<Answer>(&self, call: impl FnOnce(&mut WatchStore) -> Answer) -> Option<Answer> {
        let Ok(mut store) = self.store.try_lock() else {
            crate::diagnostics::add_one(crate::diagnostics::Counter::StoreCallsFoundBusy);
            return None;
        };
        let store = store.as_mut()?;
        store.flash_mut().polls_without_yielding = true;
        let answer = call(store);
        store.flash_mut().polls_without_yielding = false;
        self.sleep_flash_if_allowed();
        Some(answer)
    }

    /// Lets the chip sleep while the screen is off, and puts it down now if the store is idle.
    pub fn let_flash_sleep(&self, may_sleep: bool) {
        self.flash_may_sleep.store(may_sleep, Ordering::Relaxed);
        self.sleep_flash_if_idle();
    }

    /// Puts the chip into deep power-down if the screen is off and no task is in a store call.
    fn sleep_flash_if_idle(&self) {
        if let Ok(_store) = self.store.try_lock() {
            self.sleep_flash_if_allowed();
        }
    }

    /// Puts the chip into deep power-down if the screen is off; only called with the store held, so no store call is under way.
    fn sleep_flash_if_allowed(&self) {
        if self.flash_may_sleep.load(Ordering::Relaxed) {
            let _ = self.external_flash.lock(|flash| flash.borrow_mut().sleep());
        }
    }

    /// Starts reading up to `length` bytes of a file from `offset` into RAM and returns at once, so
    /// the screen keeps drawing while they arrive; nothing when the file is missing or the store busy.
    ///
    /// A part of a file kept as a record, never more than 1 KB, is read at once instead.
    pub fn start_reading_part(&self, path: &str, offset: u32, length: usize) -> Option<FilePart> {
        self.now(|store| {
            let mut extents = [Extent::default(); MOST_READ_EXTENTS];
            let found = run_on_heap(store.file_extents(path, offset as usize, length, &mut extents)).ok()?;
            let Some(found) = found else {
                let mut bytes = alloc::vec![0; length];
                let read =
                    run_on_heap(FileAccess::read_file(store, path, offset as usize, &mut bytes)).ok()??;
                bytes.truncate(read);
                return Some(FilePart::Arrived(bytes.into_boxed_slice()));
            };
            let first = extents[0];
            self.external_flash
                .lock(|flash| flash.borrow_mut().prepare_read(first.address, found.bytes))
                .ok()?;
            let buffer = alloc::vec![0; found.bytes].into_boxed_slice();
            Some(FilePart::Moving(start_flash_read(
                &extents[..found.count],
                buffer,
            )))
        })?
    }

    pub fn read_now(&self, path: &str) -> Option<alloc::vec::Vec<u8>> {
        self.now(|store| {
            let length = run_on_heap(FileAccess::file_length(store, path)).ok()??;
            let mut bytes = alloc::vec![0; length];
            run_on_heap(FileAccess::read_file(store, path, 0, &mut bytes)).ok()??;
            Some(bytes)
        })?
    }

    /// Reads part of a file, for the app host while it draws.
    pub fn read_part_now(&self, path: &str, offset: u32, out: &mut [u8]) -> Option<usize> {
        self.now(|store| run_on_heap(FileAccess::read_file(store, path, offset as usize, out)).ok()?)?
    }

    /// Writes the whole file; false when the store is missing, full or held by another task.
    pub fn write_now(&self, path: &str, bytes: &[u8]) -> bool {
        self.now(|store| run_on_heap(FileAccess::write_file(store, path, bytes)).is_ok()) == Some(true)
    }

    /// Lists the folder at `path` into `folder` from every file of the store; false when the store is busy or missing.
    pub fn list_folder_now(&self, path: &str, folder: &mut neotime_app_host::folder::FolderListing) -> bool {
        if !folder.begin(path) {
            return true;
        }
        let listed = self.now(|store| {
            run_on_heap(FileAccess::for_each_file_with_length(
                store,
                "/",
                &mut |file_path, length| folder.add_file(file_path, length),
            ))
            .is_ok()
        });
        listed == Some(true)
    }

    pub fn free_kilobytes_now(&self) -> Option<u32> {
        self.now(|store| (FileAccess::free_bytes(store) / 1024) as u32)
    }

    pub fn installed_apps_now(&self) -> alloc::vec::Vec<InstalledApp> {
        self.now(|store| run_on_heap(neotime_packages::Packages::new(store).list()).unwrap_or_default())
            .unwrap_or_default()
    }

    pub fn set_app_enabled_now(&self, id: &str, enabled: bool) -> bool {
        self.now(|store| run_on_heap(neotime_packages::Packages::new(store).set_enabled(id, enabled)).is_ok())
            == Some(true)
    }

    /// The image of the app `id` in `directory`, `/apps` or `/temp`.
    /// An installed app's icon alone, read without the rest of its image.
    pub fn read_app_icon_now(&self, id: &str) -> Option<alloc::vec::Vec<u8>> {
        self.now(|store| run_on_heap(neotime_packages::Packages::new(store).read_icon(id)))?
    }

    /// An installed app's name, read without its whole image.
    pub fn read_app_name_now(&self, id: &str) -> Option<alloc::string::String> {
        self.now(|store| run_on_heap(neotime_packages::Packages::new(store).read_name(id)))?
    }

    /// Whether an installed app has subscriptions that run while it is closed, read without its whole image.
    pub fn runs_while_closed_now(&self, id: &str) -> bool {
        self.now(|store| run_on_heap(neotime_packages::Packages::new(store).runs_while_closed(id)))
            == Some(true)
    }

    pub fn read_app_image_now(&self, directory: &str, id: &str) -> Option<alloc::vec::Vec<u8>> {
        self.now(|store| {
            run_on_heap(neotime_packages::Packages::new(store).read_image_in(directory, id)).ok()
        })?
    }

    /// Carries out one request from another task; see `storage_requests`.
    async fn carry_out(&self, transfer: &mut FileTransfer, request: StorageRequest) {
        let mut store = self.store.lock().await;
        let Some(store) = store.as_mut() else {
            if let StorageRequest::FileTransfer { request, .. } = request {
                storage_requests::FILE_ANSWERS
                    .send(FileTransfer::refuse(&request))
                    .await;
            }
            storage_requests::STEPS_LOADED.signal(());
            return;
        };
        match request {
            StorageRequest::FileTransfer { request, room } => {
                foreground::wait_for_a_pause().await;
                transfer.lock_folders(folders_locked_by_settings());
                let answers = transfer.answer(Some(store), &request, room).await;
                storage_requests::FILE_ANSWERS.send(answers).await;
            }
            StorageRequest::StartFileTransfer => {
                transfer.abort();
                FileAccess::abandon_file(store);
            }
            #[cfg(with_shell)]
            StorageRequest::LoadSteps => {
                load_days(store).await;
                if let Some(steps) = log::load_trip(store).await {
                    motion::set_trip_steps(steps);
                }
                storage_requests::STEPS_LOADED.signal(());
            }
            #[cfg(with_shell)]
            StorageRequest::StoreSteps(recorded) => {
                if let Some(finished) = recorded.finished {
                    if log::write_slot(store, finished).await.is_ok() {
                        let _ = log::save_trip(store, motion::trip_steps()).await;
                    }
                }
                if recorded.day_change != DayChange::SameDay {
                    let _ = log::forget_old_days(store, motion::recording_day()).await;
                }
                if recorded.day_change == DayChange::Jump {
                    load_days(store).await;
                }
            }
            #[cfg(with_shell)]
            StorageRequest::SaveTrip(steps) => {
                let _ = log::save_trip(store, steps).await;
            }
            #[cfg(with_shell)]
            StorageRequest::StoreSlotLogs {
                ended,
                beats_per_minute,
                battery,
            } => {
                let _ = neotime_slot_log::store_ended_slot(store, ended, beats_per_minute, battery).await;
            }
            StorageRequest::WriteSmallFile { path, bytes } => {
                let _ = FileAccess::write_file(store, path, &bytes).await;
            }
            // Only the step counter and the battery task ask for these, and the recovery firmware runs neither.
            #[cfg(not(with_shell))]
            StorageRequest::LoadSteps
            | StorageRequest::StoreSteps(_)
            | StorageRequest::SaveTrip(_)
            | StorageRequest::StoreSlotLogs { .. } => {}
        }
    }

    /// Runs one line from the packages characteristic, returning the reply line and what changed.
    /// Runs a packages command, its reply going to the Bluetooth task as it is written.
    async fn run_packages_command(&self, line: &[u8]) -> Change {
        let mut store = self.store.lock().await;
        let Some(store) = store.as_mut() else {
            let (reply, change) = refuse_command(line, "the flash store is not mounted");
            packages::REPLIES.send(reply).await;
            return change;
        };
        self.run_packages_command_on(store, line).await
    }

    #[cfg(with_shell)]
    async fn run_packages_command_on(&self, store: &mut WatchStore, line: &[u8]) -> Change {
        let library_version =
            crate::apps::shared_library().and_then(neotime_packages::library::library_version);
        neotime_packages::commands::handle_command(line, store, library_version, &mut ToBluetooth).await
    }

    /// The recovery firmware has no apps, so it leaves the package manager out.
    #[cfg(not(with_shell))]
    async fn run_packages_command_on(&self, _store: &mut WatchStore, line: &[u8]) -> Change {
        let (reply, change) = refuse_command(line, "the recovery firmware does not install apps");
        packages::REPLIES.send(reply).await;
        change
    }
}

/// Reads each remembered day back from the log into the step history, changing nothing else in it, so steps
/// recorded meanwhile are kept.
#[cfg(with_shell)]
async fn load_days(store: &mut WatchStore) {
    let today = motion::recording_day();
    for days_back in 0..DAYS_REMEMBERED {
        let hours = log::read_hours(store, today - days_back as i64).await;
        motion::set_day(days_back, hours);
    }
}

/// Builds the files' state straight into its static, outside any async function, so start-up's task never holds
/// a 2.4 KB copy of it.
#[inline(never)]
fn make_files(
    external_flash: &'static SharedExternalFlash,
    internal_flash: &'static SharedInternalFlash,
) -> &'static Files {
    let flash = StoreFlash {
        flash: external_flash,
        polls_without_yielding: false,
    };
    FILES.init_with(|| Files {
        store: AsyncMutex::new(Some(WatchStore::unmounted(flash, STORE_LAYOUT))),
        internal_flash,
        external_flash,
        flash_may_sleep: AtomicBool::new(false),
    })
}

/// Leaves the files without a store after a failed mount; nothing else holds it yet at start-up.
#[inline(never)]
fn forget_store(files: &Files) {
    if let Ok(mut store) = files.store.try_lock() {
        *store = None;
    }
}

/// Mounts the store by polling the chip rather than waiting, outside any async function, so the mount's future
/// lives on the stack only while it runs rather than in start-up's task for good.
#[inline(never)]
fn mount_store(
    files: &Files,
) -> Result<(), StoreError<<StoreFlash as embedded_storage_async::nor_flash::ErrorType>::Error>> {
    files
        .now(|store| run_on_heap(store.mount_in_place()))
        .unwrap_or(Ok(()))
}

// ============================================================================
// The storage task
// ============================================================================

/// Hands each piece of a packages reply to the Bluetooth task, which notifies it to the phone.
#[cfg(with_shell)]
struct ToBluetooth;

#[cfg(with_shell)]
impl neotime_packages::commands::ReplySink for ToBluetooth {
    async fn send(&mut self, piece: alloc::string::String) {
        packages::REPLIES.send(piece).await;
    }
}

/// Runs every store call other tasks ask for, and the packages characteristic's commands, one at a time.
///
/// A packages command starts near the top of the stack here, where the verifier inside it has room. Each call's
/// state is on the heap while it runs, so the task's static space holds no store call.
#[embassy_executor::task]
pub async fn run_storage(files: &'static Files) {
    crate::tasks::name_this_task("storage");
    let mut transfer = FileTransfer::new();
    loop {
        match select(storage_requests::REQUESTS.receive(), packages::COMMANDS.receive()).await {
            Either::First(request) => alloc::boxed::Box::pin(files.carry_out(&mut transfer, request)).await,
            Either::Second(line) => {
                foreground::wait_for_a_pause().await;
                let change = alloc::boxed::Box::pin(files.run_packages_command(&line)).await;
                match change {
                    Change::None => {}
                    Change::Apps => packages::CHANGED.signal(change),
                    Change::RunTemporary(_) | Change::StopTemporary => packages::TEMPORARY.signal(change),
                }
            }
        }
        files.sleep_flash_if_idle();
    }
}
