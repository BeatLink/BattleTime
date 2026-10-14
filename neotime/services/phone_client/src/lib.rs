//! A small GATT client for the phone's own services, used as InfiniTime's CurrentTimeClient and AlertNotificationClient use them.
//!
//! The watch is the peripheral, but ATT lets either side ask the other, so once a phone connects
//! the watch can read the phone's Current Time service (0x1805) and subscribe to its Alert
//! Notification service's New Alert (0x2A46). [`PhoneClient`] takes the phone's answers one at a
//! time and gives the next request, so the firmware only carries bytes and every step is tested
//! here against a pretend phone. Any answer but the one expected, an error included, ends the step
//! that asked, so a phone without a service is left alone. It asks only for 16-bit UUIDs and skips
//! every 128-bit one it is shown.

#![no_std]

#[cfg(test)]
mod tests;

// ============================================================================
// ATT opcodes and GATT types
// ============================================================================

const FIND_INFORMATION_REQUEST: u8 = 0x04;
const FIND_INFORMATION_RESPONSE: u8 = 0x05;
const FIND_BY_TYPE_VALUE_REQUEST: u8 = 0x06;
const FIND_BY_TYPE_VALUE_RESPONSE: u8 = 0x07;
const READ_BY_TYPE_REQUEST: u8 = 0x08;
const READ_BY_TYPE_RESPONSE: u8 = 0x09;
const WRITE_REQUEST: u8 = 0x12;
const WRITE_RESPONSE: u8 = 0x13;
const HANDLE_VALUE_NOTIFICATION: u8 = 0x1B;
const HANDLE_VALUE_INDICATION: u8 = 0x1D;

const PRIMARY_SERVICE: u16 = 0x2800;
const CHARACTERISTIC: u16 = 0x2803;
const CLIENT_CHARACTERISTIC_CONFIGURATION: u16 = 0x2902;

/// Find Information's format byte for a list of 16-bit UUIDs; the other format lists 128-bit ones.
const SHORT_UUID_FORMAT: u8 = 1;
const SHORT_UUID_INFORMATION_BYTES: usize = 4;
const LONG_UUID_INFORMATION_BYTES: usize = 18;

pub const CURRENT_TIME_SERVICE: u16 = 0x1805;
pub const CURRENT_TIME: u16 = 0x2A2B;
pub const LOCAL_TIME_INFORMATION: u16 = 0x2A0F;
pub const ALERT_NOTIFICATION_SERVICE: u16 = 0x1811;
pub const NEW_ALERT: u16 = 0x2A46;
pub const ALERT_NOTIFICATION_CONTROL_POINT: u16 = 0x2A44;

/// The Client Characteristic Configuration value that turns notifications on.
pub const NOTIFICATIONS_ON: [u8; 2] = [0x01, 0x00];

/// The Alert Notification Control Point command that turns on New Alert notifications for every category.
pub const ENABLE_EVERY_NEW_ALERT: [u8; 2] = [0x02, 0xFF];

/// The first and last attribute handles of a service or a part of one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HandleRange {
    pub start: u16,
    pub end: u16,
}

// ============================================================================
// Building requests
// ============================================================================

/// A Find By Type Value request for the primary service with this UUID, anywhere on the phone.
pub fn find_service_request(uuid: u16) -> [u8; 9] {
    let [type_low, type_high] = PRIMARY_SERVICE.to_le_bytes();
    let [uuid_low, uuid_high] = uuid.to_le_bytes();
    [
        FIND_BY_TYPE_VALUE_REQUEST,
        0x01,
        0x00,
        0xFF,
        0xFF,
        type_low,
        type_high,
        uuid_low,
        uuid_high,
    ]
}

/// A Read By Type request for the attributes of this type in the range.
pub fn read_by_type_request(range: HandleRange, attribute_type: u16) -> [u8; 7] {
    let [start_low, start_high] = range.start.to_le_bytes();
    let [end_low, end_high] = range.end.to_le_bytes();
    let [type_low, type_high] = attribute_type.to_le_bytes();
    [
        READ_BY_TYPE_REQUEST,
        start_low,
        start_high,
        end_low,
        end_high,
        type_low,
        type_high,
    ]
}

