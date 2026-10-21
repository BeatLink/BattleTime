//! The SPI bus the display and the external flash share, and the display's side of it.
//!
//! The display's strips go out by EasyDMA while the next is drawn. EasyDMA moves at most 255 bytes
//! at a time, so a strip is sent as equal pieces in array-list mode, each piece's END starting the
//! next through PPI. A counter timer counts the ENDs; one before the last it disables the channel
//! that restarts the transfer, and at the last its interrupt ends the send: it puts the bus back as
//! embassy's driver expects it, deselects the display and wakes whoever waits for the bus. Nothing
//! waits when a strip starts. Async callers, such as drawing and the flash store, await
//! `bus_is_free` and let every other task run meanwhile; a blocking caller sleeps in WFE until the
//! interrupt. The bus is taken for one transaction at a time.
//!
//! Flash reads also move by EasyDMA, a piece of at most 255 bytes at a time, each piece's end
//! raising the same interrupt, which starts the next. They fill the moments the display leaves the
//! bus free, such as while the next strip is drawn, and stop after their current piece whenever
//! the display asks for the bus. A read may move a piece before each of the display's commands as
//! well as before a strip, so a strip can wait for several pieces. `read_flash` sleeps the whole
//! executor in WFE until its bytes have moved.

use alloc::boxed::Box;
use core::{
    cell::{Cell, RefCell},
    future::poll_fn,
    ptr::NonNull,
    sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering},
    task::Poll,
};

use battletime_display::{PanelInterface, Pieces};
use battletime_flash_store::Extent;
use embassy_nrf::{
    gpio::Output,
    interrupt::{
        self,
        typelevel::{Handler, Interrupt},
    },
    peripherals::{PPI_CH5, PPI_CH6, PPI_CH7, PPI_GROUP0, TIMER1},
    ppi::{Event, Ppi, PpiGroup, Task},
    spim::{self, Spim},
    timer::Timer,
    Peri,
};
use embassy_sync::{
    blocking_mutex::{raw::NoopRawMutex, CriticalSectionMutex, Mutex},
    waitqueue::MultiWakerRegistration,
};
use embassy_time::{Delay, Instant};
use embedded_hal::{
    delay::DelayNs,
    spi::{ErrorType, Operation, SpiDevice},
};
use nrf_pac as pac;

use crate::diagnostics::{self, Counter};

// ============================================================================
// Limits
// ============================================================================

/// The most bytes one EasyDMA transfer moves on the nRF52832.
const LARGEST_PIECE_BYTES: usize = 255;

/// The counter timer's compare channels: one disables the restart, the other marks the strip sent.
const LAST_RESTART_COMPARE: usize = 0;
const ALL_SENT_COMPARE: usize = 1;

/// The display's chip select, P0.25, which the interrupt raises when a strip is sent.
const DISPLAY_SELECT_PIN: usize = 25;

// ============================================================================
// The peripherals it takes
// ============================================================================

pub struct ChainPeripherals {
    pub counter: Peri<'static, TIMER1>,
    pub restart_channel: Peri<'static, PPI_CH5>,
    pub count_channel: Peri<'static, PPI_CH6>,
    pub stop_channel: Peri<'static, PPI_CH7>,
    pub restart_group: Peri<'static, PPI_GROUP0>,
}

/// The PPI links that send a strip as a chain of transfers, kept alive for the whole run.
struct Chain {
    counter: Timer<'static>,
    restart_group: PpiGroup<'static, PPI_GROUP0>,
    _restart: Ppi<'static, PPI_CH5, 1, 1>,
    _count: Ppi<'static, PPI_CH6, 1, 1>,
    _stop: Ppi<'static, PPI_CH7, 1, 1>,
}

