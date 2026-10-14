//! The link layer: advertising, then one connection as the peripheral, timed to the microsecond.
//!
//! It never touches hardware. The radio driver reports what happened on air and runs the
//! `RadioOperation` that comes back, so the whole state machine runs in tests on a PC.

use heapless::{Deque, Vec};

use crate::packets::*;

// ============================================================================
// Radio timing, in microseconds on a clock that wraps
// ============================================================================

pub const INTERFRAME_SPACING: u32 = 150;
const RAMP_UP: u32 = 40;
const RECEIVE_CHAIN_DELAY: u32 = 10;
const TRANSMIT_CHAIN_DELAY: u32 = 1;
const PREAMBLE_AND_ACCESS_ADDRESS: u32 = 40;
const TIMING_JITTER: u32 = 16;
const SAFETY_MARGIN: u32 = 4;

/// From the end of a received packet to the transmit enable that puts our reply on air one spacing later.
pub const REPLY_DELAY: u32 = INTERFRAME_SPACING - RECEIVE_CHAIN_DELAY - RAMP_UP - TRANSMIT_CHAIN_DELAY;
/// From the end of our packet to the receive enable that catches a reply one spacing later.
pub const LISTEN_DELAY: u32 = INTERFRAME_SPACING - RAMP_UP - 10;
/// From the end of our packet until a reply must have started.
pub const LISTEN_DEADLINE: u32 =
    INTERFRAME_SPACING + PREAMBLE_AND_ACCESS_ADDRESS + RECEIVE_CHAIN_DELAY + TIMING_JITTER;

const OWN_CLOCK_ACCURACY_IN_PPM: u32 = 50;
const UNIT_1250_MICROSECONDS: u32 = 1250;
const UNIT_625_MICROSECONDS: u32 = 625;
const UNIT_10_MILLISECONDS: u32 = 10_000;

const ADVERTISING_START_DELAY: u32 = 500;
const GAP_BETWEEN_ADVERTISING_CHANNELS: u32 = 400;
const MAXIMUM_ADVERTISING_DELAY: u32 = 10_000;
const MINIMUM_SCHEDULING_LEAD: u32 = 60;
const TIME_FOR_ANOTHER_EXCHANGE: u32 = 1_000;

fn microseconds_from(start: u32, end: u32) -> i32 {
    end.wrapping_sub(start) as i32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadioOperation {
    Off,
    /// Send the transmit buffer, enabling the transmitter at `start_at`.
    Transmit {
        start_at: u32,
        channel: Channel,
    },
    /// Listen from `start_at`, and give up at `give_up_at` unless a packet has started arriving.
    Receive {
        start_at: u32,
        give_up_at: u32,
        channel: Channel,
    },
}

// ============================================================================
// What the link layer tells the host
// ============================================================================

pub const OUTGOING_QUEUE_LENGTH: usize = 4;
pub const EVENT_QUEUE_LENGTH: usize = 8;

/// Slots kept free for connection events, so incoming data can never crowd them out.
const EVENT_SLOTS_KEPT_FOR_NOTICES: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkEvent {
    Connected {
        peer: DeviceAddress,
        interval: u16,
        latency: u16,
        timeout: u16,
        clock_accuracy: u8,
    },
    ParametersChanged {
        interval: u16,
        latency: u16,
        timeout: u16,
    },
    Disconnected {
        reason: u8,
    },
    Received(DataPacket),
}

pub mod disconnect_reason {
    pub const CONNECTION_TIMEOUT: u8 = 0x08;
    pub const REMOTE_USER_TERMINATED: u8 = 0x13;
    pub const LOCAL_HOST_TERMINATED: u8 = 0x16;
    pub const INSTANT_PASSED: u8 = 0x28;
    pub const FAILED_TO_ESTABLISH: u8 = 0x3E;
}

// ============================================================================
// Advertising settings, as the host sets them
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdvertisingKind {
    Connectable,
    Scannable,
    NonConnectable,
}

pub const ACCEPT_LIST_LENGTH: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdvertisingSettings {
    pub interval_in_microseconds: u32,
    pub kind: AdvertisingKind,
    pub channel_map: u8,
    pub filter_scan_requests: bool,
    pub filter_connect_requests: bool,
    pub own_address: DeviceAddress,
    pub advertising_data: Vec<u8, MAXIMUM_ADVERTISING_DATA_LENGTH>,
    pub scan_response_data: Vec<u8, MAXIMUM_ADVERTISING_DATA_LENGTH>,
    pub accept_list: Vec<DeviceAddress, ACCEPT_LIST_LENGTH>,
}

impl AdvertisingSettings {
    pub const fn new() -> AdvertisingSettings {
        AdvertisingSettings {
            interval_in_microseconds: 0x0800 * UNIT_625_MICROSECONDS,
            kind: AdvertisingKind::Connectable,
            channel_map: 0b111,
            filter_scan_requests: false,
            filter_connect_requests: false,
            own_address: DeviceAddress {
                is_random: true,
                bytes: [0; ADDRESS_LENGTH],
            },
            advertising_data: Vec::new(),
            scan_response_data: Vec::new(),
            accept_list: Vec::new(),
        }
    }

    fn channels(&self) -> impl Iterator<Item = u8> + '_ {
        (0..3)
            .filter(|bit| self.channel_map & (1 << bit) != 0)
            .map(|bit| 37 + bit)
    }

    fn allows(&self, peer: DeviceAddress, filtered: bool) -> bool {
        !filtered || self.accept_list.contains(&peer)
    }

    fn packet_type(&self) -> u8 {
        match self.kind {
            AdvertisingKind::Connectable => CONNECTABLE_ADVERTISEMENT,
            AdvertisingKind::Scannable => SCANNABLE_ADVERTISEMENT,
            AdvertisingKind::NonConnectable => NON_CONNECTABLE_ADVERTISEMENT,
        }
    }
}

