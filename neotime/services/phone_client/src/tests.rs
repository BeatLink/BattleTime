extern crate std;

use std::{vec, vec::Vec};

use super::*;

// ============================================================================
// A phone's attribute table, answering as the ATT standard says
// ============================================================================

const ATTRIBUTE_NOT_FOUND: u8 = 0x0A;
const WRITE_NOT_PERMITTED: u8 = 0x03;
const ERROR_OPCODE: u8 = 0x01;
const UNREAD_ALERT_STATUS: u16 = 0x2A45;
const SUPPORTED_NEW_ALERT_CATEGORY: u16 = 0x2A47;
const NOTIFY: u8 = 0x10;
const READ: u8 = 0x02;
const WRITE: u8 = 0x08;

#[derive(Clone)]
enum Type {
    Short(u16),
    Long([u8; 16]),
}

impl Type {
    fn bytes(&self) -> Vec<u8> {
        match self {
            Type::Short(uuid) => uuid.to_le_bytes().to_vec(),
            Type::Long(uuid) => uuid.to_vec(),
        }
    }
}

struct Attribute {
    handle: u16,
    kind: Type,
    value: Vec<u8>,
}

/// A phone that answers the watch's requests from its table, at most `mtu` bytes at a time.
struct Phone {
    attributes: Vec<Attribute>,
    mtu: usize,
    writes: Vec<(u16, Vec<u8>)>,
    requests: usize,
    /// How many requests it answers before it stops answering.
    answers_left: usize,
    /// How much of each answer the watch keeps.
    kept_bytes: usize,
    /// A handle whose writes it refuses.
    refused_write: Option<u16>,
}

impl Phone {
    fn new() -> Phone {
        Phone {
            attributes: Vec::new(),
            mtu: 23,
            writes: Vec::new(),
            requests: 0,
            answers_left: usize::MAX,
            kept_bytes: 128,
            refused_write: None,
        }
    }

    fn next_handle(&self) -> u16 {
        self.attributes.last().map_or(1, |attribute| attribute.handle + 1)
    }

    fn add(&mut self, kind: Type, value: Vec<u8>) -> u16 {
        let handle = self.next_handle();
        self.attributes.push(Attribute { handle, kind, value });
        handle
    }

    fn service(mut self, uuid: u16) -> Phone {
        self.add(Type::Short(PRIMARY_SERVICE), uuid.to_le_bytes().to_vec());
        self
    }

    fn characteristic(mut self, kind: Type, properties: u8, value: &[u8]) -> Phone {
        let value_handle = self.next_handle() + 1;
        let mut declaration = vec![properties];
        declaration.extend_from_slice(&value_handle.to_le_bytes());
        declaration.extend_from_slice(&kind.bytes());
        self.add(Type::Short(CHARACTERISTIC), declaration);
        self.add(kind, value.to_vec());
        self
    }

    fn descriptor(mut self, uuid: u16) -> Phone {
        self.add(Type::Short(uuid), vec![0, 0]);
        self
    }

    fn handle_of(&self, uuid: u16) -> u16 {
        self.attributes
            .iter()
            .find(|attribute| matches!(attribute.kind, Type::Short(found) if found == uuid))
            .unwrap()
            .handle
    }

    fn group_end(&self, service_handle: u16) -> u16 {
        self.attributes
            .iter()
            .filter(|attribute| attribute.handle > service_handle)
            .take_while(|attribute| !matches!(attribute.kind, Type::Short(PRIMARY_SERVICE)))
            .last()
            .map_or(service_handle, |attribute| attribute.handle)
    }

    fn in_range(&self, start: u16, end: u16) -> impl Iterator<Item = &Attribute> {
        self.attributes
            .iter()
            .filter(move |attribute| (start..=end).contains(&attribute.handle))
    }