impl Chain {
    fn new(peripherals: ChainPeripherals) -> Chain {
        let registers = pac::SPIM0;
        // SAFETY: each pointer is an EVENTS_ or TASKS_ register of SPIM0, which exists for the whole run.
        let end = || unsafe { Event::new_unchecked(NonNull::new_unchecked(registers.events_end().as_ptr())) };
        let start = unsafe { Task::new_unchecked(NonNull::new_unchecked(registers.tasks_start().as_ptr())) };

        let counter = Timer::new_counter(peripherals.counter);
        let mut restart_group = PpiGroup::new(peripherals.restart_group);
        let restart = Ppi::new_one_to_one(peripherals.restart_channel, end(), start);
        let mut count = Ppi::new_one_to_one(peripherals.count_channel, end(), counter.task_count());
        let mut stop = Ppi::new_one_to_one(
            peripherals.stop_channel,
            counter.cc(LAST_RESTART_COMPARE).event_compare(),
            restart_group.task_disable_all(),
        );
        restart_group.add_channel(&restart);
        restart_group.disable_all();
        count.enable();
        stop.enable();
        counter.start();
        pac::TIMER1
            .intenset()
            .write(|w| w.set_compare(ALL_SENT_COMPARE, true));
        interrupt::typelevel::TIMER1::unpend();
        // SAFETY: the handler touches only the chain's registers, the display's chip select and atomics.
        unsafe { interrupt::typelevel::TIMER1::enable() };
        Chain {
            counter,
            restart_group,
            _restart: restart,
            _count: count,
            _stop: stop,
        }
    }

    /// Starts sending `length` bytes from `address` as equal pieces, one after another without the CPU.
    fn start(&mut self, address: u32, length: usize) {
        let Pieces { piece_bytes, count } = Pieces::of(length, LARGEST_PIECE_BYTES);
        let pieces = count as u32;
        let registers = pac::SPIM0;

        self.counter.clear();
        self.counter.cc(ALL_SENT_COMPARE).clear_events();
        self.counter.cc(LAST_RESTART_COMPARE).clear_events();
        self.counter.cc(ALL_SENT_COMPARE).write(pieces);
        self.counter.cc(LAST_RESTART_COMPARE).write(pieces - 1);
        registers.events_end().write_value(0);
        registers.dma().rx().maxcnt().write(|w| w.set_maxcnt(0));
        registers.dma().tx().ptr().write_value(address);
        registers
            .dma()
            .tx()
            .maxcnt()
            .write(|w| w.set_maxcnt(piece_bytes as u8));
        registers
            .dma()
            .tx()
            .list()
            .write(|w| w.set_list(pac::spim::vals::TxdListList::ArrayList));
        if pieces > 1 {
            self.restart_group.enable_all();
        }
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        registers.tasks_start().write_value(1);
    }
}

// ============================================================================
// Who holds the bus
// ============================================================================

/// No interrupt-driven transfer is using the bus.
const BUS_FREE: u8 = 0;
/// A strip is going out to the display.
const BUS_SENDING_STRIP: u8 = 1;
/// A piece of a flash read is coming in.
const BUS_READING_FLASH: u8 = 2;

/// Which interrupt-driven transfer holds the bus, if any.
static BUS_USER: AtomicU8 = AtomicU8::new(BUS_FREE);
/// Set while the display waits for the bus, so a flash read gives it up after its current piece.
static DISPLAY_IS_WAITING: AtomicBool = AtomicBool::new(false);
/// The most tasks that wait for the bus at once: the shell's drawing, the storage task's flash work, and room to spare.
const MOST_BUS_WAITERS: usize = 4;

/// The tasks waiting for the bus to come free, all woken when a strip or a flash piece has moved.
///
/// The shell's drawing and the storage task's flash work wait at once; with room for one waker the
/// second took the first's place, and the shell, never woken, stopped until the watchdog reset the watch.
static BUS_FREED: CriticalSectionMutex<RefCell<MultiWakerRegistration<MOST_BUS_WAITERS>>> =
    CriticalSectionMutex::new(RefCell::new(MultiWakerRegistration::new()));

fn wake_bus_waiters() {
    BUS_FREED.lock(|waiters| waiters.borrow_mut().wake());
}

/// Ends a strip's send or a flash read's piece when its last byte has moved.
pub struct StripSentHandler {
    _private: (),
}

impl Handler<interrupt::typelevel::TIMER1> for StripSentHandler {
    unsafe fn on_interrupt() {
        let timer = pac::TIMER1;
        if timer.events_compare(ALL_SENT_COMPARE).read() == 0 {
            return;
        }
        timer.events_compare(ALL_SENT_COMPARE).write_value(0);
        match BUS_USER.load(Ordering::Acquire) {
            BUS_SENDING_STRIP => finish_strip(),
            BUS_READING_FLASH => finish_flash_piece(),
            _ => {}
        }
        // A caller that saw the bus busy and then slept in WFE must wake, even if this ran between its check and its sleep.
        cortex_m::asm::sev();
    }
}