impl Default for AdvertisingSettings {
    fn default() -> Self {
        AdvertisingSettings::new()
    }
}

// ============================================================================
// Advertising
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AdvertisingStep {
    SentAdvertisement,
    Listening,
    SentScanResponse,
}

#[derive(Clone, Copy, Debug)]
struct Advertiser {
    event_start: u32,
    channel_position: usize,
    step: AdvertisingStep,
}

// ============================================================================
// Connection
// ============================================================================

const SLEEP_CLOCK_ACCURACY_IN_PPM: [u32; 8] = [500, 250, 150, 100, 75, 50, 30, 20];
const CONTROL_REPLY_QUEUE_LENGTH: usize = 4;

#[derive(Clone, Copy, Debug)]
struct ChannelMap {
    bits: [u8; 5],
    used_channels: [u8; DATA_CHANNEL_COUNT as usize],
    used_count: u8,
}

impl ChannelMap {
    fn new(bits: [u8; 5]) -> ChannelMap {
        let mut map = ChannelMap {
            bits,
            used_channels: [0; DATA_CHANNEL_COUNT as usize],
            used_count: 0,
        };
        for channel in 0..DATA_CHANNEL_COUNT {
            if map.is_used(channel) {
                map.used_channels[map.used_count as usize] = channel;
                map.used_count += 1;
            }
        }
        map
    }

    fn is_used(&self, channel: u8) -> bool {
        self.bits[channel as usize / 8] & (1 << (channel % 8)) != 0
    }
}

#[derive(Clone, Copy, Debug)]
struct ParameterUpdate {
    instant: u16,
    window_size: u8,
    window_offset: u16,
    interval: u16,
    latency: u16,
    timeout: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventStep {
    WaitingForFirstPacket,
    Replying { keep_going: bool },
    Listening,
}

#[derive(Clone, Copy, Debug)]
struct PacketInFlight {
    packet: DataPacket,
    from_host: bool,
    is_terminate: bool,
}

#[derive(Clone, Debug)]
struct Connection {
    access_address: u32,
    crc_initial_value: u32,

    interval: u16,
    latency: u16,
    timeout: u16,
    combined_clock_accuracy_in_ppm: u32,

    channel_map: ChannelMap,
    hop_increment: u8,
    unmapped_channel: u8,
    channel: u8,
    event_counter: u16,

    /// When the central's first packet of the current event should be heard.
    anchor: u32,
    /// How long after the anchor the central may still start, while it is inside a transmit window.
    window_length: u32,
    last_synchronised_anchor: u32,
    last_heard: u32,
    created_at: u32,
    established: bool,

    sequence_number: bool,
    next_expected_sequence_number: bool,
    in_flight: Option<PacketInFlight>,
    control_replies: Deque<DataPacket, CONTROL_REPLY_QUEUE_LENGTH>,
    terminate_reason: Option<u8>,
    leave_after_reply: Option<u8>,
    version_sent: bool,
    pending_update: Option<ParameterUpdate>,
    pending_channel_map: Option<(u16, [u8; 5])>,

    step: EventStep,
    consecutive_crc_errors: u8,
}

impl Connection {
    fn interval_in_microseconds(&self) -> u32 {
        self.interval as u32 * UNIT_1250_MICROSECONDS
    }