    fn answer(&mut self, request: &[u8]) -> Vec<u8> {
        let word = |at: usize| u16::from_le_bytes([request[at], request[at + 1]]);
        let not_found = vec![
            ERROR_OPCODE,
            request[0],
            request[1],
            request[2],
            ATTRIBUTE_NOT_FOUND,
        ];
        match request[0] {
            FIND_BY_TYPE_VALUE_REQUEST => {
                let mut answer = vec![FIND_BY_TYPE_VALUE_RESPONSE];
                for attribute in self.in_range(word(1), word(3)) {
                    if matches!(attribute.kind, Type::Short(PRIMARY_SERVICE))
                        && attribute.value == request[7..]
                        && answer.len() + 4 <= self.mtu
                    {
                        answer.extend_from_slice(&attribute.handle.to_le_bytes());
                        answer.extend_from_slice(&self.group_end(attribute.handle).to_le_bytes());
                    }
                }
                if answer.len() == 1 {
                    not_found
                } else {
                    answer
                }
            }
            READ_BY_TYPE_REQUEST => {
                let wanted = word(5);
                let found: Vec<&Attribute> = self
                    .in_range(word(1), word(3))
                    .filter(|attribute| matches!(attribute.kind, Type::Short(uuid) if uuid == wanted))
                    .collect();
                let Some(first) = found.first() else {
                    return not_found;
                };
                let value_bytes = first.value.len().min(self.mtu - 4);
                let mut answer = vec![READ_BY_TYPE_RESPONSE, (value_bytes + 2) as u8];
                for attribute in found {
                    if attribute.value.len().min(self.mtu - 4) != value_bytes
                        || answer.len() + 2 + value_bytes > self.mtu
                    {
                        break;
                    }
                    answer.extend_from_slice(&attribute.handle.to_le_bytes());
                    answer.extend_from_slice(&attribute.value[..value_bytes]);
                }
                answer
            }
            FIND_INFORMATION_REQUEST => {
                let found: Vec<&Attribute> = self.in_range(word(1), word(3)).collect();
                let Some(first) = found.first() else {
                    return not_found;
                };
                let short = matches!(first.kind, Type::Short(_));
                let mut answer = vec![FIND_INFORMATION_RESPONSE, if short { 1 } else { 2 }];
                for attribute in found {
                    let bytes = attribute.kind.bytes();
                    if matches!(attribute.kind, Type::Short(_)) != short
                        || answer.len() + 2 + bytes.len() > self.mtu
                    {
                        break;
                    }
                    answer.extend_from_slice(&attribute.handle.to_le_bytes());
                    answer.extend_from_slice(&bytes);
                }
                answer
            }
            WRITE_REQUEST if self.refused_write == Some(word(1)) => {
                vec![
                    ERROR_OPCODE,
                    WRITE_REQUEST,
                    request[1],
                    request[2],
                    WRITE_NOT_PERMITTED,
                ]
            }
            WRITE_REQUEST => {
                self.writes.push((word(1), request[3..].to_vec()));
                vec![WRITE_RESPONSE]
            }
            _ => vec![ERROR_OPCODE, request[0], 0, 0, 0x06],
        }
    }
}

/// What the client found, copied out of the answers.
#[derive(Debug, PartialEq)]
enum Value {
    CurrentTime(Vec<u8>),
    LocalTimeInformation(Vec<u8>),
}

/// Runs the client against the phone until it has nothing left to ask, as the firmware does.
fn run(phone: &mut Phone) -> (PhoneClient, Vec<Value>) {
    let mut client = PhoneClient::new();
    let mut answer = Vec::new();
    let mut values = Vec::new();
    loop {
        let (found, request) = client.next(&answer);
        match found {
            Some(Found::CurrentTime(value)) => values.push(Value::CurrentTime(value.to_vec())),
            Some(Found::LocalTimeInformation(value)) => {
                values.push(Value::LocalTimeInformation(value.to_vec()))
            }
            None => {}
        }
        let Some(request) = request.map(<[u8]>::to_vec) else {
            break;
        };
        phone.requests += 1;
        if phone.answers_left == 0 {
            break;
        }
        phone.answers_left -= 1;
        answer = phone.answer(&request);
        answer.truncate(phone.kept_bytes);
    }
    (client, values)
}

