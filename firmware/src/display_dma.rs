//! The display's side of the SPI bus: pixels go out by EasyDMA while the CPU draws the next strip.
//!
//! EasyDMA moves at most 255 bytes at a time, so a strip is sent as equal pieces in array-list
//! mode, each piece's END starting the next through PPI. A counter timer counts the ENDs; one
//! before the last it disables the channel that restarts the transfer, and at the last it marks the
//! strip sent. The bus is never handed to anyone else while a strip is still going out: every
//! command, every new strip and the end of each drawing step first waits for it. While it waits the
//! CPU sleeps: the last END pends the counter timer's interrupt, which is left off in the NVIC but
//! wakes a WFE through SEVONPEND, so no handler runs and other interrupts still wake it as usual.

use core::ptr::NonNull;

use embassy_nrf::{
    gpio::Output,
    peripherals::{PPI_CH5, PPI_CH6, PPI_CH7, PPI_GROUP0, TIMER1},
    ppi::{Event, Ppi, PpiGroup, Task},
    spim,
    timer::Timer,
    Peri,
};
use embassy_time::Instant;
use mipidsi::interface::{Interface, InterfaceKind};
use neotime_display::Pieces;
use nrf_pac as pac;

use crate::{
    diagnostics::{self, Counter},
    SpiBus,
};

// ============================================================================
// Limits
// ============================================================================

/// The most bytes one EasyDMA transfer moves on the nRF52832.
const LARGEST_PIECE_BYTES: usize = 255;

/// The buffer used when no drawing step has lent one, such as for the status page and the help screen.
const FALLBACK_BUFFER_BYTES: usize = 64;

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
        pac::TIMER1.intenset().write(|w| w.set_compare(ALL_SENT_COMPARE, true));
        // SAFETY: setting SEVONPEND only makes a pending interrupt wake WFE; nothing else reads it.
        unsafe { (*cortex_m::peripheral::SCB::PTR).scr.modify(|value| value | SEND_EVENT_ON_PENDING) };
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
// The interface
// ============================================================================

/// A mipidsi interface that sends commands by blocking writes and pixels by chained EasyDMA.
pub struct DmaInterface {
    bus: &'static SpiBus,
    chip_select: Output<'static>,
    data_or_command: Output<'static>,
    chain: Chain,
    /// A buffer a drawing step lent for the length of the step, which pixels are sent from.
    lent: Option<NonNull<[u8]>>,
    fallback: [u8; FALLBACK_BUFFER_BYTES],
    is_sending: bool,
}

impl DmaInterface {
    pub fn new(
        bus: &'static SpiBus,
        chip_select: Output<'static>,
        data_or_command: Output<'static>,
        peripherals: ChainPeripherals,
    ) -> DmaInterface {
        DmaInterface {
            bus,
            chip_select,
            data_or_command,
            chain: Chain::new(peripherals),
            lent: None,
            fallback: [0; FALLBACK_BUFFER_BYTES],
            is_sending: false,
        }
    }

    /// Sends pixels from `buffer` until `take_back_buffer`, so a strip can go out while the next is drawn.
    ///
    /// # Safety
    ///
    /// `buffer` must stay alive and untouched until `take_back_buffer` returns.
    pub unsafe fn lend_buffer(&mut self, buffer: &mut [u8]) {
        self.finish_sending();
        self.lent = Some(NonNull::from(buffer));
    }

    /// Waits for the last strip to go out, and stops using the lent buffer.
    pub fn take_back_buffer(&mut self) {
        self.finish_sending();
        self.lent = None;
    }

    fn finish_sending(&mut self) {
        if !self.is_sending {
            return;
        }
        let started = Instant::now();
        self.chain.wait_until_sent();
        self.chip_select.set_high();
        self.is_sending = false;
        diagnostics::add(
            Counter::DisplaySendMicroseconds,
            started.elapsed().as_micros() as u32,
        );
    }

    /// The buffer to fill next, free now that nothing is being sent from it.
    fn free_buffer(&mut self) -> &mut [u8] {
        self.finish_sending();
        match self.lent {
            // SAFETY: `lend_buffer`'s caller keeps the buffer alive, and nothing is sent from it now.
            Some(mut lent) => unsafe { lent.as_mut() },
            None => &mut self.fallback,
        }
    }

    /// Starts sending the first `length` bytes of the buffer `free_buffer` gave, as pixel data.
    fn start_sending(&mut self, length: usize) {
        if length == 0 {
            return;
        }
        let address = match self.lent {
            Some(lent) => lent.as_ptr() as *const u8 as u32,
            None => self.fallback.as_ptr() as u32,
        };
        self.data_or_command.set_high();
        self.chip_select.set_low();
        // Holding the bus while starting proves no blocking transfer of the flash is half done.
        self.bus.lock(|bus| {
            let _bus = bus.borrow_mut();
            self.chain.start(address, length);
        });
        self.is_sending = true;
    }

    fn write_blocking(&mut self, bytes: &[u8]) -> Result<(), spim::Error> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.chip_select.set_low();
        let result = self.bus.lock(|bus| bus.borrow_mut().blocking_write(bytes));
        self.chip_select.set_high();
        result
    }
}

impl Interface for DmaInterface {
    type Word = u8;
    type Error = spim::Error;
    const KIND: InterfaceKind = InterfaceKind::Serial4Line;

    fn send_command(&mut self, command: u8, arguments: &[u8]) -> Result<(), Self::Error> {
        self.finish_sending();
        let started = Instant::now();
        self.data_or_command.set_low();
        let result = self.write_blocking(&[command]);
        self.data_or_command.set_high();
        let result = result.and_then(|()| self.write_blocking(arguments));
        diagnostics::add(
            Counter::DisplaySendMicroseconds,
            started.elapsed().as_micros() as u32,
        );
        result
    }

    fn send_pixels<const BYTES_PER_PIXEL: usize>(
        &mut self,
        pixels: impl IntoIterator<Item = [u8; BYTES_PER_PIXEL]>,
    ) -> Result<(), Self::Error> {
        let mut pixels = pixels.into_iter().peekable();
        while pixels.peek().is_some() {
            let buffer = self.free_buffer();
            let mut length = 0;
            for chunk in buffer.chunks_exact_mut(BYTES_PER_PIXEL) {
                let Some(pixel) = pixels.next() else {
                    break;
                };
                chunk.copy_from_slice(&pixel);
                length += BYTES_PER_PIXEL;
            }
            self.start_sending(length);
        }
        // Without a lent buffer no drawing step waits for the end later, so this send must end here.
        if self.lent.is_none() {
            self.finish_sending();
        }
        Ok(())
    }

    fn send_repeated_pixel<const BYTES_PER_PIXEL: usize>(
        &mut self,
        pixel: [u8; BYTES_PER_PIXEL],
        count: u32,
    ) -> Result<(), Self::Error> {
        self.send_pixels(core::iter::repeat_n(pixel, count as usize))
    }
}