    fn channel(&self) -> Channel {
        Channel {
            index: self.channel,
            access_address: self.access_address,
            crc_initial_value: self.crc_initial_value,
        }
    }

    fn select_next_channel(&mut self) {
        self.unmapped_channel = (self.unmapped_channel + self.hop_increment) % DATA_CHANNEL_COUNT;
        self.channel = if self.channel_map.is_used(self.unmapped_channel) {
            self.unmapped_channel
        } else {
            self.channel_map.used_channels[(self.unmapped_channel % self.channel_map.used_count) as usize]
        };
    }

    fn window_widening(&self) -> u32 {
        let time_since_sync = self
            .anchor
            .wrapping_add(self.window_length)
            .wrapping_sub(self.last_synchronised_anchor);
        let drift = (self.combined_clock_accuracy_in_ppm as u64 * time_since_sync as u64 / 1_000_000) as u32;
        let widening = drift + TIMING_JITTER;
        let limit = self.interval_in_microseconds() / 2 - INTERFRAME_SPACING;
        widening.min(limit)
    }

    /// The listen for the central's first packet, which stays open across a transmit window still unheard.
    fn listen_for_anchor(&self) -> (u32, u32) {
        let widening = self.window_widening();
        let lead = widening + PREAMBLE_AND_ACCESS_ADDRESS + RECEIVE_CHAIN_DELAY + RAMP_UP + SAFETY_MARGIN;
        let start_at = self.anchor.wrapping_sub(lead);
        let room_before_next_event = self
            .interval_in_microseconds()
            .saturating_sub(lead + MINIMUM_SCHEDULING_LEAD);
        let listen_after_anchor = (self.window_length + widening + SAFETY_MARGIN).min(room_before_next_event);
        let give_up_at = self.anchor.wrapping_add(listen_after_anchor);
        (start_at, give_up_at)
    }
}

// ============================================================================
// Link layer
// ============================================================================

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
enum State {
    Standby,
    Advertising(Advertiser),
    Connected(Connection),
}

pub struct LinkLayer {
    state: State,
    pub settings: AdvertisingSettings,
    pub outgoing: Deque<DataPacket, OUTGOING_QUEUE_LENGTH>,
    pub events: Deque<LinkEvent, EVENT_QUEUE_LENGTH>,
    /// Host packets the central has acknowledged since the host last asked.
    pub completed_packets: u16,
    pub last_signal_strength: i8,
    random_state: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    NotAllowedNow,
    NothingToAdvertiseOn,
}

impl LinkLayer {
    pub const fn new(random_seed: u32) -> LinkLayer {
        LinkLayer {
            state: State::Standby,
            settings: AdvertisingSettings::new(),
            outgoing: Deque::new(),
            events: Deque::new(),
            completed_packets: 0,
            last_signal_strength: 127,
            random_state: random_seed | 1,
        }
    }

    pub fn is_advertising(&self) -> bool {
        matches!(self.state, State::Advertising(_))
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.state, State::Connected(_))
    }

    pub fn current_interval(&self) -> Option<u16> {
        match &self.state {
            State::Connected(connection) => Some(connection.interval),
            _ => None,
        }
    }

    pub fn reset(&mut self) -> RadioOperation {
        self.state = State::Standby;
        self.outgoing.clear();
        self.events.clear();
        self.completed_packets = 0;
        self.settings = AdvertisingSettings::new();
        RadioOperation::Off
    }

    /// Starts advertising, or gives no new operation when it is already running.
    pub fn start_advertising(
        &mut self,
        now: u32,
        transmit_buffer: &mut [u8],
    ) -> Result<Option<RadioOperation>, Refusal> {
        match self.state {
            State::Advertising(_) => return Ok(None),
            State::Connected(_) => return Err(Refusal::NotAllowedNow),
            State::Standby => {}
        }
        if self.settings.channels().next().is_none() {
            return Err(Refusal::NothingToAdvertiseOn);
        }

        let event_start = now.wrapping_add(ADVERTISING_START_DELAY);
        self.state = State::Advertising(Advertiser {
            event_start,
            channel_position: 0,
            step: AdvertisingStep::SentAdvertisement,
        });
        Ok(Some(self.send_advertisement(event_start, transmit_buffer)))
    }

    pub fn stop_advertising(&mut self) -> Option<RadioOperation> {
        if self.is_advertising() {
            self.state = State::Standby;
            Some(RadioOperation::Off)
        } else {
            None
        }
    }

