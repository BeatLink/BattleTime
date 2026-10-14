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

use core::{
    cell::RefCell,
    future::poll_fn,
    ptr::NonNull,
    sync::atomic::{AtomicBool, Ordering},
    task::Poll,
};

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
    blocking_mutex::{raw::NoopRawMutex, Mutex},
    waitqueue::AtomicWaker,
};
use embassy_time::{Delay, Instant};
use embedded_hal::{
    delay::DelayNs,
    spi::{ErrorType, Operation, SpiDevice},
};
use neotime_display::{PanelInterface, Pieces};
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
// The end of a send
// ============================================================================

/// Whether a strip is still going out to the display.
static IS_SENDING: AtomicBool = AtomicBool::new(false);
/// The task waiting for the bus to come free, woken when a strip has gone out.
static BUS_FREED: AtomicWaker = AtomicWaker::new();

/// Ends a strip's send when its last piece has gone out.
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
        pac::PPI.tasks_chg(0).dis().write_value(1);
        let registers = pac::SPIM0;
        registers
            .dma()
            .tx()
            .list()
            .write(|w| w.set_list(pac::spim::vals::TxdListList::Disabled));
        registers.events_end().write_value(0);
        pac::P0.outset().write(|w| w.set_pin(DISPLAY_SELECT_PIN, true));
        IS_SENDING.store(false, Ordering::Release);
        BUS_FREED.wake();
    }
}

/// Waits, letting other tasks run, until no strip is going out to the display.
pub async fn bus_is_free() {
    poll_fn(|context| {
        BUS_FREED.register(context.waker());
        match IS_SENDING.load(Ordering::Acquire) {
            true => Poll::Pending,
            false => Poll::Ready(()),
        }
    })
    .await
}

/// Sleeps until no strip is going out, for a caller that cannot await.
fn wait_for_free_bus() {
    if !IS_SENDING.load(Ordering::Acquire) {
        return;
    }
    let started = Instant::now();
    while IS_SENDING.load(Ordering::Acquire) {
        cortex_m::asm::wfe();
    }
    // A task woken meanwhile signalled the executor with an event this WFE took, so it is given back.
    cortex_m::asm::sev();
    diagnostics::add(
        Counter::DisplaySendMicroseconds,
        started.elapsed().as_micros() as u32,
    );
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
        self.bus.lock(|bus| {
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
        })
    }

    fn send_pixels(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        wait_for_free_bus();
        self.bus.lock(|bus| {
            let started = Instant::now();
            let result = bus.borrow_mut().write_to_display(bytes);
            diagnostics::add(
                Counter::DisplaySendMicroseconds,
                started.elapsed().as_micros() as u32,
            );
            result
        })
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
            IS_SENDING.store(true, Ordering::Release);
            bus.chain.start(address, length);
        });
        Ok(())
    }
}

// ============================================================================
// The external flash
// ============================================================================

/// The flash's side of the bus: each transaction first waits for a strip going out to the display.
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
        wait_for_free_bus();
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
