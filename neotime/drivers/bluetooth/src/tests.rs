extern crate std;

use std::vec;
use std::vec::Vec;

use bt_hci::event::le::{LeConnectionComplete, LeEventKind, LeEventPacket};
use bt_hci::event::{CommandComplete, CommandCompleteWithStatus, CommandStatus, EventKind, EventPacket};
use bt_hci::param::Status;
use bt_hci::FromHciBytes;

use crate::host_interface::*;
use crate::link_layer::*;
use crate::packets::*;

// ============================================================================
// A simulated central
// ============================================================================

const OWN_ADDRESS: [u8; 6] = [0x11, 0x22, 0x33, 0x44, 0x55, 0xC6];
const CENTRAL_ADDRESS: [u8; 6] = [0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6];
const OTHER_ADDRESS: [u8; 6] = [0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xF6];
const ACCESS_ADDRESS: u32 = 0x5065_A1B2;
const CRC_INITIAL_VALUE: u32 = 0x00AB_CDEF;
const HOP_INCREMENT: u8 = 7;
const INTERVAL_IN_UNITS: u16 = 24;
const INTERVAL_IN_MICROSECONDS: u32 = 24 * 1250;
const TIMEOUT_IN_UNITS: u16 = 100;
const CONNECT_REQUEST_END: u32 = 10_000;
const ALL_DATA_CHANNELS: [u8; 5] = [0xFF, 0xFF, 0xFF, 0xFF, 0x1F];

fn transmit_buffer() -> [u8; RADIO_BUFFER_LENGTH] {
    [0; RADIO_BUFFER_LENGTH]
}

fn advertising_link_layer(kind: AdvertisingKind, transmit_buffer: &mut [u8]) -> (LinkLayer, RadioOperation) {
    let mut link_layer = LinkLayer::new(12345);
    link_layer.settings.own_address = DeviceAddress {
        is_random: true,
        bytes: OWN_ADDRESS,
    };
    link_layer.settings.kind = kind;
    link_layer.settings.interval_in_microseconds = 100_000;
    link_layer
        .settings
        .advertising_data
        .extend_from_slice(&[2, 1, 6])
        .unwrap();
    link_layer
        .settings
        .scan_response_data
        .extend_from_slice(&[4, 9, b'N', b'e', b'o'])
        .unwrap();

    let operation = link_layer.start_advertising(0, transmit_buffer).unwrap().unwrap();
    (link_layer, operation)
}

fn scan_request(advertiser: [u8; 6]) -> Vec<u8> {
    let mut packet = vec![SCAN_REQUEST | 0x80, 12];
    packet.extend_from_slice(&CENTRAL_ADDRESS);
    packet.extend_from_slice(&advertiser);
    packet
}

struct ConnectParameters {
    window_size: u8,
    window_offset: u16,
    interval: u16,
    latency: u16,
    timeout: u16,
    channel_map: [u8; 5],
}

const DEFAULT_CONNECT: ConnectParameters = ConnectParameters {
    window_size: 3,
    window_offset: 2,
    interval: INTERVAL_IN_UNITS,
    latency: 0,
    timeout: TIMEOUT_IN_UNITS,
    channel_map: ALL_DATA_CHANNELS,
};

fn connect_request(parameters: &ConnectParameters) -> Vec<u8> {
    let mut packet = vec![CONNECT_REQUEST | 0x80, 34];
    packet.extend_from_slice(&CENTRAL_ADDRESS);
    packet.extend_from_slice(&OWN_ADDRESS);
    packet.extend_from_slice(&ACCESS_ADDRESS.to_le_bytes());
    packet.extend_from_slice(&CRC_INITIAL_VALUE.to_le_bytes()[..3]);
    packet.push(parameters.window_size);
    packet.extend_from_slice(&parameters.window_offset.to_le_bytes());
    packet.extend_from_slice(&parameters.interval.to_le_bytes());
    packet.extend_from_slice(&parameters.latency.to_le_bytes());
    packet.extend_from_slice(&parameters.timeout.to_le_bytes());
    packet.extend_from_slice(&parameters.channel_map);
    let fifty_parts_per_million = 5;
    packet.push(HOP_INCREMENT | (fifty_parts_per_million << 5));
    packet
}

/// A connected link layer, and the listening operation for the first connection event.
fn connected_link_layer(
    parameters: &ConnectParameters,
    transmit_buffer: &mut [u8],
) -> (LinkLayer, RadioOperation) {
    let (mut link_layer, _) = advertising_link_layer(AdvertisingKind::Connectable, transmit_buffer);
    link_layer.on_transmitted(1_000, transmit_buffer);

    let operation = link_layer.on_received(
        &connect_request(parameters),
        true,
        CONNECT_REQUEST_END - 300,
        CONNECT_REQUEST_END,
        -40,
        transmit_buffer,
    );
    (link_layer, operation)
}

struct CentralPacket<'payload> {
    kind: DataKind,
    sequence_number: bool,
    next_expected_sequence_number: bool,
    more_data: bool,
    payload: &'payload [u8],
}

impl CentralPacket<'_> {
    fn empty(sequence_number: bool, next_expected_sequence_number: bool) -> CentralPacket<'static> {
        CentralPacket {
            kind: DataKind::Continuation,
            sequence_number,
            next_expected_sequence_number,
            more_data: false,
            payload: &[],
        }
    }

    fn bytes(&self) -> Vec<u8> {
        let mut packet = vec![0; 2];
        DataHeader {
            kind: self.kind,
            next_expected_sequence_number: self.next_expected_sequence_number,
            sequence_number: self.sequence_number,
            more_data: self.more_data,
            length: self.payload.len() as u8,
        }
        .write(&mut packet);
        packet.extend_from_slice(self.payload);
        packet
    }
}

fn anchor_of(operation: RadioOperation) -> u32 {
    match operation {
        RadioOperation::Receive {
            start_at, give_up_at, ..
        } => start_at + (give_up_at - start_at) / 2,
        other => panic!("expected to be listening, got {other:?}"),
    }
}

/// Sends one packet from the central at `address_time`, and gives back the peripheral's reply header.
fn central_sends(
    link_layer: &mut LinkLayer,
    packet: &CentralPacket,
    address_time: u32,
    transmit_buffer: &mut [u8],
) -> (RadioOperation, DataHeader) {
    let bytes = packet.bytes();
    let end_time = address_time + 8 * bytes.len() as u32 + 24;
    let operation = link_layer.on_received(&bytes, true, address_time, end_time, -50, transmit_buffer);
    let header = DataHeader::parse(transmit_buffer).unwrap();
    (operation, header)
}

fn reply_payload(transmit_buffer: &[u8]) -> Vec<u8> {
    let length = transmit_buffer[1] as usize;
    transmit_buffer[2..2 + length].to_vec()
}

// ============================================================================
// Channels
// ============================================================================

#[test]
fn channels_map_to_their_bluetooth_frequencies() {
    let frequency = |index| Channel::advertising(index).megahertz_above_2400();

    assert_eq!(frequency(37), 2);
    assert_eq!(frequency(38), 26);
    assert_eq!(frequency(39), 80);
    assert_eq!(frequency(0), 4);
    assert_eq!(frequency(10), 24);
    assert_eq!(frequency(11), 28);
    assert_eq!(frequency(36), 78);
}

// ============================================================================
// Advertising
// ============================================================================

#[test]
fn advertising_sends_on_each_channel_in_turn_then_waits_an_interval() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) =
        advertising_link_layer(AdvertisingKind::NonConnectable, &mut transmit_buffer);

    let mut channels = Vec::new();
    let mut operation = first;
    let mut starts = Vec::new();
    for _ in 0..4 {
        let RadioOperation::Transmit { start_at, channel } = operation else {
            panic!("expected a transmit")
        };
        channels.push(channel.index);
        starts.push(start_at);
        operation = link_layer.on_transmitted(start_at + 200, &mut transmit_buffer);
    }

    assert_eq!(channels, [37, 38, 39, 37]);
    let gap_between_events = starts[3] - starts[0];
    assert!(
        (100_000..=110_000 + 2_000).contains(&gap_between_events),
        "gap {gap_between_events}"
    );
}

#[test]
fn advertisement_carries_our_address_and_data() {
    let mut transmit_buffer = transmit_buffer();
    advertising_link_layer(AdvertisingKind::Connectable, &mut transmit_buffer);

    assert_eq!(transmit_buffer[0], CONNECTABLE_ADVERTISEMENT | 0x40);
    assert_eq!(transmit_buffer[1], 9);
    assert_eq!(&transmit_buffer[2..8], &OWN_ADDRESS);
    assert_eq!(&transmit_buffer[8..11], &[2, 1, 6]);
}

#[test]
fn connectable_advertising_listens_after_each_advertisement() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, _) = advertising_link_layer(AdvertisingKind::Connectable, &mut transmit_buffer);

    let operation = link_layer.on_transmitted(1_000, &mut transmit_buffer);

    assert_eq!(
        operation,
        RadioOperation::Receive {
            start_at: 1_000 + LISTEN_DELAY,
            give_up_at: 1_000 + LISTEN_DEADLINE,
            channel: Channel::advertising(37),
        }
    );
}

#[test]
fn scan_request_for_us_gets_the_scan_response() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, _) = advertising_link_layer(AdvertisingKind::Connectable, &mut transmit_buffer);
    link_layer.on_transmitted(1_000, &mut transmit_buffer);

    let operation = link_layer.on_received(
        &scan_request(OWN_ADDRESS),
        true,
        1_200,
        1_350,
        -40,
        &mut transmit_buffer,
    );

    assert_eq!(
        operation,
        RadioOperation::Transmit {
            start_at: 1_350 + REPLY_DELAY,
            channel: Channel::advertising(37)
        }
    );
    assert_eq!(transmit_buffer[0] & 0x0F, SCAN_RESPONSE);
    assert_eq!(&transmit_buffer[2..8], &OWN_ADDRESS);
    assert_eq!(&transmit_buffer[8..13], &[4, 9, b'N', b'e', b'o']);
}

