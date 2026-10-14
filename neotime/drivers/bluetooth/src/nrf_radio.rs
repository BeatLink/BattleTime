//! The nRF52832's radio, driven straight from its registers and timed by TIMER0 through fixed PPI channels.
//!
//! TIMER0 counts microseconds. A compare on CC0 enables the transmitter or receiver at the planned
//! moment, CC1 gives up on a reception, and the radio's ADDRESS and END events capture their times
//! into CC1 and CC2. Capturing ADDRESS into CC1 also moves the give-up time into the past, which is
//! how a reception that has started is never cut off.
//!
//! Between operations TIMER0 and the 64 MHz crystal sleep. RTC0 keeps the time, and two of its
//! compares start the crystal and then TIMER0 again through PPI channels 3 and 4.

use core::cell::Cell;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicU32, Ordering};

use critical_section::Mutex;
use embassy_nrf::interrupt::typelevel::{Binding, Handler, Interrupt};
use embassy_nrf::interrupt::{self, InterruptExt, Priority};
use embassy_nrf::peripherals::{
    PPI_CH20, PPI_CH21, PPI_CH22, PPI_CH26, PPI_CH27, PPI_CH3, PPI_CH4, RADIO, RNG, RTC0, TIMER0,
};
use embassy_nrf::Peri;
use nrf_pac as pac;

use crate::controller::{BluetoothController, ControllerState, Radio, RadioEvent};
use crate::host_interface::HostInterface;
use crate::link_layer::RadioOperation;
use crate::packets::{Channel, ADDRESS_LENGTH, MAXIMUM_PAYLOAD_LENGTH, RADIO_BUFFER_LENGTH};
use crate::wake_plan::{self, Plan, Wake, COUNTER_MASK};

// ============================================================================
// Register values
// ============================================================================

const MODE_BLUETOOTH_1_MEGABIT: u32 = 3;
const FAST_RAMP_UP_WITH_CENTRED_TRANSMIT: u32 = 1 | (2 << 8);
const LENGTH_FIELD_8_BITS_AND_1_BYTE_HEADER: u32 = 8 | (1 << 8);
const MAXIMUM_LENGTH_3_BYTE_BASE_ADDRESS_AND_WHITENING: u32 =
    MAXIMUM_PAYLOAD_LENGTH as u32 | (3 << 16) | (1 << 25);
const CRC_3_BYTES_SKIPPING_ADDRESS: u32 = 3 | (1 << 8);
const BLUETOOTH_CRC_POLYNOMIAL: u32 = 0x0000_065B;
const WHITENING_ALWAYS_SET_BIT: u32 = 0x40;

const SHORT_READY_START: u32 = 1 << 0;
const SHORT_END_DISABLE: u32 = 1 << 1;
const SHORT_ADDRESS_SIGNAL_STRENGTH_START: u32 = 1 << 4;
const SHORT_DISABLED_SIGNAL_STRENGTH_STOP: u32 = 1 << 8;
const INTERRUPT_ON_DISABLED: u32 = 1 << 4;

const PART_CODE_ADDRESS: *const u32 = 0x1000_0130 as *const u32;
const PART_REVISION_ADDRESS: *const u32 = 0x1000_0134 as *const u32;
const NRF52832_PART_CODE: u32 = 6;
const FIRST_SILICON_REVISION: u32 = 3;
const RADIO_ERRATA_102_106_107_REGISTER: *mut u32 = 0x4000_1774 as *mut u32;

const TIMER_32_BITS: u32 = 3;
const TIMER_PRESCALER_FOR_1_MEGAHERTZ: u32 = 4;

const START_TRANSMIT_CHANNEL: u32 = 1 << 20;
const START_RECEIVE_CHANNEL: u32 = 1 << 21;
const GIVE_UP_CHANNEL: u32 = 1 << 22;
const ADDRESS_TIME_CHANNEL: u32 = 1 << 26;
const END_TIME_CHANNEL: u32 = 1 << 27;
const SCHEDULING_CHANNELS: u32 = START_TRANSMIT_CHANNEL | START_RECEIVE_CHANNEL | GIVE_UP_CHANNEL;
const START_CRYSTAL_CHANNEL: usize = 3;
const START_TIMER_CHANNEL: usize = 4;
const WAKE_CHANNELS: u32 = (1 << START_CRYSTAL_CHANNEL) | (1 << START_TIMER_CHANNEL);

