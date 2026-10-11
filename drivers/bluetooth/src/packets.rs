//! Link layer packets as the radio sends and receives them: a two-byte header, then the payload.

// ============================================================================
// Sizes
// ============================================================================

pub const HEADER_LENGTH: usize = 2;
pub const MAXIMUM_PAYLOAD_LENGTH: usize = 255;
pub const RADIO_BUFFER_LENGTH: usize = HEADER_LENGTH + MAXIMUM_PAYLOAD_LENGTH;

/// The largest data payload either side may send while the data length stays at its default.
pub const MAXIMUM_DATA_PAYLOAD_LENGTH: usize = 27;
pub const MAXIMUM_ADVERTISING_DATA_LENGTH: usize = 31;
pub const ADDRESS_LENGTH: usize = 6;

// ============================================================================
// Channels
// ============================================================================

pub const ADVERTISING_ACCESS_ADDRESS: u32 = 0x8E89_BED6;
pub const ADVERTISING_CRC_INITIAL_VALUE: u32 = 0x55_5555;
pub const DATA_CHANNEL_COUNT: u8 = 37;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Channel {
    pub index: u8,
    pub access_address: u32,
    pub crc_initial_value: u32,
}

impl Channel {
    pub const fn advertising(index: u8) -> Channel {
        Channel {
            index,
            access_address: ADVERTISING_ACCESS_ADDRESS,
            crc_initial_value: ADVERTISING_CRC_INITIAL_VALUE,
        }
    }

    pub const fn megahertz_above_2400(self) -> u8 {
        match self.index {
            37 => 2,
            38 => 26,
            39 => 80,
            0..=10 => 4 + 2 * self.index,
            _ => 6 + 2 * self.index,
        }
    }
}

// ============================================================================
// Device addresses
// ============================================================================

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeviceAddress {
    pub is_random: bool,
    pub bytes: [u8; ADDRESS_LENGTH],
}

impl DeviceAddress {
    fn read(bytes: &[u8], is_random: bool) -> DeviceAddress {
        let mut address = DeviceAddress {
            is_random,
            bytes: [0; ADDRESS_LENGTH],
        };
        address.bytes.copy_from_slice(&bytes[..ADDRESS_LENGTH]);
        address
    }
}

// ============================================================================
// Advertising packets
// ============================================================================

pub const CONNECTABLE_ADVERTISEMENT: u8 = 0;
pub const NON_CONNECTABLE_ADVERTISEMENT: u8 = 2;
pub const SCAN_REQUEST: u8 = 3;
pub const SCAN_RESPONSE: u8 = 4;
pub const CONNECT_REQUEST: u8 = 5;
pub const SCANNABLE_ADVERTISEMENT: u8 = 6;

const SCAN_REQUEST_LENGTH: u8 = 12;
const CONNECT_REQUEST_LENGTH: u8 = 34;