/// Puts the bus back as embassy's driver expects it, deselects the display and lets a flash read go on.
fn finish_strip() {
    pac::PPI.tasks_chg(0).dis().write_value(1);
    let registers = pac::SPIM0;
    registers
        .dma()
        .tx()
        .list()
        .write(|w| w.set_list(pac::spim::vals::TxdListList::Disabled));
    registers.events_end().write_value(0);
    pac::P0.outset().write(|w| w.set_pin(DISPLAY_SELECT_PIN, true));
    BUS_USER.store(BUS_FREE, Ordering::Release);
    wake_bus_waiters();
    go_on_with_flash_read();
}

/// Waits, letting other tasks run, until no strip or flash piece is moving.
pub async fn bus_is_free() {
    poll_fn(|context| {
        BUS_FREED.lock(|waiters| waiters.borrow_mut().register(context.waker()));
        match BUS_USER.load(Ordering::Acquire) {
            BUS_FREE => Poll::Ready(()),
            _ => Poll::Pending,
        }
    })
    .await
}

/// Sleeps until the display may have the bus, for the display's calls, which cannot await.
///
/// A flash read moving meanwhile stops after its current piece and goes on once the display is done.
fn wait_for_free_bus() {
    DISPLAY_IS_WAITING.store(true, Ordering::Release);
    sleep_until_bus_is_free();
}

/// Sleeps until no strip or flash piece is moving, for a caller that cannot await.
fn sleep_until_bus_is_free() {
    if BUS_USER.load(Ordering::Acquire) == BUS_FREE {
        return;
    }
    let started = Instant::now();
    while BUS_USER.load(Ordering::Acquire) != BUS_FREE {
        cortex_m::asm::wfe();
    }
    // A task woken meanwhile signalled the executor with an event this WFE took, so it is given back.
    cortex_m::asm::sev();
    diagnostics::add(
        Counter::DisplaySendMicroseconds,
        started.elapsed().as_micros() as u32,
    );
}

/// Ends the display's wait once its blocking writes are done, and lets a paused flash read go on.
fn display_is_done() {
    DISPLAY_IS_WAITING.store(false, Ordering::Release);
    go_on_with_flash_read();
}

// ============================================================================
// Flash reads that move in the bus's free moments
// ============================================================================

/// The flash's chip select, P0.05, which a flash read drives itself while its pieces move.
const FLASH_SELECT_PIN: usize = 5;

/// The most extents one read follows, enough for a prefetch that crosses from one sector into the next.
pub const MOST_READ_EXTENTS: usize = 4;

/// A flash read the interrupt moves a piece at a time while the display does not need the bus.
#[derive(Clone, Copy)]
struct FlashRead {
    /// The runs of flash it reads, one after another, into RAM that follows on.
    extents: [Extent; MOST_READ_EXTENTS],
    extent_count: usize,
    /// The extent being read, and where in the flash and in RAM its next piece goes.
    extent_index: usize,
    address: u32,
    destination: u32,
    /// The bytes left in the extent being read.
    bytes_left: usize,
    /// The bytes the piece now moving takes, or 0 while the read command goes out.
    piece_bytes: usize,
    /// Whether the chip is selected with the read command sent, so pieces can follow.
    is_streaming: bool,
}

impl FlashRead {
    /// Moves on to the next extent once this one is read, giving whether there was one.
    fn take_next_extent(&mut self) -> bool {
        if self.extent_index + 1 >= self.extent_count {
            return false;
        }
        self.extent_index += 1;
        let extent = self.extents[self.extent_index];
        self.address = extent.address;
        self.bytes_left = extent.length as usize;
        true
    }
}

/// The one flash read under way, if any.
static FLASH_READ: CriticalSectionMutex<Cell<Option<FlashRead>>> = CriticalSectionMutex::new(Cell::new(None));
/// The read command and its address, which EasyDMA sends from RAM.
static READ_COMMAND: [AtomicU8; 4] = [const { AtomicU8::new(0) }; 4];

/// Reads started and reads ended, so a read in progress can tell its own end from a later read's.
static FLASH_READS_STARTED: AtomicU32 = AtomicU32::new(0);
static FLASH_READS_ENDED: AtomicU32 = AtomicU32::new(0);