/// An Android-like phone with Generic Access first, then the services the watch uses.
fn phone_with_both_services() -> Phone {
    Phone::new()
        .service(0x1800)
        .characteristic(Type::Short(0x2A00), READ, b"Phone")
        .service(CURRENT_TIME_SERVICE)
        .characteristic(
            Type::Short(CURRENT_TIME),
            READ | NOTIFY,
            &[0xEA, 0x07, 10, 14, 9, 30, 15, 3, 0, 0],
        )
        .descriptor(CLIENT_CHARACTERISTIC_CONFIGURATION)
        .characteristic(Type::Short(LOCAL_TIME_INFORMATION), READ, &[4, 4])
        .service(ALERT_NOTIFICATION_SERVICE)
        .characteristic(Type::Short(SUPPORTED_NEW_ALERT_CATEGORY), READ, &[0xFF, 0x03])
        .characteristic(Type::Short(NEW_ALERT), NOTIFY, &[])
        .descriptor(CLIENT_CHARACTERISTIC_CONFIGURATION)
        .characteristic(Type::Short(UNREAD_ALERT_STATUS), NOTIFY, &[])
        .descriptor(CLIENT_CHARACTERISTIC_CONFIGURATION)
        .characteristic(Type::Short(ALERT_NOTIFICATION_CONTROL_POINT), WRITE, &[])
}

// ============================================================================
// Requests and answers
// ============================================================================

#[test]
fn requests_are_laid_out_as_att_says() {
    assert_eq!(
        find_service_request(0x1805),
        [0x06, 0x01, 0x00, 0xFF, 0xFF, 0x00, 0x28, 0x05, 0x18]
    );
    assert_eq!(
        read_by_type_request(
            HandleRange {
                start: 0x0010,
                end: 0x0123
            },
            0x2A2B
        ),
        [0x08, 0x10, 0x00, 0x23, 0x01, 0x2B, 0x2A]
    );
    assert_eq!(
        find_information_request(HandleRange { start: 5, end: 9 }),
        [0x04, 0x05, 0x00, 0x09, 0x00]
    );
    assert_eq!(write_request(0x0102, [1, 0]), [0x12, 0x02, 0x01, 0x01, 0x00]);
}

#[test]
fn an_error_or_a_short_answer_names_no_service() {
    assert_eq!(service_range_in(&[0x01, 0x06, 0x01, 0x00, 0x0A]), None);
    assert_eq!(service_range_in(&[0x07, 0x05, 0x00, 0x09]), None);
    assert_eq!(
        service_range_in(&[0x07, 0x05, 0x00, 0x09, 0x00]),
        Some(HandleRange { start: 5, end: 9 })
    );
}

#[test]
fn a_cut_read_by_type_entry_is_left_for_the_next_page() {
    let answer = [0x09, 4, 0x01, 0x00, 0xAA, 0xBB, 0x02, 0x00, 0xCC];
    let entries: Vec<_> = read_by_type_entries(&answer).collect();
    assert_eq!(entries, [(1, &[0xAA, 0xBB][..])]);
    assert_eq!(read_by_type_entries(&[0x09, 1, 0, 0]).count(), 0);
    assert_eq!(read_by_type_entries(&[0x01, 0x08, 0, 0, 0x0A]).count(), 0);
}

#[test]
fn find_information_gives_short_uuids_and_skips_long_ones() {
    let short: Vec<_> = find_information_entries(&[0x05, 1, 0x07, 0x00, 0x02, 0x29]).collect();
    assert_eq!(short, [(7, Some(0x2902))]);
    let mut long_answer = vec![0x05, 2, 0x08, 0x00];
    long_answer.extend_from_slice(&[0x11; 16]);
    let long: Vec<_> = find_information_entries(&long_answer).collect();
    assert_eq!(long, [(8, None)]);
}

#[test]
fn a_declaration_gives_its_value_handle_and_short_uuid() {
    assert_eq!(
        characteristic_declaration(&[0x10, 0x09, 0x00, 0x46, 0x2A]),
        Some((9, Some(NEW_ALERT)))
    );
    assert_eq!(
        characteristic_declaration(&[0x10, 0x09, 0x00, 0x11, 0x22, 0x33]),
        Some((9, None))
    );
    assert_eq!(characteristic_declaration(&[0x10, 0x09]), None);
}

#[test]
fn a_notification_for_another_handle_carries_nothing() {
    let notification = [0x1B, 0x09, 0x00, 5, 1, b'H', b'i'];
    assert!(is_unasked(&notification));
    assert_eq!(notified_value(&notification, 9), Some(&[5, 1, b'H', b'i'][..]));
    assert_eq!(notified_value(&notification, 10), None);
    let indication = [0x1D, 0x09, 0x00, 5];
    assert!(is_unasked(&indication));
    assert_eq!(notified_value(&indication, 9), Some(&[5][..]));
    assert!(!is_unasked(&[0x13]));
    assert!(!is_unasked(&[]));
    assert_eq!(notified_value(&[0x13], 9), None);
}