    pub fn disconnect(&mut self, reason: u8) -> Result<(), Refusal> {
        match &mut self.state {
            State::Connected(connection) => {
                connection.terminate_reason.get_or_insert(reason);
                Ok(())
            }
            _ => Err(Refusal::NotAllowedNow),
        }
    }

    pub fn send_to_peer(&mut self, packet: DataPacket) -> Result<(), DataPacket> {
        if !self.is_connected() {
            return Ok(());
        }
        self.outgoing.push_back(packet)
    }

    fn next_random(&mut self) -> u32 {
        let mut value = self.random_state;
        value ^= value << 13;
        value ^= value >> 17;
        value ^= value << 5;
        self.random_state = value;
        value
    }

    fn push_notice(&mut self, event: LinkEvent) {
        if self.events.is_full() {
            self.events.pop_back();
        }
        let _ = self.events.push_back(event);
    }

    fn has_room_for_incoming_data(&self) -> bool {
        self.events.len() + EVENT_SLOTS_KEPT_FOR_NOTICES < EVENT_QUEUE_LENGTH
    }

    // ------------------------------------------------------------------------
    // What the radio reports
    // ------------------------------------------------------------------------

    pub fn on_transmitted(&mut self, end_time: u32, transmit_buffer: &mut [u8]) -> RadioOperation {
        match self.state {
            State::Standby => RadioOperation::Off,
            State::Advertising(_) => self.advertising_transmitted(end_time, transmit_buffer),
            State::Connected(_) => self.connection_transmitted(end_time),
        }
    }

    pub fn on_received(
        &mut self,
        packet: &[u8],
        crc_is_valid: bool,
        address_time: u32,
        end_time: u32,
        signal_strength: i8,
        transmit_buffer: &mut [u8],
    ) -> RadioOperation {
        self.last_signal_strength = signal_strength;
        match self.state {
            State::Standby => RadioOperation::Off,
            State::Advertising(_) => {
                self.advertising_received(packet, crc_is_valid, end_time, transmit_buffer)
            }
            State::Connected(_) => {
                self.connection_received(packet, crc_is_valid, address_time, end_time, transmit_buffer)
            }
        }
    }

    pub fn on_nothing_received(&mut self, now: u32, transmit_buffer: &mut [u8]) -> RadioOperation {
        match self.state {
            State::Standby => RadioOperation::Off,
            State::Advertising(_) => self.next_advertising_channel(now, transmit_buffer),
            State::Connected(_) => self.close_connection_event(now),
        }
    }

    // ------------------------------------------------------------------------
    // Advertising events
    // ------------------------------------------------------------------------

    fn advertiser(&mut self) -> &mut Advertiser {
        match &mut self.state {
            State::Advertising(advertiser) => advertiser,
            _ => unreachable!("only called while advertising"),
        }
    }

    fn current_advertising_channel(&mut self) -> Channel {
        let position = self.advertiser().channel_position;
        let index = self.settings.channels().nth(position).unwrap_or(37);
        Channel::advertising(index)
    }

    fn send_advertisement(&mut self, start_at: u32, transmit_buffer: &mut [u8]) -> RadioOperation {
        write_advertising_packet(
            transmit_buffer,
            self.settings.packet_type(),
            self.settings.own_address,
            &self.settings.advertising_data,
        );
        self.advertiser().step = AdvertisingStep::SentAdvertisement;
        RadioOperation::Transmit {
            start_at,
            channel: self.current_advertising_channel(),
        }
    }

    fn advertising_transmitted(&mut self, end_time: u32, transmit_buffer: &mut [u8]) -> RadioOperation {
        let step = self.advertiser().step;
        let expects_reply = self.settings.kind != AdvertisingKind::NonConnectable;

        if step == AdvertisingStep::SentAdvertisement && expects_reply {
            self.advertiser().step = AdvertisingStep::Listening;
            return RadioOperation::Receive {
                start_at: end_time.wrapping_add(LISTEN_DELAY),
                give_up_at: end_time.wrapping_add(LISTEN_DEADLINE),
                channel: self.current_advertising_channel(),
            };
        }

        self.next_advertising_channel(end_time, transmit_buffer)
    }

