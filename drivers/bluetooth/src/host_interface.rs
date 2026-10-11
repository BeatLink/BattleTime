//! The controller's side of HCI: commands from the host in, events and data back out, as raw bytes.

use bt_hci::transport::WithIndicator;
use heapless::Deque;

use crate::link_layer::*;
use crate::packets::*;

// ============================================================================
// Opcodes and event codes
// ============================================================================

mod opcode {
    pub const DISCONNECT: u16 = 0x0406;
    pub const SET_EVENT_MASK: u16 = 0x0C01;
    pub const RESET: u16 = 0x0C03;
    pub const SET_CONTROLLER_TO_HOST_FLOW_CONTROL: u16 = 0x0C31;
    pub const HOST_BUFFER_SIZE: u16 = 0x0C33;
    pub const HOST_NUMBER_OF_COMPLETED_PACKETS: u16 = 0x0C35;
    pub const SET_EVENT_MASK_PAGE_2: u16 = 0x0C63;
    pub const READ_LOCAL_VERSION_INFORMATION: u16 = 0x1001;
    pub const READ_LOCAL_SUPPORTED_FEATURES: u16 = 0x1003;
    pub const READ_DEVICE_ADDRESS: u16 = 0x1009;
    pub const READ_SIGNAL_STRENGTH: u16 = 0x1405;
    pub const LE_SET_EVENT_MASK: u16 = 0x2001;
    pub const LE_READ_BUFFER_SIZE: u16 = 0x2002;
    pub const LE_READ_LOCAL_SUPPORTED_FEATURES: u16 = 0x2003;
    pub const LE_SET_RANDOM_ADDRESS: u16 = 0x2005;
    pub const LE_SET_ADVERTISING_PARAMETERS: u16 = 0x2006;
    pub const LE_READ_ADVERTISING_TRANSMIT_POWER: u16 = 0x2007;
    pub const LE_SET_ADVERTISING_DATA: u16 = 0x2008;
    pub const LE_SET_SCAN_RESPONSE_DATA: u16 = 0x2009;
    pub const LE_SET_ADVERTISING_ENABLE: u16 = 0x200A;
    pub const LE_READ_ACCEPT_LIST_SIZE: u16 = 0x200F;
    pub const LE_CLEAR_ACCEPT_LIST: u16 = 0x2010;
    pub const LE_ADD_TO_ACCEPT_LIST: u16 = 0x2011;
    pub const LE_REMOVE_FROM_ACCEPT_LIST: u16 = 0x2012;
    pub const LE_CONNECTION_UPDATE: u16 = 0x2013;
    pub const LE_RANDOM: u16 = 0x2018;
    pub const LE_SET_DATA_LENGTH: u16 = 0x2022;
    pub const LE_READ_SUGGESTED_DEFAULT_DATA_LENGTH: u16 = 0x2023;
    pub const LE_WRITE_SUGGESTED_DEFAULT_DATA_LENGTH: u16 = 0x2024;
    pub const LE_READ_MAXIMUM_DATA_LENGTH: u16 = 0x202F;
    pub const LE_READ_PHYSICAL_LAYER: u16 = 0x2030;
}

mod event_code {
    pub const DISCONNECTION_COMPLETE: u8 = 0x05;
    pub const COMMAND_COMPLETE: u8 = 0x0E;
    pub const COMMAND_STATUS: u8 = 0x0F;
    pub const NUMBER_OF_COMPLETED_PACKETS: u8 = 0x13;
    pub const LE_META: u8 = 0x3E;
    pub const LE_CONNECTION_COMPLETE: u8 = 0x01;
    pub const LE_CONNECTION_UPDATE_COMPLETE: u8 = 0x03;
}

pub mod status {
    pub const SUCCESS: u8 = 0x00;
    pub const UNKNOWN_COMMAND: u8 = 0x01;
    pub const UNKNOWN_CONNECTION: u8 = 0x02;
    pub const MEMORY_CAPACITY_EXCEEDED: u8 = 0x07;
    pub const COMMAND_DISALLOWED: u8 = 0x0C;
    pub const UNSUPPORTED_FEATURE_OR_VALUE: u8 = 0x11;
    pub const INVALID_PARAMETERS: u8 = 0x12;
}