// ============================================================================
// The Current Time service
// ============================================================================

const PHONE_CURRENT_TIME: [u8; 10] = [0xEA, 0x07, 10, 14, 9, 30, 15, 3, 0, 0];

#[test]
fn the_phones_current_time_and_zone_are_read() {
    let mut phone = phone_with_both_services();
    let (_, values) = run(&mut phone);
    assert_eq!(
        values,
        [
            Value::CurrentTime(PHONE_CURRENT_TIME.to_vec()),
            Value::LocalTimeInformation(vec![4, 4])
        ]
    );
}

#[test]
fn a_phone_without_local_time_information_gives_only_the_time() {
    let mut phone = Phone::new().service(CURRENT_TIME_SERVICE).characteristic(
        Type::Short(CURRENT_TIME),
        READ,
        &PHONE_CURRENT_TIME,
    );
    let (client, values) = run(&mut phone);
    assert_eq!(values, [Value::CurrentTime(PHONE_CURRENT_TIME.to_vec())]);
    assert_eq!(client.new_alert(), None);
}

#[test]
fn a_phone_without_current_time_skips_to_alerts() {
    let mut phone = Phone::new()
        .service(CURRENT_TIME_SERVICE)
        .characteristic(Type::Short(LOCAL_TIME_INFORMATION), READ, &[4, 4])
        .service(ALERT_NOTIFICATION_SERVICE)
        .characteristic(Type::Short(NEW_ALERT), NOTIFY, &[])
        .descriptor(CLIENT_CHARACTERISTIC_CONFIGURATION);
    let (client, values) = run(&mut phone);
    assert!(values.is_empty());
    assert_eq!(client.new_alert(), Some(phone.handle_of(NEW_ALERT)));
}

#[test]
fn a_phone_with_neither_service_is_asked_twice_and_left_alone() {
    let mut phone = Phone::new()
        .service(0x1800)
        .characteristic(Type::Short(0x2A00), READ, b"Phone");
    let (client, values) = run(&mut phone);
    assert!(values.is_empty());
    assert_eq!(client.new_alert(), None);
    assert_eq!(phone.requests, 2);
    assert!(phone.writes.is_empty());
}

#[test]
fn a_phone_that_never_answers_gives_nothing() {
    let mut phone = phone_with_both_services();
    phone.answers_left = 0;
    let (client, values) = run(&mut phone);
    assert!(values.is_empty());
    assert_eq!(client.new_alert(), None);
}

// ============================================================================
// The Alert Notification service
// ============================================================================

#[test]
fn new_alerts_are_turned_on_and_every_category_asked_for() {
    let mut phone = phone_with_both_services();
    let (client, _) = run(&mut phone);
    let new_alert = client.new_alert().unwrap();
    assert_eq!(new_alert, phone.handle_of(NEW_ALERT));
    let control_point = phone.handle_of(ALERT_NOTIFICATION_CONTROL_POINT);
    assert_eq!(
        phone.writes,
        [
            (new_alert + 1, NOTIFICATIONS_ON.to_vec()),
            (control_point, ENABLE_EVERY_NEW_ALERT.to_vec())
        ]
    );
}

#[test]
fn a_phone_without_the_control_point_still_sends_new_alerts() {
    let mut phone = Phone::new()
        .service(ALERT_NOTIFICATION_SERVICE)
        .characteristic(Type::Short(NEW_ALERT), NOTIFY, &[])
        .descriptor(CLIENT_CHARACTERISTIC_CONFIGURATION);
    let (client, _) = run(&mut phone);
    let new_alert = client.new_alert().unwrap();
    assert_eq!(phone.writes, [(new_alert + 1, NOTIFICATIONS_ON.to_vec())]);
}

#[test]
fn a_new_alert_without_its_own_cccd_is_not_subscribed_through_the_next_ones() {
    let mut phone = Phone::new()
        .service(ALERT_NOTIFICATION_SERVICE)
        .characteristic(Type::Short(NEW_ALERT), READ, &[])
        .characteristic(Type::Short(UNREAD_ALERT_STATUS), NOTIFY, &[])
        .descriptor(CLIENT_CHARACTERISTIC_CONFIGURATION);
    let (client, _) = run(&mut phone);
    assert_eq!(client.new_alert(), None);
    assert!(phone.writes.is_empty());
}