#[test]
fn scan_request_for_another_device_moves_to_the_next_channel() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, _) = advertising_link_layer(AdvertisingKind::Connectable, &mut transmit_buffer);
    link_layer.on_transmitted(1_000, &mut transmit_buffer);

    let operation = link_layer.on_received(
        &scan_request(OTHER_ADDRESS),
        true,
        1_200,
        1_350,
        -40,
        &mut transmit_buffer,
    );

    let RadioOperation::Transmit { channel, .. } = operation else {
        panic!("expected a transmit")
    };
    assert_eq!(channel.index, 38);
    assert_eq!(transmit_buffer[0] & 0x0F, CONNECTABLE_ADVERTISEMENT);
}

#[test]
fn filtered_advertising_ignores_scanners_not_on_the_accept_list() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, _) = advertising_link_layer(AdvertisingKind::Connectable, &mut transmit_buffer);
    link_layer.settings.filter_scan_requests = true;
    link_layer.on_transmitted(1_000, &mut transmit_buffer);

    let operation = link_layer.on_received(
        &scan_request(OWN_ADDRESS),
        true,
        1_200,
        1_350,
        -40,
        &mut transmit_buffer,
    );

    let RadioOperation::Transmit { channel, .. } = operation else {
        panic!("expected a transmit")
    };
    assert_eq!(channel.index, 38);
}

#[test]
fn stopping_advertising_turns_the_radio_off() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, _) = advertising_link_layer(AdvertisingKind::Connectable, &mut transmit_buffer);

    assert_eq!(link_layer.stop_advertising(), Some(RadioOperation::Off));
    assert!(!link_layer.is_advertising());
    assert_eq!(link_layer.stop_advertising(), None);
}

#[test]
fn starting_advertising_twice_leaves_the_radio_alone() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, _) = advertising_link_layer(AdvertisingKind::Connectable, &mut transmit_buffer);

    assert_eq!(
        link_layer.start_advertising(5_000, &mut transmit_buffer),
        Ok(None)
    );
}

// ============================================================================
// Connecting
// ============================================================================

#[test]
fn connect_request_starts_a_connection_and_tells_the_host() {
    let mut transmit_buffer = transmit_buffer();
    let (link_layer, _) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);

    assert!(link_layer.is_connected());
    assert_eq!(
        link_layer.events.front(),
        Some(&LinkEvent::Connected {
            peer: DeviceAddress {
                is_random: false,
                bytes: CENTRAL_ADDRESS
            },
            interval: INTERVAL_IN_UNITS,
            latency: 0,
            timeout: TIMEOUT_IN_UNITS,
            clock_accuracy: 5,
        })
    );
}

#[test]
fn first_listen_covers_the_transmit_window_on_the_first_hopped_channel() {
    let mut transmit_buffer = transmit_buffer();
    let (_, operation) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);

    let window_start = CONNECT_REQUEST_END + 1250 + 40 + 2 * 1250;
    let window_end = window_start + 3 * 1250;
    let RadioOperation::Receive {
        start_at,
        give_up_at,
        channel,
    } = operation
    else {
        panic!("expected to listen")
    };

    assert_eq!(
        channel,
        Channel {
            index: HOP_INCREMENT,
            access_address: ACCESS_ADDRESS,
            crc_initial_value: CRC_INITIAL_VALUE
        }
    );
    assert!(
        start_at < window_start - 40,
        "listening starts too late: {start_at}"
    );
    assert!(
        window_start - start_at < 200,
        "listening starts too early: {start_at}"
    );
    assert!(
        give_up_at > window_end && give_up_at - window_end < 100,
        "gives up at {give_up_at}"
    );
}

#[test]
fn invalid_connect_request_is_ignored() {
    let mut transmit_buffer = transmit_buffer();
    let one_channel = ConnectParameters {
        channel_map: [1, 0, 0, 0, 0],
        ..DEFAULT_CONNECT
    };
    let (link_layer, operation) = connected_link_layer(&one_channel, &mut transmit_buffer);

    assert!(link_layer.is_advertising());
    assert!(matches!(operation, RadioOperation::Transmit { .. }));
}

#[test]
fn channels_hop_and_skip_unused_ones() {
    let mut transmit_buffer = transmit_buffer();
    let even_channels_only = ConnectParameters {
        channel_map: [0x55, 0x55, 0x55, 0x55, 0x15],
        ..DEFAULT_CONNECT
    };
    let (mut link_layer, first) = connected_link_layer(&even_channels_only, &mut transmit_buffer);

    let mut channels = Vec::new();
    let mut operation = first;
    for _ in 0..4 {
        let RadioOperation::Receive {
            channel, give_up_at, ..
        } = operation
        else {
            panic!("expected to listen")
        };
        channels.push(channel.index);
        operation = link_layer.on_nothing_received(give_up_at, &mut transmit_buffer);
    }

    // Unmapped channels 7, 14, 21 and 28; the odd ones are remapped to the used channel at index
    // unmapped % 19 among the 19 even channels.
    assert_eq!(channels, [14, 14, 4, 28]);
}

// ============================================================================
// Data and acknowledgements
// ============================================================================

#[test]
fn new_data_is_acknowledged_and_handed_to_the_host() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);
    let anchor = anchor_of(first);

    let packet = CentralPacket {
        kind: DataKind::Start,
        sequence_number: false,
        next_expected_sequence_number: false,
        more_data: false,
        payload: b"hello",
    };
    let (operation, reply) = central_sends(&mut link_layer, &packet, anchor, &mut transmit_buffer);

    assert!(matches!(operation, RadioOperation::Transmit { .. }));
    assert!(reply.next_expected_sequence_number);
    assert!(!reply.sequence_number);
    assert_eq!(reply.length, 0);
    assert_eq!(
        link_layer.events.back(),
        Some(&LinkEvent::Received(DataPacket::new(DataKind::Start, b"hello")))
    );
}

#[test]
fn a_resent_packet_is_not_handed_to_the_host_twice() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);
    let anchor = anchor_of(first);
    let packet = CentralPacket {
        kind: DataKind::Start,
        sequence_number: false,
        next_expected_sequence_number: false,
        more_data: true,
        payload: b"once",
    };

    central_sends(&mut link_layer, &packet, anchor, &mut transmit_buffer);
    link_layer.on_transmitted(anchor + 600, &mut transmit_buffer);
    central_sends(&mut link_layer, &packet, anchor + 900, &mut transmit_buffer);

    let received = link_layer
        .events
        .iter()
        .filter(|event| matches!(event, LinkEvent::Received(_)))
        .count();
    assert_eq!(received, 1);
}

#[test]
fn host_data_is_resent_until_acknowledged_then_counted_as_completed() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);
    let anchor = anchor_of(first);
    link_layer
        .send_to_peer(DataPacket::new(DataKind::Start, b"ping"))
        .unwrap();

    let (_, first_reply) = central_sends(
        &mut link_layer,
        &CentralPacket::empty(false, false),
        anchor,
        &mut transmit_buffer,
    );
    let first_payload = reply_payload(&transmit_buffer);
    link_layer.on_transmitted(anchor + 500, &mut transmit_buffer);

    let not_acknowledged = CentralPacket::empty(true, false);
    let (_, second_reply) = central_sends(
        &mut link_layer,
        &not_acknowledged,
        anchor + 800,
        &mut transmit_buffer,
    );
    let second_payload = reply_payload(&transmit_buffer);
    link_layer.on_transmitted(anchor + 1_300, &mut transmit_buffer);

    let acknowledged = CentralPacket::empty(false, true);
    central_sends(
        &mut link_layer,
        &acknowledged,
        anchor + 1_600,
        &mut transmit_buffer,
    );

    assert_eq!(first_payload, b"ping");
    assert_eq!(second_payload, b"ping");
    assert_eq!(first_reply.sequence_number, second_reply.sequence_number);
    assert_eq!(link_layer.completed_packets, 1);
}

#[test]
fn data_is_not_acknowledged_while_the_host_queue_is_full() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);
    let anchor = anchor_of(first);
    while link_layer.events.len() < EVENT_QUEUE_LENGTH - 2 {
        link_layer
            .events
            .push_back(LinkEvent::Disconnected { reason: 0 })
            .unwrap();
    }

    let packet = CentralPacket {
        kind: DataKind::Start,
        sequence_number: false,
        next_expected_sequence_number: false,
        more_data: false,
        payload: b"wait",
    };
    let (_, reply) = central_sends(&mut link_layer, &packet, anchor, &mut transmit_buffer);

    assert!(!reply.next_expected_sequence_number);
}

#[test]
fn the_event_continues_while_either_side_has_more_data() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);
    let anchor = anchor_of(first);
    let packet = CentralPacket {
        more_data: true,
        ..CentralPacket::empty(false, false)
    };

    central_sends(&mut link_layer, &packet, anchor, &mut transmit_buffer);
    let operation = link_layer.on_transmitted(anchor + 400, &mut transmit_buffer);

    assert_eq!(
        operation,
        RadioOperation::Receive {
            start_at: anchor + 400 + LISTEN_DELAY,
            give_up_at: anchor + 400 + LISTEN_DEADLINE,
            channel: Channel {
                index: HOP_INCREMENT,
                access_address: ACCESS_ADDRESS,
                crc_initial_value: CRC_INITIAL_VALUE
            },
        }
    );
}

#[test]
fn the_next_event_is_one_interval_after_the_anchor() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);
    let anchor = anchor_of(first);

    central_sends(
        &mut link_layer,
        &CentralPacket::empty(false, false),
        anchor,
        &mut transmit_buffer,
    );
    let operation = link_layer.on_transmitted(anchor + 400, &mut transmit_buffer);

    let RadioOperation::Receive {
        start_at,
        give_up_at,
        channel,
    } = operation
    else {
        panic!("expected to listen")
    };
    let next_anchor = anchor + INTERVAL_IN_MICROSECONDS;
    assert!(start_at < next_anchor - 90 && next_anchor - start_at < 200);
    assert!(give_up_at > next_anchor && give_up_at - next_anchor < 40);
    assert_eq!(channel.index, 2 * HOP_INCREMENT);
}

