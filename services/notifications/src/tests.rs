extern crate std;

use alloc::boxed::Box;

use neotime_companion::{PhoneNotification, NOTIFICATION_TITLE_BYTES};
use neotime_runtime::{SystemValue, Value, NONE};

use crate::{NotificationList, MOST_KEPT};

// ============================================================================
// Helpers
// ============================================================================

fn from_phone(id: Value, title: &str) -> Box<PhoneNotification> {
    let mut notification = PhoneNotification {
        id,
        ..PhoneNotification::default()
    };
    let _ = notification.source.push_str("Messages");
    let _ = notification.title.push_str(title);
    let _ = notification.body.push_str("On my way");
    Box::new(notification)
}

fn titles(list: &NotificationList) -> std::vec::Vec<&str> {
    (0..list.len())
        .map(|place| list.get(place).unwrap().title.as_str())
        .collect()
}

// ============================================================================
// Keeping
// ============================================================================

#[test]
fn a_new_list_is_empty() {
    let list = NotificationList::default();

    assert!(list.is_empty());
    assert_eq!(list.unread_count(), 0);
    assert_eq!(list.get(0), None);
}

#[test]
fn the_newest_notification_comes_first_and_unread() {
    let mut list = NotificationList::new();

    list.add(from_phone(1, "First"), 100);
    list.add(from_phone(2, "Second"), 200);

    assert_eq!(titles(&list), ["Second", "First"]);
    assert_eq!(list.unread_count(), 2);
}

#[test]
fn a_full_list_lets_the_oldest_go() {
    let mut list = NotificationList::new();

    for id in 0..=MOST_KEPT as Value {
        list.add(from_phone(id, &std::format!("Number {id}")), id);
    }

    assert_eq!(list.len(), MOST_KEPT);
    assert_eq!(list.get(0).unwrap().title, "Number 5");
    assert_eq!(list.get(MOST_KEPT - 1).unwrap().title, "Number 1");
}

#[test]
fn a_notification_with_a_kept_id_replaces_it_as_the_newest() {
    let mut list = NotificationList::new();
    list.add(from_phone(1, "First"), 100);
    list.add(from_phone(2, "Second"), 200);
    list.mark_all_read();

    list.add(from_phone(1, "First, edited"), 300);

    assert_eq!(titles(&list), ["First, edited", "Second"]);
    assert_eq!(list.unread_count(), 1);
}

#[test]
fn an_apps_notification_gets_its_own_id_and_text_cut_to_fit() {
    let mut list = NotificationList::new();
    let long_title = "é".repeat(NOTIFICATION_TITLE_BYTES);

    let first = list.add_from_app("Timer", &long_title, "Time is up", 100);
    let second = list.add_from_app("Timer", "Again", "", 200);

    assert!(first < 0 && second < 0 && first != second);
    let notification = list.get(1).unwrap();
    assert_eq!(notification.id, first);
    assert_eq!(notification.source, "Timer");
    assert_eq!(notification.title.chars().count(), NOTIFICATION_TITLE_BYTES / 2);
    assert_eq!(notification.body, "Time is up");
}

// ============================================================================
// Dismissing and reading
// ============================================================================

#[test]
fn dismissing_removes_only_that_notification() {
    let mut list = NotificationList::new();
    list.add(from_phone(1, "First"), 100);
    list.add(from_phone(2, "Second"), 200);

    assert!(list.dismiss(1));
    assert!(!list.dismiss(1));

    assert_eq!(titles(&list), ["Second"]);
}

#[test]
fn marking_read_leaves_nothing_unread_and_says_whether_it_changed_anything() {
    let mut list = NotificationList::new();
    list.add(from_phone(1, "First"), 100);

    assert!(list.mark_all_read());
    assert!(!list.mark_all_read());

    assert_eq!(list.unread_count(), 0);
    assert_eq!(list.len(), 1);
}

// ============================================================================
// System values
// ============================================================================

#[test]
fn apps_read_the_counts_and_each_field_newest_first() {
    let mut list = NotificationList::new();
    list.add(from_phone(7, "First"), 100);
    list.add(from_phone(8, "Second"), 200);
    list.dismiss(7);
    list.add(from_phone(9, "Third"), 300);
    list.mark_all_read();
    list.add(from_phone(10, "Fourth"), 400);

    assert_eq!(list.read(SystemValue::UnreadNotifications), Some(1));
    assert_eq!(list.read(SystemValue::NotificationCount), Some(3));
    assert_eq!(list.read(SystemValue::BatteryPercent), None);
    assert_eq!(list.read_element(SystemValue::NotificationId, 0), 10);
    assert_eq!(list.read_element(SystemValue::NotificationTime, 2), 200);
    assert_eq!(list.read_element_text(SystemValue::NotificationTitle, 1), "Third");
    assert_eq!(
        list.read_element_text(SystemValue::NotificationApp, 1),
        "Messages"
    );
    assert_eq!(
        list.read_element_text(SystemValue::NotificationBody, 2),
        "On my way"
    );
}

#[test]
fn reading_past_the_end_or_another_value_gives_nothing() {
    let mut list = NotificationList::new();
    list.add(from_phone(1, "First"), 100);

    assert_eq!(list.read_element(SystemValue::NotificationId, 1), NONE);
    assert_eq!(list.read_element(SystemValue::NotificationId, -1), NONE);
    assert_eq!(list.read_element(SystemValue::NotificationTitle, 0), NONE);
    assert_eq!(list.read_element_text(SystemValue::NotificationTitle, 1), "");
    assert_eq!(list.read_element_text(SystemValue::NotificationId, 0), "");
}