const START_CRYSTAL_COMPARE: usize = 0;
const START_TIMER_COMPARE: usize = 1;
const COMPARE_EVENTS_0_AND_1: u32 = (1 << 16) | (1 << 17);

const START_COMPARE: usize = 0;
const GIVE_UP_COMPARE: usize = 1;
const END_CAPTURE: usize = 2;
const NOW_CAPTURE: usize = 3;
const ADDRESS_CAPTURE: usize = GIVE_UP_COMPARE;

/// How close to now an operation may be planned and still be started by the timer.
const MINIMUM_LEAD_IN_MICROSECONDS: i32 = 3;

// ============================================================================
// Buffers and shared state
// ============================================================================

#[repr(C, align(4))]
struct RadioBuffer {
    bytes: [u8; RADIO_BUFFER_LENGTH],
}

static mut TRANSMIT_BUFFER: RadioBuffer = RadioBuffer {
    bytes: [0; RADIO_BUFFER_LENGTH],
};
static mut RECEIVE_BUFFER: RadioBuffer = RadioBuffer {
    bytes: [0; RADIO_BUFFER_LENGTH],
};

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Activity {
    Idle,
    Transmitting,
    Receiving,
}

impl Activity {
    fn from_byte(byte: u8) -> Option<Activity> {
        [Activity::Idle, Activity::Transmitting, Activity::Receiving]
            .into_iter()
            .find(|activity| *activity as u8 == byte)
    }
}

/// Kept as a byte, so a value overwritten by something else reads as unknown rather than as undefined behaviour.
static ACTIVITY: Mutex<Cell<u8>> = Mutex::new(Cell::new(Activity::Idle as u8));

/// Whether TIMER0 is counting, the clock's reading where it counted from zero, and RTC0 extended to 64 bits.
#[derive(Clone, Copy)]
struct Clock {
    timer_is_running: bool,
    timer_base: u32,
    last_tick: u64,
}

static CLOCK: Mutex<Cell<Clock>> = Mutex::new(Cell::new(Clock {
    timer_is_running: false,
    timer_base: 0,
    last_tick: 0,
}));
static STATE: ControllerState = ControllerState::new();

// ============================================================================
// Starting the controller
// ============================================================================

/// The peripherals the controller takes for itself.
pub struct RadioPeripherals {
    pub radio: Peri<'static, RADIO>,
    pub timer: Peri<'static, TIMER0>,
    pub random_number_generator: Peri<'static, RNG>,
    pub start_transmit_channel: Peri<'static, PPI_CH20>,
    pub start_receive_channel: Peri<'static, PPI_CH21>,
    pub give_up_channel: Peri<'static, PPI_CH22>,
    pub address_time_channel: Peri<'static, PPI_CH26>,
    pub end_time_channel: Peri<'static, PPI_CH27>,
    pub real_time_counter: Peri<'static, RTC0>,
    pub start_crystal_channel: Peri<'static, PPI_CH3>,
    pub start_timer_channel: Peri<'static, PPI_CH4>,
}

/// Starts the radio and returns the controller, ready to wrap in `ExternalController` for TrouBLE.
pub fn start(
    _peripherals: RadioPeripherals,
    _interrupts: impl Binding<interrupt::typelevel::RADIO, RadioInterruptHandler>,
    free_host_packets: fn() -> usize,
) -> BluetoothController<'static, NrfRadio> {
    configure_radio();
    configure_timer();
    configure_wake_up();
    fall_asleep();

    let device_address = read_device_address();
    STATE.start(
        HostInterface::new(device_address, read_random_bytes),
        free_host_packets,
    );

    interrupt::RADIO.set_priority(Priority::P0);
    interrupt::typelevel::RADIO::unpend();
    unsafe { interrupt::typelevel::RADIO::enable() };

    BluetoothController::new(&STATE, NrfRadio { _private: () })
}