#[test]
fn a_new_alert_last_in_its_service_without_a_cccd_is_left_alone() {
    let mut phone = Phone::new()
        .service(ALERT_NOTIFICATION_SERVICE)
        .characteristic(Type::Short(NEW_ALERT), READ, &[])
        .service(0x180F)
        .characteristic(Type::Short(0x2A19), NOTIFY, &[50])
        .descriptor(CLIENT_CHARACTERISTIC_CONFIGURATION);
    let (client, _) = run(&mut phone);
    assert_eq!(client.new_alert(), None);
    assert!(phone.writes.is_empty());
}

#[test]
fn a_service_without_new_alert_is_left_alone() {
    let mut phone = Phone::new().service(ALERT_NOTIFICATION_SERVICE).characteristic(
        Type::Short(SUPPORTED_NEW_ALERT_CATEGORY),
        READ,
        &[0xFF, 0x03],
    );
    let (client, _) = run(&mut phone);
    assert_eq!(client.new_alert(), None);
    assert!(phone.writes.is_empty());
}

#[test]
fn a_refused_cccd_write_leaves_new_alerts_off() {
    let mut phone = phone_with_both_services();
    let configuration = phone.handle_of(NEW_ALERT) + 1;
    phone.refused_write = Some(configuration);
    let (client, _) = run(&mut phone);
    assert_eq!(client.new_alert(), None);
    assert!(phone.writes.is_empty());
}

#[test]
fn declarations_are_read_page_by_page_past_long_uuids() {
    let mut phone = Phone::new()
        .service(ALERT_NOTIFICATION_SERVICE)
        .characteristic(Type::Short(SUPPORTED_NEW_ALERT_CATEGORY), READ, &[0xFF, 0x03])
        .characteristic(Type::Long([0x42; 16]), READ, &[1])
        .characteristic(Type::Short(0x2A48), READ, &[0xFF, 0x03])
        .characteristic(Type::Short(UNREAD_ALERT_STATUS), NOTIFY, &[])
        .descriptor(CLIENT_CHARACTERISTIC_CONFIGURATION)
        .descriptor(0x2901)
        .characteristic(Type::Short(NEW_ALERT), NOTIFY, &[])
        .descriptor(0x2901)
        .descriptor(CLIENT_CHARACTERISTIC_CONFIGURATION)
        .characteristic(Type::Short(ALERT_NOTIFICATION_CONTROL_POINT), WRITE, &[]);
    let (client, _) = run(&mut phone);
    assert_eq!(client.new_alert(), Some(13));
    assert_eq!(phone.writes[0], (15, NOTIFICATIONS_ON.to_vec()));
    assert_eq!(phone.writes[1], (17, ENABLE_EVERY_NEW_ALERT.to_vec()));
}

#[test]
fn answers_longer_than_the_watch_keeps_are_asked_for_again() {
    let mut phone = Phone::new().service(ALERT_NOTIFICATION_SERVICE);
    for uuid in 0x3000..0x3014 {
        phone = phone.characteristic(Type::Short(uuid), READ, &[0]);
    }
    phone = phone
        .characteristic(Type::Short(NEW_ALERT), NOTIFY, &[])
        .descriptor(CLIENT_CHARACTERISTIC_CONFIGURATION);
    phone.mtu = 247;
    phone.kept_bytes = 64;
    let (client, _) = run(&mut phone);
    let new_alert = client.new_alert().unwrap();
    assert_eq!(new_alert, phone.handle_of(NEW_ALERT));
    assert_eq!(phone.writes, [(new_alert + 1, NOTIFICATIONS_ON.to_vec())]);
}

#[test]
fn a_phone_that_stops_answering_part_way_is_not_subscribed() {
    for answers in 0..9 {
        let mut phone = phone_with_both_services();
        phone.answers_left = answers;
        let (client, _) = run(&mut phone);
        assert_eq!(client.new_alert(), None, "after {answers} answers");
    }
}

#[test]
fn nothing_is_asked_once_the_client_is_done() {
    let mut phone = phone_with_both_services();
    let (mut client, _) = run(&mut phone);
    assert_eq!(client.next(&[]), (None, None));
    assert_eq!(client.next(&[0x13]), (None, None));
}