fn listen_window_of(operation: RadioOperation) -> (u32, u32) {
    match operation {
        RadioOperation::Receive {
            start_at, give_up_at, ..
        } => (start_at, give_up_at),
        other => panic!("expected to be listening, got {other:?}"),
    }
}

/// Checks that the listen around `expected_anchor` is the narrow one of a connection back in step.
fn assert_listens_narrowly_around(operation: RadioOperation, expected_anchor: u32) {
    let (start_at, give_up_at) = listen_window_of(operation);

    assert!(
        start_at < expected_anchor && expected_anchor - start_at < 200,
        "starts at {start_at}"
    );
    assert!(
        give_up_at > expected_anchor && give_up_at - expected_anchor < 60,
        "gives up at {give_up_at}"
    );
}

#[test]
fn the_transmit_window_stays_open_until_the_central_is_heard() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);
    let window_start = CONNECT_REQUEST_END + 1250 + 40 + 2 * 1250;
    let window_end = window_start + 3 * 1250;
    let (_, first_give_up) = listen_window_of(first);

    let second = link_layer.on_nothing_received(first_give_up, &mut transmit_buffer);
    let (_, second_give_up) = listen_window_of(second);
    let late_anchor = window_end + INTERVAL_IN_MICROSECONDS - 300;
    central_sends(
        &mut link_layer,
        &CentralPacket::empty(false, false),
        late_anchor,
        &mut transmit_buffer,
    );
    let third = link_layer.on_transmitted(late_anchor + 400, &mut transmit_buffer);

    assert!(
        second_give_up > window_end + INTERVAL_IN_MICROSECONDS,
        "gives up at {second_give_up}"
    );
    assert!(link_layer.is_connected());
    assert_listens_narrowly_around(third, late_anchor + INTERVAL_IN_MICROSECONDS);
}

#[test]
fn a_connection_update_window_stays_open_until_the_central_is_heard() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);
    let anchor = anchor_of(first);
    let new_interval: u16 = 40;
    let window_size: u8 = 2;
    let window_offset: u16 = 1;
    let instant: u16 = 3;
    let mut update = vec![CONNECTION_UPDATE_INDICATION, window_size];
    update.extend_from_slice(&window_offset.to_le_bytes());
    update.extend_from_slice(&new_interval.to_le_bytes());
    update.extend_from_slice(&0u16.to_le_bytes());
    update.extend_from_slice(&200u16.to_le_bytes());
    update.extend_from_slice(&instant.to_le_bytes());
    let packet = CentralPacket {
        kind: DataKind::Control,
        sequence_number: false,
        next_expected_sequence_number: false,
        more_data: false,
        payload: &update,
    };
    central_sends(&mut link_layer, &packet, anchor, &mut transmit_buffer);
    let mut operation = link_layer.on_transmitted(anchor + 400, &mut transmit_buffer);
    for _ in 1..instant {
        let (_, give_up_at) = listen_window_of(operation);
        operation = link_layer.on_nothing_received(give_up_at, &mut transmit_buffer);
    }
    let window_start = anchor + instant as u32 * INTERVAL_IN_MICROSECONDS + window_offset as u32 * 1250;
    let window_end = window_start + window_size as u32 * 1250;
    let new_interval_in_microseconds = new_interval as u32 * 1250;

    let (_, give_up_at) = listen_window_of(operation);
    let after_instant = link_layer.on_nothing_received(give_up_at, &mut transmit_buffer);
    let (_, after_instant_give_up) = listen_window_of(after_instant);
    let late_anchor = window_end + new_interval_in_microseconds - 300;
    central_sends(
        &mut link_layer,
        &CentralPacket::empty(true, true),
        late_anchor,
        &mut transmit_buffer,
    );
    let next = link_layer.on_transmitted(late_anchor + 400, &mut transmit_buffer);

    assert!(
        give_up_at > window_end,
        "the instant's listen gives up at {give_up_at}"
    );
    assert!(
        after_instant_give_up > window_end + new_interval_in_microseconds,
        "gives up at {after_instant_give_up}"
    );
    assert!(link_layer.is_connected());
    assert_listens_narrowly_around(next, late_anchor + new_interval_in_microseconds);
}

// ============================================================================
// Control procedures
// ============================================================================

fn control_reply(link_layer: &mut LinkLayer, request: &[u8], transmit_buffer: &mut [u8]) -> Vec<u8> {
    let anchor = 50_000;
    let packet = CentralPacket {
        kind: DataKind::Control,
        sequence_number: false,
        next_expected_sequence_number: false,
        more_data: false,
        payload: request,
    };
    let (_, header) = central_sends(link_layer, &packet, anchor, transmit_buffer);

    assert_eq!(header.kind, DataKind::Control);
    reply_payload(transmit_buffer)
}

#[test]
fn feature_request_gets_an_empty_feature_set() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, _) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);

    let reply = control_reply(
        &mut link_layer,
        &[FEATURE_REQUEST, 0xFF, 0, 0, 0, 0, 0, 0, 0],
        &mut transmit_buffer,
    );

    assert_eq!(reply, [FEATURE_RESPONSE, 0, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn version_indication_is_answered_with_ours() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, _) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);

    let reply = control_reply(
        &mut link_layer,
        &[VERSION_INDICATION, 0x0B, 0x59, 0x00, 0x01, 0x00],
        &mut transmit_buffer,
    );

    assert_eq!(reply[0], VERSION_INDICATION);
    assert_eq!(&reply[1..], &VERSION);
}

#[test]
fn unsupported_procedures_get_an_unknown_response() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, _) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);
    let encryption_request = 0x03;

    let reply = control_reply(
        &mut link_layer,
        &[encryption_request, 0, 0, 0],
        &mut transmit_buffer,
    );

    assert_eq!(reply, [UNKNOWN_RESPONSE, encryption_request]);
}

/// Sends each request as a new packet in one event without acknowledging our replies, and says which were accepted.
fn control_requests_accepted(
    link_layer: &mut LinkLayer,
    requests: &[&[u8]],
    transmit_buffer: &mut [u8],
) -> Vec<bool> {
    let mut address_time = 50_000;
    let mut sequence_number = false;
    let mut accepted = Vec::new();
    for request in requests {
        let packet = CentralPacket {
            kind: DataKind::Control,
            sequence_number,
            next_expected_sequence_number: false,
            more_data: true,
            payload: request,
        };
        let (_, reply) = central_sends(link_layer, &packet, address_time, transmit_buffer);
        link_layer.on_transmitted(address_time + 400, transmit_buffer);
        let was_accepted = reply.next_expected_sequence_number != sequence_number;
        if was_accepted {
            sequence_number = !sequence_number;
        }
        accepted.push(was_accepted);
        address_time += 700;
    }
    accepted
}

#[test]
fn four_control_requests_in_one_event_are_all_accepted() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, _) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);
    let feature_request: &[u8] = &[FEATURE_REQUEST, 0, 0, 0, 0, 0, 0, 0, 0];
    let version_indication: &[u8] = &[VERSION_INDICATION, 0x0B, 0x59, 0x00, 0x01, 0x00];
    let ping_request: &[u8] = &[PING_REQUEST];
    let encryption_request: &[u8] = &[0x03, 0, 0, 0];

    let accepted = control_requests_accepted(
        &mut link_layer,
        &[
            feature_request,
            version_indication,
            ping_request,
            encryption_request,
        ],
        &mut transmit_buffer,
    );

    assert_eq!(accepted, [true; 4]);
}

#[test]
fn a_terminate_is_accepted_while_the_reply_queue_is_full() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, _) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);
    let ping_request: &[u8] = &[PING_REQUEST];
    let terminate: &[u8] = &[TERMINATE_INDICATION, disconnect_reason::REMOTE_USER_TERMINATED];

    let accepted = control_requests_accepted(
        &mut link_layer,
        &[
            ping_request,
            ping_request,
            ping_request,
            ping_request,
            ping_request,
            ping_request,
            terminate,
        ],
        &mut transmit_buffer,
    );

    assert_eq!(accepted, [true, true, true, true, true, false, true]);
}

#[test]
fn central_terminating_ends_the_connection_after_our_acknowledgement() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);
    let anchor = anchor_of(first);
    let packet = CentralPacket {
        kind: DataKind::Control,
        sequence_number: false,
        next_expected_sequence_number: false,
        more_data: false,
        payload: &[TERMINATE_INDICATION, disconnect_reason::REMOTE_USER_TERMINATED],
    };

    let (_, reply) = central_sends(&mut link_layer, &packet, anchor, &mut transmit_buffer);
    let operation = link_layer.on_transmitted(anchor + 400, &mut transmit_buffer);

    assert!(reply.next_expected_sequence_number);
    assert_eq!(operation, RadioOperation::Off);
    assert!(!link_layer.is_connected());
    assert_eq!(
        link_layer.events.back(),
        Some(&LinkEvent::Disconnected {
            reason: disconnect_reason::REMOTE_USER_TERMINATED
        })
    );
}

#[test]
fn host_disconnect_sends_terminate_and_ends_once_acknowledged() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);
    let anchor = anchor_of(first);
    link_layer
        .disconnect(disconnect_reason::REMOTE_USER_TERMINATED)
        .unwrap();

    central_sends(
        &mut link_layer,
        &CentralPacket::empty(false, false),
        anchor,
        &mut transmit_buffer,
    );
    let terminate = reply_payload(&transmit_buffer);
    link_layer.on_transmitted(anchor + 400, &mut transmit_buffer);
    let operation = link_layer.on_received(
        &CentralPacket::empty(true, true).bytes(),
        true,
        anchor + 700,
        anchor + 800,
        -50,
        &mut transmit_buffer,
    );

    assert_eq!(
        terminate,
        [TERMINATE_INDICATION, disconnect_reason::REMOTE_USER_TERMINATED]
    );
    assert_eq!(operation, RadioOperation::Off);
    assert_eq!(
        link_layer.events.back(),
        Some(&LinkEvent::Disconnected {
            reason: disconnect_reason::LOCAL_HOST_TERMINATED
        })
    );
}