    fn next_advertising_channel(&mut self, now: u32, transmit_buffer: &mut [u8]) -> RadioOperation {
        let channel_count = self.settings.channels().count();
        let advertiser = self.advertiser();
        advertiser.channel_position += 1;

        if advertiser.channel_position < channel_count {
            let start_at = now.wrapping_add(GAP_BETWEEN_ADVERTISING_CHANNELS);
            return self.send_advertisement(start_at, transmit_buffer);
        }

        let random_delay = self.next_random() % (MAXIMUM_ADVERTISING_DELAY + 1);
        let interval = self.settings.interval_in_microseconds;
        let advertiser = self.advertiser();
        advertiser.channel_position = 0;
        advertiser.event_start = advertiser.event_start.wrapping_add(interval + random_delay);
        if microseconds_from(now, advertiser.event_start) < MINIMUM_SCHEDULING_LEAD as i32 {
            advertiser.event_start = now.wrapping_add(ADVERTISING_START_DELAY);
        }

        let event_start = advertiser.event_start;
        self.send_advertisement(event_start, transmit_buffer)
    }

    fn advertising_received(
        &mut self,
        packet: &[u8],
        crc_is_valid: bool,
        end_time: u32,
        transmit_buffer: &mut [u8],
    ) -> RadioOperation {
        if !crc_is_valid {
            return self.next_advertising_channel(end_time, transmit_buffer);
        }

        let settings = &self.settings;
        match parse_advertising_packet(packet) {
            ReceivedAdvertisingPacket::ScanRequest(request)
                if request.advertiser == settings.own_address
                    && settings.kind != AdvertisingKind::NonConnectable
                    && settings.allows(request.scanner, settings.filter_scan_requests) =>
            {
                write_advertising_packet(
                    transmit_buffer,
                    SCAN_RESPONSE,
                    settings.own_address,
                    &settings.scan_response_data,
                );
                let channel = self.current_advertising_channel();
                self.advertiser().step = AdvertisingStep::SentScanResponse;
                RadioOperation::Transmit {
                    start_at: end_time.wrapping_add(REPLY_DELAY),
                    channel,
                }
            }

            ReceivedAdvertisingPacket::ConnectRequest(request)
                if request.advertiser == settings.own_address
                    && settings.kind == AdvertisingKind::Connectable
                    && settings.allows(request.initiator, settings.filter_connect_requests)
                    && connect_request_is_valid(&request) =>
            {
                self.connect(request, end_time)
            }

            _ => self.next_advertising_channel(end_time, transmit_buffer),
        }
    }

    // ------------------------------------------------------------------------
    // Connection events
    // ------------------------------------------------------------------------

    fn connection(&mut self) -> &mut Connection {
        match &mut self.state {
            State::Connected(connection) => connection,
            _ => unreachable!("only called while connected"),
        }
    }

    fn connect(&mut self, request: ConnectRequest, end_time: u32) -> RadioOperation {
        let central_accuracy = SLEEP_CLOCK_ACCURACY_IN_PPM[request.sleep_clock_accuracy as usize & 7];
        let first_window_start = end_time
            .wrapping_add(UNIT_1250_MICROSECONDS + PREAMBLE_AND_ACCESS_ADDRESS)
            .wrapping_add(request.window_offset as u32 * UNIT_1250_MICROSECONDS);

        let mut connection = Connection {
            access_address: request.access_address,
            crc_initial_value: request.crc_initial_value,
            interval: request.interval,
            latency: request.latency,
            timeout: request.timeout,
            combined_clock_accuracy_in_ppm: central_accuracy + OWN_CLOCK_ACCURACY_IN_PPM,
            channel_map: ChannelMap::new(request.channel_map),
            hop_increment: request.hop_increment,
            unmapped_channel: 0,
            channel: 0,
            event_counter: 0,
            anchor: first_window_start,
            window_length: request.window_size as u32 * UNIT_1250_MICROSECONDS,
            last_synchronised_anchor: end_time,
            last_heard: end_time,
            created_at: end_time,
            established: false,
            sequence_number: false,
            next_expected_sequence_number: false,
            in_flight: None,
            control_replies: Deque::new(),
            terminate_reason: None,
            leave_after_reply: None,
            version_sent: false,
            pending_update: None,
            pending_channel_map: None,
            step: EventStep::WaitingForFirstPacket,
            consecutive_crc_errors: 0,
        };
        connection.select_next_channel();

        let (start_at, give_up_at) = connection.listen_for_anchor();
        let channel = connection.channel();
        self.state = State::Connected(connection);
        self.push_notice(LinkEvent::Connected {
            peer: request.initiator,
            interval: request.interval,
            latency: request.latency,
            timeout: request.timeout,
            clock_accuracy: request.sleep_clock_accuracy,
        });

        RadioOperation::Receive {
            start_at,
            give_up_at,
            channel,
        }
    }