/// A flash read moving by EasyDMA in the background, owning its buffer until the read has ended.
///
/// Dropping it waits for the read to end, so the buffer is never freed while bytes still arrive.
pub struct FlashReadInProgress {
    buffer: Box<[u8]>,
    number: u32,
}

impl FlashReadInProgress {
    /// How many bytes the read brings.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Sleeps until every byte has arrived, and gives the buffer back.
    pub fn finish(mut self) -> Box<[u8]> {
        wait_for_read(self.number);
        core::mem::take(&mut self.buffer)
    }
}

impl Drop for FlashReadInProgress {
    fn drop(&mut self) {
        wait_for_read(self.number);
    }
}

/// Starts reading `extents` of the external flash, one after another, into `buffer` and returns at
/// once, so the caller can draw meanwhile. Extents past `MOST_READ_EXTENTS` or the buffer's end are left out.
///
/// The display always comes first: the read moves only while no strip is going out and stops
/// after its current piece, of at most 255 bytes, whenever the display asks for the bus. Only one
/// read moves at a time, so this first waits for one already under way.
/// The caller first makes sure no erase or program is running, as `ExternalFlash::prepare_read` does.
pub fn start_flash_read(extents: &[Extent], mut buffer: Box<[u8]>) -> FlashReadInProgress {
    let number = begin_flash_read(extents, &mut buffer);
    FlashReadInProgress { buffer, number }
}

/// Reads `bytes` from the external flash at `address` by EasyDMA, sleeping in WFE until they have moved.
#[inline(never)]
pub fn read_flash(address: u32, bytes: &mut [u8]) {
    let extent = Extent {
        address,
        length: bytes.len() as u32,
    };
    let number = begin_flash_read(&[extent], bytes);
    wait_for_read(number);
}

/// Queues the read and starts it if the bus is free, giving back its number.
fn begin_flash_read(extents: &[Extent], bytes: &mut [u8]) -> u32 {
    wait_for_flash_reads_to_end();
    let number = FLASH_READS_STARTED
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(1);
    let mut read = FlashRead {
        extents: [Extent::default(); MOST_READ_EXTENTS],
        extent_count: 0,
        extent_index: 0,
        address: 0,
        destination: bytes.as_mut_ptr() as u32,
        bytes_left: 0,
        piece_bytes: 0,
        is_streaming: false,
    };
    let mut room = bytes.len();
    for extent in extents.iter().take(MOST_READ_EXTENTS) {
        let length = (extent.length as usize).min(room);
        if length > 0 {
            read.extents[read.extent_count] = Extent {
                address: extent.address,
                length: length as u32,
            };
            read.extent_count += 1;
            room -= length;
        }
    }
    if read.extent_count == 0 {
        FLASH_READS_ENDED.store(number, Ordering::Release);
        return number;
    }
    read.address = read.extents[0].address;
    read.bytes_left = read.extents[0].length as usize;
    FLASH_READ.lock(|cell| cell.set(Some(read)));
    go_on_with_flash_read();
    number
}

/// Sleeps until the read numbered `number` has ended, starting it again whenever the bus is free.
fn wait_for_read(number: u32) {
    loop {
        go_on_with_flash_read();
        if read_has_ended(number) {
            break;
        }
        cortex_m::asm::wfe();
    }
    // A task woken meanwhile signalled the executor with an event this WFE took, so it is given back.
    cortex_m::asm::sev();
}

/// Whether the read numbered `number` has ended, counting round past the largest `u32`.
fn read_has_ended(number: u32) -> bool {
    FLASH_READS_ENDED.load(Ordering::Acquire).wrapping_sub(number) as i32 >= 0
}

/// Sleeps until no flash read is under way, starting a paused one whenever the bus is free.
fn wait_for_flash_reads_to_end() {
    if FLASH_READ.lock(|read| read.get().is_none()) {
        return;
    }
    loop {
        go_on_with_flash_read();
        if FLASH_READ.lock(|read| read.get().is_none()) {
            break;
        }
        cortex_m::asm::wfe();
    }
    // A task woken meanwhile signalled the executor with an event this WFE took, so it is given back.
    cortex_m::asm::sev();
}