/// Writes an advertising packet that starts with the advertiser's address, as every one we send does.
pub fn write_advertising_packet(buffer: &mut [u8], packet_type: u8, advertiser: DeviceAddress, data: &[u8]) {
    let payload_length = ADDRESS_LENGTH + data.len();

    buffer[0] = packet_type | (u8::from(advertiser.is_random) << 6);
    buffer[1] = payload_length as u8;
    buffer[2..2 + ADDRESS_LENGTH].copy_from_slice(&advertiser.bytes);
    buffer[2 + ADDRESS_LENGTH..2 + payload_length].copy_from_slice(data);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanRequest {
    pub scanner: DeviceAddress,
    pub advertiser: DeviceAddress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectRequest {
    pub initiator: DeviceAddress,
    pub advertiser: DeviceAddress,
    pub access_address: u32,
    pub crc_initial_value: u32,
    pub window_size: u8,
    pub window_offset: u16,
    pub interval: u16,
    pub latency: u16,
    pub timeout: u16,
    pub channel_map: [u8; 5],
    pub hop_increment: u8,
    pub sleep_clock_accuracy: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceivedAdvertisingPacket {
    ScanRequest(ScanRequest),
    ConnectRequest(ConnectRequest),
    Other,
}

pub fn parse_advertising_packet(packet: &[u8]) -> ReceivedAdvertisingPacket {
    if packet.len() < HEADER_LENGTH {
        return ReceivedAdvertisingPacket::Other;
    }

    let packet_type = packet[0] & 0x0F;
    let sender_is_random = packet[0] & 0x40 != 0;
    let receiver_is_random = packet[0] & 0x80 != 0;
    let payload_length = packet[1];
    let payload = &packet[HEADER_LENGTH..];

    if packet_type == SCAN_REQUEST && payload_length == SCAN_REQUEST_LENGTH && payload.len() >= 12 {
        return ReceivedAdvertisingPacket::ScanRequest(ScanRequest {
            scanner: DeviceAddress::read(&payload[0..6], sender_is_random),
            advertiser: DeviceAddress::read(&payload[6..12], receiver_is_random),
        });
    }

    if packet_type == CONNECT_REQUEST && payload_length == CONNECT_REQUEST_LENGTH && payload.len() >= 34 {
        let link_data = &payload[12..34];
        let read_u16 = |offset: usize| u16::from_le_bytes([link_data[offset], link_data[offset + 1]]);

        let mut channel_map = [0; 5];
        channel_map.copy_from_slice(&link_data[16..21]);

        return ReceivedAdvertisingPacket::ConnectRequest(ConnectRequest {
            initiator: DeviceAddress::read(&payload[0..6], sender_is_random),
            advertiser: DeviceAddress::read(&payload[6..12], receiver_is_random),
            access_address: u32::from_le_bytes([link_data[0], link_data[1], link_data[2], link_data[3]]),
            crc_initial_value: u32::from_le_bytes([link_data[4], link_data[5], link_data[6], 0]),
            window_size: link_data[7],
            window_offset: read_u16(8),
            interval: read_u16(10),
            latency: read_u16(12),
            timeout: read_u16(14),
            channel_map,
            hop_increment: link_data[21] & 0x1F,
            sleep_clock_accuracy: link_data[21] >> 5,
        });
    }

    ReceivedAdvertisingPacket::Other
}

// ============================================================================
// Data packets
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum DataKind {
    /// The rest of a host message, or an empty packet that only carries acknowledgements.
    Continuation = 1,
    /// The start of a host message.
    Start = 2,
    Control = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataHeader {
    pub kind: DataKind,
    pub next_expected_sequence_number: bool,
    pub sequence_number: bool,
    pub more_data: bool,
    pub length: u8,
}

impl DataHeader {
    pub fn parse(packet: &[u8]) -> Option<DataHeader> {
        if packet.len() < HEADER_LENGTH {
            return None;
        }

        let kind = match packet[0] & 0x03 {
            1 => DataKind::Continuation,
            2 => DataKind::Start,
            3 => DataKind::Control,
            _ => return None,
        };

        Some(DataHeader {
            kind,
            next_expected_sequence_number: packet[0] & 0x04 != 0,
            sequence_number: packet[0] & 0x08 != 0,
            more_data: packet[0] & 0x10 != 0,
            length: packet[1],
        })
    }

    pub fn write(self, buffer: &mut [u8]) {
        buffer[0] = self.kind as u8
            | (u8::from(self.next_expected_sequence_number) << 2)
            | (u8::from(self.sequence_number) << 3)
            | (u8::from(self.more_data) << 4);
        buffer[1] = self.length;
    }
}

/// A data packet waiting to go out, without the acknowledgement bits that are only known when it is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataPacket {
    pub kind: DataKind,
    pub length: u8,
    pub payload: [u8; MAXIMUM_DATA_PAYLOAD_LENGTH],
}

impl DataPacket {
    pub const EMPTY: DataPacket = DataPacket {
        kind: DataKind::Continuation,
        length: 0,
        payload: [0; MAXIMUM_DATA_PAYLOAD_LENGTH],
    };

    pub fn new(kind: DataKind, payload: &[u8]) -> DataPacket {
        let mut packet = DataPacket {
            kind,
            length: payload.len() as u8,
            ..DataPacket::EMPTY
        };
        packet.payload[..payload.len()].copy_from_slice(payload);
        packet
    }

    pub fn control(opcode: u8, parameters: &[u8]) -> DataPacket {
        let mut packet = DataPacket {
            kind: DataKind::Control,
            length: 1 + parameters.len() as u8,
            ..DataPacket::EMPTY
        };
        packet.payload[0] = opcode;
        packet.payload[1..1 + parameters.len()].copy_from_slice(parameters);
        packet
    }

    pub fn bytes(&self) -> &[u8] {
        &self.payload[..self.length as usize]
    }

    pub fn is_empty(&self) -> bool {
        self.kind == DataKind::Continuation && self.length == 0
    }
}

// ============================================================================
// Control procedures
// ============================================================================

pub const CONNECTION_UPDATE_INDICATION: u8 = 0x00;
pub const CHANNEL_MAP_INDICATION: u8 = 0x01;
pub const TERMINATE_INDICATION: u8 = 0x02;
pub const UNKNOWN_RESPONSE: u8 = 0x07;
pub const FEATURE_REQUEST: u8 = 0x08;
pub const FEATURE_RESPONSE: u8 = 0x09;
pub const VERSION_INDICATION: u8 = 0x0C;
pub const REJECT_INDICATION: u8 = 0x0D;
pub const PING_REQUEST: u8 = 0x12;
pub const PING_RESPONSE: u8 = 0x13;
pub const REJECT_EXTENDED_INDICATION: u8 = 0x11;