    fn end_connection(&mut self, reason: u8) -> RadioOperation {
        self.state = State::Standby;
        self.outgoing.clear();
        self.completed_packets = 0;
        self.push_notice(LinkEvent::Disconnected { reason });
        RadioOperation::Off
    }

    fn connection_received(
        &mut self,
        packet: &[u8],
        crc_is_valid: bool,
        address_time: u32,
        end_time: u32,
        transmit_buffer: &mut [u8],
    ) -> RadioOperation {
        let connection = self.connection();
        if connection.step == EventStep::WaitingForFirstPacket {
            connection.anchor = address_time;
            connection.last_synchronised_anchor = address_time;
            connection.window_length = 0;
        }

        let mut central_has_more = false;
        if crc_is_valid {
            let Some(header) = DataHeader::parse(packet) else {
                return self.close_connection_event(end_time);
            };

            let connection = self.connection();
            connection.last_heard = address_time;
            connection.established = true;
            connection.consecutive_crc_errors = 0;
            central_has_more = header.more_data;

            if header.next_expected_sequence_number != connection.sequence_number {
                connection.sequence_number = !connection.sequence_number;
                if let Some(acknowledged) = connection.in_flight.take() {
                    if acknowledged.is_terminate {
                        return self.end_connection(disconnect_reason::LOCAL_HOST_TERMINATED);
                    }
                    if acknowledged.from_host {
                        self.completed_packets += 1;
                    }
                }
            }

            let connection = self.connection();
            if header.sequence_number == connection.next_expected_sequence_number {
                let payload_end = (HEADER_LENGTH + header.length as usize).min(packet.len());
                let payload = &packet[HEADER_LENGTH..payload_end];
                match self.accept_from_central(header.kind, payload) {
                    Acceptance::Accepted => {
                        let connection = self.connection();
                        connection.next_expected_sequence_number = !connection.next_expected_sequence_number;
                    }
                    Acceptance::NoRoom => {}
                    Acceptance::ConnectionLost(reason) => return self.end_connection(reason),
                }
            }
        } else {
            self.connection().consecutive_crc_errors += 1;
        }

        let reply_more_data = self.prepare_reply(transmit_buffer);

        let connection = self.connection();
        let next_anchor = connection
            .anchor
            .wrapping_add(connection.interval_in_microseconds());
        let time_left = microseconds_from(end_time, next_anchor);
        let keep_going = crc_is_valid
            && (central_has_more || reply_more_data)
            && time_left > (TIME_FOR_ANOTHER_EXCHANGE + connection.window_widening()) as i32;
        connection.step = EventStep::Replying { keep_going };

        RadioOperation::Transmit {
            start_at: end_time.wrapping_add(REPLY_DELAY),
            channel: connection.channel(),
        }
    }

    /// Writes the next packet to send into the buffer, and says whether more are waiting after it.
    fn prepare_reply(&mut self, transmit_buffer: &mut [u8]) -> bool {
        if self.connection().in_flight.is_none() {
            let next = self.next_packet_to_send();
            self.connection().in_flight = Some(next);
        }

        let more_waiting = !self.outgoing.is_empty() || !self.connection().control_replies.is_empty();
        let connection = self.connection();
        let in_flight = connection.in_flight.as_ref().map(|in_flight| in_flight.packet);
        let packet = in_flight.unwrap_or(DataPacket::EMPTY);

        DataHeader {
            kind: packet.kind,
            next_expected_sequence_number: connection.next_expected_sequence_number,
            sequence_number: connection.sequence_number,
            more_data: more_waiting,
            length: packet.length,
        }
        .write(transmit_buffer);
        transmit_buffer[HEADER_LENGTH..HEADER_LENGTH + packet.length as usize]
            .copy_from_slice(packet.bytes());

        more_waiting
    }

    fn next_packet_to_send(&mut self) -> PacketInFlight {
        let connection = self.connection();
        if let Some(reply) = connection.control_replies.pop_front() {
            return PacketInFlight {
                packet: reply,
                from_host: false,
                is_terminate: false,
            };
        }
        if let Some(reason) = connection.terminate_reason.take() {
            return PacketInFlight {
                packet: DataPacket::control(TERMINATE_INDICATION, &[reason]),
                from_host: false,
                is_terminate: true,
            };
        }
        if let Some(packet) = self.outgoing.pop_front() {
            return PacketInFlight {
                packet,
                from_host: true,
                is_terminate: false,
            };
        }
        PacketInFlight {
            packet: DataPacket::EMPTY,
            from_host: false,
            is_terminate: false,
        }
    }