/// A Find Information request for the handles and types in the range.
pub fn find_information_request(range: HandleRange) -> [u8; 5] {
    let [start_low, start_high] = range.start.to_le_bytes();
    let [end_low, end_high] = range.end.to_le_bytes();
    [FIND_INFORMATION_REQUEST, start_low, start_high, end_low, end_high]
}

/// A Write request of a two-byte value, as a CCCD or the control point takes.
pub fn write_request(handle: u16, value: [u8; 2]) -> [u8; 5] {
    let [handle_low, handle_high] = handle.to_le_bytes();
    [WRITE_REQUEST, handle_low, handle_high, value[0], value[1]]
}

// ============================================================================
// Reading answers
// ============================================================================

fn handle_at(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

/// The first service a Find By Type Value answer names.
pub fn service_range_in(answer: &[u8]) -> Option<HandleRange> {
    match answer {
        [FIND_BY_TYPE_VALUE_RESPONSE, found @ ..] if found.len() >= 4 => Some(HandleRange {
            start: handle_at(found),
            end: handle_at(&found[2..]),
        }),
        _ => None,
    }
}

/// Each whole entry of a Read By Type answer, as its handle and value.
pub fn read_by_type_entries(answer: &[u8]) -> impl Iterator<Item = (u16, &[u8])> {
    let (entry_bytes, entries) = match answer {
        [READ_BY_TYPE_RESPONSE, entry_bytes, entries @ ..] if *entry_bytes >= 2 => {
            (usize::from(*entry_bytes), entries)
        }
        _ => (2, &[][..]),
    };
    entries
        .chunks_exact(entry_bytes)
        .map(|entry| (handle_at(entry), &entry[2..]))
}

/// Each whole entry of a Find Information answer, as its handle and its type when that is a 16-bit UUID.
pub fn find_information_entries(answer: &[u8]) -> impl Iterator<Item = (u16, Option<u16>)> + '_ {
    let (entry_bytes, entries) = match answer {
        [FIND_INFORMATION_RESPONSE, SHORT_UUID_FORMAT, entries @ ..] => {
            (SHORT_UUID_INFORMATION_BYTES, entries)
        }
        [FIND_INFORMATION_RESPONSE, _, entries @ ..] => (LONG_UUID_INFORMATION_BYTES, entries),
        _ => (SHORT_UUID_INFORMATION_BYTES, &[][..]),
    };
    entries.chunks_exact(entry_bytes).map(move |entry| {
        let short_uuid = (entry_bytes == SHORT_UUID_INFORMATION_BYTES).then(|| handle_at(&entry[2..]));
        (handle_at(entry), short_uuid)
    })
}

/// A characteristic declaration's value handle, and its UUID when that is a 16-bit one.
pub fn characteristic_declaration(declaration: &[u8]) -> Option<(u16, Option<u16>)> {
    match declaration {
        [_properties, handle_low, handle_high, uuid @ ..] => {
            let short_uuid = (uuid.len() == 2).then(|| handle_at(uuid));
            Some((u16::from_le_bytes([*handle_low, *handle_high]), short_uuid))
        }
        _ => None,
    }
}

/// Whether the phone sent this unasked, as a notification or indication, rather than as an answer.
pub fn is_unasked(pdu: &[u8]) -> bool {
    matches!(
        pdu.first(),
        Some(&(HANDLE_VALUE_NOTIFICATION | HANDLE_VALUE_INDICATION))
    )
}

/// The value a notification or indication carries for this handle.
pub fn notified_value(pdu: &[u8], handle: u16) -> Option<&[u8]> {
    match pdu {
        [HANDLE_VALUE_NOTIFICATION | HANDLE_VALUE_INDICATION, low, high, value @ ..]
            if u16::from_le_bytes([*low, *high]) == handle =>
        {
            Some(value)
        }
        _ => None,
    }
}