fn configure_radio() {
    let radio = pac::RADIO;

    radio.power().write(|power| power.set_power(true));
    radio.mode().write(|mode| mode.0 = MODE_BLUETOOTH_1_MEGABIT);
    radio
        .modecnf0()
        .write(|mode| mode.0 = FAST_RAMP_UP_WITH_CENTRED_TRANSMIT);
    radio.txpower().write(|power| power.0 = 0);
    radio
        .pcnf0()
        .write(|layout| layout.0 = LENGTH_FIELD_8_BITS_AND_1_BYTE_HEADER);
    radio
        .pcnf1()
        .write(|layout| layout.0 = MAXIMUM_LENGTH_3_BYTE_BASE_ADDRESS_AND_WHITENING);
    radio.crccnf().write(|crc| crc.0 = CRC_3_BYTES_SKIPPING_ADDRESS);
    radio.crcpoly().write(|crc| crc.0 = BLUETOOTH_CRC_POLYNOMIAL);
    radio.txaddress().write(|address| address.0 = 0);
    radio.rxaddresses().write(|addresses| addresses.0 = 1);
    radio.shorts().write(|shorts| {
        shorts.0 = SHORT_READY_START
            | SHORT_END_DISABLE
            | SHORT_ADDRESS_SIGNAL_STRENGTH_START
            | SHORT_DISABLED_SIGNAL_STRENGTH_STOP
    });
    radio
        .intenset()
        .write(|interrupts| interrupts.0 = INTERRUPT_ON_DISABLED);

    apply_first_silicon_radio_errata();
}

/// Errata 102, 106 and 107 make the first nRF52832 silicon miss packets at 1 megabit; later chips skip this.
fn apply_first_silicon_radio_errata() {
    let part_code = unsafe { PART_CODE_ADDRESS.read_volatile() };
    let revision = unsafe { PART_REVISION_ADDRESS.read_volatile() };
    if part_code != NRF52832_PART_CODE || revision != FIRST_SILICON_REVISION {
        return;
    }
    unsafe {
        let value = RADIO_ERRATA_102_106_107_REGISTER.read_volatile();
        RADIO_ERRATA_102_106_107_REGISTER.write_volatile((value & !1) | 0x0100_0000);
    }
}

fn configure_timer() {
    let timer = pac::TIMER0;

    timer.tasks_stop().write_value(1);
    timer.mode().write(|mode| mode.0 = 0);
    timer.bitmode().write(|bits| bits.0 = TIMER_32_BITS);
    timer
        .prescaler()
        .write(|prescaler| prescaler.0 = TIMER_PRESCALER_FOR_1_MEGAHERTZ);
    timer.tasks_clear().write_value(1);

    pac::PPI
        .chenclr()
        .write(|channels| channels.0 = SCHEDULING_CHANNELS);
    pac::PPI
        .chenset()
        .write(|channels| channels.0 = ADDRESS_TIME_CHANNEL | END_TIME_CHANNEL);
}

/// RTC0 counts at 32768 Hz, and its compares 0 and 1 start the crystal and TIMER0.
fn configure_wake_up() {
    let counter = pac::RTC0;
    counter.tasks_stop().write_value(1);
    counter.prescaler().write(|prescaler| prescaler.0 = 0);
    counter.intenclr().write(|interrupts| interrupts.0 = u32::MAX);
    counter
        .evtenset()
        .write(|events| events.0 = COMPARE_EVENTS_0_AND_1);
    counter.tasks_clear().write_value(1);
    counter.tasks_start().write_value(1);

    let ppi = pac::PPI;
    ppi.chenclr().write(|channels| channels.0 = WAKE_CHANNELS);
    ppi.ch(START_CRYSTAL_CHANNEL)
        .eep()
        .write_value(counter.events_compare(START_CRYSTAL_COMPARE).as_ptr() as u32);
    ppi.ch(START_CRYSTAL_CHANNEL)
        .tep()
        .write_value(pac::CLOCK.tasks_hfclkstart().as_ptr() as u32);
    ppi.ch(START_TIMER_CHANNEL)
        .eep()
        .write_value(counter.events_compare(START_TIMER_COMPARE).as_ptr() as u32);
    ppi.ch(START_TIMER_CHANNEL)
        .tep()
        .write_value(pac::TIMER0.tasks_start().as_ptr() as u32);
}