#[test]
fn connection_update_takes_effect_at_its_instant() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);
    let anchor = anchor_of(first);
    let new_interval: u16 = 40;
    let instant: u16 = 3;
    let mut update = vec![CONNECTION_UPDATE_INDICATION, 1];
    update.extend_from_slice(&0u16.to_le_bytes());
    update.extend_from_slice(&new_interval.to_le_bytes());
    update.extend_from_slice(&0u16.to_le_bytes());
    update.extend_from_slice(&200u16.to_le_bytes());
    update.extend_from_slice(&instant.to_le_bytes());
    let packet = CentralPacket {
        kind: DataKind::Control,
        sequence_number: false,
        next_expected_sequence_number: false,
        more_data: false,
        payload: &update,
    };

    central_sends(&mut link_layer, &packet, anchor, &mut transmit_buffer);
    let mut operation = link_layer.on_transmitted(anchor + 400, &mut transmit_buffer);
    for _ in 0..3 {
        let RadioOperation::Receive { give_up_at, .. } = operation else {
            panic!("expected to listen")
        };
        operation = link_layer.on_nothing_received(give_up_at, &mut transmit_buffer);
    }

    assert_eq!(link_layer.current_interval(), Some(new_interval));
    let changed = LinkEvent::ParametersChanged {
        interval: new_interval,
        latency: 0,
        timeout: 200,
    };
    assert!(link_layer.events.iter().any(|event| *event == changed));
}

#[test]
fn an_update_whose_instant_has_passed_drops_the_connection() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);
    let anchor = anchor_of(first);
    let instant_already_reached: u16 = 0;
    let mut update = vec![CONNECTION_UPDATE_INDICATION, 1, 0, 0, 40, 0, 0, 0, 200, 0];
    update.extend_from_slice(&instant_already_reached.to_le_bytes());
    let packet = CentralPacket {
        kind: DataKind::Control,
        sequence_number: false,
        next_expected_sequence_number: false,
        more_data: false,
        payload: &update,
    };

    let bytes = packet.bytes();
    let operation = link_layer.on_received(&bytes, true, anchor, anchor + 200, -50, &mut transmit_buffer);

    assert_eq!(operation, RadioOperation::Off);
    assert_eq!(
        link_layer.events.back(),
        Some(&LinkEvent::Disconnected {
            reason: disconnect_reason::INSTANT_PASSED
        })
    );
}

// ============================================================================
// Peripheral latency
// ============================================================================

const LATENCY_OF_TWO: ConnectParameters = ConnectParameters {
    latency: 2,
    ..DEFAULT_CONNECT
};

fn channel_of(operation: RadioOperation) -> u8 {
    match operation {
        RadioOperation::Receive { channel, .. } => channel.index,
        other => panic!("expected to be listening, got {other:?}"),
    }
}

/// Plays one quiet event the central opens at `anchor`, and gives back the listen that follows it.
fn quiet_event(
    link_layer: &mut LinkLayer,
    sequence_numbers: bool,
    anchor: u32,
    transmit_buffer: &mut [u8],
) -> RadioOperation {
    central_sends(
        link_layer,
        &CentralPacket::empty(sequence_numbers, sequence_numbers),
        anchor,
        transmit_buffer,
    );
    link_layer.on_transmitted(anchor + 400, transmit_buffer)
}

#[test]
fn latency_skips_events_after_a_quiet_one() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&LATENCY_OF_TWO, &mut transmit_buffer);
    let anchor = anchor_of(first);

    let next = quiet_event(&mut link_layer, false, anchor, &mut transmit_buffer);

    assert_listens_narrowly_around(next, anchor + 3 * INTERVAL_IN_MICROSECONDS);
    assert_eq!(channel_of(next), 4 * HOP_INCREMENT % DATA_CHANNEL_COUNT);
}

#[test]
fn latency_resynchronises_on_the_next_event_heard() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&LATENCY_OF_TWO, &mut transmit_buffer);
    let anchor = anchor_of(first);
    quiet_event(&mut link_layer, false, anchor, &mut transmit_buffer);
    let drifted_anchor = anchor + 3 * INTERVAL_IN_MICROSECONDS + 20;

    let next = quiet_event(&mut link_layer, true, drifted_anchor, &mut transmit_buffer);

    assert_listens_narrowly_around(next, drifted_anchor + 3 * INTERVAL_IN_MICROSECONDS);
    assert_eq!(channel_of(next), 7 * HOP_INCREMENT % DATA_CHANNEL_COUNT);
}

#[test]
fn latency_skips_nothing_while_data_waits_to_be_sent() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&LATENCY_OF_TWO, &mut transmit_buffer);
    let anchor = anchor_of(first);
    link_layer
        .send_to_peer(DataPacket::new(DataKind::Start, b"hello"))
        .unwrap();

    let next = quiet_event(&mut link_layer, false, anchor, &mut transmit_buffer);

    assert_listens_narrowly_around(next, anchor + INTERVAL_IN_MICROSECONDS);
}

#[test]
fn latency_skips_nothing_while_an_update_waits_for_its_instant() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&LATENCY_OF_TWO, &mut transmit_buffer);
    let anchor = anchor_of(first);
    let instant: u16 = 2;
    let mut update = vec![CONNECTION_UPDATE_INDICATION, 1, 0, 0, 40, 0, 0, 0, 200, 0];
    update.extend_from_slice(&instant.to_le_bytes());
    let packet = CentralPacket {
        kind: DataKind::Control,
        sequence_number: false,
        next_expected_sequence_number: false,
        more_data: false,
        payload: &update,
    };

    central_sends(&mut link_layer, &packet, anchor, &mut transmit_buffer);
    let next = link_layer.on_transmitted(anchor + 400, &mut transmit_buffer);

    assert_listens_narrowly_around(next, anchor + INTERVAL_IN_MICROSECONDS);
}

#[test]
fn data_from_the_host_brings_back_a_skipped_event() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&LATENCY_OF_TWO, &mut transmit_buffer);
    let anchor = anchor_of(first);
    quiet_event(&mut link_layer, false, anchor, &mut transmit_buffer);
    link_layer
        .send_to_peer(DataPacket::new(DataKind::Start, b"hello"))
        .unwrap();

    let resumed = link_layer.resume_skipped_events(anchor + 1_000).unwrap();

    assert_listens_narrowly_around(resumed, anchor + INTERVAL_IN_MICROSECONDS);
    assert_eq!(channel_of(resumed), 2 * HOP_INCREMENT % DATA_CHANNEL_COUNT);
}

#[test]
fn a_skipped_event_too_close_to_reach_stays_skipped() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&LATENCY_OF_TWO, &mut transmit_buffer);
    let anchor = anchor_of(first);
    quiet_event(&mut link_layer, false, anchor, &mut transmit_buffer);

    let resumed = link_layer.resume_skipped_events(anchor + 2 * INTERVAL_IN_MICROSECONDS - 1_000);

    assert_eq!(resumed, None);
}

// ============================================================================
// Losing the connection
// ============================================================================

#[test]
fn a_central_that_never_answers_fails_to_establish() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);

    let mut operation = first;
    for _ in 0..10 {
        let RadioOperation::Receive { give_up_at, .. } = operation else {
            break;
        };
        operation = link_layer.on_nothing_received(give_up_at, &mut transmit_buffer);
    }

    assert_eq!(operation, RadioOperation::Off);
    assert_eq!(
        link_layer.events.back(),
        Some(&LinkEvent::Disconnected {
            reason: disconnect_reason::FAILED_TO_ESTABLISH
        })
    );
}

#[test]
fn silence_longer_than_the_supervision_timeout_drops_the_connection() {
    let mut transmit_buffer = transmit_buffer();
    let (mut link_layer, first) = connected_link_layer(&DEFAULT_CONNECT, &mut transmit_buffer);
    central_sends(
        &mut link_layer,
        &CentralPacket::empty(false, false),
        anchor_of(first),
        &mut transmit_buffer,
    );
    let mut operation = link_layer.on_transmitted(anchor_of(first) + 400, &mut transmit_buffer);

    let mut events_listened = 0;
    while let RadioOperation::Receive { give_up_at, .. } = operation {
        operation = link_layer.on_nothing_received(give_up_at, &mut transmit_buffer);
        events_listened += 1;
    }

    let events_in_one_second = 1_000_000 / INTERVAL_IN_MICROSECONDS;
    assert!(events_listened >= events_in_one_second && events_listened <= events_in_one_second + 2);
    assert_eq!(
        link_layer.events.back(),
        Some(&LinkEvent::Disconnected {
            reason: disconnect_reason::CONNECTION_TIMEOUT
        })
    );
}

// ============================================================================
// Host interface: commands
// ============================================================================

fn fake_random_bytes(destination: &mut [u8]) {
    for (index, byte) in destination.iter_mut().enumerate() {
        *byte = index as u8 + 1;
    }
}

fn plenty_of_host_packets() -> usize {
    usize::MAX
}

fn host_interface() -> HostInterface {
    HostInterface::new(OWN_ADDRESS, fake_random_bytes)
}

fn run_command<Command: bt_hci::transport::PacketToController>(
    host_interface: &mut HostInterface,
    command: &Command,
    transmit_buffer: &mut [u8],
) -> Option<RadioOperation> {
    let mut bytes = [0; 300];
    let length = serialise_host_packet(command, &mut bytes).unwrap();
    host_interface
        .handle_command(&bytes[1..length], 0, transmit_buffer)
        .unwrap()
}

fn next_event(host_interface: &mut HostInterface) -> Vec<u8> {
    let mut buffer = [0; MAXIMUM_HOST_PACKET_LENGTH];
    let (kind, length) = host_interface
        .next_packet_for_host(&mut buffer)
        .expect("an event is waiting");
    assert_eq!(kind, PacketKind::Event);
    buffer[..length].to_vec()
}