/// Starts the flash read's next transfer if the bus is free and the display is not waiting for it.
fn go_on_with_flash_read() {
    FLASH_READ.lock(|cell| {
        let Some(mut read) = cell.get() else {
            return;
        };
        if DISPLAY_IS_WAITING.load(Ordering::Acquire) || BUS_USER.load(Ordering::Acquire) != BUS_FREE {
            return;
        }
        BUS_USER.store(BUS_READING_FLASH, Ordering::Release);
        start_flash_transfer(&mut read);
        cell.set(Some(read));
    })
}

/// Counts the piece that just moved, then ends the read, pauses it for the display, or moves the next piece.
fn finish_flash_piece() {
    FLASH_READ.lock(|cell| {
        let Some(mut read) = cell.get() else {
            return;
        };
        if read.piece_bytes == 0 {
            read.is_streaming = true;
        } else {
            read.address += read.piece_bytes as u32;
            read.destination += read.piece_bytes as u32;
            read.bytes_left -= read.piece_bytes;
            diagnostics::add(Counter::FlashReadBytes, read.piece_bytes as u32);
            read.piece_bytes = 0;
        }
        // A new extent starts at another address, so the chip is deselected and sent a new command.
        let is_between_extents = read.bytes_left == 0 && read.take_next_extent();
        let has_ended = read.bytes_left == 0;
        if has_ended || is_between_extents || DISPLAY_IS_WAITING.load(Ordering::Acquire) {
            pac::P0.outset().write(|w| w.set_pin(FLASH_SELECT_PIN, true));
            pac::SPIM0.events_end().write_value(0);
            read.is_streaming = false;
            cell.set(if has_ended { None } else { Some(read) });
            if has_ended {
                FLASH_READS_ENDED.fetch_add(1, Ordering::Release);
            }
            if !has_ended && !is_between_extents {
                diagnostics::add_one(Counter::FlashReadPauses);
            }
            BUS_USER.store(BUS_FREE, Ordering::Release);
            wake_bus_waiters();
            if is_between_extents {
                go_on_with_flash_read();
            }
            return;
        }
        start_flash_transfer(&mut read);
        cell.set(Some(read));
    })
}

/// Sends the read command if the chip is not streaming yet, or else starts the next piece into RAM.
///
/// Each transfer is started here, with the CPU awake, rather than chained by PPI, so the nRF52832's
/// EasyDMA anomaly 109 cannot corrupt a read that starts while the CPU sleeps.
fn start_flash_transfer(read: &mut FlashRead) {
    let registers = pac::SPIM0;
    let (send_from, send_count, receive_into, receive_count) = if read.is_streaming {
        read.piece_bytes = battletime_external_flash::next_read_piece(read.bytes_left);
        (0, 0, read.destination, read.piece_bytes)
    } else {
        for (byte, value) in READ_COMMAND
            .iter()
            .zip(battletime_external_flash::read_command(read.address))
        {
            byte.store(value, Ordering::Relaxed);
        }
        pac::P0.outclr().write(|w| w.set_pin(FLASH_SELECT_PIN, true));
        (READ_COMMAND.as_ptr() as u32, READ_COMMAND.len(), 0, 0)
    };

    let timer = pac::TIMER1;
    timer.tasks_clear().write_value(1);
    timer.events_compare(ALL_SENT_COMPARE).write_value(0);
    timer.cc(ALL_SENT_COMPARE).write_value(1);
    registers.events_end().write_value(0);
    registers.dma().tx().ptr().write_value(send_from);
    registers
        .dma()
        .tx()
        .maxcnt()
        .write(|w| w.set_maxcnt(send_count as u8));
    registers.dma().rx().ptr().write_value(receive_into);
    registers
        .dma()
        .rx()
        .maxcnt()
        .write(|w| w.set_maxcnt(receive_count as u8));
    core::sync::atomic::compiler_fence(Ordering::SeqCst);
    registers.tasks_start().write_value(1);
}

// ============================================================================
// The shared bus
// ============================================================================

/// The SPI peripheral, the display's chip select and the chain that sends its strips.
pub struct SharedBus {
    spim: Spim<'static>,
    display_select: Output<'static>,
    chain: Chain,
}

/// The bus, taken for one transaction at a time.
pub type SpiBus = Mutex<NoopRawMutex, RefCell<SharedBus>>;