/// The chip's factory address, made a static random address by setting its top two bits.
fn read_device_address() -> [u8; ADDRESS_LENGTH] {
    let low = pac::FICR.deviceaddr(0).read().to_le_bytes();
    let high = pac::FICR.deviceaddr(1).read().to_le_bytes();
    [low[0], low[1], low[2], low[3], high[0], high[1] | 0xC0]
}

fn read_random_bytes(destination: &mut [u8]) {
    let generator = pac::RNG;
    generator.config().write(|config| config.set_dercen(true));
    generator.tasks_start().write_value(1);

    for byte in destination.iter_mut() {
        generator.events_valrdy().write_value(0);
        while generator.events_valrdy().read() == 0 {}
        *byte = generator.value().read().value();
    }

    generator.tasks_stop().write_value(1);
}

// ============================================================================
// The radio and its clock
// ============================================================================

pub struct NrfRadio {
    _private: (),
}

impl Radio for NrfRadio {
    fn now(&self) -> u32 {
        clock_now()
    }

    fn run(&self, operation: RadioOperation) {
        run_operation(operation);
    }

    fn with_transmit_buffer<Result>(&self, use_buffer: impl FnOnce(&mut [u8]) -> Result) -> Result {
        // The radio only reads this buffer while transmitting, and every caller runs while it is idle.
        let buffer = unsafe { &mut (*addr_of_mut!(TRANSMIT_BUFFER)).bytes };
        use_buffer(buffer)
    }
}

fn timer_now() -> u32 {
    pac::TIMER0.tasks_capture(NOW_CAPTURE).write_value(1);
    pac::TIMER0.cc(NOW_CAPTURE).read()
}

fn read_clock() -> Clock {
    critical_section::with(|critical_section| CLOCK.borrow(critical_section).get())
}

fn write_clock(clock: Clock) {
    critical_section::with(|critical_section| CLOCK.borrow(critical_section).set(clock));
}

/// RTC0's count, extended past its 24-bit wrap.
fn current_tick() -> u64 {
    critical_section::with(|critical_section| {
        let clock = CLOCK.borrow(critical_section);
        let mut reading = clock.get();
        reading.last_tick = wake_plan::extend_tick(reading.last_tick, pac::RTC0.counter().read().0);
        clock.set(reading);
        reading.last_tick
    })
}

/// Microseconds since RTC0 started: from TIMER0 while it runs, otherwise from RTC0.
fn clock_now() -> u32 {
    let clock = read_clock();
    if clock.timer_is_running {
        clock.timer_base.wrapping_add(timer_now())
    } else {
        wake_plan::microseconds_at_tick(current_tick())
    }
}

/// The clock's reading when TIMER0 captured into `capture`.
fn clock_at_capture(capture: usize) -> u32 {
    read_clock()
        .timer_base
        .wrapping_add(pac::TIMER0.cc(capture).read())
}

/// Converts a time on the clock to a TIMER0 compare value.
fn timer_value_at(at: u32) -> u32 {
    at.wrapping_sub(read_clock().timer_base)
}

fn set_activity(activity: Activity) {
    critical_section::with(|critical_section| ACTIVITY.borrow(critical_section).set(activity as u8));
}

/// What the radio was doing, or None when the byte holds no activity at all.
fn take_activity() -> Option<Activity> {
    let byte = critical_section::with(|critical_section| {
        ACTIVITY.borrow(critical_section).replace(Activity::Idle as u8)
    });
    Activity::from_byte(byte)
}

fn stop_radio() {
    let radio = pac::RADIO;
    pac::PPI
        .chenclr()
        .write(|channels| channels.0 = SCHEDULING_CHANNELS);
    radio.tasks_disable().write_value(1);
    while radio.state().read().0 != 0 {}
    radio.events_disabled().write_value(0);
    interrupt::typelevel::RADIO::unpend();
    set_activity(Activity::Idle);
}

// ============================================================================
// Sleeping and waking
// ============================================================================

/// Stops TIMER0 and the crystal; SHUTDOWN rather than STOP, as STOP alone keeps drawing current (erratum 78).
fn fall_asleep() {
    pac::TIMER0.tasks_shutdown().write_value(1);
    pac::TIMER0.tasks_clear().write_value(1);
    pac::CLOCK.tasks_hfclkstop().write_value(1);
    let mut clock = read_clock();
    clock.timer_is_running = false;
    write_clock(clock);
}