/// The one connection's handle; this controller never has two.
pub const CONNECTION_HANDLE: u16 = 0x0000;

const DEFAULT_EVENT_MASK: u64 = 0x0000_1FFF_FFFF_FFFF;
const DEFAULT_LE_EVENT_MASK: u64 = 0x1F;
const DISCONNECTION_COMPLETE_MASK_BIT: u64 = 1 << 4;
const LE_META_MASK_BIT: u64 = 1 << 61;
const LE_CONNECTION_COMPLETE_MASK_BIT: u64 = 1 << 0;
const LE_CONNECTION_UPDATE_COMPLETE_MASK_BIT: u64 = 1 << 2;

const DEFAULT_DATA_TIME_IN_MICROSECONDS: u16 = 328;
const LMP_FEATURES_LE_ONLY: [u8; 8] = [0, 0, 0, 0, 0x60, 0, 0, 0];

// ============================================================================
// Packets to the host
// ============================================================================

pub const MAXIMUM_HOST_PACKET_LENGTH: usize = 40;
const COMMAND_RESULT_QUEUE_LENGTH: usize = 4;

#[derive(Clone, Copy, Debug)]
struct EventBytes {
    length: u8,
    bytes: [u8; MAXIMUM_HOST_PACKET_LENGTH],
}

impl EventBytes {
    fn new(code: u8, parameters: &[u8]) -> EventBytes {
        let mut event = EventBytes {
            length: 2 + parameters.len() as u8,
            bytes: [0; MAXIMUM_HOST_PACKET_LENGTH],
        };
        event.bytes[0] = code;
        event.bytes[1] = parameters.len() as u8;
        event.bytes[2..2 + parameters.len()].copy_from_slice(parameters);
        event
    }

    fn command_complete(opcode: u16, return_parameters: &[u8]) -> EventBytes {
        let mut parameters = [0; MAXIMUM_HOST_PACKET_LENGTH - 2];
        parameters[0] = 1;
        parameters[1..3].copy_from_slice(&opcode.to_le_bytes());
        parameters[3..3 + return_parameters.len()].copy_from_slice(return_parameters);
        EventBytes::new(
            event_code::COMMAND_COMPLETE,
            &parameters[..3 + return_parameters.len()],
        )
    }

