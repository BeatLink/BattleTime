//! The SPI bus the display and the external flash share, and the display's side of it.
//!
//! The display draws through two strip buffers in RAM: while one goes out by EasyDMA the next is
//! drawn into the other. EasyDMA moves at most 255 bytes at a time, so a strip is sent as equal
//! pieces in array-list mode, each piece's END starting the next through PPI. A counter timer counts
//! the ENDs; one before the last it disables the channel that restarts the transfer, and at the last
//! it marks the strip sent. Nothing waits for that mark when a strip starts: the bus is taken for
//! one transaction at a time, and whoever takes it next, a display command or a flash transaction,
//! first finishes the strip still going out. So a drawing step returns as soon as its last strip has
//! started, and the shell and every other task run while it goes out. A wait sleeps in WFE: the last
//! END pends the counter timer's interrupt, which is left off in the NVIC but wakes WFE through
//! SEVONPEND, so no handler runs.

use core::{cell::RefCell, ptr::NonNull};

use embassy_nrf::{
    gpio::Output,
    peripherals::{PPI_CH5, PPI_CH6, PPI_CH7, PPI_GROUP0, TIMER1},
    ppi::{Event, Ppi, PpiGroup, Task},
    spim::{self, Spim},
    timer::Timer,
    Peri,
};
use embassy_sync::blocking_mutex::{raw::NoopRawMutex, Mutex};
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

/// The SCR bit that makes an interrupt becoming pending wake WFE, even one left off in the NVIC.
const SEND_EVENT_ON_PENDING: u32 = 1 << 4;

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
        // SAFETY: setting SEVONPEND only makes a pending interrupt wake WFE; nothing else reads it.
        unsafe {
            (*cortex_m::peripheral::SCB::PTR)
                .scr
                .modify(|value| value | SEND_EVENT_ON_PENDING)
        };
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
        // The interrupt follows the event's level, so it is unpended only once the event is clear.
        cortex_m::peripheral::NVIC::unpend(pac::Interrupt::TIMER1);
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

    /// Sleeps until the last piece has gone out, then puts the bus back as embassy's driver expects it.
    fn wait_until_sent(&mut self) {
        let all_sent = self.counter.cc(ALL_SENT_COMPARE).event_compare();
        // A wake for any other reason just checks again; the event already set makes WFE return at once.
        while !all_sent.is_triggered() {
            cortex_m::asm::wfe();
        }
        // A task woken meanwhile signalled the executor with an event this WFE took, so it is given back.
        cortex_m::asm::sev();
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        self.restart_group.disable_all();
        let registers = pac::SPIM0;
        registers
            .dma()
            .tx()
            .list()
            .write(|w| w.set_list(pac::spim::vals::TxdListList::Disabled));
        registers.events_end().write_value(0);
    }
}

// ============================================================================
// The shared bus
// ============================================================================

/// The SPI peripheral and what the display leaves on it while a strip goes out.
pub struct SharedBus {
    spim: Spim<'static>,
    display_select: Output<'static>,
    chain: Chain,
    is_sending: bool,
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
            is_sending: false,
        }
    }

    /// Waits for a strip still going out to end, and lets go of the display.
    fn finish_display_send(&mut self) {
        if !self.is_sending {
            return;
        }
        let started = Instant::now();
        self.chain.wait_until_sent();
        self.display_select.set_high();
        self.is_sending = false;
        diagnostics::add(
            Counter::DisplaySendMicroseconds,
            started.elapsed().as_micros() as u32,
        );
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

/// The pixels a strip buffer holds: two rows of the screen, so the pair costs 1,920 B of RAM.
pub const STRIP_BUFFER_PIXELS: usize = 480;

/// The display's panel interface: commands by blocking writes, strips by chained EasyDMA from two buffers.
pub struct DmaInterface {
    bus: &'static SpiBus,
    data_or_command: Output<'static>,
    buffers: &'static mut [[u16; STRIP_BUFFER_PIXELS]; 2],
    /// The buffer to draw the next strip into; the other may still be going out.
    filling: usize,
}

impl DmaInterface {
    pub fn new(
        bus: &'static SpiBus,
        data_or_command: Output<'static>,
        buffers: &'static mut [[u16; STRIP_BUFFER_PIXELS]; 2],
    ) -> DmaInterface {
        DmaInterface {
            bus,
            data_or_command,
            buffers,
            filling: 0,
        }
    }
}

impl PanelInterface for DmaInterface {
    type Error = spim::Error;

    fn send_command(&mut self, command: u8, arguments: &[u8]) -> Result<(), Self::Error> {
        self.bus.lock(|bus| {
            let mut bus = bus.borrow_mut();
            bus.finish_display_send();
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
        self.bus.lock(|bus| {
            let mut bus = bus.borrow_mut();
            bus.finish_display_send();
            let started = Instant::now();
            let result = bus.write_to_display(bytes);
            diagnostics::add(
                Counter::DisplaySendMicroseconds,
                started.elapsed().as_micros() as u32,
            );
            result
        })
    }

    fn strip_buffer(&mut self) -> Option<&mut [u16]> {
        Some(&mut self.buffers[self.filling])
    }

    fn send_strip_pixels(&mut self, first: usize, count: usize) -> Result<(), Self::Error> {
        let pixels = &self.buffers[self.filling][first..first + count];
        let (address, length) = (pixels.as_ptr() as u32, core::mem::size_of_val(pixels));
        if length == 0 {
            return Ok(());
        }
        self.bus.lock(|bus| {
            let mut bus = bus.borrow_mut();
            bus.finish_display_send();
            bus.display_select.set_low();
            bus.chain.start(address, length);
            bus.is_sending = true;
        });
        Ok(())
    }

    /// The buffer just sent is left to go out, and the other, already sent, is drawn into next.
    fn finish_strip(&mut self) {
        self.filling = 1 - self.filling;
    }
}

// ============================================================================
// The external flash
// ============================================================================

/// The flash's side of the bus: each transaction first finishes a strip going out to the display.
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
        self.bus.lock(|bus| {
            let mut bus = bus.borrow_mut();
            bus.finish_display_send();
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