fn command_result(host_interface: &mut HostInterface) -> (u16, Status, Vec<u8>) {
    let bytes = next_event(host_interface);
    let event = EventPacket::from_hci_bytes_complete(&bytes).unwrap();
    assert!(matches!(event.kind, EventKind::CommandComplete));
    let complete = CommandComplete::from_hci_bytes_complete(event.data).unwrap();
    let complete: CommandCompleteWithStatus = complete.try_into().unwrap();
    (
        complete.cmd_opcode.to_raw(),
        complete.status,
        complete.return_param_bytes.as_ref().to_vec(),
    )
}

#[test]
fn reset_completes_successfully() {
    let mut transmit_buffer = transmit_buffer();
    let mut host_interface = host_interface();

    let operation = run_command(
        &mut host_interface,
        &bt_hci::cmd::controller_baseband::Reset::new(),
        &mut transmit_buffer,
    );

    assert_eq!(operation, Some(RadioOperation::Off));
    let (opcode, status, _) = command_result(&mut host_interface);
    assert_eq!(opcode, 0x0C03);
    assert_eq!(status, Status::SUCCESS);
}

#[test]
fn buffer_size_matches_the_default_data_length_and_queue() {
    let mut transmit_buffer = transmit_buffer();
    let mut host_interface = host_interface();

    run_command(
        &mut host_interface,
        &bt_hci::cmd::le::LeReadBufferSize::new(),
        &mut transmit_buffer,
    );

    let (_, status, returned) = command_result(&mut host_interface);
    assert_eq!(status, Status::SUCCESS);
    assert_eq!(returned, [27, 0, OUTGOING_QUEUE_LENGTH as u8]);
}

#[test]
fn unknown_commands_are_refused_by_status() {
    let mut transmit_buffer = transmit_buffer();
    let mut host_interface = host_interface();

    run_command(
        &mut host_interface,
        &bt_hci::cmd::le::LeSetScanEnable::new(true, false),
        &mut transmit_buffer,
    );

    let (opcode, status, _) = command_result(&mut host_interface);
    assert_eq!(opcode, 0x200C);
    assert_eq!(status, Status::UNKNOWN_CMD);
}

#[test]
fn advertising_commands_start_the_radio_with_the_hosts_data() {
    use bt_hci::cmd::le::{LeSetAdvData, LeSetAdvEnable, LeSetAdvParams};
    use bt_hci::param::{AddrKind, AdvChannelMap, AdvFilterPolicy, AdvKind, BdAddr, Duration};

    let mut transmit_buffer = transmit_buffer();
    let mut host_interface = host_interface();
    let random_address = [1, 2, 3, 4, 5, 0xC0];
    let mut advertising_data = [0; 31];
    advertising_data[..3].copy_from_slice(&[2, 1, 6]);

    run_command(
        &mut host_interface,
        &bt_hci::cmd::le::LeSetRandomAddr::new(BdAddr::new(random_address)),
        &mut transmit_buffer,
    );
    run_command(
        &mut host_interface,
        &LeSetAdvParams::new(
            Duration::from_millis(100),
            Duration::from_millis(100),
            AdvKind::AdvInd,
            AddrKind::RANDOM,
            AddrKind::PUBLIC,
            BdAddr::default(),
            AdvChannelMap::ALL,
            AdvFilterPolicy::default(),
        ),
        &mut transmit_buffer,
    );
    run_command(
        &mut host_interface,
        &LeSetAdvData::new(3, advertising_data),
        &mut transmit_buffer,
    );
    let operation = run_command(
        &mut host_interface,
        &LeSetAdvEnable::new(true),
        &mut transmit_buffer,
    );

    for _ in 0..4 {
        assert_eq!(command_result(&mut host_interface).1, Status::SUCCESS);
    }
    assert!(matches!(operation, Some(RadioOperation::Transmit { channel, .. }) if channel.index == 37));
    assert_eq!(&transmit_buffer[2..8], &random_address);
    assert_eq!(&transmit_buffer[8..11], &[2, 1, 6]);
}

#[test]
fn disconnect_without_a_connection_reports_an_unknown_connection() {
    use bt_hci::cmd::link_control::Disconnect;
    use bt_hci::param::{ConnHandle, DisconnectReason};

    let mut transmit_buffer = transmit_buffer();
    let mut host_interface = host_interface();

    run_command(
        &mut host_interface,
        &Disconnect::new(ConnHandle::new(0), DisconnectReason::RemoteUserTerminatedConn),
        &mut transmit_buffer,
    );

    let bytes = next_event(&mut host_interface);
    let event = EventPacket::from_hci_bytes_complete(&bytes).unwrap();
    let status = CommandStatus::from_hci_bytes_complete(event.data).unwrap();
    assert_eq!(status.status, Status::UNKNOWN_CONN_IDENTIFIER);
}

// ============================================================================
// Host interface: events and data
// ============================================================================

#[test]
fn a_connection_is_reported_as_le_connection_complete_once_le_events_are_on() {
    use bt_hci::cmd::controller_baseband::SetEventMask;
    use bt_hci::param::EventMask;

    let mut transmit_buffer = transmit_buffer();
    let mut host_interface = host_interface();
    let le_events_on = SetEventMask::new(EventMask::new().enable_le_meta(true));
    run_command(&mut host_interface, &le_events_on, &mut transmit_buffer);
    command_result(&mut host_interface);
    host_interface
        .link_layer
        .events
        .push_back(LinkEvent::Connected {
            peer: DeviceAddress {
                is_random: true,
                bytes: CENTRAL_ADDRESS,
            },
            interval: INTERVAL_IN_UNITS,
            latency: 0,
            timeout: TIMEOUT_IN_UNITS,
            clock_accuracy: 5,
        })
        .unwrap();

    let bytes = next_event(&mut host_interface);
    let event = EventPacket::from_hci_bytes_complete(&bytes).unwrap();
    assert!(matches!(event.kind, EventKind::Le));
    let le_event = LeEventPacket::from_hci_bytes_complete(event.data).unwrap();
    assert!(matches!(le_event.kind, LeEventKind::LeConnectionComplete));
    let complete = LeConnectionComplete::from_hci_bytes_complete(le_event.data).unwrap();

    assert_eq!(complete.status, Status::SUCCESS);
    assert_eq!(complete.handle.raw(), CONNECTION_HANDLE);
    assert_eq!(complete.peer_addr.raw(), &CENTRAL_ADDRESS);
    assert_eq!(
        complete.conn_interval.as_micros(),
        INTERVAL_IN_MICROSECONDS as u64
    );
    assert_eq!(complete.supervision_timeout.as_millis(), 1_000);
}

#[test]
fn received_data_reaches_the_host_as_acl_packets() {
    let mut host_interface = host_interface();
    host_interface
        .link_layer
        .events
        .push_back(LinkEvent::Received(DataPacket::new(DataKind::Start, b"abc")))
        .unwrap();
    host_interface
        .link_layer
        .events
        .push_back(LinkEvent::Received(DataPacket::new(
            DataKind::Continuation,
            b"de",
        )))
        .unwrap();

    let mut buffer = [0; MAXIMUM_HOST_PACKET_LENGTH];
    let (first_kind, first_length) = host_interface.next_packet_for_host(&mut buffer).unwrap();
    let first = buffer[..first_length].to_vec();
    let (_, second_length) = host_interface.next_packet_for_host(&mut buffer).unwrap();
    let second = buffer[..second_length].to_vec();

    assert_eq!(first_kind, PacketKind::AclData);
    assert_eq!(first, [0x00, 0x20, 3, 0, b'a', b'b', b'c']);
    assert_eq!(second, [0x00, 0x10, 2, 0, b'd', b'e']);
}

#[test]
fn acknowledged_packets_are_reported_to_the_host() {
    let mut host_interface = host_interface();
    host_interface.link_layer.completed_packets = 2;

    let bytes = next_event(&mut host_interface);

    assert_eq!(bytes, [0x13, 5, 1, 0, 0, 2, 0]);
    assert_eq!(host_interface.link_layer.completed_packets, 0);
}

#[test]
fn host_data_longer_than_the_data_length_is_refused() {
    let mut host_interface = host_interface();
    let mut packet = vec![0x00, 0x00, 28, 0];
    packet.extend_from_slice(&[0; 28]);

    assert_eq!(
        host_interface.handle_acl_data(&packet),
        Err(HostPacketError::TooLong)
    );
}

// ============================================================================
// TrouBLE end to end, on a fake radio
// ============================================================================

mod end_to_end {
    use core::cell::{Cell, RefCell};

    use bt_hci::controller::ExternalController;
    use embassy_futures::block_on;
    use embassy_futures::select::{select, Either};
    use trouble_host::prelude::*;

    use super::{
        connect_request, fake_random_bytes, plenty_of_host_packets, vec, CentralPacket, Vec, DEFAULT_CONNECT,
        OWN_ADDRESS,
    };
    use crate::controller::{BluetoothController, ControllerState, Radio, RadioEvent};
    use crate::host_interface::HostInterface;
    use crate::link_layer::{disconnect_reason, RadioOperation};
    use crate::packets::*;

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

    /// Plays the central: hears one advertisement and answers it with a connect request.
    fn central_connects(state: &ControllerState, radio: &FakeRadio) {
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

        let packet = connect_request(&DEFAULT_CONNECT);
        let end_time = advertisement_end + 150 + 8 * packet.len() as u32;
        let event = RadioEvent::Received {
            packet: &packet,
            crc_is_valid: true,
            address_time: end_time - 300,
            end_time,
            signal_strength: -40,
        };
        state.on_radio_event(&radio, event);
    }

    /// Plays the central: ends the connection, then lets the peripheral's acknowledgement go out.
    fn central_terminates(state: &ControllerState, radio: &FakeRadio) {
        let RadioOperation::Receive { start_at, .. } = radio.last_operation() else {
            panic!("not listening")
        };
        let terminate = [TERMINATE_INDICATION, disconnect_reason::REMOTE_USER_TERMINATED];
        let packet = CentralPacket {
            kind: DataKind::Control,
            sequence_number: false,
            next_expected_sequence_number: false,
            more_data: false,
            payload: &terminate,
        }
        .bytes();
        let address_time = start_at + 200;
        let event = RadioEvent::Received {
            packet: &packet,
            crc_is_valid: true,
            address_time,
            end_time: address_time + 80,
            signal_strength: -40,
        };

        state.on_radio_event(&radio, event);
        state.on_radio_event(
            &radio,
            RadioEvent::Transmitted {
                end_time: address_time + 500,
            },
        );
    }