    fn command_status(opcode: u16, status: u8) -> EventBytes {
        let [low, high] = opcode.to_le_bytes();
        EventBytes::new(event_code::COMMAND_STATUS, &[status, 1, low, high])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacketKind {
    Event,
    AclData,
}

impl From<PacketKind> for bt_hci::transport::PacketKind {
    fn from(kind: PacketKind) -> Self {
        match kind {
            PacketKind::Event => bt_hci::transport::PacketKind::Event,
            PacketKind::AclData => bt_hci::transport::PacketKind::AclData,
        }
    }
}

// ============================================================================
// Host interface
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostPacketError {
    Malformed,
    TooLong,
}

pub struct HostInterface {
    pub link_layer: LinkLayer,
    command_results: Deque<EventBytes, COMMAND_RESULT_QUEUE_LENGTH>,
    event_mask: u64,
    le_event_mask: u64,
    device_address: [u8; ADDRESS_LENGTH],
    random_address: Option<[u8; ADDRESS_LENGTH]>,
    advertise_as_random: bool,
    random_bytes: fn(&mut [u8]),
}

impl HostInterface {
    /// `device_address` is the chip's own static random address, used until the host sets another.
    pub fn new(device_address: [u8; ADDRESS_LENGTH], random_bytes: fn(&mut [u8])) -> HostInterface {
        let mut seed = [0; 4];
        random_bytes(&mut seed);

        let mut host_interface = HostInterface {
            link_layer: LinkLayer::new(u32::from_le_bytes(seed)),
            command_results: Deque::new(),
            event_mask: DEFAULT_EVENT_MASK,
            le_event_mask: DEFAULT_LE_EVENT_MASK,
            device_address,
            random_address: None,
            advertise_as_random: false,
            random_bytes,
        };
        host_interface.refresh_own_address();
        host_interface
    }

    pub fn can_take_command(&self) -> bool {
        !self.command_results.is_full()
    }

    fn refresh_own_address(&mut self) {
        let bytes = match (self.advertise_as_random, self.random_address) {
            (true, Some(random_address)) => random_address,
            _ => self.device_address,
        };
        self.link_layer.settings.own_address = DeviceAddress {
            is_random: true,
            bytes,
        };
    }

    fn complete(&mut self, opcode: u16, return_parameters: &[u8]) {
        let _ = self
            .command_results
            .push_back(EventBytes::command_complete(opcode, return_parameters));
    }

    fn complete_with_status(&mut self, opcode: u16, status: u8) {
        self.complete(opcode, &[status]);
    }

    fn report_status(&mut self, opcode: u16, status: u8) {
        let _ = self
            .command_results
            .push_back(EventBytes::command_status(opcode, status));
    }

    // ------------------------------------------------------------------------
    // Commands
    // ------------------------------------------------------------------------

    /// Runs one command packet (without its indicator byte) and gives any radio operation it starts.
    pub fn handle_command(
        &mut self,
        packet: &[u8],
        now: u32,
        transmit_buffer: &mut [u8],
    ) -> Result<Option<RadioOperation>, HostPacketError> {
        if packet.len() < 3 || packet.len() < 3 + packet[2] as usize {
            return Err(HostPacketError::Malformed);
        }
        let command = u16::from_le_bytes([packet[0], packet[1]]);
        let parameters = &packet[3..3 + packet[2] as usize];
        let parameter = |offset: usize| parameters.get(offset).copied().unwrap_or(0);
        let parameter_u16 = |offset: usize| u16::from_le_bytes([parameter(offset), parameter(offset + 1)]);
        let parameter_u64 = |offset: usize| {
            let mut bytes = [0; 8];
            for (index, byte) in bytes.iter_mut().enumerate() {
                *byte = parameter(offset + index);
            }
            u64::from_le_bytes(bytes)
        };
        let handle_bytes = CONNECTION_HANDLE.to_le_bytes();

        let mut radio_operation = None;
        match command {
            opcode::RESET => {
                radio_operation = Some(self.link_layer.reset());
                self.event_mask = DEFAULT_EVENT_MASK;
                self.le_event_mask = DEFAULT_LE_EVENT_MASK;
                self.random_address = None;
                self.advertise_as_random = false;
                self.refresh_own_address();
                self.complete_with_status(command, status::SUCCESS);
            }

            opcode::SET_EVENT_MASK => {
                self.event_mask = parameter_u64(0);
                self.complete_with_status(command, status::SUCCESS);
            }

            opcode::LE_SET_EVENT_MASK => {
                self.le_event_mask = parameter_u64(0);
                self.complete_with_status(command, status::SUCCESS);
            }

            opcode::SET_EVENT_MASK_PAGE_2
            | opcode::HOST_BUFFER_SIZE
            | opcode::LE_WRITE_SUGGESTED_DEFAULT_DATA_LENGTH => {
                self.complete_with_status(command, status::SUCCESS);
            }

            opcode::SET_CONTROLLER_TO_HOST_FLOW_CONTROL => {
                let flow_control_off = parameter(0) == 0;
                let result = if flow_control_off {
                    status::SUCCESS
                } else {
                    status::UNSUPPORTED_FEATURE_OR_VALUE
                };
                self.complete_with_status(command, result);
            }

            opcode::HOST_NUMBER_OF_COMPLETED_PACKETS => {}

            opcode::READ_LOCAL_VERSION_INFORMATION => {
                let [version, manufacturer_low, manufacturer_high, ..] = VERSION;
                self.complete(
                    command,
                    &[
                        status::SUCCESS,
                        version,
                        0,
                        0,
                        version,
                        manufacturer_low,
                        manufacturer_high,
                        0,
                        0,
                    ],
                );
            }

            opcode::READ_LOCAL_SUPPORTED_FEATURES => {
                let mut result = [0; 9];
                result[1..].copy_from_slice(&LMP_FEATURES_LE_ONLY);
                self.complete(command, &result);
            }

            opcode::READ_DEVICE_ADDRESS => {
                self.complete(command, &[status::SUCCESS, 0, 0, 0, 0, 0, 0]);
            }

            opcode::LE_READ_LOCAL_SUPPORTED_FEATURES => {
                self.complete(command, &[status::SUCCESS, 0, 0, 0, 0, 0, 0, 0, 0]);
            }

            opcode::LE_READ_BUFFER_SIZE => {
                let [length_low, length_high] = (MAXIMUM_DATA_PAYLOAD_LENGTH as u16).to_le_bytes();
                self.complete(
                    command,
                    &[
                        status::SUCCESS,
                        length_low,
                        length_high,
                        OUTGOING_QUEUE_LENGTH as u8,
                    ],
                );
            }

            opcode::LE_RANDOM => {
                let mut result = [0; 9];
                (self.random_bytes)(&mut result[1..]);
                self.complete(command, &result);
            }

            opcode::LE_SET_RANDOM_ADDRESS => {
                if self.link_layer.is_advertising() {
                    self.complete_with_status(command, status::COMMAND_DISALLOWED);
                } else {
                    let mut address = [0; ADDRESS_LENGTH];
                    address.copy_from_slice(&parameters[..ADDRESS_LENGTH.min(parameters.len())]);
                    self.random_address = Some(address);
                    self.refresh_own_address();
                    self.complete_with_status(command, status::SUCCESS);
                }
            }

            opcode::LE_SET_ADVERTISING_PARAMETERS => {
                let result = self.set_advertising_parameters(parameters);
                self.complete_with_status(command, result);
            }

            opcode::LE_READ_ADVERTISING_TRANSMIT_POWER => {
                self.complete(command, &[status::SUCCESS, 0]);
            }

            opcode::LE_SET_ADVERTISING_DATA | opcode::LE_SET_SCAN_RESPONSE_DATA => {
                let length = parameter(0) as usize;
                if length > MAXIMUM_ADVERTISING_DATA_LENGTH || parameters.len() < 1 + length {
                    self.complete_with_status(command, status::INVALID_PARAMETERS);
                } else {
                    let settings = &mut self.link_layer.settings;
                    let target = if command == opcode::LE_SET_ADVERTISING_DATA {
                        &mut settings.advertising_data
                    } else {
                        &mut settings.scan_response_data
                    };
                    target.clear();
                    let _ = target.extend_from_slice(&parameters[1..1 + length]);
                    self.complete_with_status(command, status::SUCCESS);
                }
            }

            opcode::LE_SET_ADVERTISING_ENABLE => {
                let result = if parameter(0) == 1 {
                    match self.link_layer.start_advertising(now, transmit_buffer) {
                        Ok(operation) => {
                            radio_operation = operation;
                            status::SUCCESS
                        }
                        Err(Refusal::NotAllowedNow) => status::COMMAND_DISALLOWED,
                        Err(Refusal::NothingToAdvertiseOn) => status::INVALID_PARAMETERS,
                    }
                } else {
                    radio_operation = self.link_layer.stop_advertising();
                    status::SUCCESS
                };
                self.complete_with_status(command, result);
            }

            opcode::LE_READ_ACCEPT_LIST_SIZE => {
                self.complete(command, &[status::SUCCESS, ACCEPT_LIST_LENGTH as u8]);
            }

            opcode::LE_CLEAR_ACCEPT_LIST => {
                self.link_layer.settings.accept_list.clear();
                self.complete_with_status(command, status::SUCCESS);
            }

            opcode::LE_ADD_TO_ACCEPT_LIST | opcode::LE_REMOVE_FROM_ACCEPT_LIST => {
                let result = self.change_accept_list(command == opcode::LE_ADD_TO_ACCEPT_LIST, parameters);
                self.complete_with_status(command, result);
            }

            opcode::DISCONNECT => {
                let result = if parameter_u16(0) != CONNECTION_HANDLE {
                    status::UNKNOWN_CONNECTION
                } else if self.link_layer.disconnect(parameter(2)).is_ok() {
                    status::SUCCESS
                } else {
                    status::UNKNOWN_CONNECTION
                };
                self.report_status(command, result);
            }

            opcode::LE_CONNECTION_UPDATE => {
                self.report_status(command, status::UNSUPPORTED_FEATURE_OR_VALUE);
            }

            opcode::READ_SIGNAL_STRENGTH => {
                let [handle_low, handle_high] = handle_bytes;
                let strength = self.link_layer.last_signal_strength as u8;
                let result = if self.link_layer.is_connected() {
                    status::SUCCESS
                } else {
                    status::UNKNOWN_CONNECTION
                };
                self.complete(command, &[result, handle_low, handle_high, strength]);
            }

            opcode::LE_READ_PHYSICAL_LAYER => {
                let [handle_low, handle_high] = handle_bytes;
                let one_megabit = 1;
                self.complete(
                    command,
                    &[status::SUCCESS, handle_low, handle_high, one_megabit, one_megabit],
                );
            }

            opcode::LE_SET_DATA_LENGTH => {
                let [handle_low, handle_high] = handle_bytes;
                self.complete(
                    command,
                    &[status::UNSUPPORTED_FEATURE_OR_VALUE, handle_low, handle_high],
                );
            }

            opcode::LE_READ_SUGGESTED_DEFAULT_DATA_LENGTH => {
                let [length_low, length_high] = (MAXIMUM_DATA_PAYLOAD_LENGTH as u16).to_le_bytes();
                let [time_low, time_high] = DEFAULT_DATA_TIME_IN_MICROSECONDS.to_le_bytes();
                self.complete(
                    command,
                    &[status::SUCCESS, length_low, length_high, time_low, time_high],
                );
            }

            opcode::LE_READ_MAXIMUM_DATA_LENGTH => {
                let [length_low, length_high] = (MAXIMUM_DATA_PAYLOAD_LENGTH as u16).to_le_bytes();
                let [time_low, time_high] = DEFAULT_DATA_TIME_IN_MICROSECONDS.to_le_bytes();
                self.complete(
                    command,
                    &[
                        status::SUCCESS,
                        length_low,
                        length_high,
                        time_low,
                        time_high,
                        length_low,
                        length_high,
                        time_low,
                        time_high,
                    ],
                );
            }

            _ => self.complete_with_status(command, status::UNKNOWN_COMMAND),
        }

        Ok(radio_operation)
    }

    fn set_advertising_parameters(&mut self, parameters: &[u8]) -> u8 {
        if parameters.len() < 15 {
            return status::INVALID_PARAMETERS;
        }
        if self.link_layer.is_advertising() {
            return status::COMMAND_DISALLOWED;
        }

        let interval_minimum = u16::from_le_bytes([parameters[0], parameters[1]]);
        let kind = match parameters[4] {
            0x00 => AdvertisingKind::Connectable,
            0x02 => AdvertisingKind::Scannable,
            0x03 => AdvertisingKind::NonConnectable,
            0x01 | 0x04 => return status::UNSUPPORTED_FEATURE_OR_VALUE,
            _ => return status::INVALID_PARAMETERS,
        };
        let own_address_type = parameters[5];
        let channel_map = parameters[13] & 0b111;
        let filter_policy = parameters[14];

        if !(0x0020..=0x4000).contains(&interval_minimum) || channel_map == 0 || filter_policy > 3 {
            return status::INVALID_PARAMETERS;
        }
        if own_address_type > 1 {
            return status::UNSUPPORTED_FEATURE_OR_VALUE;
        }

        let settings = &mut self.link_layer.settings;
        settings.interval_in_microseconds = interval_minimum as u32 * 625;
        settings.kind = kind;
        settings.channel_map = channel_map;
        settings.filter_scan_requests = filter_policy & 1 != 0;
        settings.filter_connect_requests = filter_policy & 2 != 0;
        self.advertise_as_random = own_address_type == 1;
        self.refresh_own_address();
        status::SUCCESS
    }

    fn change_accept_list(&mut self, add: bool, parameters: &[u8]) -> u8 {
        if parameters.len() < 1 + ADDRESS_LENGTH || parameters[0] > 1 {
            return status::UNSUPPORTED_FEATURE_OR_VALUE;
        }
        if self.link_layer.is_advertising() {
            return status::COMMAND_DISALLOWED;
        }

        let mut entry = DeviceAddress {
            is_random: parameters[0] == 1,
            bytes: [0; ADDRESS_LENGTH],
        };
        entry.bytes.copy_from_slice(&parameters[1..1 + ADDRESS_LENGTH]);
        let accept_list = &mut self.link_layer.settings.accept_list;

        if !add {
            accept_list.retain(|existing| *existing != entry);
            return status::SUCCESS;
        }
        if accept_list.contains(&entry) {
            return status::SUCCESS;
        }
        match accept_list.push(entry) {
            Ok(()) => status::SUCCESS,
            Err(_) => status::MEMORY_CAPACITY_EXCEEDED,
        }
    }

    // ------------------------------------------------------------------------
    // Data from the host
    // ------------------------------------------------------------------------

    /// Queues one ACL packet (without its indicator byte), or hands it back when the queue is full.
    pub fn handle_acl_data(&mut self, packet: &[u8]) -> Result<Result<(), ()>, HostPacketError> {
        if packet.len() < 4 {
            return Err(HostPacketError::Malformed);
        }
        let handle_and_flags = u16::from_le_bytes([packet[0], packet[1]]);
        let length = u16::from_le_bytes([packet[2], packet[3]]) as usize;
        if packet.len() < 4 + length {
            return Err(HostPacketError::Malformed);
        }
        if length > MAXIMUM_DATA_PAYLOAD_LENGTH {
            return Err(HostPacketError::TooLong);
        }
        if handle_and_flags & 0x0FFF != CONNECTION_HANDLE {
            return Ok(Ok(()));
        }

        let continues_a_message = (handle_and_flags >> 12) & 0b11 == 0b01;
        let kind = if continues_a_message {
            DataKind::Continuation
        } else {
            DataKind::Start
        };
        let packet = DataPacket::new(kind, &packet[4..4 + length]);
        Ok(self.link_layer.send_to_peer(packet).map_err(|_| ()))
    }

    // ------------------------------------------------------------------------
    // Packets to the host
    // ------------------------------------------------------------------------

    /// Writes the next packet for the host into `buffer`, without its indicator byte.
    pub fn next_packet_for_host(&mut self, buffer: &mut [u8]) -> Option<(PacketKind, usize)> {
        if let Some(event) = self.command_results.pop_front() {
            let length = event.length as usize;
            buffer[..length].copy_from_slice(&event.bytes[..length]);
            return Some((PacketKind::Event, length));
        }

        if self.link_layer.completed_packets > 0 {
            let [handle_low, handle_high] = CONNECTION_HANDLE.to_le_bytes();
            let [count_low, count_high] = self.link_layer.completed_packets.to_le_bytes();
            self.link_layer.completed_packets = 0;
            let event = EventBytes::new(
                event_code::NUMBER_OF_COMPLETED_PACKETS,
                &[1, handle_low, handle_high, count_low, count_high],
            );
            return Some(Self::copy_event(event, buffer));
        }

        while let Some(link_event) = self.link_layer.events.pop_front() {
            if let Some(written) = self.write_link_event(link_event, buffer) {
                return Some(written);
            }
        }
        None
    }

    fn copy_event(event: EventBytes, buffer: &mut [u8]) -> (PacketKind, usize) {
        let length = event.length as usize;
        buffer[..length].copy_from_slice(&event.bytes[..length]);
        (PacketKind::Event, length)
    }

    fn le_event_enabled(&self, bit: u64) -> bool {
        self.event_mask & LE_META_MASK_BIT != 0 && self.le_event_mask & bit != 0
    }

    fn write_link_event(&self, link_event: LinkEvent, buffer: &mut [u8]) -> Option<(PacketKind, usize)> {
        let [handle_low, handle_high] = CONNECTION_HANDLE.to_le_bytes();

        match link_event {
            LinkEvent::Connected {
                peer,
                interval,
                latency,
                timeout,
                clock_accuracy,
            } => {
                if !self.le_event_enabled(LE_CONNECTION_COMPLETE_MASK_BIT) {
                    return None;
                }
                let peripheral_role = 1;
                let mut parameters = [0; 19];
                parameters[..5].copy_from_slice(&[
                    event_code::LE_CONNECTION_COMPLETE,
                    status::SUCCESS,
                    handle_low,
                    handle_high,
                    peripheral_role,
                ]);
                parameters[5] = u8::from(peer.is_random);
                parameters[6..12].copy_from_slice(&peer.bytes);
                parameters[12..14].copy_from_slice(&interval.to_le_bytes());
                parameters[14..16].copy_from_slice(&latency.to_le_bytes());
                parameters[16..18].copy_from_slice(&timeout.to_le_bytes());
                parameters[18] = clock_accuracy;
                Some(Self::copy_event(
                    EventBytes::new(event_code::LE_META, &parameters),
                    buffer,
                ))
            }

            LinkEvent::ParametersChanged {
                interval,
                latency,
                timeout,
            } => {
                if !self.le_event_enabled(LE_CONNECTION_UPDATE_COMPLETE_MASK_BIT) {
                    return None;
                }
                let mut parameters = [0; 10];
                parameters[..4].copy_from_slice(&[
                    event_code::LE_CONNECTION_UPDATE_COMPLETE,
                    status::SUCCESS,
                    handle_low,
                    handle_high,
                ]);
                parameters[4..6].copy_from_slice(&interval.to_le_bytes());
                parameters[6..8].copy_from_slice(&latency.to_le_bytes());
                parameters[8..10].copy_from_slice(&timeout.to_le_bytes());
                Some(Self::copy_event(
                    EventBytes::new(event_code::LE_META, &parameters),
                    buffer,
                ))
            }

            LinkEvent::Disconnected { reason } => {
                if self.event_mask & DISCONNECTION_COMPLETE_MASK_BIT == 0 {
                    return None;
                }
                let parameters = [status::SUCCESS, handle_low, handle_high, reason];
                Some(Self::copy_event(
                    EventBytes::new(event_code::DISCONNECTION_COMPLETE, &parameters),
                    buffer,
                ))
            }

            LinkEvent::Received(packet) => {
                let boundary_flags: u16 = match packet.kind {
                    DataKind::Continuation => 0b01,
                    _ => 0b10,
                };
                let handle_and_flags = CONNECTION_HANDLE | (boundary_flags << 12);
                let length = packet.length as usize;
                buffer[0..2].copy_from_slice(&handle_and_flags.to_le_bytes());
                buffer[2..4].copy_from_slice(&(length as u16).to_le_bytes());
                buffer[4..4 + length].copy_from_slice(packet.bytes());
                Some((PacketKind::AclData, 4 + length))
            }
        }
    }
}

/// Serialises a host packet the way `bt-hci` writes it, with the indicator byte first.
pub fn serialise_host_packet<Packet: bt_hci::transport::PacketToController>(
    packet: &Packet,
    buffer: &mut [u8],
) -> Option<usize> {
    let size = 1 + packet.size();
    if size > buffer.len() {
        return None;
    }
    let mut writer = &mut buffer[..size];
    bt_hci::transport::PacketToController::write_hci(&WithIndicator::new(packet), &mut writer).ok()?;
    Some(size)
}