// ============================================================================
// The client's steps
// ============================================================================

/// What the client is waiting to hear about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Starting,
    FindingTimeService,
    ReadingCurrentTime,
    ReadingLocalTimeInformation,
    FindingAlertService,
    ReadingDeclarations,
    FindingConfiguration,
    TurningOnNewAlerts,
    EnablingCategories,
    Done,
}

/// A value the client read from the phone, for the firmware to apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Found<'answer> {
    /// The phone's Current Time (0x2A2B), its local time.
    CurrentTime(&'answer [u8]),
    /// The phone's Local Time Information (0x2A0F), its time zone and daylight saving offset.
    LocalTimeInformation(&'answer [u8]),
}

/// The longest request the client sends, a Find By Type Value.
const MOST_REQUEST_BYTES: usize = 9;

/// Finds and uses the phone's services, one request and answer at a time.
pub struct PhoneClient {
    stage: Stage,
    /// The service being read.
    service: HandleRange,
    /// Where the next page of declarations or descriptors starts.
    next_handle: u16,
    /// Where New Alert's descriptors end: before the next declaration, or at the service's end.
    descriptors_end: u16,
    new_alert: Option<u16>,
    control_point: Option<u16>,
    configuration: u16,
    subscribed: bool,
    request: [u8; MOST_REQUEST_BYTES],
    request_length: usize,
}

impl Default for PhoneClient {
    fn default() -> PhoneClient {
        PhoneClient::new()
    }
}

impl PhoneClient {
    pub const fn new() -> PhoneClient {
        PhoneClient {
            stage: Stage::Starting,
            service: HandleRange { start: 0, end: 0 },
            next_handle: 0,
            descriptors_end: 0,
            new_alert: None,
            control_point: None,
            configuration: 0,
            subscribed: false,
            request: [0; MOST_REQUEST_BYTES],
            request_length: 0,
        }
    }

    /// New Alert's value handle once its notifications are on, for reading what the phone notifies.
    pub fn new_alert(&self) -> Option<u16> {
        self.new_alert.filter(|_| self.subscribed)
    }

    /// Takes the answer to the last request, empty at the start, and gives what it found and the next request.
    pub fn next<'answer>(&mut self, answer: &'answer [u8]) -> (Option<Found<'answer>>, Option<&[u8]>) {
        let found = self.take_answer(answer);
        self.request_length = self.build_request();
        let request = (self.request_length > 0).then(|| &self.request[..self.request_length]);
        (found, request)
    }

    /// Moves on to the stage the answer leads to, giving any value it carried.
    fn take_answer<'answer>(&mut self, answer: &'answer [u8]) -> Option<Found<'answer>> {
        match self.stage {
            Stage::Starting => self.stage = Stage::FindingTimeService,
            Stage::FindingTimeService => match service_range_in(answer) {
                Some(service) => {
                    self.service = service;
                    self.stage = Stage::ReadingCurrentTime;
                }
                None => self.stage = Stage::FindingAlertService,
            },
            Stage::ReadingCurrentTime => {
                let current_time = read_by_type_entries(answer).next();
                self.stage = if current_time.is_some() {
                    Stage::ReadingLocalTimeInformation
                } else {
                    Stage::FindingAlertService
                };
                return current_time.map(|(_, value)| Found::CurrentTime(value));
            }
            Stage::ReadingLocalTimeInformation => {
                self.stage = Stage::FindingAlertService;
                return read_by_type_entries(answer)
                    .next()
                    .map(|(_, value)| Found::LocalTimeInformation(value));
            }
            Stage::FindingAlertService => match service_range_in(answer) {
                Some(service) => {
                    self.service = service;
                    self.next_handle = service.start;
                    self.descriptors_end = service.end;
                    self.stage = Stage::ReadingDeclarations;
                }
                None => self.stage = Stage::Done,
            },
            Stage::ReadingDeclarations => self.take_declarations(answer),
            Stage::FindingConfiguration => self.take_descriptors(answer),
            Stage::TurningOnNewAlerts => {
                self.subscribed = answer.first() == Some(&WRITE_RESPONSE);
                self.stage = match self.control_point {
                    Some(_) if self.subscribed => Stage::EnablingCategories,
                    _ => Stage::Done,
                };
            }
            Stage::EnablingCategories | Stage::Done => self.stage = Stage::Done,
        }
        None
    }

    /// Notes New Alert and the control point from a page of declarations, then reads the next page or looks for the CCCD.
    fn take_declarations(&mut self, answer: &[u8]) {
        let mut last_handle = None;
        for (declaration_handle, declaration) in read_by_type_entries(answer) {
            last_handle = Some(declaration_handle);
            if self.new_alert.is_some() && self.descriptors_end == self.service.end {
                self.descriptors_end = declaration_handle.saturating_sub(1);
            }
            match characteristic_declaration(declaration) {
                Some((value_handle, Some(NEW_ALERT))) => self.new_alert = Some(value_handle),
                Some((value_handle, Some(ALERT_NOTIFICATION_CONTROL_POINT))) => {
                    self.control_point = Some(value_handle)
                }
                _ => {}
            }
        }
        match last_handle.and_then(|handle| handle.checked_add(1)) {
            Some(next) if next <= self.service.end => self.next_handle = next,
            _ => self.look_for_configuration(),
        }
    }

    /// Starts looking through New Alert's descriptors, or stops when it has none.
    fn look_for_configuration(&mut self) {
        match self.new_alert.and_then(|handle| handle.checked_add(1)) {
            Some(first_descriptor) if first_descriptor <= self.descriptors_end => {
                self.next_handle = first_descriptor;
                self.stage = Stage::FindingConfiguration;
            }
            _ => self.stage = Stage::Done,
        }
    }

    /// Finds New Alert's CCCD in a page of descriptors, or reads the next page.
    fn take_descriptors(&mut self, answer: &[u8]) {
        let mut last_handle = None;
        for (handle, short_uuid) in find_information_entries(answer) {
            if short_uuid == Some(CLIENT_CHARACTERISTIC_CONFIGURATION) {
                self.configuration = handle;
                self.stage = Stage::TurningOnNewAlerts;
                return;
            }
            last_handle = Some(handle);
        }
        match last_handle.and_then(|handle| handle.checked_add(1)) {
            Some(next) if next <= self.descriptors_end => self.next_handle = next,
            _ => self.stage = Stage::Done,
        }
    }

    /// Writes the request the stage asks, giving its length, or 0 when there is nothing left to ask.
    fn build_request(&mut self) -> usize {
        let request = &mut self.request;
        let mut put = |bytes: &[u8]| {
            request[..bytes.len()].copy_from_slice(bytes);
            bytes.len()
        };
        match self.stage {
            Stage::FindingTimeService => put(&find_service_request(CURRENT_TIME_SERVICE)),
            Stage::ReadingCurrentTime => put(&read_by_type_request(self.service, CURRENT_TIME)),
            Stage::ReadingLocalTimeInformation => {
                put(&read_by_type_request(self.service, LOCAL_TIME_INFORMATION))
            }
            Stage::FindingAlertService => put(&find_service_request(ALERT_NOTIFICATION_SERVICE)),
            Stage::ReadingDeclarations => put(&read_by_type_request(
                HandleRange {
                    start: self.next_handle,
                    end: self.service.end,
                },
                CHARACTERISTIC,
            )),
            Stage::FindingConfiguration => put(&find_information_request(HandleRange {
                start: self.next_handle,
                end: self.descriptors_end,
            })),
            Stage::TurningOnNewAlerts => put(&write_request(self.configuration, NOTIFICATIONS_ON)),
            Stage::EnablingCategories => put(&write_request(
                self.control_point.unwrap_or_default(),
                ENABLE_EVERY_NEW_ALERT,
            )),
            Stage::Starting | Stage::Done => 0,
        }
    }
}