    #[test]
    fn trouble_advertises_accepts_a_connection_and_sees_the_central_leave() {
        let radio = FakeRadio::new();
        let state = ControllerState::new();
        state.start(
            HostInterface::new(OWN_ADDRESS, fake_random_bytes),
            plenty_of_host_packets,
        );
        let controller: ExternalController<_, 4> =
            ExternalController::new(BluetoothController::new(&state, &radio));
        let mut resources: HostResources<DefaultPacketPool, 1, 2> = HostResources::new();
        let stack = trouble_host::new(controller, &mut resources)
            .set_random_address(Address::random(OWN_ADDRESS))
            .build();
        let mut runner = stack.runner();
        let mut peripheral = stack.peripheral();

        let watch = async {
            let advertisement = Advertisement::ConnectableScannableUndirected {
                adv_data: &[2, 1, 6],
                scan_data: &[],
            };
            let advertiser = peripheral
                .advertise(&AdvertisementParameters::default(), advertisement)
                .await
                .unwrap();

            central_connects(&state, &radio);
            let connection = advertiser.accept().await.unwrap();

            central_terminates(&state, &radio);
            loop {
                if let ConnectionEvent::Disconnected { reason } = connection.next().await {
                    break reason;
                }
            }
        };
        let outcome = block_on(select(runner.run(), watch));

        let Either::Second(reason) = outcome else {
            panic!("the host stopped: {outcome:?}")
        };
        assert_eq!(reason.into_inner(), disconnect_reason::REMOTE_USER_TERMINATED);
    }

    // ------------------------------------------------------------------------
    // A GATT read, through TrouBLE's attribute server
    // ------------------------------------------------------------------------

    const BATTERY_LEVEL: u8 = 57;
    const ATTRIBUTE_PROTOCOL_CHANNEL: u16 = 4;
    const READ_REQUEST: u8 = 0x0A;
    const READ_RESPONSE: u8 = 0x0B;

    #[gatt_server]
    struct Server {
        battery_service: BatteryService,
    }

    #[gatt_service(uuid = service::BATTERY)]
    struct BatteryService {
        #[characteristic(uuid = characteristic::BATTERY_LEVEL, read, value = BATTERY_LEVEL)]
        level: u8,
    }

