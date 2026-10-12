extern crate std;

use core::cell::{Cell, RefCell};
use core::future::Future;
use core::pin::pin;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::task::Context;
use std::sync::Arc;
use std::task::Wake;
use std::vec;
use std::vec::Vec;

use bt_hci::cmd::controller_baseband::Reset;
use bt_hci::cmd::le::{LeSetAdvEnable, LeSetAdvParams};
use bt_hci::data::{AclBroadcastFlag, AclPacket, AclPacketBoundary, SyncPacket};
use bt_hci::event::{CommandComplete, EventKind};
use bt_hci::param::{AddrKind, AdvChannelMap, AdvFilterPolicy, AdvKind, BdAddr, ConnHandle, Duration};
use bt_hci::{ControllerToHostPacket, FromHciBytes};
use embassy_futures::block_on;

use super::*;
use crate::link_layer::OUTGOING_QUEUE_LENGTH;
use crate::packets::{DataHeader, DataKind, CONNECT_REQUEST};

// ============================================================================
// A fake radio that records what it was asked to do
// ============================================================================

const OWN_ADDRESS: [u8; 6] = [0x11, 0x22, 0x33, 0x44, 0x55, 0xC6];
const CENTRAL_ADDRESS: [u8; 6] = [0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6];
const RESET_OPCODE: u16 = 0x0C03;

struct FakeRadio {
    now: Cell<u32>,
    operations: RefCell<Vec<RadioOperation>>,
    transmit_buffer: RefCell<[u8; RADIO_BUFFER_LENGTH]>,
}

impl FakeRadio {
    fn new() -> FakeRadio {
        FakeRadio {
            now: Cell::new(0),
            operations: RefCell::new(Vec::new()),
            transmit_buffer: RefCell::new([0; RADIO_BUFFER_LENGTH]),
        }
    }

    fn last_operation(&self) -> RadioOperation {
        *self
            .operations
            .borrow()
            .last()
            .expect("the radio has been given something to do")
    }
}

impl Radio for &FakeRadio {
    fn now(&self) -> u32 {
        self.now.get()
    }

    fn run(&self, operation: RadioOperation) {
        self.operations.borrow_mut().push(operation);
    }

    fn with_transmit_buffer<Result>(&self, use_buffer: impl FnOnce(&mut [u8]) -> Result) -> Result {
        use_buffer(&mut self.transmit_buffer.borrow_mut()[..])
    }
}

// ============================================================================
// A waker that counts how often it was woken
// ============================================================================

struct WakeCounter {
    wakes: AtomicUsize,
}

impl WakeCounter {
    fn new() -> Arc<WakeCounter> {
        Arc::new(WakeCounter {
            wakes: AtomicUsize::new(0),
        })
    }

    fn count(&self) -> usize {
        self.wakes.load(Ordering::SeqCst)
    }
}

