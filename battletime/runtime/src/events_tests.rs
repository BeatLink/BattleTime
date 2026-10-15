use super::*;

// ============================================================================
// Widget properties
// ============================================================================

#[test]
fn every_handler_property_names_its_own_event() {
    let pairs = [
        (Property::OnClick, EventKind::Click),
        (Property::OnLongPress, EventKind::LongPress),
        (Property::OnDoubleClick, EventKind::DoubleClick),
        (Property::OnPress, EventKind::Press),
        (Property::OnRelease, EventKind::Release),
        (Property::OnDrag, EventKind::Drag),
        (Property::OnSwipe, EventKind::Swipe),
        (Property::OnButton, EventKind::Button),
        (Property::OnChange, EventKind::Change),
        (Property::OnPageChange, EventKind::PageChange),
        (Property::OnScrollEnd, EventKind::ScrollEnd),
        (Property::OnShow, EventKind::Show),
        (Property::OnHide, EventKind::Hide),
    ];

    for (property, event) in pairs {
        assert_eq!(EventKind::for_property(property), Some(event), "{property:?}");
    }
}

#[test]
fn a_property_that_is_not_a_handler_names_no_event() {
    for property in [
        Property::Text,
        Property::Width,
        Property::Visible,
        Property::Value,
    ] {
        assert_eq!(EventKind::for_property(property), None, "{property:?}");
    }
}

// ============================================================================
// Field order
// ============================================================================

#[test]
fn touches_carry_the_point_and_the_touched_value() {
    for event in [
        EventKind::Click,
        EventKind::LongPress,
        EventKind::DoubleClick,
        EventKind::Press,
        EventKind::Release,
    ] {
        assert_eq!(event.fields(), &["x", "y", "value"], "{event:?}");
    }
}

#[test]
fn each_event_lists_its_fields_in_reading_order() {
    let expected: [(EventKind, &[&str]); 18] = [
        (EventKind::Drag, &["x", "y", "start_x", "start_y", "value"]),
        (EventKind::Swipe, &["direction"]),
        (EventKind::Button, &["kind"]),
        (EventKind::Change, &["value"]),
        (EventKind::PageChange, &["page"]),
        (EventKind::ScrollEnd, &["offset"]),
        (EventKind::Time, &["time"]),
        (EventKind::HeartRate, &["beats_per_minute", "confidence"]),
        (EventKind::Steps, &["steps_today"]),
        (EventKind::Motion, &["kind"]),
        (EventKind::Acceleration, &["x", "y", "z"]),
        (EventKind::Battery, &["percent"]),
        (EventKind::Phone, &["is_connected"]),
        (EventKind::Notification, &["id", "app", "title", "body", "time"]),
        (EventKind::Call, &["kind", "caller"]),
        (EventKind::SettingsChange, &["key"]),
        (EventKind::Show, &[]),
        (EventKind::Hide, &[]),
    ];

    for (event, fields) in expected {
        assert_eq!(event.fields(), fields, "{event:?}");
    }
}

#[test]
fn life_and_phone_data_events_carry_no_fields() {
    for event in [
        EventKind::PhoneData,
        EventKind::Open,
        EventKind::Close,
        EventKind::ScreenOn,
        EventKind::ScreenOff,
        EventKind::Derive,
    ] {
        assert!(event.fields().is_empty(), "{event:?}");
    }
}

#[test]
fn a_field_is_found_by_name_at_its_position() {
    assert_eq!(EventKind::HeartRate.field_index("confidence"), Some(1));
    assert_eq!(EventKind::Acceleration.field_index("z"), Some(2));
    assert_eq!(EventKind::Notification.field_index("body"), Some(3));
    assert_eq!(EventKind::Call.field_index("caller"), Some(1));
}

#[test]
fn a_name_the_event_lacks_has_no_position() {
    assert_eq!(EventKind::Swipe.field_index("x"), None);
    assert_eq!(EventKind::Open.field_index("time"), None);
}

// ============================================================================
// Text fields
// ============================================================================

#[test]
fn notification_app_title_and_body_are_text_but_id_and_time_are_not() {
    let text_fields: [bool; 5] =
        core::array::from_fn(|field| EventKind::Notification.is_text_field(field as u8));

    assert_eq!(text_fields, [false, true, true, true, false]);
}

#[test]
fn a_callers_name_and_a_settings_key_are_text() {
    assert!(EventKind::Call.is_text_field(1));
    assert!(!EventKind::Call.is_text_field(0));
    assert!(EventKind::SettingsChange.is_text_field(0));
}

#[test]
fn touch_and_sensor_fields_are_numbers() {
    for event in [
        EventKind::Click,
        EventKind::Drag,
        EventKind::HeartRate,
        EventKind::Time,
    ] {
        for field in 0..event.fields().len() as u8 {
            assert!(!event.is_text_field(field), "{event:?} field {field}");
        }
    }
}
