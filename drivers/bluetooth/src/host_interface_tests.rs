extern crate std;

use std::vec;
use std::vec::Vec;

use super::*;

// ============================================================================
// A host that sends raw command bytes
// ============================================================================

const OWN_ADDRESS: [u8; 6] = [0x11, 0x22, 0x33, 0x44, 0x55, 0xC6];
const CENTRAL_ADDRESS: [u8; 6] = [0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6];
const OTHER_ADDRESS: [u8; 6] = [0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xF6];
const RANDOM_ADDRESS: [u8; 6] = [1, 2, 3, 4, 5, 0xC0];
const ALL_ADVERTISING_CHANNELS: u8 = 0b111;
const PUBLIC_ADDRESS_TYPE: u8 = 0;
const RANDOM_ADDRESS_TYPE: u8 = 1;
const CONNECTABLE_KIND: u8 = 0x00;
const ONE_HUNDRED_MILLISECONDS_IN_UNITS: u16 = 160;

fn fake_random_bytes(destination: &mut [u8]) {
    for (index, byte) in destination.iter_mut().enumerate() {
        *byte = index as u8 + 1;
    }
}

fn host_interface() -> HostInterface {
    HostInterface::new(OWN_ADDRESS, fake_random_bytes)
}

fn transmit_buffer() -> [u8; RADIO_BUFFER_LENGTH] {
    [0; RADIO_BUFFER_LENGTH]
}

/// Builds a command packet without its indicator byte.
fn command_packet(command_opcode: u16, parameters: &[u8]) -> Vec<u8> {
    let mut packet = command_opcode.to_le_bytes().to_vec();
    packet.push(parameters.len() as u8);
    packet.extend_from_slice(parameters);
    packet
}

fn send_command(
    host_interface: &mut HostInterface,
    command_opcode: u16,
    parameters: &[u8],
) -> Option<RadioOperation> {
    let mut transmit_buffer = transmit_buffer();
    host_interface
        .handle_command(
            &command_packet(command_opcode, parameters),
            0,
            &mut transmit_buffer,
        )
        .unwrap()
}

fn next_packet(host_interface: &mut HostInterface) -> Option<(PacketKind, Vec<u8>)> {
    let mut buffer = [0; MAXIMUM_HOST_PACKET_LENGTH];
    host_interface
        .next_packet_for_host(&mut buffer)
        .map(|(kind, length)| (kind, buffer[..length].to_vec()))
}

fn next_event(host_interface: &mut HostInterface) -> Vec<u8> {
    let (kind, bytes) = next_packet(host_interface).expect("an event is waiting");
    assert_eq!(kind, PacketKind::Event);
    bytes
}

/// Reads a Command Complete event and gives its opcode and return parameters, status first.
fn command_complete(host_interface: &mut HostInterface) -> (u16, Vec<u8>) {
    let bytes = next_event(host_interface);
    assert_eq!(bytes[0], event_code::COMMAND_COMPLETE);
    assert_eq!(bytes[1] as usize, bytes.len() - 2);
    assert_eq!(bytes[2], 1, "the host may send one more command");
    (u16::from_le_bytes([bytes[3], bytes[4]]), bytes[5..].to_vec())
}

fn status_of_completed(host_interface: &mut HostInterface, command_opcode: u16) -> u8 {
    let (completed_opcode, return_parameters) = command_complete(host_interface);
    assert_eq!(completed_opcode, command_opcode);
    return_parameters[0]
}

/// Reads a Command Status event and gives its opcode and status.
fn command_status(host_interface: &mut HostInterface) -> (u16, u8) {
    let bytes = next_event(host_interface);
    assert_eq!(bytes[0], event_code::COMMAND_STATUS);
    assert_eq!(bytes[1], 4);
    (u16::from_le_bytes([bytes[4], bytes[5]]), bytes[2])
}

struct AdvertisingParameters {
    interval_in_units: u16,
    kind: u8,
    own_address_type: u8,
    channel_map: u8,
    filter_policy: u8,
}

const CONNECTABLE_ADVERTISING: AdvertisingParameters = AdvertisingParameters {
    interval_in_units: ONE_HUNDRED_MILLISECONDS_IN_UNITS,
    kind: CONNECTABLE_KIND,
    own_address_type: PUBLIC_ADDRESS_TYPE,
    channel_map: ALL_ADVERTISING_CHANNELS,
    filter_policy: 0,
};

fn advertising_parameter_bytes(parameters: &AdvertisingParameters) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&parameters.interval_in_units.to_le_bytes());
    bytes.extend_from_slice(&parameters.interval_in_units.to_le_bytes());
    bytes.push(parameters.kind);
    bytes.push(parameters.own_address_type);
    bytes.push(PUBLIC_ADDRESS_TYPE);
    bytes.extend_from_slice(&[0; ADDRESS_LENGTH]);
    bytes.push(parameters.channel_map);
    bytes.push(parameters.filter_policy);
    bytes
}

fn set_advertising_parameters(host_interface: &mut HostInterface, parameters: &AdvertisingParameters) -> u8 {
    send_command(
        host_interface,
        opcode::LE_SET_ADVERTISING_PARAMETERS,
        &advertising_parameter_bytes(parameters),
    );
    status_of_completed(host_interface, opcode::LE_SET_ADVERTISING_PARAMETERS)
}