    fn connection_transmitted(&mut self, end_time: u32) -> RadioOperation {
        if let Some(reason) = self.connection().leave_after_reply {
            return self.end_connection(reason);
        }

        let connection = self.connection();
        match connection.step {
            EventStep::Replying { keep_going: true } => {
                connection.step = EventStep::Listening;
                RadioOperation::Receive {
                    start_at: end_time.wrapping_add(LISTEN_DELAY),
                    give_up_at: end_time.wrapping_add(LISTEN_DEADLINE),
                    channel: connection.channel(),
                }
            }
            _ => self.close_connection_event(end_time),
        }
    }

    /// Ends a connection the radio has stopped serving, for a timer to call, since the check in
    /// `close_connection_event` only runs when the radio reports something. A link that is still
    /// running ends there first, as it allows one interval less.
    pub fn check_supervision(&mut self, now: u32) -> Option<RadioOperation> {
        let State::Connected(connection) = &self.state else {
            return None;
        };
        let (since, limit) = if connection.established {
            (
                connection.last_heard,
                connection.timeout as u32 * UNIT_10_MILLISECONDS,
            )
        } else {
            (connection.created_at, 6 * connection.interval_in_microseconds())
        };
        let limit_with_margin = limit + connection.interval_in_microseconds();
        if microseconds_from(since, now) > limit_with_margin as i32 {
            Some(self.end_connection(disconnect_reason::CONNECTION_TIMEOUT))
        } else {
            None
        }
    }

    fn close_connection_event(&mut self, now: u32) -> RadioOperation {
        let connection = self.connection();
        connection.step = EventStep::WaitingForFirstPacket;

        let supervision_limit = connection.timeout as u32 * UNIT_10_MILLISECONDS;
        if connection.established && microseconds_from(connection.last_heard, now) > supervision_limit as i32
        {
            return self.end_connection(disconnect_reason::CONNECTION_TIMEOUT);
        }
        let establish_limit = 6 * connection.interval_in_microseconds();
        if !connection.established && microseconds_from(connection.created_at, now) >= establish_limit as i32
        {
            return self.end_connection(disconnect_reason::FAILED_TO_ESTABLISH);
        }

        loop {
            if let Some(changed) = self.advance_to_next_event() {
                self.push_notice(changed);
            }
            let connection = self.connection();
            let (start_at, give_up_at) = connection.listen_for_anchor();
            if microseconds_from(now, start_at) >= MINIMUM_SCHEDULING_LEAD as i32 {
                return RadioOperation::Receive {
                    start_at,
                    give_up_at,
                    channel: connection.channel(),
                };
            }
        }
    }

    /// Moves on one connection event, applying any update whose instant has come.
    fn advance_to_next_event(&mut self) -> Option<LinkEvent> {
        let connection = self.connection();
        connection.anchor = connection
            .anchor
            .wrapping_add(connection.interval_in_microseconds());
        connection.event_counter = connection.event_counter.wrapping_add(1);
        connection.consecutive_crc_errors = 0;

        if let Some((instant, bits)) = connection.pending_channel_map {
            if instant == connection.event_counter {
                connection.channel_map = ChannelMap::new(bits);
                connection.pending_channel_map = None;
            }
        }

        let mut changed = None;
        if let Some(update) = connection.pending_update {
            if update.instant == connection.event_counter {
                connection.anchor = connection
                    .anchor
                    .wrapping_add(update.window_offset as u32 * UNIT_1250_MICROSECONDS);
                connection.window_length = update.window_size as u32 * UNIT_1250_MICROSECONDS;
                let parameters_differ = (update.interval, update.latency, update.timeout)
                    != (connection.interval, connection.latency, connection.timeout);
                connection.interval = update.interval;
                connection.latency = update.latency;
                connection.timeout = update.timeout;
                connection.pending_update = None;
                if parameters_differ {
                    changed = Some(LinkEvent::ParametersChanged {
                        interval: update.interval,
                        latency: update.latency,
                        timeout: update.timeout,
                    });
                }
            }
        }

        connection.select_next_channel();
        changed
    }

    // ------------------------------------------------------------------------
    // Packets from the central
    // ------------------------------------------------------------------------

    fn accept_from_central(&mut self, kind: DataKind, payload: &[u8]) -> Acceptance {
        match kind {
            DataKind::Control => self.handle_control(payload),
            _ if payload.is_empty() => Acceptance::Accepted,
            _ if !self.has_room_for_incoming_data() => Acceptance::NoRoom,
            _ => {
                let length = payload.len().min(MAXIMUM_DATA_PAYLOAD_LENGTH);
                let _ = self
                    .events
                    .push_back(LinkEvent::Received(DataPacket::new(kind, &payload[..length])));
                Acceptance::Accepted
            }
        }
    }

