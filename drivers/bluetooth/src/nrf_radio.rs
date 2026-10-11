//! The nRF52832's radio, driven straight from its registers and timed by TIMER0 through fixed PPI channels.
//!
//! TIMER0 counts microseconds. A compare on CC0 enables the transmitter or receiver at the planned
//! moment, CC1 gives up on a reception, and the radio's ADDRESS and END events capture their times
//! into CC1 and CC2. Capturing ADDRESS into CC1 also moves the give-up time into the past, which is
//! how a reception that has started is never cut off.

use core::cell::Cell;
use core::ptr::addr_of_mut;

use critical_section::Mutex;
use embassy_nrf::interrupt::typelevel::{Binding, Handler, Interrupt};
use embassy_nrf::interrupt::{self, InterruptExt, Priority};
use embassy_nrf::peripherals::{PPI_CH20, PPI_CH21, PPI_CH22, PPI_CH26, PPI_CH27, RADIO, RNG, TIMER0};
use embassy_nrf::Peri;
use nrf_pac as pac;

use crate::controller::{BluetoothController, ControllerState, Radio, RadioEvent};
use crate::host_interface::HostInterface;
use crate::link_layer::RadioOperation;
use crate::packets::{Channel, ADDRESS_LENGTH, MAXIMUM_PAYLOAD_LENGTH, RADIO_BUFFER_LENGTH};

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

const CRYSTAL_SELECTED_AND_RUNNING: u32 = 1 | (1 << 16);

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
enum Activity {
    Idle,
    Transmitting,
    Receiving,
}

static ACTIVITY: Mutex<Cell<Activity>> = Mutex::new(Cell::new(Activity::Idle));
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
}

/// Starts the radio and returns the controller, ready to wrap in `ExternalController` for TrouBLE.
pub fn start(
    _peripherals: RadioPeripherals,
    _interrupts: impl Binding<interrupt::typelevel::RADIO, RadioInterruptHandler>,
) -> BluetoothController<'static, NrfRadio> {
    start_high_frequency_crystal();
    configure_radio();
    configure_timer();

    let device_address = read_device_address();
    STATE.start(HostInterface::new(device_address, read_random_bytes));

    interrupt::RADIO.set_priority(Priority::P0);
    interrupt::typelevel::RADIO::unpend();
    unsafe { interrupt::typelevel::RADIO::enable() };

    BluetoothController::new(&STATE, NrfRadio { _private: () })
}

fn start_high_frequency_crystal() {
    if pac::CLOCK.hfclkstat().read().0 & CRYSTAL_SELECTED_AND_RUNNING == CRYSTAL_SELECTED_AND_RUNNING {
        return;
    }
    pac::CLOCK.events_hfclkstarted().write_value(0);
    pac::CLOCK.tasks_hfclkstart().write_value(1);
    while pac::CLOCK.events_hfclkstarted().read() == 0 {}
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
    timer.tasks_start().write_value(1);

    pac::PPI
        .chenclr()
        .write(|channels| channels.0 = SCHEDULING_CHANNELS);
    pac::PPI
        .chenset()
        .write(|channels| channels.0 = ADDRESS_TIME_CHANNEL | END_TIME_CHANNEL);
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
// Running operations
// ============================================================================

pub struct NrfRadio {
    _private: (),
}

impl Radio for NrfRadio {
    fn now(&self) -> u32 {
        timer_now()
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

fn set_activity(activity: Activity) {
    critical_section::with(|critical_section| ACTIVITY.borrow(critical_section).set(activity));
}

fn take_activity() -> Activity {
    critical_section::with(|critical_section| ACTIVITY.borrow(critical_section).replace(Activity::Idle))
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
    pac::TIMER0.cc(compare).write_value(at);
}

fn is_too_late(at: u32) -> bool {
    (at.wrapping_sub(timer_now()) as i32) < MINIMUM_LEAD_IN_MICROSECONDS
}

fn run_operation(operation: RadioOperation) {
    stop_radio();

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
            if is_too_late(start_at) {
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

            let give_up_at = if is_too_late(give_up_at) {
                timer_now().wrapping_add(200)
            } else {
                give_up_at
            };
            arm_compare(GIVE_UP_COMPARE, give_up_at);
            arm_compare(START_COMPARE, start_at);
            pac::PPI
                .chenset()
                .write(|channels| channels.0 = START_RECEIVE_CHANNEL | GIVE_UP_CHANNEL);
            if is_too_late(start_at) {
                pac::RADIO.tasks_rxen().write_value(1);
            }
        }
    }
}

// ============================================================================
// Interrupt
// ============================================================================

pub struct RadioInterruptHandler {
    _private: (),
}

impl Handler<interrupt::typelevel::RADIO> for RadioInterruptHandler {
    unsafe fn on_interrupt() {
        let radio = pac::RADIO;
        if radio.events_disabled().read() == 0 {
            return;
        }
        radio.events_disabled().write_value(0);
        pac::PPI
            .chenclr()
            .write(|channels| channels.0 = SCHEDULING_CHANNELS);

        let packet_ended = radio.events_end().read() != 0;
        let end_time = pac::TIMER0.cc(END_CAPTURE).read();
        let event = match take_activity() {
            Activity::Idle => return,
            Activity::Transmitting => RadioEvent::Transmitted { end_time },
            Activity::Receiving if packet_ended => {
                let buffer = &(*addr_of_mut!(RECEIVE_BUFFER)).bytes;
                let length = (2 + buffer[1] as usize).min(RADIO_BUFFER_LENGTH);
                RadioEvent::Received {
                    packet: &buffer[..length],
                    crc_is_valid: radio.crcstatus().read().0 & 1 == 1,
                    address_time: pac::TIMER0.cc(ADDRESS_CAPTURE).read(),
                    end_time,
                    signal_strength: -(radio.rssisample().read().0 as i8 & 0x7F),
                }
            }
            Activity::Receiving => RadioEvent::NothingReceived { now: timer_now() },
        };

        STATE.on_radio_event(&NrfRadio { _private: () }, event);
    }
}
