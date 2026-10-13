use crate::*;

fn parsed(line: &str) -> Message<'_> {
    parse_line(line.as_bytes()).unwrap()
}

#[test]
fn a_bare_slots_message_asks_where_they_are() {
    let message = parsed(r#"GB({"t":"slots"})"#);

    assert_eq!(message, Message::Slots(SlotsRequest::default()));
    assert!(!SlotsRequest::default().moves_anything());
}

#[test]
fn a_slot_named_with_its_start_and_size_moves() {
    let message = parsed(r#"{"t":"slots","recovery":[409600,176128]}"#);

    let Message::Slots(request) = message else {
        panic!("not a slots message: {message:?}");
    };
    assert_eq!(
        request.recovery,
        Some(SlotRange {
            start: 409_600,
            size: 176_128
        })
    );
    assert_eq!(request.update, None);
    assert!(request.moves_anything());
}

#[test]
fn both_slots_can_move_at_once() {
    let message = parsed(r#"{"t":"slots","update":[0,401408],"recovery":[413696,176128]}"#);

    let Message::Slots(request) = message else {
        panic!("not a slots message: {message:?}");
    };
    assert_eq!(request.update.map(|range| range.size), Some(401_408));
    assert_eq!(request.recovery.map(|range| range.start), Some(413_696));
}

#[test]
fn a_slot_that_is_not_two_numbers_is_refused() {
    assert_eq!(
        parse_line(br#"{"t":"slots","recovery":[409600]}"#),
        Err(MessageError::NotJson)
    );
    assert_eq!(
        parse_line(br#"{"t":"slots","recovery":"here"}"#),
        Err(MessageError::NotJson)
    );
}

#[test]
fn the_report_names_both_slots_and_the_limits() {
    let report = slots_report(&SlotsReport {
        update: SlotRange {
            start: 0,
            size: 401_408,
        },
        recovery: SlotRange {
            start: 413_696,
            size: 397_312,
        },
        area_end: 1_048_576,
        firmware_size: 397_312,
        recovery_image_size: 171_428,
        restarting: false,
    });

    assert_eq!(
        report.as_str(),
        "{\"t\":\"slots\",\"update\":[0,401408],\"recovery\":[413696,397312],\"area\":1048576,\"firmware\":397312,\"image\":171428,\"restart\":false}\n"
    );
}

#[test]
fn a_refusal_says_why() {
    assert_eq!(
        slots_refusal("the slots overlap").as_str(),
        "{\"t\":\"slots\",\"err\":\"the slots overlap\"}\n"
    );
}

#[test]
fn a_negative_or_too_large_number_is_refused() {
    assert_eq!(
        parse_line(br#"{"t":"slots","recovery":[-4096,65536]}"#),
        Err(MessageError::NotJson)
    );
    assert_eq!(
        parse_line(br#"{"t":"slots","update":[4294967296,65536]}"#),
        Err(MessageError::NotJson)
    );
}