fn advertising_host_interface() -> HostInterface {
    let mut host_interface = host_interface();
    set_advertising_parameters(&mut host_interface, &CONNECTABLE_ADVERTISING);
    send_command(&mut host_interface, opcode::LE_SET_ADVERTISING_ENABLE, &[1]);
    assert_eq!(
        status_of_completed(&mut host_interface, opcode::LE_SET_ADVERTISING_ENABLE),
        status::SUCCESS
    );
    host_interface
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

/// A host interface whose link layer has just accepted a central's connect request.
fn connected_host_interface() -> HostInterface {
    let mut host_interface = advertising_host_interface();
    let mut transmit_buffer = transmit_buffer();
    let connect_request_end = 10_000;

    host_interface
        .link_layer
        .on_transmitted(1_000, &mut transmit_buffer);
    host_interface.link_layer.on_received(
        &connect_request(),
        true,
        connect_request_end - 300,
        connect_request_end,
        -40,
        &mut transmit_buffer,
    );

    assert!(host_interface.link_layer.is_connected());
    host_interface.link_layer.events.clear();
    host_interface
}

fn accept_list_entry(address_type: u8, address: [u8; ADDRESS_LENGTH]) -> Vec<u8> {
    let mut parameters = vec![address_type];
    parameters.extend_from_slice(&address);
    parameters
}

fn enable_le_events(host_interface: &mut HostInterface, le_event_mask: u64) {
    send_command(
        host_interface,
        opcode::SET_EVENT_MASK,
        &LE_META_MASK_BIT.to_le_bytes(),
    );
    send_command(
        host_interface,
        opcode::LE_SET_EVENT_MASK,
        &le_event_mask.to_le_bytes(),
    );
    command_complete(host_interface);
    command_complete(host_interface);
}

// ============================================================================
// Command packets
// ============================================================================

#[test]
fn a_command_shorter_than_its_header_is_malformed() {
    let mut host_interface = host_interface();
    let mut transmit_buffer = transmit_buffer();

    let result = host_interface.handle_command(&[0x03, 0x0C], 0, &mut transmit_buffer);

    assert_eq!(result, Err(HostPacketError::Malformed));
    assert!(next_packet(&mut host_interface).is_none());
}

#[test]
fn a_command_shorter_than_its_stated_length_is_malformed() {
    let mut host_interface = host_interface();
    let mut transmit_buffer = transmit_buffer();

    let result = host_interface.handle_command(&[0x01, 0x0C, 8, 0, 0], 0, &mut transmit_buffer);

    assert_eq!(result, Err(HostPacketError::Malformed));
}

#[test]
fn commands_are_refused_once_four_answers_are_waiting() {
    let mut host_interface = host_interface();

    for _ in 0..COMMAND_RESULT_QUEUE_LENGTH {
        assert!(host_interface.can_take_command());
        send_command(&mut host_interface, opcode::HOST_BUFFER_SIZE, &[]);
    }

    assert!(!host_interface.can_take_command());
    command_complete(&mut host_interface);
    assert!(host_interface.can_take_command());
}

#[test]
fn a_full_answer_queue_drops_further_answers() {
    let mut host_interface = host_interface();

    for _ in 0..COMMAND_RESULT_QUEUE_LENGTH + 1 {
        send_command(&mut host_interface, opcode::HOST_BUFFER_SIZE, &[]);
    }

    for _ in 0..COMMAND_RESULT_QUEUE_LENGTH {
        command_complete(&mut host_interface);
    }
    assert!(next_packet(&mut host_interface).is_none());
}

// ============================================================================
// Controller and baseband commands
// ============================================================================

#[test]
fn reset_restores_the_default_event_masks_and_the_device_address() {
    let mut host_interface = host_interface();
    send_command(&mut host_interface, opcode::SET_EVENT_MASK, &0u64.to_le_bytes());
    send_command(
        &mut host_interface,
        opcode::LE_SET_RANDOM_ADDRESS,
        &RANDOM_ADDRESS,
    );
    let random_advertising = AdvertisingParameters {
        own_address_type: RANDOM_ADDRESS_TYPE,
        ..CONNECTABLE_ADVERTISING
    };
    send_command(
        &mut host_interface,
        opcode::LE_SET_ADVERTISING_PARAMETERS,
        &advertising_parameter_bytes(&random_advertising),
    );
    assert_eq!(
        host_interface.link_layer.settings.own_address.bytes,
        RANDOM_ADDRESS
    );
    for _ in 0..3 {
        command_complete(&mut host_interface);
    }

    let operation = send_command(&mut host_interface, opcode::RESET, &[]);

    assert_eq!(operation, Some(RadioOperation::Off));
    assert_eq!(host_interface.link_layer.settings.own_address.bytes, OWN_ADDRESS);
    assert_eq!(
        status_of_completed(&mut host_interface, opcode::RESET),
        status::SUCCESS
    );
    host_interface
        .link_layer
        .events
        .push_back(LinkEvent::Disconnected { reason: 0x13 })
        .unwrap();
    assert_eq!(
        next_event(&mut host_interface)[0],
        event_code::DISCONNECTION_COMPLETE
    );
}

#[test]
fn commands_that_need_no_work_complete_successfully() {
    let mut host_interface = host_interface();
    let commands = [
        opcode::SET_EVENT_MASK_PAGE_2,
        opcode::HOST_BUFFER_SIZE,
        opcode::LE_WRITE_SUGGESTED_DEFAULT_DATA_LENGTH,
    ];

    for command in commands {
        send_command(&mut host_interface, command, &[0; 8]);
    }

    for command in commands {
        assert_eq!(status_of_completed(&mut host_interface, command), status::SUCCESS);
    }
}

#[test]
fn controller_to_host_flow_control_is_accepted_only_when_off() {
    let mut host_interface = host_interface();

    send_command(
        &mut host_interface,
        opcode::SET_CONTROLLER_TO_HOST_FLOW_CONTROL,
        &[0],
    );
    send_command(
        &mut host_interface,
        opcode::SET_CONTROLLER_TO_HOST_FLOW_CONTROL,
        &[1],
    );

    let flow_control = opcode::SET_CONTROLLER_TO_HOST_FLOW_CONTROL;
    assert_eq!(
        status_of_completed(&mut host_interface, flow_control),
        status::SUCCESS
    );
    assert_eq!(
        status_of_completed(&mut host_interface, flow_control),
        status::UNSUPPORTED_FEATURE_OR_VALUE
    );
}

#[test]
fn host_number_of_completed_packets_gets_no_answer() {
    let mut host_interface = host_interface();

    let operation = send_command(
        &mut host_interface,
        opcode::HOST_NUMBER_OF_COMPLETED_PACKETS,
        &[0],
    );

    assert_eq!(operation, None);
    assert!(next_packet(&mut host_interface).is_none());
}

#[test]
fn local_version_reports_bluetooth_5_0_and_no_manufacturer() {
    let mut host_interface = host_interface();

    send_command(&mut host_interface, opcode::READ_LOCAL_VERSION_INFORMATION, &[]);

    let (_, returned) = command_complete(&mut host_interface);
    assert_eq!(returned, [status::SUCCESS, 0x09, 0, 0, 0x09, 0xFF, 0xFF, 0, 0]);
}

#[test]
fn local_features_say_the_controller_is_le_only() {
    let mut host_interface = host_interface();

    send_command(&mut host_interface, opcode::READ_LOCAL_SUPPORTED_FEATURES, &[]);

    let (_, returned) = command_complete(&mut host_interface);
    let le_supported_and_no_br_edr = 0x60;
    assert_eq!(
        returned,
        [status::SUCCESS, 0, 0, 0, 0, le_supported_and_no_br_edr, 0, 0, 0]
    );
}

#[test]
fn there_is_no_public_device_address() {
    let mut host_interface = host_interface();

    send_command(&mut host_interface, opcode::READ_DEVICE_ADDRESS, &[]);

    let (_, returned) = command_complete(&mut host_interface);
    assert_eq!(returned, [status::SUCCESS, 0, 0, 0, 0, 0, 0]);
}

// ============================================================================
// LE information commands
// ============================================================================

#[test]
fn le_features_are_all_off() {
    let mut host_interface = host_interface();

    send_command(&mut host_interface, opcode::LE_READ_LOCAL_SUPPORTED_FEATURES, &[]);

    let (_, returned) = command_complete(&mut host_interface);
    assert_eq!(returned, [status::SUCCESS, 0, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn le_random_returns_eight_bytes_from_the_random_source() {
    let mut host_interface = host_interface();

    send_command(&mut host_interface, opcode::LE_RANDOM, &[]);

    let (_, returned) = command_complete(&mut host_interface);
    assert_eq!(returned, [status::SUCCESS, 1, 2, 3, 4, 5, 6, 7, 8]);
}

#[test]
fn advertising_transmit_power_reads_as_zero_dbm() {
    let mut host_interface = host_interface();

    send_command(
        &mut host_interface,
        opcode::LE_READ_ADVERTISING_TRANSMIT_POWER,
        &[],
    );

    let (_, returned) = command_complete(&mut host_interface);
    assert_eq!(returned, [status::SUCCESS, 0]);
}

#[test]
fn the_physical_layer_is_one_megabit_both_ways() {
    let mut host_interface = host_interface();

    send_command(&mut host_interface, opcode::LE_READ_PHYSICAL_LAYER, &[0, 0]);

    let (_, returned) = command_complete(&mut host_interface);
    assert_eq!(returned, [status::SUCCESS, 0, 0, 1, 1]);
}

#[test]
fn changing_the_data_length_is_unsupported() {
    let mut host_interface = host_interface();

    send_command(
        &mut host_interface,
        opcode::LE_SET_DATA_LENGTH,
        &[0, 0, 251, 0, 0x48, 0x08],
    );

    let (_, returned) = command_complete(&mut host_interface);
    assert_eq!(returned, [status::UNSUPPORTED_FEATURE_OR_VALUE, 0, 0]);
}

#[test]
fn suggested_data_length_is_27_bytes_in_328_microseconds() {
    let mut host_interface = host_interface();

    send_command(
        &mut host_interface,
        opcode::LE_READ_SUGGESTED_DEFAULT_DATA_LENGTH,
        &[],
    );

    let (_, returned) = command_complete(&mut host_interface);
    assert_eq!(returned, [status::SUCCESS, 27, 0, 0x48, 0x01]);
}

#[test]
fn maximum_data_length_is_27_bytes_in_328_microseconds_both_ways() {
    let mut host_interface = host_interface();

    send_command(&mut host_interface, opcode::LE_READ_MAXIMUM_DATA_LENGTH, &[]);

    let (_, returned) = command_complete(&mut host_interface);
    assert_eq!(returned, [status::SUCCESS, 27, 0, 0x48, 0x01, 27, 0, 0x48, 0x01]);
}

// ============================================================================
// Advertising commands
// ============================================================================

#[test]
fn the_random_address_cannot_change_while_advertising() {
    let mut host_interface = advertising_host_interface();

    send_command(
        &mut host_interface,
        opcode::LE_SET_RANDOM_ADDRESS,
        &RANDOM_ADDRESS,
    );

    assert_eq!(
        status_of_completed(&mut host_interface, opcode::LE_SET_RANDOM_ADDRESS),
        status::COMMAND_DISALLOWED
    );
    assert_eq!(host_interface.link_layer.settings.own_address.bytes, OWN_ADDRESS);
}

#[test]
fn a_random_address_shorter_than_six_bytes_is_invalid() {
    let mut host_interface = host_interface();

    send_command(
        &mut host_interface,
        opcode::LE_SET_RANDOM_ADDRESS,
        &RANDOM_ADDRESS[..3],
    );

    assert_eq!(
        status_of_completed(&mut host_interface, opcode::LE_SET_RANDOM_ADDRESS),
        status::INVALID_PARAMETERS
    );
    assert_eq!(host_interface.random_address, None);
}

#[test]
fn a_random_address_is_used_only_once_advertising_asks_for_it() {
    let mut host_interface = host_interface();

    send_command(
        &mut host_interface,
        opcode::LE_SET_RANDOM_ADDRESS,
        &RANDOM_ADDRESS,
    );
    command_complete(&mut host_interface);
    let address_before_asking = host_interface.link_layer.settings.own_address.bytes;
    set_advertising_parameters(
        &mut host_interface,
        &AdvertisingParameters {
            own_address_type: RANDOM_ADDRESS_TYPE,
            ..CONNECTABLE_ADVERTISING
        },
    );
    let address_when_asked = host_interface.link_layer.settings.own_address.bytes;
    set_advertising_parameters(&mut host_interface, &CONNECTABLE_ADVERTISING);

    assert_eq!(address_before_asking, OWN_ADDRESS);
    assert_eq!(address_when_asked, RANDOM_ADDRESS);
    assert_eq!(host_interface.link_layer.settings.own_address.bytes, OWN_ADDRESS);
}

#[test]
fn advertising_parameters_set_the_interval_kind_channels_and_filters() {
    let mut host_interface = host_interface();
    let scannable_kind = 0x02;
    let filter_both = 3;
    let first_two_channels = 0b011;

    let result = set_advertising_parameters(
        &mut host_interface,
        &AdvertisingParameters {
            interval_in_units: 0x0020,
            kind: scannable_kind,
            own_address_type: PUBLIC_ADDRESS_TYPE,
            channel_map: first_two_channels,
            filter_policy: filter_both,
        },
    );

    assert_eq!(result, status::SUCCESS);
    let settings = &host_interface.link_layer.settings;
    assert_eq!(settings.interval_in_microseconds, 20_000);
    assert_eq!(settings.kind, AdvertisingKind::Scannable);
    assert_eq!(settings.channel_map, first_two_channels);
    assert!(settings.filter_scan_requests);
    assert!(settings.filter_connect_requests);
}

#[test]
fn non_connectable_advertising_is_accepted() {
    let mut host_interface = host_interface();
    let non_connectable_kind = 0x03;

    let result = set_advertising_parameters(
        &mut host_interface,
        &AdvertisingParameters {
            kind: non_connectable_kind,
            ..CONNECTABLE_ADVERTISING
        },
    );

    assert_eq!(result, status::SUCCESS);
    assert_eq!(
        host_interface.link_layer.settings.kind,
        AdvertisingKind::NonConnectable
    );
}

#[test]
fn advertising_parameters_shorter_than_fifteen_bytes_are_invalid() {
    let mut host_interface = host_interface();
    let parameters = advertising_parameter_bytes(&CONNECTABLE_ADVERTISING);

    send_command(
        &mut host_interface,
        opcode::LE_SET_ADVERTISING_PARAMETERS,
        &parameters[..14],
    );

    assert_eq!(
        status_of_completed(&mut host_interface, opcode::LE_SET_ADVERTISING_PARAMETERS),
        status::INVALID_PARAMETERS
    );
}

#[test]
fn advertising_parameters_cannot_change_while_advertising() {
    let mut host_interface = advertising_host_interface();

    let result = set_advertising_parameters(&mut host_interface, &CONNECTABLE_ADVERTISING);

    assert_eq!(result, status::COMMAND_DISALLOWED);
}

#[test]
fn directed_advertising_is_unsupported() {
    let mut host_interface = host_interface();
    let mut results = Vec::new();

    for directed_kind in [0x01, 0x04] {
        let parameters = AdvertisingParameters {
            kind: directed_kind,
            ..CONNECTABLE_ADVERTISING
        };
        results.push(set_advertising_parameters(&mut host_interface, &parameters));
    }

    assert_eq!(results, [status::UNSUPPORTED_FEATURE_OR_VALUE; 2]);
}

#[test]
fn out_of_range_advertising_parameters_are_invalid() {
    let mut host_interface = host_interface();
    let unknown_kind = AdvertisingParameters {
        kind: 0x05,
        ..CONNECTABLE_ADVERTISING
    };
    let interval_too_short = AdvertisingParameters {
        interval_in_units: 0x001F,
        ..CONNECTABLE_ADVERTISING
    };
    let interval_too_long = AdvertisingParameters {
        interval_in_units: 0x4001,
        ..CONNECTABLE_ADVERTISING
    };
    let no_channels = AdvertisingParameters {
        channel_map: 0b1000,
        ..CONNECTABLE_ADVERTISING
    };
    let unknown_filter = AdvertisingParameters {
        filter_policy: 4,
        ..CONNECTABLE_ADVERTISING
    };

    let results: Vec<u8> = [
        unknown_kind,
        interval_too_short,
        interval_too_long,
        no_channels,
        unknown_filter,
    ]
    .iter()
    .map(|parameters| set_advertising_parameters(&mut host_interface, parameters))
    .collect();

    assert_eq!(results, [status::INVALID_PARAMETERS; 5]);
}

#[test]
fn resolvable_own_addresses_are_unsupported() {
    let mut host_interface = host_interface();
    let resolvable_or_public = 2;

    let result = set_advertising_parameters(
        &mut host_interface,
        &AdvertisingParameters {
            own_address_type: resolvable_or_public,
            ..CONNECTABLE_ADVERTISING
        },
    );

    assert_eq!(result, status::UNSUPPORTED_FEATURE_OR_VALUE);
}

#[test]
fn scan_response_data_is_stored_for_scan_requests() {
    let mut host_interface = host_interface();
    let mut parameters = [0; 32];
    parameters[..6].copy_from_slice(&[5, 4, 9, b'N', b'e', b'o']);

    send_command(
        &mut host_interface,
        opcode::LE_SET_SCAN_RESPONSE_DATA,
        &parameters,
    );

    assert_eq!(
        status_of_completed(&mut host_interface, opcode::LE_SET_SCAN_RESPONSE_DATA),
        status::SUCCESS
    );
    assert_eq!(
        &host_interface.link_layer.settings.scan_response_data[..],
        &[4, 9, b'N', b'e', b'o']
    );
    assert!(host_interface.link_layer.settings.advertising_data.is_empty());
}

#[test]
fn advertising_data_longer_than_31_bytes_is_invalid() {
    let mut host_interface = host_interface();
    let mut parameters = [0; 33];
    parameters[0] = 32;

    send_command(&mut host_interface, opcode::LE_SET_ADVERTISING_DATA, &parameters);

    assert_eq!(
        status_of_completed(&mut host_interface, opcode::LE_SET_ADVERTISING_DATA),
        status::INVALID_PARAMETERS
    );
}

#[test]
fn advertising_data_shorter_than_its_stated_length_is_invalid() {
    let mut host_interface = host_interface();

    send_command(&mut host_interface, opcode::LE_SET_ADVERTISING_DATA, &[3, 2, 1]);

    assert_eq!(
        status_of_completed(&mut host_interface, opcode::LE_SET_ADVERTISING_DATA),
        status::INVALID_PARAMETERS
    );
}

#[test]
fn advertising_cannot_start_while_connected() {
    let mut host_interface = connected_host_interface();

    let operation = send_command(&mut host_interface, opcode::LE_SET_ADVERTISING_ENABLE, &[1]);

    assert_eq!(operation, None);
    assert_eq!(
        status_of_completed(&mut host_interface, opcode::LE_SET_ADVERTISING_ENABLE),
        status::COMMAND_DISALLOWED
    );
}

#[test]
fn advertising_without_channels_is_invalid() {
    let mut host_interface = host_interface();
    host_interface.link_layer.settings.channel_map = 0;

    let operation = send_command(&mut host_interface, opcode::LE_SET_ADVERTISING_ENABLE, &[1]);

    assert_eq!(operation, None);
    assert_eq!(
        status_of_completed(&mut host_interface, opcode::LE_SET_ADVERTISING_ENABLE),
        status::INVALID_PARAMETERS
    );
    assert!(!host_interface.link_layer.is_advertising());
}

#[test]
fn disabling_advertising_turns_the_radio_off() {
    let mut host_interface = advertising_host_interface();

    let operation = send_command(&mut host_interface, opcode::LE_SET_ADVERTISING_ENABLE, &[0]);

    assert_eq!(operation, Some(RadioOperation::Off));
    assert!(!host_interface.link_layer.is_advertising());
    assert_eq!(
        status_of_completed(&mut host_interface, opcode::LE_SET_ADVERTISING_ENABLE),
        status::SUCCESS
    );
}

#[test]
fn disabling_advertising_that_is_not_running_leaves_the_radio_alone() {
    let mut host_interface = host_interface();

    let operation = send_command(&mut host_interface, opcode::LE_SET_ADVERTISING_ENABLE, &[0]);

    assert_eq!(operation, None);
    assert_eq!(
        status_of_completed(&mut host_interface, opcode::LE_SET_ADVERTISING_ENABLE),
        status::SUCCESS
    );
}

// ============================================================================
// Accept list commands
// ============================================================================

#[test]
fn the_accept_list_holds_four_addresses() {
    let mut host_interface = host_interface();

    send_command(&mut host_interface, opcode::LE_READ_ACCEPT_LIST_SIZE, &[]);

    let (_, returned) = command_complete(&mut host_interface);
    assert_eq!(returned, [status::SUCCESS, 4]);
}

#[test]
fn addresses_join_the_accept_list_once_each() {
    let mut host_interface = host_interface();
    let central = accept_list_entry(PUBLIC_ADDRESS_TYPE, CENTRAL_ADDRESS);
    let other = accept_list_entry(RANDOM_ADDRESS_TYPE, OTHER_ADDRESS);

    send_command(&mut host_interface, opcode::LE_ADD_TO_ACCEPT_LIST, &central);
    send_command(&mut host_interface, opcode::LE_ADD_TO_ACCEPT_LIST, &central);
    send_command(&mut host_interface, opcode::LE_ADD_TO_ACCEPT_LIST, &other);

    for _ in 0..3 {
        assert_eq!(
            status_of_completed(&mut host_interface, opcode::LE_ADD_TO_ACCEPT_LIST),
            status::SUCCESS
        );
    }
    let expected = [
        DeviceAddress {
            is_random: false,
            bytes: CENTRAL_ADDRESS,
        },
        DeviceAddress {
            is_random: true,
            bytes: OTHER_ADDRESS,
        },
    ];
    assert_eq!(&host_interface.link_layer.settings.accept_list[..], &expected);
}

#[test]
fn a_fifth_accept_list_address_exceeds_its_memory() {
    let mut host_interface = host_interface();
    let mut results = Vec::new();

    for last_byte in 0..ACCEPT_LIST_LENGTH as u8 + 1 {
        let address = [0x10, 0x20, 0x30, 0x40, 0x50, last_byte];
        let entry = accept_list_entry(PUBLIC_ADDRESS_TYPE, address);
        send_command(&mut host_interface, opcode::LE_ADD_TO_ACCEPT_LIST, &entry);
        results.push(status_of_completed(
            &mut host_interface,
            opcode::LE_ADD_TO_ACCEPT_LIST,
        ));
    }

    assert_eq!(
        results,
        [
            status::SUCCESS,
            status::SUCCESS,
            status::SUCCESS,
            status::SUCCESS,
            status::MEMORY_CAPACITY_EXCEEDED
        ]
    );
    assert_eq!(
        host_interface.link_layer.settings.accept_list.len(),
        ACCEPT_LIST_LENGTH
    );
}

#[test]
fn removing_an_address_matches_its_type_as_well_as_its_bytes() {
    let mut host_interface = host_interface();
    let public_central = accept_list_entry(PUBLIC_ADDRESS_TYPE, CENTRAL_ADDRESS);
    let random_central = accept_list_entry(RANDOM_ADDRESS_TYPE, CENTRAL_ADDRESS);
    send_command(
        &mut host_interface,
        opcode::LE_ADD_TO_ACCEPT_LIST,
        &public_central,
    );
    command_complete(&mut host_interface);

    send_command(
        &mut host_interface,
        opcode::LE_REMOVE_FROM_ACCEPT_LIST,
        &random_central,
    );
    let length_after_wrong_type = host_interface.link_layer.settings.accept_list.len();
    send_command(
        &mut host_interface,
        opcode::LE_REMOVE_FROM_ACCEPT_LIST,
        &public_central,
    );

    assert_eq!(length_after_wrong_type, 1);
    assert!(host_interface.link_layer.settings.accept_list.is_empty());
    for _ in 0..2 {
        assert_eq!(
            status_of_completed(&mut host_interface, opcode::LE_REMOVE_FROM_ACCEPT_LIST),
            status::SUCCESS
        );
    }
}

#[test]
fn clearing_the_accept_list_empties_it() {
    let mut host_interface = host_interface();
    let central = accept_list_entry(PUBLIC_ADDRESS_TYPE, CENTRAL_ADDRESS);
    send_command(&mut host_interface, opcode::LE_ADD_TO_ACCEPT_LIST, &central);
    command_complete(&mut host_interface);

    send_command(&mut host_interface, opcode::LE_CLEAR_ACCEPT_LIST, &[]);

    assert_eq!(
        status_of_completed(&mut host_interface, opcode::LE_CLEAR_ACCEPT_LIST),
        status::SUCCESS
    );
    assert!(host_interface.link_layer.settings.accept_list.is_empty());
}

#[test]
fn accept_list_entries_need_a_known_type_and_a_whole_address() {
    let mut host_interface = host_interface();
    let anonymous_type = 0xFF;
    let unknown_type = accept_list_entry(anonymous_type, CENTRAL_ADDRESS);
    let short_entry = accept_list_entry(PUBLIC_ADDRESS_TYPE, CENTRAL_ADDRESS);

    send_command(&mut host_interface, opcode::LE_ADD_TO_ACCEPT_LIST, &unknown_type);
    send_command(
        &mut host_interface,
        opcode::LE_ADD_TO_ACCEPT_LIST,
        &short_entry[..6],
    );

    for _ in 0..2 {
        assert_eq!(
            status_of_completed(&mut host_interface, opcode::LE_ADD_TO_ACCEPT_LIST),
            status::UNSUPPORTED_FEATURE_OR_VALUE
        );
    }
    assert!(host_interface.link_layer.settings.accept_list.is_empty());
}

#[test]
fn the_accept_list_cannot_change_while_advertising() {
    let mut host_interface = advertising_host_interface();
    let central = accept_list_entry(PUBLIC_ADDRESS_TYPE, CENTRAL_ADDRESS);

    send_command(&mut host_interface, opcode::LE_ADD_TO_ACCEPT_LIST, &central);

    assert_eq!(
        status_of_completed(&mut host_interface, opcode::LE_ADD_TO_ACCEPT_LIST),
        status::COMMAND_DISALLOWED
    );
    assert!(host_interface.link_layer.settings.accept_list.is_empty());
}

// ============================================================================
// Connection commands
// ============================================================================

#[test]
fn disconnecting_the_connection_is_accepted_by_status() {
    let mut host_interface = connected_host_interface();
    let remote_user_terminated = 0x13;

    send_command(
        &mut host_interface,
        opcode::DISCONNECT,
        &[0, 0, remote_user_terminated],
    );

    assert_eq!(
        command_status(&mut host_interface),
        (opcode::DISCONNECT, status::SUCCESS)
    );
}

#[test]
fn disconnecting_another_handle_reports_an_unknown_connection() {
    let mut host_interface = connected_host_interface();

    send_command(&mut host_interface, opcode::DISCONNECT, &[1, 0, 0x13]);

    assert_eq!(
        command_status(&mut host_interface),
        (opcode::DISCONNECT, status::UNKNOWN_CONNECTION)
    );
}

#[test]
fn connection_updates_from_the_host_are_unsupported() {
    let mut host_interface = connected_host_interface();

    send_command(&mut host_interface, opcode::LE_CONNECTION_UPDATE, &[0; 14]);

    assert_eq!(
        command_status(&mut host_interface),
        (opcode::LE_CONNECTION_UPDATE, status::UNSUPPORTED_FEATURE_OR_VALUE)
    );
}

#[test]
fn signal_strength_reports_the_last_packet_heard_while_connected() {
    let mut host_interface = connected_host_interface();

    send_command(&mut host_interface, opcode::READ_SIGNAL_STRENGTH, &[0, 0]);

    let (_, returned) = command_complete(&mut host_interface);
    assert_eq!(returned, [status::SUCCESS, 0, 0, (-40i8) as u8]);
}

#[test]
fn signal_strength_without_a_connection_reports_an_unknown_connection() {
    let mut host_interface = host_interface();

    send_command(&mut host_interface, opcode::READ_SIGNAL_STRENGTH, &[0, 0]);

    let (_, returned) = command_complete(&mut host_interface);
    let not_available = 127;
    assert_eq!(returned, [status::UNKNOWN_CONNECTION, 0, 0, not_available]);
}

// ============================================================================
// Data from the host
// ============================================================================

#[test]
fn a_data_packet_shorter_than_its_header_is_malformed() {
    let mut host_interface = connected_host_interface();

    assert_eq!(
        host_interface.handle_acl_data(&[0, 0, 0]),
        Err(HostPacketError::Malformed)
    );
}

#[test]
fn a_data_packet_shorter_than_its_stated_length_is_malformed() {
    let mut host_interface = connected_host_interface();

    let result = host_interface.handle_acl_data(&[0, 0, 4, 0, b'a', b'b']);

    assert_eq!(result, Err(HostPacketError::Malformed));
    assert!(host_interface.link_layer.outgoing.is_empty());
}

#[test]
fn data_for_another_handle_is_dropped() {
    let mut host_interface = connected_host_interface();

    let result = host_interface.handle_acl_data(&[0x01, 0x20, 1, 0, b'a']);

    assert_eq!(result, Ok(Ok(())));
    assert!(host_interface.link_layer.outgoing.is_empty());
}

#[test]
fn host_data_is_queued_for_the_peer_with_its_boundary() {
    let mut host_interface = connected_host_interface();

    let first = host_interface.handle_acl_data(&[0x00, 0x00, 2, 0, b'a', b'b']);
    let continuation = host_interface.handle_acl_data(&[0x00, 0x10, 1, 0, b'c']);

    assert_eq!(first, Ok(Ok(())));
    assert_eq!(continuation, Ok(Ok(())));
    let outgoing: Vec<DataPacket> = host_interface.link_layer.outgoing.iter().copied().collect();
    assert_eq!(
        outgoing,
        [
            DataPacket::new(DataKind::Start, b"ab"),
            DataPacket::new(DataKind::Continuation, b"c")
        ]
    );
}

#[test]
fn host_data_is_handed_back_while_the_queue_is_full() {
    let mut host_interface = connected_host_interface();
    let packet = [0x00, 0x00, 1, 0, b'a'];
    for _ in 0..OUTGOING_QUEUE_LENGTH {
        assert_eq!(host_interface.handle_acl_data(&packet), Ok(Ok(())));
    }

    let result = host_interface.handle_acl_data(&packet);

    assert_eq!(result, Ok(Err(())));
    assert_eq!(host_interface.link_layer.outgoing.len(), OUTGOING_QUEUE_LENGTH);
}

// ============================================================================
// Events to the host
// ============================================================================

#[test]
fn command_answers_come_before_completed_packets_and_link_events() {
    let mut host_interface = host_interface();
    host_interface
        .link_layer
        .events
        .push_back(LinkEvent::Disconnected { reason: 0x13 })
        .unwrap();
    host_interface.link_layer.completed_packets = 1;
    send_command(&mut host_interface, opcode::HOST_BUFFER_SIZE, &[]);

    let first = next_event(&mut host_interface)[0];
    let second = next_event(&mut host_interface)[0];
    let third = next_event(&mut host_interface)[0];

    assert_eq!(
        [first, second, third],
        [
            event_code::COMMAND_COMPLETE,
            event_code::NUMBER_OF_COMPLETED_PACKETS,
            event_code::DISCONNECTION_COMPLETE
        ]
    );
    assert!(next_packet(&mut host_interface).is_none());
}

#[test]
fn a_disconnection_is_reported_with_its_reason() {
    let mut host_interface = host_interface();
    let connection_timeout = 0x08;
    host_interface
        .link_layer
        .events
        .push_back(LinkEvent::Disconnected {
            reason: connection_timeout,
        })
        .unwrap();

    let bytes = next_event(&mut host_interface);

    assert_eq!(
        bytes,
        [
            event_code::DISCONNECTION_COMPLETE,
            4,
            status::SUCCESS,
            0,
            0,
            connection_timeout
        ]
    );
}

#[test]
fn a_disconnection_is_not_reported_once_masked_off() {
    let mut host_interface = host_interface();
    let all_but_disconnection = DEFAULT_EVENT_MASK & !DISCONNECTION_COMPLETE_MASK_BIT;
    send_command(
        &mut host_interface,
        opcode::SET_EVENT_MASK,
        &all_but_disconnection.to_le_bytes(),
    );
    command_complete(&mut host_interface);
    host_interface
        .link_layer
        .events
        .push_back(LinkEvent::Disconnected { reason: 0x13 })
        .unwrap();

    assert!(next_packet(&mut host_interface).is_none());
    assert!(host_interface.link_layer.events.is_empty());
}

#[test]
fn le_events_stay_silent_until_the_host_turns_on_le_meta_events() {
    let mut host_interface = host_interface();
    host_interface
        .link_layer
        .events
        .push_back(LinkEvent::ParametersChanged {
            interval: 24,
            latency: 0,
            timeout: 100,
        })
        .unwrap();

    assert!(next_packet(&mut host_interface).is_none());
}

#[test]
fn a_parameter_change_is_reported_as_le_connection_update_complete() {
    let mut host_interface = host_interface();
    enable_le_events(&mut host_interface, LE_CONNECTION_UPDATE_COMPLETE_MASK_BIT);
    host_interface
        .link_layer
        .events
        .push_back(LinkEvent::ParametersChanged {
            interval: 0x0028,
            latency: 0x0002,
            timeout: 0x01F4,
        })
        .unwrap();

    let bytes = next_event(&mut host_interface);

    assert_eq!(
        bytes,
        [
            event_code::LE_META,
            10,
            event_code::LE_CONNECTION_UPDATE_COMPLETE,
            status::SUCCESS,
            0,
            0,
            0x28,
            0,
            0x02,
            0,
            0xF4,
            0x01
        ]
    );
}

#[test]
fn a_masked_le_event_is_skipped_for_the_next_one() {
    let mut host_interface = host_interface();
    enable_le_events(&mut host_interface, LE_CONNECTION_UPDATE_COMPLETE_MASK_BIT);
    host_interface
        .link_layer
        .events
        .push_back(LinkEvent::Connected {
            peer: DeviceAddress {
                is_random: false,
                bytes: CENTRAL_ADDRESS,
            },
            interval: 24,
            latency: 0,
            timeout: 100,
            clock_accuracy: 5,
        })
        .unwrap();
    host_interface
        .link_layer
        .events
        .push_back(LinkEvent::Received(DataPacket::new(DataKind::Start, b"x")))
        .unwrap();

    let (kind, bytes) = next_packet(&mut host_interface).unwrap();

    assert_eq!(kind, PacketKind::AclData);
    assert_eq!(bytes, [0x00, 0x20, 1, 0, b'x']);
}

#[test]
fn a_connection_from_a_public_address_says_so() {
    let mut host_interface = host_interface();
    enable_le_events(&mut host_interface, LE_CONNECTION_COMPLETE_MASK_BIT);
    host_interface
        .link_layer
        .events
        .push_back(LinkEvent::Connected {
            peer: DeviceAddress {
                is_random: false,
                bytes: CENTRAL_ADDRESS,
            },
            interval: 24,
            latency: 0,
            timeout: 100,
            clock_accuracy: 5,
        })
        .unwrap();

    let bytes = next_event(&mut host_interface);

    let peripheral_role = 1;
    assert_eq!(
        &bytes[..8],
        &[
            event_code::LE_META,
            19,
            0x01,
            status::SUCCESS,
            0,
            0,
            peripheral_role,
            0
        ]
    );
    assert_eq!(&bytes[8..14], &CENTRAL_ADDRESS);
    assert_eq!(bytes[20], 5);
}

// ============================================================================
// Packet kinds and serialising
// ============================================================================

#[test]
fn packet_kinds_map_to_the_transport_kinds() {
    let event: bt_hci::transport::PacketKind = PacketKind::Event.into();
    let data: bt_hci::transport::PacketKind = PacketKind::AclData.into();

    assert_eq!(event, bt_hci::transport::PacketKind::Event);
    assert_eq!(data, bt_hci::transport::PacketKind::AclData);
}

#[test]
fn serialising_into_too_small_a_buffer_gives_nothing() {
    let reset = bt_hci::cmd::controller_baseband::Reset::new();
    let mut too_small = [0; 3];
    let mut big_enough = [0; 4];

    let refused = serialise_host_packet(&reset, &mut too_small);
    let written = serialise_host_packet(&reset, &mut big_enough);

    assert_eq!(refused, None);
    assert_eq!(written, Some(4));
    assert_eq!(big_enough, [0x01, 0x03, 0x0C, 0]);
}