    /// A central that keeps its own sequence numbers, so it can talk to the peripheral for many packets.
    struct Central<'test> {
        state: &'test ControllerState,
        radio: &'test FakeRadio,
        sequence_number: bool,
        next_expected_sequence_number: bool,
        received: Vec<Vec<u8>>,
    }

    impl Central<'_> {
        /// Sends one packet and records whatever new data the peripheral replied with.
        fn exchange(&mut self, kind: DataKind, payload: &[u8]) -> bool {
            let RadioOperation::Receive { start_at, .. } = self.radio.last_operation() else {
                panic!("not listening")
            };
            let packet = CentralPacket {
                kind,
                sequence_number: self.sequence_number,
                next_expected_sequence_number: self.next_expected_sequence_number,
                more_data: false,
                payload,
            }
            .bytes();
            let address_time = start_at + 150;
            let end_time = address_time + 8 * packet.len() as u32 + 24;
            let event = RadioEvent::Received {
                packet: &packet,
                crc_is_valid: true,
                address_time,
                end_time,
                signal_strength: -40,
            };

            self.state.on_radio_event(&self.radio, event);
            let reply = self.radio.transmit_buffer.borrow();
            let header = DataHeader::parse(&reply[..]).unwrap();
            let acknowledged = header.next_expected_sequence_number != self.sequence_number;
            if acknowledged {
                self.sequence_number = !self.sequence_number;
            }
            if header.sequence_number == self.next_expected_sequence_number {
                self.next_expected_sequence_number = !self.next_expected_sequence_number;
                if header.length > 0 {
                    self.received.push(reply[2..2 + header.length as usize].to_vec());
                }
            }
            drop(reply);
            self.state.on_radio_event(
                &self.radio,
                RadioEvent::Transmitted {
                    end_time: end_time + 400,
                },
            );
            acknowledged
        }
    }

    #[test]
    fn a_central_reads_a_characteristic_through_the_controller() {
        let radio = FakeRadio::new();
        let state = ControllerState::new();
        state.start(
            HostInterface::new(OWN_ADDRESS, fake_random_bytes),
            plenty_of_host_packets,
        );
        let controller: ExternalController<_, 4> =
            ExternalController::new(BluetoothController::new(&state, &radio));
        let mut resources: HostResources<DefaultPacketPool, 1, 2> = HostResources::new();
        let stack = trouble_host::new(controller, &mut resources)
            .set_random_address(Address::random(OWN_ADDRESS))
            .build();
        let mut runner = stack.runner();
        let mut peripheral = stack.peripheral();
        let server = Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
            name: "NeoTime",
            appearance: &appearance::watch::GENERIC_WATCH,
        }))
        .unwrap();
        let level_handle = server.battery_service.level.handle;

        let watch = async {
            let advertisement = Advertisement::ConnectableScannableUndirected {
                adv_data: &[2, 1, 6],
                scan_data: &[],
            };
            let advertiser = peripheral
                .advertise(&AdvertisementParameters::default(), advertisement)
                .await
                .unwrap();
            central_connects(&state, &radio);
            let connection = advertiser
                .accept()
                .await
                .unwrap()
                .with_attribute_server(&server)
                .unwrap();

            let answer_reads = async {
                loop {
                    if let GattConnectionEvent::Gatt { event } = connection.next().await {
                        event.accept().unwrap().send().await;
                    }
                }
            };

            let mut central = Central {
                state: &state,
                radio: &radio,
                sequence_number: false,
                next_expected_sequence_number: false,
                received: Vec::new(),
            };
            let read_level = async {
                let [handle_low, handle_high] = level_handle.to_le_bytes();
                let mut request = Vec::new();
                request.extend_from_slice(&3u16.to_le_bytes());
                request.extend_from_slice(&ATTRIBUTE_PROTOCOL_CHANNEL.to_le_bytes());
                request.extend_from_slice(&[READ_REQUEST, handle_low, handle_high]);

                while !central.exchange(DataKind::Start, &request) {}
                for _ in 0..100 {
                    embassy_futures::yield_now().await;
                    central.exchange(DataKind::Continuation, &[]);
                    if !central.received.is_empty() {
                        break;
                    }
                }
                central.received.clone()
            };

            match select(answer_reads, read_level).await {
                Either::Second(received) => received,
                Either::First(()) => unreachable!(),
            }
        };
        let outcome = block_on(select(runner.run(), watch));

        let Either::Second(received) = outcome else {
            panic!("the host stopped: {outcome:?}")
        };
        let channel_bytes = ATTRIBUTE_PROTOCOL_CHANNEL.to_le_bytes();
        assert_eq!(
            received,
            [vec![
                2,
                0,
                channel_bytes[0],
                channel_bytes[1],
                READ_RESPONSE,
                BATTERY_LEVEL
            ]]
        );
    }

    /// Its own module, since a second GATT server would clash with the first one's generated names.
    mod discovery {
        use super::*;

        // ------------------------------------------------------------------------
        // Characteristic discovery across 16-bit and 128-bit UUIDs
        // ------------------------------------------------------------------------

        const READ_BY_TYPE_REQUEST: u8 = 0x08;
        const READ_BY_TYPE_RESPONSE: u8 = 0x09;
        const CHARACTERISTIC_DECLARATION: u16 = 0x2803;
        const SHORT_DECLARATION_ENTRY: u8 = 2 + 1 + 2 + 2;
        const LONG_DECLARATION_ENTRY: u8 = 2 + 1 + 2 + 16;
        const CONTROL_POINT_UUID: [u8; 16] = [
            0x23, 0xd1, 0xbc, 0xea, 0x5f, 0x78, 0x23, 0x15, 0xde, 0xef, 0x12, 0x12, 0x31, 0x15, 0x00, 0x00,
        ];

        #[gatt_server]
        struct DiscoveryServer {
            update_service: UpdateService,
        }

        /// The watch's layout: the GAP service's two 16-bit characteristics, then a 128-bit one.
        #[gatt_service(uuid = "00001530-1212-efde-1523-785feabcd123")]
        struct UpdateService {
            #[characteristic(uuid = "00001531-1212-efde-1523-785feabcd123", write, notify)]
            control_point: [u8; 20],
            #[characteristic(uuid = "00001534-1212-efde-1523-785feabcd123", read, value = [8, 0])]
            version: [u8; 2],
        }

        fn read_by_type_request(start: u16) -> Vec<u8> {
            let mut request = Vec::new();
            request.extend_from_slice(&7u16.to_le_bytes());
            request.extend_from_slice(&ATTRIBUTE_PROTOCOL_CHANNEL.to_le_bytes());
            request.push(READ_BY_TYPE_REQUEST);
            request.extend_from_slice(&start.to_le_bytes());
            request.extend_from_slice(&0xFFFFu16.to_le_bytes());
            request.extend_from_slice(&CHARACTERISTIC_DECLARATION.to_le_bytes());
            request
        }

        #[test]
        fn discovery_never_cuts_a_128_bit_uuid_after_16_bit_ones() {
            let radio = FakeRadio::new();
            let state = ControllerState::new();
            state.start(
                HostInterface::new(OWN_ADDRESS, fake_random_bytes),
                plenty_of_host_packets,
            );
            let controller: ExternalController<_, 4> =
                ExternalController::new(BluetoothController::new(&state, &radio));
            let mut resources: HostResources<DefaultPacketPool, 1, 2> = HostResources::new();
            let stack = trouble_host::new(controller, &mut resources)
                .set_random_address(Address::random(OWN_ADDRESS))
                .build();
            let mut runner = stack.runner();
            let mut peripheral = stack.peripheral();
            let server = DiscoveryServer::new_with_config(GapConfig::Peripheral(PeripheralConfig {
                name: "NeoTime",
                appearance: &appearance::watch::GENERIC_WATCH,
            }))
            .unwrap();
            let control_point_declaration = server.update_service.control_point.handle - 1;

            let watch = async {
                let advertisement = Advertisement::ConnectableScannableUndirected {
                    adv_data: &[2, 1, 6],
                    scan_data: &[],
                };
                let advertiser = peripheral
                    .advertise(&AdvertisementParameters::default(), advertisement)
                    .await
                    .unwrap();
                central_connects(&state, &radio);
                let connection = advertiser
                    .accept()
                    .await
                    .unwrap()
                    .with_attribute_server(&server)
                    .unwrap();

                let answer_requests = async {
                    loop {
                        if let GattConnectionEvent::Gatt { event } = connection.next().await {
                            event.accept().unwrap().send().await;
                        }
                    }
                };

                let mut central = Central {
                    state: &state,
                    radio: &radio,
                    sequence_number: false,
                    next_expected_sequence_number: false,
                    received: Vec::new(),
                };
                let discover = async {
                    let mut responses = Vec::new();
                    let mut start = 0x0001;
                    for _ in 0..2 {
                        central.received.clear();
                        while !central.exchange(DataKind::Start, &read_by_type_request(start)) {}
                        for _ in 0..100 {
                            embassy_futures::yield_now().await;
                            central.exchange(DataKind::Continuation, &[]);
                            if !central.received.is_empty() {
                                break;
                            }
                        }
                        let response = central.received.last().cloned().unwrap_or_default();
                        let entry_length = response.get(5).copied().unwrap_or(1).max(1) as usize;
                        let last_entry = response.len().saturating_sub(entry_length);
                        let last_handle =
                            u16::from_le_bytes([response[last_entry], response[last_entry + 1]]);
                        responses.push(response);
                        start = last_handle + 1;
                    }
                    responses
                };

                match select(answer_requests, discover).await {
                    Either::Second(responses) => responses,
                    Either::First(()) => unreachable!(),
                }
            };
            let outcome = block_on(select(runner.run(), watch));

            let Either::Second(responses) = outcome else {
                panic!("the host stopped: {outcome:?}")
            };
            let first = &responses[0];
            assert_eq!(first[4], READ_BY_TYPE_RESPONSE);
            assert_eq!(first[5], SHORT_DECLARATION_ENTRY);
            assert_eq!(
                (first.len() - 6) / SHORT_DECLARATION_ENTRY as usize,
                2,
                "the 16-bit response holds only the GAP service's two characteristics"
            );
            let second = &responses[1];
            assert_eq!(second[4], READ_BY_TYPE_RESPONSE);
            assert_eq!(second[5], LONG_DECLARATION_ENTRY);
            assert_eq!(
                u16::from_le_bytes([second[6], second[7]]),
                control_point_declaration
            );
            assert_eq!(second[11..27], CONTROL_POINT_UUID);
        }
    }

    /// Its own module, since each GATT server's generated names would clash with another's.
    mod transcript {
        extern crate std;

        use super::*;

        // ------------------------------------------------------------------------
        // Every ATT answer from a server laid out as the watch's
        // ------------------------------------------------------------------------

        const READ_BY_GROUP_TYPE_REQUEST: u8 = 0x10;
        const READ_BY_TYPE_REQUEST: u8 = 0x08;
        const FIND_INFORMATION_REQUEST: u8 = 0x04;
        const FIND_BY_TYPE_VALUE_REQUEST: u8 = 0x06;
        const READ_REQUEST: u8 = 0x0A;
        const READ_BLOB_REQUEST: u8 = 0x0C;
        const WRITE_REQUEST: u8 = 0x12;
        const WRITE_COMMAND: u8 = 0x52;
        const PREPARE_WRITE_REQUEST: u8 = 0x16;
        const PRIMARY_SERVICE: u16 = 0x2800;
        const CHARACTERISTIC_DECLARATION: u16 = 0x2803;
        const LAST_HANDLE: u16 = 0xFFFF;
        /// Empty packets the central sends after each request, enough for every answer and notification.
        const EXCHANGES_PER_REQUEST: usize = 30;
        const FIRMWARE_VERSION: &str = "1.2.3-test";
        const PROTOCOL_VERSION: [u8; 2] = [8, 0];
        const SERIAL_NUMBER: [u8; 16] = *b"0123456789abcdef";
        const PACKAGES_UUID: [u8; 16] = [
            0x6d, 0x69, 0x54, 0x6f, 0x65, 0x4e, 0x00, 0x80, 0x00, 0x40, 0x00, 0x00, 0x03, 0x00, 0x54, 0x6e,
        ];
        const UART_SERVICE_UUID: [u8; 16] = [
            0x9e, 0xca, 0xdc, 0x24, 0x0e, 0xe5, 0xa9, 0xe0, 0x93, 0xf3, 0xa3, 0xb5, 0x01, 0x00, 0x40, 0x6e,
        ];

        /// The watch's services, with every kind of characteristic and descriptor the macros make.
        #[gatt_server]
        struct TranscriptServer {
            update: UpdateService,
            time: TimeService,
            neotime: NeoTimeService,
            uart: UartService,
            battery: BatteryService,
            information: InformationService,
            alert: AlertService,
        }

        #[gatt_service(uuid = "00001530-1212-efde-1523-785feabcd123")]
        struct UpdateService {
            #[characteristic(uuid = "00001531-1212-efde-1523-785feabcd123", write, notify)]
            control_point: [u8; 20],
            #[characteristic(uuid = "00001532-1212-efde-1523-785feabcd123", write_without_response)]
            packet: [u8; 20],
            #[characteristic(uuid = "00001534-1212-efde-1523-785feabcd123", read, value = &PROTOCOL_VERSION)]
            version: &'static [u8; 2],
        }

        #[gatt_service(uuid = "1805")]
        struct TimeService {
            #[characteristic(uuid = "2a2b", read, write)]
            current_time: [u8; 10],
            #[characteristic(uuid = "2a0f", read, write)]
            local_time_information: [u8; 2],
        }

        #[gatt_service(uuid = "6e540001-0000-4000-8000-4e656f54696d")]
        struct NeoTimeService {
            #[characteristic(uuid = "6e540002-0000-4000-8000-4e656f54696d", read, write, value = [7; 30])]
            counters: [u8; 30],
            #[characteristic(
                uuid = "6e540003-0000-4000-8000-4e656f54696d",
                write,
                write_without_response,
                notify
            )]
            packages: [u8; 20],
            #[characteristic(uuid = "adaf0100-4669-6c65-5472-616e73666572", read, value = [4, 0, 0, 0])]
            files_version: [u8; 4],
        }

        #[gatt_service(uuid = "6e400001-b5a3-f393-e0a9-e50e24dcca9e")]
        struct UartService {
            #[characteristic(uuid = "6e400002-b5a3-f393-e0a9-e50e24dcca9e", write, write_without_response)]
            receive: [u8; 20],
            #[characteristic(uuid = "6e400003-b5a3-f393-e0a9-e50e24dcca9e", notify)]
            transmit: [u8; 20],
        }

        #[gatt_service(uuid = service::BATTERY)]
        struct BatteryService {
            #[descriptor(uuid = descriptors::CHARACTERISTIC_USER_DESCRIPTION, read, type = &'static str, value = "Charge")]
            #[descriptor(uuid = "2904", name = "format", read, write, type = [u8; 7], value = [4, 0, 0xAD, 0x27, 1, 0, 0])]
            #[descriptor(uuid = "2a20", read, value = "Demo")]
            #[characteristic(uuid = characteristic::BATTERY_LEVEL, read, notify, value = 57)]
            level: u8,
        }

        #[gatt_service(uuid = "180a")]
        struct InformationService {
            #[characteristic(uuid = "2a25", read)]
            serial_number: [u8; 16],
            #[characteristic(uuid = "2a26", read, value = FIRMWARE_VERSION)]
            firmware_revision: &'static str,
        }

        #[gatt_service(uuid = "1811")]
        struct AlertService {
            #[characteristic(uuid = "2a46", write)]
            new_alert: u8,
        }

        fn request(attribute_protocol: &[u8]) -> Vec<u8> {
            let mut request = Vec::new();
            request.extend_from_slice(&(attribute_protocol.len() as u16).to_le_bytes());
            request.extend_from_slice(&ATTRIBUTE_PROTOCOL_CHANNEL.to_le_bytes());
            request.extend_from_slice(attribute_protocol);
            request
        }

        fn with_handle(opcode: u8, handle: u16, rest: &[u8]) -> Vec<u8> {
            let mut pdu = vec![opcode];
            pdu.extend_from_slice(&handle.to_le_bytes());
            pdu.extend_from_slice(rest);
            request(&pdu)
        }

        fn from_handle(opcode: u8, start: u16, rest: &[u8]) -> Vec<u8> {
            let mut range = LAST_HANDLE.to_le_bytes().to_vec();
            range.extend_from_slice(rest);
            with_handle(opcode, start, &range)
        }

        /// Discovery from every handle, then reads, writes, a subscription and a notification, then reads again.
        fn requests(server: &TranscriptServer<'_>) -> Vec<Vec<u8>> {
            let last_handle = server.alert.new_alert.handle + 2;
            let battery_cccd = server.battery.level.cccd_handle.unwrap();
            let transmit_cccd = server.uart.transmit.cccd_handle.unwrap();
            let format = server.battery.level_format_descriptor.handle();
            let mut requests = Vec::new();
            for start in 1..=last_handle {
                requests.push(from_handle(
                    READ_BY_GROUP_TYPE_REQUEST,
                    start,
                    &PRIMARY_SERVICE.to_le_bytes(),
                ));
                requests.push(from_handle(
                    READ_BY_TYPE_REQUEST,
                    start,
                    &CHARACTERISTIC_DECLARATION.to_le_bytes(),
                ));
                requests.push(from_handle(FIND_INFORMATION_REQUEST, start, &[]));
            }
            let mut reads = Vec::new();
            for handle in 0..=last_handle {
                reads.push(with_handle(READ_REQUEST, handle, &[]));
                reads.push(with_handle(READ_BLOB_REQUEST, handle, &1u16.to_le_bytes()));
            }
            requests.extend(reads.iter().cloned());
            requests.push(from_handle(READ_BY_TYPE_REQUEST, 1, &PACKAGES_UUID));
            requests.push(from_handle(
                READ_BY_TYPE_REQUEST,
                1,
                &characteristic::BATTERY_LEVEL.to_le_bytes(),
            ));
            let mut battery = PRIMARY_SERVICE.to_le_bytes().to_vec();
            battery.extend_from_slice(&service::BATTERY.to_le_bytes());
            requests.push(from_handle(FIND_BY_TYPE_VALUE_REQUEST, 1, &battery));
            let mut uart = PRIMARY_SERVICE.to_le_bytes().to_vec();
            uart.extend_from_slice(&UART_SERVICE_UUID);
            requests.push(from_handle(FIND_BY_TYPE_VALUE_REQUEST, 1, &uart));

            let current_time = server.time.current_time.handle;
            requests.push(with_handle(
                WRITE_REQUEST,
                current_time,
                &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            ));
            requests.push(with_handle(WRITE_REQUEST, current_time, &[0xEE; 11]));
            requests.push(with_handle(
                WRITE_REQUEST,
                server.time.local_time_information.handle,
                &[0x42],
            ));
            requests.push(with_handle(
                WRITE_REQUEST,
                server.information.firmware_revision.handle,
                b"x",
            ));
            requests.push(with_handle(
                WRITE_REQUEST,
                server.information.firmware_revision.handle - 1,
                b"x",
            ));
            requests.push(with_handle(WRITE_REQUEST, server.alert.new_alert.handle, &[3]));
            requests.push(with_handle(WRITE_REQUEST, server.alert.new_alert.handle, &[3, 4]));
            requests.push(with_handle(
                WRITE_COMMAND,
                server.update.packet.handle,
                &[0x5A; 20],
            ));
            requests.push(with_handle(
                WRITE_REQUEST,
                server.neotime.counters.handle,
                &[9; 20],
            ));
            requests.push(with_handle(WRITE_REQUEST, format, &[1, 2, 3, 4, 5, 6, 7]));
            requests.push(with_handle(WRITE_REQUEST, format + 1, b"New"));
            requests.push(with_handle(WRITE_REQUEST, format - 1, b"x"));
            requests.push(with_handle(WRITE_REQUEST, battery_cccd, &[1, 0]));
            requests.push(with_handle(WRITE_REQUEST, transmit_cccd, &[1, 0]));
            requests.push(with_handle(WRITE_REQUEST, server.uart.receive.handle, b"hello"));
            let mut prepare = 0u16.to_le_bytes().to_vec();
            prepare.extend_from_slice(b"ab");
            requests.push(with_handle(PREPARE_WRITE_REQUEST, current_time, &prepare));
            for offset in [0u16, 22, 30, 31] {
                requests.push(with_handle(
                    READ_BLOB_REQUEST,
                    server.neotime.counters.handle,
                    &offset.to_le_bytes(),
                ));
            }
            requests.extend(reads);
            requests
        }

        /// Plays the requests through the controller to the server and returns every packet the watch sent.
        fn transcript() -> Vec<Vec<u8>> {
            let radio = FakeRadio::new();
            let state = ControllerState::new();
            state.start(
                HostInterface::new(OWN_ADDRESS, fake_random_bytes),
                plenty_of_host_packets,
            );
            let controller: ExternalController<_, 4> =
                ExternalController::new(BluetoothController::new(&state, &radio));
            let mut resources: HostResources<DefaultPacketPool, 1, 2> = HostResources::new();
            let stack = trouble_host::new(controller, &mut resources)
                .set_random_address(Address::random(OWN_ADDRESS))
                .build();
            let mut runner = stack.runner();
            let mut peripheral = stack.peripheral();
            let server = TranscriptServer::new_with_config(GapConfig::Peripheral(PeripheralConfig {
                name: "NeoTime",
                appearance: &appearance::watch::GENERIC_WATCH,
            }))
            .unwrap();
            server
                .set(&server.information.serial_number, &SERIAL_NUMBER)
                .unwrap();
            let requests = requests(&server);
            let receive = server.uart.receive.handle;

            let watch = async {
                let advertisement = Advertisement::ConnectableScannableUndirected {
                    adv_data: &[2, 1, 6],
                    scan_data: &[],
                };
                let advertiser = peripheral
                    .advertise(&AdvertisementParameters::default(), advertisement)
                    .await
                    .unwrap();
                central_connects(&state, &radio);
                let connection = advertiser
                    .accept()
                    .await
                    .unwrap()
                    .with_attribute_server(&server)
                    .unwrap();

                let answer_requests = async {
                    loop {
                        if let GattConnectionEvent::Gatt { event } = connection.next().await {
                            let echo = matches!(&event, GattEvent::Write(write) if write.handle() == receive);
                            event.accept().unwrap().send().await;
                            if echo {
                                server
                                    .uart
                                    .transmit
                                    .notify(&connection, &[0x21; 20], false)
                                    .await
                                    .unwrap();
                            }
                        }
                    }
                };

                let mut central = Central {
                    state: &state,
                    radio: &radio,
                    sequence_number: false,
                    next_expected_sequence_number: false,
                    received: Vec::new(),
                };
                let play = async {
                    for request in &requests {
                        while !central.exchange(DataKind::Start, request) {}
                        for _ in 0..EXCHANGES_PER_REQUEST {
                            embassy_futures::yield_now().await;
                            central.exchange(DataKind::Continuation, &[]);
                        }
                    }
                    central.received.clone()
                };

                match select(answer_requests, play).await {
                    Either::Second(received) => received,
                    Either::First(()) => unreachable!(),
                }
            };
            let outcome = block_on(select(runner.run(), watch));
            let Either::Second(received) = outcome else {
                panic!("the host stopped: {outcome:?}")
            };
            received
        }

        fn hex(packets: &[Vec<u8>]) -> std::string::String {
            use core::fmt::Write;
            let mut text = std::string::String::new();
            for packet in packets {
                for byte in packet {
                    write!(text, "{byte:02x}").unwrap();
                }
                text.push('\n');
            }
            text
        }

        #[test]
        fn every_request_gets_the_same_bytes_as_from_a_table_built_at_start_up() {
            let received = transcript();

            let expected = include_str!("gatt_transcript.txt");
            let actual = hex(&received);
            if std::env::var_os("NEOTIME_UPDATE_SNAPSHOTS").is_some() {
                std::fs::write(
                    concat!(env!("CARGO_MANIFEST_DIR"), "/src/gatt_transcript.txt"),
                    &actual,
                )
                .unwrap();
            }
            assert_eq!(actual, expected);
        }
    }

    /// Its own module, since each GATT server's generated names would clash with another's.
    mod split {
        use super::*;

        // ------------------------------------------------------------------------
        // The table in flash and the values that change in RAM
        // ------------------------------------------------------------------------

        const NAME: &str = "NeoTime";

        #[gatt_server]
        struct SplitServer {
            values: SplitService,
        }

        #[gatt_service(uuid = "6e540001-0000-4000-8000-4e656f54696d")]
        struct SplitService {
            #[characteristic(uuid = "2a19", read, write, notify, value = 5)]
            small: u8,
            #[characteristic(uuid = "6e540002-0000-4000-8000-4e656f54696d", read, write, value = [1, 2, 3])]
            bytes: [u8; 3],
            #[characteristic(uuid = "2a29", read, value = NAME)]
            name: &'static str,
        }

        #[test]
        fn only_the_values_that_change_take_ram() {
            let fixed_byte = 1;
            let three_bytes_and_their_length = 3 + 2;

            assert_eq!(
                __SPLITSERVER_STORED_BYTES,
                fixed_byte + three_bytes_and_their_length
            );
            assert_eq!(
                __SPLITSERVER_ATTRIBUTES.len(),
                trouble_host::gap::GAP_SERVICE_ATTRIBUTE_COUNT + SplitService::ATTRIBUTE_COUNT
            );
            assert_eq!(SplitService::ATTRIBUTE_COUNT, 1 + 3 + 2 + 2);
            assert_eq!(SplitService::CCCD_COUNT, 1);
        }

        #[test]
        fn stored_values_change_while_constants_and_declarations_refuse_to() {
            let server = SplitServer::new_default("NeoTime").unwrap();
            let values = &server.values;

            server.set(&values.small, &9).unwrap();
            server.set(&values.bytes, &[7, 8, 9]).unwrap();
            let refused_constant = server.set(&values.name, NAME);
            let refused_length = server.server.table().write(values.small.handle, 0, &[1, 2]);
            let refused_declaration = server.server.table().write(values.name.handle - 1, 0, &[0]);

            assert_eq!(server.get(&values.small).unwrap(), 9);
            assert_eq!(server.get(&values.bytes).unwrap(), [7, 8, 9]);
            assert!(refused_constant.is_err());
            assert!(refused_length.is_err());
            assert!(refused_declaration.is_err());
            assert_eq!(
                values.name.with_raw_value(&server, |value| value.to_vec()),
                Some(NAME.as_bytes().to_vec())
            );
            assert_eq!(values.small.end_handle, values.small.cccd_handle.unwrap());
            assert_eq!(values.bytes.end_handle, values.bytes.handle);
        }
    }

    /// Its own module, since each GATT server's generated names would clash with another's.
    mod built_once {
        use super::*;

        #[gatt_server]
        struct OnceServer {
            battery: OnceBatteryService,
        }

        #[gatt_service(uuid = service::BATTERY)]
        struct OnceBatteryService {
            #[characteristic(uuid = characteristic::BATTERY_LEVEL, read, notify)]
            level: u8,
        }

        #[test]
        #[should_panic]
        fn a_server_can_be_built_only_once_since_its_values_are_a_static() {
            let first = OnceServer::new_default("NeoTime").unwrap();
            assert!(first.battery.level.cccd_handle.is_some());

            let _second = OnceServer::new_default("NeoTime");
        }
    }
}