    fn handle_control(&mut self, payload: &[u8]) -> Acceptance {
        let Some((&opcode, parameters)) = payload.split_first() else {
            return Acceptance::Accepted;
        };
        let connection = self.connection();
        let read_u16 = |offset: usize| u16::from_le_bytes([parameters[offset], parameters[offset + 1]]);

        match opcode {
            CONNECTION_UPDATE_INDICATION if parameters.len() >= 11 => {
                let update = ParameterUpdate {
                    window_size: parameters[0],
                    window_offset: read_u16(1),
                    interval: read_u16(3),
                    latency: read_u16(5),
                    timeout: read_u16(7),
                    instant: read_u16(9),
                };
                if !instant_is_ahead(update.instant, connection.event_counter) {
                    return Acceptance::ConnectionLost(disconnect_reason::INSTANT_PASSED);
                }
                connection.pending_update = Some(update);
            }

            CHANNEL_MAP_INDICATION if parameters.len() >= 7 => {
                let mut bits = [0; 5];
                bits.copy_from_slice(&parameters[0..5]);
                let instant = read_u16(5);
                if !instant_is_ahead(instant, connection.event_counter) {
                    return Acceptance::ConnectionLost(disconnect_reason::INSTANT_PASSED);
                }
                connection.pending_channel_map = Some((instant, bits));
            }

            TERMINATE_INDICATION => {
                let reason = parameters
                    .first()
                    .copied()
                    .unwrap_or(disconnect_reason::REMOTE_USER_TERMINATED);
                connection.leave_after_reply = Some(reason);
            }

            FEATURE_REQUEST => {
                let no_optional_features = [0; 8];
                return self
                    .queue_control_reply(DataPacket::control(FEATURE_RESPONSE, &no_optional_features));
            }

            VERSION_INDICATION if !connection.version_sent => {
                let acceptance = self.queue_control_reply(DataPacket::control(VERSION_INDICATION, &VERSION));
                if matches!(acceptance, Acceptance::Accepted) {
                    self.connection().version_sent = true;
                }
                return acceptance;
            }

            PING_REQUEST => return self.queue_control_reply(DataPacket::control(PING_RESPONSE, &[])),

            VERSION_INDICATION
            | UNKNOWN_RESPONSE
            | REJECT_INDICATION
            | REJECT_EXTENDED_INDICATION
            | FEATURE_RESPONSE
            | PING_RESPONSE => {}

            _ => return self.queue_control_reply(DataPacket::control(UNKNOWN_RESPONSE, &[opcode])),
        }
        Acceptance::Accepted
    }

    /// Queues a control reply, or refuses the request so the central resends it once there is room.
    fn queue_control_reply(&mut self, reply: DataPacket) -> Acceptance {
        match self.connection().control_replies.push_back(reply) {
            Ok(()) => Acceptance::Accepted,
            Err(_) => Acceptance::NoRoom,
        }
    }
}

/// Bluetooth 5.0, with the company id kept for testing and a sub-version of zero.
pub const VERSION: [u8; 5] = [0x09, 0xFF, 0xFF, 0x00, 0x00];

enum Acceptance {
    Accepted,
    NoRoom,
    ConnectionLost(u8),
}

fn instant_is_ahead(instant: u16, event_counter: u16) -> bool {
    let distance = instant.wrapping_sub(event_counter);
    distance != 0 && distance < 32767
}

fn connect_request_is_valid(request: &ConnectRequest) -> bool {
    let interval_is_valid = (6..=3200).contains(&request.interval);
    let timeout_is_valid = (10..=3200).contains(&request.timeout);
    let timeout_in_microseconds = request.timeout as u32 * UNIT_10_MILLISECONDS;
    let shortest_timeout =
        (1 + request.latency as u32) * request.interval as u32 * UNIT_1250_MICROSECONDS * 2;
    let window_is_valid = request.window_size >= 1 && request.window_size as u16 <= 8;
    let hop_is_valid = (5..=16).contains(&request.hop_increment);
    let used_channels = request
        .channel_map
        .iter()
        .map(|byte| byte.count_ones())
        .sum::<u32>()
        - (request.channel_map[4] & 0xE0).count_ones();

    interval_is_valid
        && timeout_is_valid
        && timeout_in_microseconds > shortest_timeout
        && window_is_valid
        && hop_is_valid
        && used_channels >= 2
}