impl Wake for WakeCounter {
    fn wake(self: Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn fake_random_bytes(destination: &mut [u8]) {
    for (index, byte) in destination.iter_mut().enumerate() {
        *byte = index as u8 + 1;
    }
}

fn started_state() -> ControllerState {
    let state = ControllerState::new();
    state.start(HostInterface::new(OWN_ADDRESS, fake_random_bytes));
    state
}

/// Reads the next packet for the host and gives back the opcode of the command it completes.
fn read_completed_opcode<RadioDriver: Radio>(controller: &BluetoothController<'_, RadioDriver>) -> u16 {
    let mut receive_buffer = [0; 64];
    let packet: ControllerToHostPacket = block_on(controller.read(&mut receive_buffer)).unwrap();
    let ControllerToHostPacket::Event(event) = packet else {
        panic!("expected an event, got {packet:?}")
    };
    assert!(matches!(event.kind, EventKind::CommandComplete));
    CommandComplete::from_hci_bytes_complete(event.data)
        .unwrap()
        .cmd_opcode
        .to_raw()
}

fn start_advertising<RadioDriver: Radio>(controller: &BluetoothController<'_, RadioDriver>) {
    let parameters = LeSetAdvParams::new(
        Duration::from_millis(100),
        Duration::from_millis(100),
        AdvKind::AdvInd,
        AddrKind::PUBLIC,
        AddrKind::PUBLIC,
        BdAddr::default(),
        AdvChannelMap::ALL,
        AdvFilterPolicy::default(),
    );
    block_on(controller.write(&parameters)).unwrap();
    block_on(controller.write(&LeSetAdvEnable::new(true))).unwrap();
    read_completed_opcode(controller);
    read_completed_opcode(controller);
}

fn connect_request() -> Vec<u8> {
    let access_address: u32 = 0x5065_A1B2;
    let crc_initial_value: u32 = 0x00AB_CDEF;
    let window_size = 3;
    let window_offset: u16 = 2;
    let interval: u16 = 24;
    let latency: u16 = 0;
    let timeout: u16 = 100;
    let all_data_channels = [0xFF, 0xFF, 0xFF, 0xFF, 0x1F];
    let hop_increment = 7;
    let fifty_parts_per_million = 5;

    let mut packet = vec![CONNECT_REQUEST | 0x80, 34];
    packet.extend_from_slice(&CENTRAL_ADDRESS);
    packet.extend_from_slice(&OWN_ADDRESS);
    packet.extend_from_slice(&access_address.to_le_bytes());
    packet.extend_from_slice(&crc_initial_value.to_le_bytes()[..3]);
    packet.push(window_size);
    packet.extend_from_slice(&window_offset.to_le_bytes());
    packet.extend_from_slice(&interval.to_le_bytes());
    packet.extend_from_slice(&latency.to_le_bytes());
    packet.extend_from_slice(&timeout.to_le_bytes());
    packet.extend_from_slice(&all_data_channels);
    packet.push(hop_increment | (fifty_parts_per_million << 5));
    packet
}

/// Plays the central: hears the advertisement, answers with a connect request, and gives the first anchor.
fn central_connects(state: &ControllerState, radio: &FakeRadio) -> u32 {
    let RadioOperation::Transmit { start_at, .. } = radio.last_operation() else {
        panic!("not advertising")
    };
    let advertisement_end = start_at + 300;
    state.on_radio_event(
        &radio,
        RadioEvent::Transmitted {
            end_time: advertisement_end,
        },
    );
    let packet = connect_request();
    let end_time = advertisement_end + 150 + 8 * packet.len() as u32;

    state.on_radio_event(
        &radio,
        RadioEvent::Received {
            packet: &packet,
            crc_is_valid: true,
            address_time: end_time - 300,
            end_time,
            signal_strength: -40,
        },
    );

    let RadioOperation::Receive {
        start_at, give_up_at, ..
    } = radio.last_operation()
    else {
        panic!("not listening for the first connection event")
    };
    start_at + (give_up_at - start_at) / 2
}

/// Plays the central: sends an empty data packet with the given sequence numbers at `address_time`.
fn central_sends_empty_packet(
    state: &ControllerState,
    radio: &FakeRadio,
    sequence_number: bool,
    next_expected_sequence_number: bool,
    address_time: u32,
) {
    let mut packet = [0; 2];
    DataHeader {
        kind: DataKind::Continuation,
        next_expected_sequence_number,
        sequence_number,
        more_data: false,
        length: 0,
    }
    .write(&mut packet);

    state.on_radio_event(
        &radio,
        RadioEvent::Received {
            packet: &packet,
            crc_is_valid: true,
            address_time,
            end_time: address_time + 8 * packet.len() as u32 + 24,
            signal_strength: -50,
        },
    );
}

fn acl_packet(data: &[u8]) -> AclPacket<'_> {
    AclPacket::new(
        ConnHandle::new(0),
        AclPacketBoundary::FirstNonFlushable,
        AclBroadcastFlag::PointToPoint,
        data,
    )
}

// ============================================================================
// Errors
// ============================================================================

#[test]
fn errors_display_as_their_names() {
    let shown = std::format!("{}", Error::HostBufferTooSmall);

    assert_eq!(shown, "HostBufferTooSmall");
}

#[test]
fn bad_host_packets_are_invalid_data_and_a_small_buffer_is_out_of_memory() {
    use embedded_io::Error as _;

    assert_eq!(Error::MalformedPacket.kind(), embedded_io::ErrorKind::InvalidData);
    assert_eq!(Error::PacketTooLong.kind(), embedded_io::ErrorKind::InvalidData);
    assert_eq!(
        Error::HostBufferTooSmall.kind(),
        embedded_io::ErrorKind::OutOfMemory
    );
}

#[test]
fn host_packet_errors_become_transport_errors() {
    assert_eq!(Error::from(HostPacketError::Malformed), Error::MalformedPacket);
    assert_eq!(Error::from(HostPacketError::TooLong), Error::PacketTooLong);
}

#[test]
fn read_errors_other_than_a_small_buffer_mean_a_malformed_packet() {
    let too_small: bt_hci::ReadHciError<core::convert::Infallible> = bt_hci::ReadHciError::BufferTooSmall;
    let invalid: bt_hci::ReadHciError<core::convert::Infallible> = bt_hci::ReadHciError::InvalidValue;

    assert_eq!(Error::from(too_small), Error::HostBufferTooSmall);
    assert_eq!(Error::from(invalid), Error::MalformedPacket);
}

// ============================================================================
// Commands through the transport
// ============================================================================

#[test]
fn a_default_state_answers_a_reset_once_started() {
    let radio = FakeRadio::new();
    let state = ControllerState::default();
    state.start(HostInterface::new(OWN_ADDRESS, fake_random_bytes));
    let controller = BluetoothController::new(&state, &radio);

    block_on(controller.write(&Reset::new())).unwrap();

    assert_eq!(radio.last_operation(), RadioOperation::Off);
    assert_eq!(read_completed_opcode(&controller), RESET_OPCODE);
}

#[test]
fn a_command_that_starts_advertising_starts_the_radio_at_its_clock() {
    let radio = FakeRadio::new();
    radio.now.set(5_000);
    let state = started_state();
    let controller = BluetoothController::new(&state, &radio);

    start_advertising(&controller);

    let RadioOperation::Transmit { start_at, channel } = radio.last_operation() else {
        panic!("expected a transmit")
    };
    assert_eq!(channel.index, 37);
    assert!(start_at > 5_000);
}

#[test]
fn a_command_waits_while_four_answers_are_unread() {
    let radio = FakeRadio::new();
    let state = started_state();
    let controller = BluetoothController::new(&state, &radio);
    for _ in 0..4 {
        block_on(controller.write(&Reset::new())).unwrap();
    }
    let wake_counter = WakeCounter::new();
    let waker = Arc::clone(&wake_counter).into();
    let mut context = Context::from_waker(&waker);
    let reset = Reset::new();
    let mut fifth_write = pin!(controller.write(&reset));

    let while_full = fifth_write.as_mut().poll(&mut context);
    read_completed_opcode(&controller);
    let after_one_read = fifth_write.as_mut().poll(&mut context);

    assert!(while_full.is_pending());
    assert_eq!(wake_counter.count(), 1);
    assert_eq!(after_one_read, Poll::Ready(Ok(())));
}

// ============================================================================
// Reading packets for the host
// ============================================================================

#[test]
fn a_read_waits_until_a_packet_is_ready_and_is_woken_for_it() {
    let radio = FakeRadio::new();
    let state = started_state();
    let controller = BluetoothController::new(&state, &radio);
    let wake_counter = WakeCounter::new();
    let waker = Arc::clone(&wake_counter).into();
    let mut context = Context::from_waker(&waker);
    let mut receive_buffer = [0; 64];
    let mut read = pin!(controller.read::<ControllerToHostPacket>(&mut receive_buffer));

    let before_any_command = read.as_mut().poll(&mut context);
    block_on(controller.write(&Reset::new())).unwrap();
    let after_reset = read.as_mut().poll(&mut context);

    assert!(before_any_command.is_pending());
    assert_eq!(wake_counter.count(), 1);
    assert!(matches!(
        after_reset,
        Poll::Ready(Ok(ControllerToHostPacket::Event(_)))
    ));
}

#[test]
fn reading_into_too_small_a_buffer_is_an_error() {
    let radio = FakeRadio::new();
    let state = started_state();
    let controller = BluetoothController::new(&state, &radio);
    block_on(controller.write(&Reset::new())).unwrap();
    let mut receive_buffer = [0; 3];

    let result = block_on(controller.read::<ControllerToHostPacket>(&mut receive_buffer));

    assert_eq!(result.unwrap_err(), Error::HostBufferTooSmall);
}

// ============================================================================
// Data through the transport
// ============================================================================

#[test]
fn synchronous_data_is_refused_as_malformed() {
    let radio = FakeRadio::new();
    let state = started_state();
    let controller = BluetoothController::new(&state, &radio);
    let packet_bytes = [0x00, 0x00, 1, 0xAA];
    let (sync_packet, _) = SyncPacket::from_hci_bytes(&packet_bytes).unwrap();

    let result = block_on(controller.write(&sync_packet));

    assert_eq!(result, Err(Error::MalformedPacket));
}

#[test]
fn data_longer_than_27_bytes_is_refused_as_too_long() {
    let radio = FakeRadio::new();
    let state = started_state();
    let controller = BluetoothController::new(&state, &radio);
    let data = [0; 28];

    let result = block_on(controller.write(&acl_packet(&data)));

    assert_eq!(result, Err(Error::PacketTooLong));
}

#[test]
fn a_data_write_waits_for_room_until_a_queued_packet_goes_out() {
    let radio = FakeRadio::new();
    let state = started_state();
    let controller = BluetoothController::new(&state, &radio);
    start_advertising(&controller);
    let anchor = central_connects(&state, &radio);
    forget_connection_events(&controller);
    for _ in 0..OUTGOING_QUEUE_LENGTH {
        block_on(controller.write(&acl_packet(b"ping"))).unwrap();
    }
    let wake_counter = WakeCounter::new();
    let waker = Arc::clone(&wake_counter).into();
    let mut context = Context::from_waker(&waker);
    let fifth_packet = acl_packet(b"pong");
    let mut fifth_write = pin!(controller.write(&fifth_packet));
    let while_full = fifth_write.as_mut().poll(&mut context);

    central_sends_empty_packet(&state, &radio, false, false, anchor);
    let after_one_went_out = fifth_write.as_mut().poll(&mut context);

    assert!(while_full.is_pending());
    assert_eq!(wake_counter.count(), 1);
    assert_eq!(after_one_went_out, Poll::Ready(Ok(())));
    assert!(matches!(radio.last_operation(), RadioOperation::Transmit { .. }));
    assert_eq!(&radio.transmit_buffer.borrow()[2..6], b"ping");
}

#[test]
fn an_acknowledged_packet_wakes_a_waiting_read() {
    let radio = FakeRadio::new();
    let state = started_state();
    let controller = BluetoothController::new(&state, &radio);
    start_advertising(&controller);
    let anchor = central_connects(&state, &radio);
    forget_connection_events(&controller);
    block_on(controller.write(&acl_packet(b"ping"))).unwrap();
    central_sends_empty_packet(&state, &radio, false, false, anchor);
    state.on_radio_event(
        &&radio,
        RadioEvent::Transmitted {
            end_time: anchor + 500,
        },
    );
    let wake_counter = WakeCounter::new();
    let waker = Arc::clone(&wake_counter).into();
    let mut context = Context::from_waker(&waker);
    let mut receive_buffer = [0; 64];
    let mut read = pin!(controller.read::<ControllerToHostPacket>(&mut receive_buffer));
    let before_acknowledgement = read.as_mut().poll(&mut context);

    central_sends_empty_packet(&state, &radio, true, true, anchor + 800);
    let after_acknowledgement = read.as_mut().poll(&mut context);

    assert!(before_acknowledgement.is_pending());
    assert_eq!(wake_counter.count(), 1);
    let Poll::Ready(Ok(ControllerToHostPacket::Event(event))) = after_acknowledgement else {
        panic!("expected an event, got {after_acknowledgement:?}")
    };
    assert!(matches!(event.kind, EventKind::NumberOfCompletedPackets));
}

/// Drops whatever the host has been told about the connection, which these tests do not check.
fn forget_connection_events<RadioDriver: Radio>(controller: &BluetoothController<'_, RadioDriver>) {
    controller
        .state
        .with_host_interface(|host_interface| host_interface.link_layer.events.clear());
}

// ============================================================================
// Radio events
// ============================================================================

#[test]
fn hearing_nothing_after_an_advertisement_moves_to_the_next_channel() {
    let radio = FakeRadio::new();
    let state = started_state();
    let controller = BluetoothController::new(&state, &radio);
    start_advertising(&controller);
    let RadioOperation::Transmit { start_at, .. } = radio.last_operation() else {
        panic!("not advertising")
    };
    state.on_radio_event(
        &&radio,
        RadioEvent::Transmitted {
            end_time: start_at + 300,
        },
    );

    state.on_radio_event(&&radio, RadioEvent::NothingReceived { now: start_at + 600 });

    let RadioOperation::Transmit { channel, .. } = radio.last_operation() else {
        panic!("expected the next advertisement")
    };
    assert_eq!(channel.index, 38);
}

// ============================================================================
// Supervision from a timer
// ============================================================================

/// The connect request's interval and supervision timeout, in microseconds.
const TEST_INTERVAL: u32 = 24 * 1_250;
const TEST_SUPERVISION_TIMEOUT: u32 = 100 * 10_000;

/// Reads the next packet for the host and says whether it reports the link gone.
fn reads_a_disconnection<RadioDriver: Radio>(controller: &BluetoothController<'_, RadioDriver>) -> bool {
    let mut receive_buffer = [0; 64];
    let packet: ControllerToHostPacket = block_on(controller.read(&mut receive_buffer)).unwrap();
    matches!(packet, ControllerToHostPacket::Event(event) if matches!(event.kind, EventKind::DisconnectionComplete))
}

#[test]
fn a_timer_check_ends_a_connection_whose_radio_went_quiet() {
    let radio = FakeRadio::new();
    let state = started_state();
    let controller = BluetoothController::new(&state, &radio);
    start_advertising(&controller);
    let anchor = central_connects(&state, &radio);
    central_sends_empty_packet(&state, &radio, false, false, anchor);
    forget_connection_events(&controller);
    let operations_before = radio.operations.borrow().len();

    radio.now.set(anchor + TEST_SUPERVISION_TIMEOUT);
    state.check_supervision(&&radio);
    assert_eq!(radio.operations.borrow().len(), operations_before);

    radio
        .now
        .set(anchor + TEST_SUPERVISION_TIMEOUT + TEST_INTERVAL + 1);
    state.check_supervision(&&radio);
    assert_eq!(radio.last_operation(), RadioOperation::Off);
    assert!(reads_a_disconnection(&controller));
}

#[test]
fn a_timer_check_ends_a_connection_that_never_got_going() {
    let radio = FakeRadio::new();
    let state = started_state();
    let controller = BluetoothController::new(&state, &radio);
    start_advertising(&controller);
    let first_anchor = central_connects(&state, &radio);
    forget_connection_events(&controller);

    radio.now.set(first_anchor + 8 * TEST_INTERVAL);
    state.check_supervision(&&radio);

    assert_eq!(radio.last_operation(), RadioOperation::Off);
    assert!(reads_a_disconnection(&controller));
}

#[test]
fn a_timer_check_leaves_advertising_and_an_unstarted_controller_alone() {
    let radio = FakeRadio::new();
    ControllerState::new().check_supervision(&&radio);
    assert!(radio.operations.borrow().is_empty());

    let state = started_state();
    let controller = BluetoothController::new(&state, &radio);
    start_advertising(&controller);
    let operations_before = radio.operations.borrow().len();
    radio.now.set(60_000_000);
    state.check_supervision(&&radio);
    assert_eq!(radio.operations.borrow().len(), operations_before);
}