/// Has RTC0 start the crystal and TIMER0 at the planned ticks.
fn schedule_wake(wake: &Wake) {
    let counter = pac::RTC0;
    counter.events_compare(START_CRYSTAL_COMPARE).write_value(0);
    counter.events_compare(START_TIMER_COMPARE).write_value(0);
    counter
        .cc(START_CRYSTAL_COMPARE)
        .write(|compare| compare.0 = wake.crystal_tick as u32 & COUNTER_MASK);
    counter
        .cc(START_TIMER_COMPARE)
        .write(|compare| compare.0 = wake.timer_tick as u32 & COUNTER_MASK);

    let mut clock = read_clock();
    clock.timer_base = wake.timer_base;
    write_clock(clock);
    pac::PPI.chenset().write(|channels| channels.0 = WAKE_CHANNELS);
}

/// Called once the radio has finished an operation, which proves a planned wake took place.
fn note_timer_running() {
    let mut clock = read_clock();
    if !clock.timer_is_running {
        clock.timer_is_running = true;
        write_clock(clock);
        pac::RTC0.events_compare(START_CRYSTAL_COMPARE).write_value(0);
        pac::RTC0.events_compare(START_TIMER_COMPARE).write_value(0);
    }
}

/// Stops any planned wake, and says whether TIMER0 is running because one already happened.
fn cancel_wake() -> bool {
    pac::PPI.chenclr().write(|channels| channels.0 = WAKE_CHANNELS);
    if read_clock().timer_is_running {
        return true;
    }
    let timer_was_started = pac::RTC0.events_compare(START_TIMER_COMPARE).read() != 0 && timer_is_counting();
    if timer_was_started {
        note_timer_running();
    }
    timer_was_started
}

/// Whether TIMER0 moves within a few microseconds, for a wake that fired just as it was cancelled.
fn timer_is_counting() -> bool {
    let first = timer_now();
    (0..64).any(|_| timer_now() != first)
}

// ============================================================================
// Running operations
// ============================================================================

fn tune_to(channel: Channel) {
    let radio = pac::RADIO;
    radio
        .frequency()
        .write(|frequency| frequency.0 = channel.megahertz_above_2400() as u32);
    radio
        .datawhiteiv()
        .write(|whitening| whitening.0 = channel.index as u32 | WHITENING_ALWAYS_SET_BIT);
    radio.base0().write_value(channel.access_address << 8);
    radio
        .prefix0()
        .write(|prefix| prefix.0 = channel.access_address >> 24);
    radio.crcinit().write(|crc| crc.0 = channel.crc_initial_value);
}

fn clear_radio_events() {
    let radio = pac::RADIO;
    radio.events_ready().write_value(0);
    radio.events_address().write_value(0);
    radio.events_end().write_value(0);
    radio.events_disabled().write_value(0);
}

fn arm_compare(compare: usize, at: u32) {
    pac::TIMER0.events_compare(compare).write_value(0);
    pac::TIMER0.cc(compare).write_value(timer_value_at(at));
}

fn is_too_late(at: u32) -> bool {
    (at.wrapping_sub(clock_now()) as i32) < MINIMUM_LEAD_IN_MICROSECONDS
}

fn run_operation(operation: RadioOperation) {
    stop_radio();
    let mut timer_is_running = cancel_wake();

    loop {
        match wake_plan::plan(operation, clock_now(), current_tick(), timer_is_running) {
            Plan::StayAwake => return arm_operation(operation, true),
            Plan::Sleep => return fall_asleep(),
            Plan::SleepThenWake(wake) => {
                fall_asleep();
                schedule_wake(&wake);
                if wake_was_set_in_time(&wake) {
                    return arm_operation(wake_plan::postpone(operation, wake.delay), false);
                }
                // A compare set too close to the counter would wait a whole 512 s wrap, so plan again.
                pac::PPI.chenclr().write(|channels| channels.0 = WAKE_CHANNELS);
                timer_is_running = false;
            }
        }
    }
}

/// Whether the counter was still far enough from both compares when they were written.
fn wake_was_set_in_time(wake: &Wake) -> bool {
    current_tick() + wake_plan::MINIMUM_COUNTER_LEAD_TICKS <= wake.crystal_tick
}