impl SharedBus {
    /// The bus with the display's chip select and the peripherals that chain its transfers.
    pub fn new(
        spim: Spim<'static>,
        display_select: Output<'static>,
        peripherals: ChainPeripherals,
    ) -> SharedBus {
        SharedBus {
            spim,
            display_select,
            chain: Chain::new(peripherals),
        }
    }

    /// Sends `bytes` to the display by blocking writes, the chip selected only while they go.
    fn write_to_display(&mut self, bytes: &[u8]) -> Result<(), spim::Error> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.display_select.set_low();
        let result = self.spim.blocking_write(bytes);
        self.display_select.set_high();
        result
    }
}

// ============================================================================
// The display
// ============================================================================

/// The display's panel interface: commands by blocking writes, strips by chained EasyDMA.
pub struct DmaInterface {
    bus: &'static SpiBus,
    data_or_command: Output<'static>,
}

impl DmaInterface {
    pub fn new(bus: &'static SpiBus, data_or_command: Output<'static>) -> DmaInterface {
        DmaInterface { bus, data_or_command }
    }
}

impl PanelInterface for DmaInterface {
    type Error = spim::Error;

    fn send_command(&mut self, command: u8, arguments: &[u8]) -> Result<(), Self::Error> {
        wait_for_free_bus();
        let result = self.bus.lock(|bus| {
            let mut bus = bus.borrow_mut();
            let started = Instant::now();
            self.data_or_command.set_low();
            let result = bus.write_to_display(&[command]);
            self.data_or_command.set_high();
            let result = result.and_then(|()| bus.write_to_display(arguments));
            diagnostics::add(
                Counter::DisplaySendMicroseconds,
                started.elapsed().as_micros() as u32,
            );
            result
        });
        display_is_done();
        result
    }

    fn send_pixels(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        wait_for_free_bus();
        let result = self.bus.lock(|bus| {
            let started = Instant::now();
            let result = bus.borrow_mut().write_to_display(bytes);
            diagnostics::add(
                Counter::DisplaySendMicroseconds,
                started.elapsed().as_micros() as u32,
            );
            result
        });
        display_is_done();
        result
    }

    /// Starts the send and returns; the interrupt ends it, and the pixels must stay untouched till then.
    fn send_strip_pixels(&mut self, pixels: &[u16]) -> Result<(), Self::Error> {
        let (address, length) = (pixels.as_ptr() as u32, core::mem::size_of_val(pixels));
        if length == 0 {
            return Ok(());
        }
        wait_for_free_bus();
        self.bus.lock(|bus| {
            let mut bus = bus.borrow_mut();
            bus.display_select.set_low();
            BUS_USER.store(BUS_SENDING_STRIP, Ordering::Release);
            DISPLAY_IS_WAITING.store(false, Ordering::Release);
            bus.chain.start(address, length);
        });
        Ok(())
    }
}

// ============================================================================
// The external flash
// ============================================================================

/// The flash's side of the bus: each transaction first waits for a flash read under way and a strip going out.
pub struct FlashOnBus {
    bus: &'static SpiBus,
    chip_select: Output<'static>,
}

impl FlashOnBus {
    pub fn new(bus: &'static SpiBus, chip_select: Output<'static>) -> FlashOnBus {
        FlashOnBus { bus, chip_select }
    }
}

impl ErrorType for FlashOnBus {
    type Error = spim::Error;
}

impl SpiDevice for FlashOnBus {
    fn transaction(&mut self, operations: &mut [Operation<'_, u8>]) -> Result<(), Self::Error> {
        wait_for_flash_reads_to_end();
        sleep_until_bus_is_free();
        self.bus.lock(|bus| {
            let mut bus = bus.borrow_mut();
            self.chip_select.set_low();
            let result = operations.iter_mut().try_for_each(|operation| match operation {
                Operation::Read(words) => bus.spim.blocking_read(words),
                Operation::Write(words) => bus.spim.blocking_write(words),
                Operation::Transfer(read, write) => bus.spim.blocking_transfer(read, write),
                Operation::TransferInPlace(words) => bus.spim.blocking_transfer_in_place(words),
                Operation::DelayNs(nanoseconds) => {
                    Delay.delay_ns(*nanoseconds);
                    Ok(())
                }
            });
            self.chip_select.set_high();
            result
        })
    }
}