/// Sets TIMER0 to start the operation; a running timer that has already passed its start starts it at once.
fn arm_operation(operation: RadioOperation, timer_is_running: bool) {
    match operation {
        RadioOperation::Off => {}

        RadioOperation::Transmit { start_at, channel } => {
            tune_to(channel);
            clear_radio_events();
            let buffer = addr_of_mut!(TRANSMIT_BUFFER);
            pac::RADIO.packetptr().write_value(buffer as u32);
            set_activity(Activity::Transmitting);

            arm_compare(START_COMPARE, start_at);
            pac::PPI
                .chenset()
                .write(|channels| channels.0 = START_TRANSMIT_CHANNEL);
            if timer_is_running && is_too_late(start_at) {
                pac::RADIO.tasks_txen().write_value(1);
            }
        }

        RadioOperation::Receive {
            start_at,
            give_up_at,
            channel,
        } => {
            tune_to(channel);
            clear_radio_events();
            let buffer = addr_of_mut!(RECEIVE_BUFFER);
            pac::RADIO.packetptr().write_value(buffer as u32);
            set_activity(Activity::Receiving);

            let give_up_at = if timer_is_running && is_too_late(give_up_at) {
                clock_now().wrapping_add(200)
            } else {
                give_up_at
            };
            arm_compare(GIVE_UP_COMPARE, give_up_at);
            arm_compare(START_COMPARE, start_at);
            pac::PPI
                .chenset()
                .write(|channels| channels.0 = START_RECEIVE_CHANNEL | GIVE_UP_CHANNEL);
            if timer_is_running && is_too_late(start_at) {
                pac::RADIO.tasks_rxen().write_value(1);
            }
        }
    }
}

// ============================================================================
// Interrupt
// ============================================================================

/// Radio interrupts so far, which show the link layer is still running.
static RADIO_EVENTS: AtomicU32 = AtomicU32::new(0);

/// How many radio interrupts there have been, and whether the radio has nothing to do; the
/// watchdog task reads these to tell a stalled radio from an idle one.
pub fn radio_progress() -> (u32, bool) {
    (RADIO_EVENTS.load(Ordering::Relaxed), STATE.radio_is_idle())
}

/// Ends a connection the radio has stopped serving; the firmware calls it from a timer.
pub fn check_supervision() {
    STATE.check_supervision(&NrfRadio { _private: () });
}

pub struct RadioInterruptHandler {
    _private: (),
}

impl Handler<interrupt::typelevel::RADIO> for RadioInterruptHandler {
    unsafe fn on_interrupt() {
        RADIO_EVENTS.fetch_add(1, Ordering::Relaxed);
        let radio = pac::RADIO;
        if radio.events_disabled().read() == 0 {
            return;
        }
        radio.events_disabled().write_value(0);
        pac::PPI
            .chenclr()
            .write(|channels| channels.0 = SCHEDULING_CHANNELS);

        let activity = take_activity();
        if activity == Some(Activity::Idle) {
            return;
        }
        note_timer_running();

        let packet_ended = radio.events_end().read() != 0;
        let end_time = clock_at_capture(END_CAPTURE);
        let event = match activity {
            Some(Activity::Idle) => return,
            Some(Activity::Transmitting) => RadioEvent::Transmitted { end_time },
            Some(Activity::Receiving) if packet_ended => {
                let buffer = &(*addr_of_mut!(RECEIVE_BUFFER)).bytes;
                let length = (2 + buffer[1] as usize).min(RADIO_BUFFER_LENGTH);
                RadioEvent::Received {
                    packet: &buffer[..length],
                    crc_is_valid: radio.crcstatus().read().0 & 1 == 1,
                    address_time: clock_at_capture(ADDRESS_CAPTURE),
                    end_time,
                    signal_strength: -(radio.rssisample().read().0 as i8 & 0x7F),
                }
            }
            Some(Activity::Receiving) => RadioEvent::NothingReceived { now: clock_now() },
            // Something overwrote the record of what the radio was doing. Hearing nothing lets the link
            // layer plan its next step, where returning would leave the radio off for good.
            None => RadioEvent::NothingReceived { now: clock_now() },
        };

        STATE.on_radio_event(&NrfRadio { _private: () }, event);
    }
}
