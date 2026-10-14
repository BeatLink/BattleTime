extern crate std;

use alloc::boxed::Box;

use neotime_companion::{PhoneNotification, NOTIFICATION_TITLE_BYTES};
use neotime_runtime::resources::{library_icon, ImageReference};
use neotime_runtime::{ColorValue, SystemValue, ThemeColor, Value, NONE, PRESET_THEMES};

use crate::{NotificationList, CALENDAR_PURPLE, MESSAGE_BLUE, MOST_KEPT};

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

// ============================================================================
// Icons
// ============================================================================

fn icon_for(app: &str, body: &str) -> Value {
    read_for(SystemValue::NotificationIcon, app, body)
}

fn color_for(app: &str, body: &str) -> Value {
    read_for(SystemValue::NotificationColor, app, body)
}

fn read_for(value: SystemValue, app: &str, body: &str) -> Value {
    let mut notification = PhoneNotification::default();
    let _ = notification.source.push_str(app);
    let _ = notification.body.push_str(body);
    let mut list = NotificationList::new();
    list.add(Box::new(notification), 0);
    list.read_element(value, 0)
}

fn library(icon: u16) -> Value {
    ImageReference::SharedLibrary(icon).to_value() as Value
}

#[test]
fn each_kind_of_notification_has_its_own_icon() {
    let kinds = [
        ("Missed call", "", library_icon::PHONE_MISSED),
        ("Phone", "Missed call", library_icon::PHONE_MISSED),
        ("Phone", "Accepted call", library_icon::PHONE),
        ("Call", "", library_icon::PHONE),
        ("SMS", "", library_icon::SMS),
        ("Messages", "", library_icon::CHAT),
        ("News", "", library_icon::NEWS),
        ("Voicemail", "", library_icon::VOICEMAIL),
        ("Schedule", "", library_icon::CALENDAR),
        ("Calendar", "", library_icon::CALENDAR),
        ("Alert", "", library_icon::BELL),
        ("Email", "", library_icon::NOTIFICATION),
        ("Signal", "", library_icon::NOTIFICATION),
    ];
    for (app, body, icon) in kinds {
        assert_eq!(icon_for(app, body), library(icon), "{app} {body}");
    }
}

#[test]
fn there_is_no_icon_past_the_end_of_the_list() {
    assert_eq!(
        NotificationList::new().read_element(SystemValue::NotificationIcon, 0),
        NONE
    );
}

// ============================================================================
// Colours
// ============================================================================

#[test]
fn each_kind_of_notification_has_its_own_colour() {
    let theme = |color| ColorValue::Theme(color);
    let kinds = [
        ("Missed call", "", theme(ThemeColor::Danger)),
        ("Phone", "Missed call", theme(ThemeColor::Danger)),
        ("Phone", "Accepted call", theme(ThemeColor::Success)),
        ("Call", "", theme(ThemeColor::Success)),
        ("SMS", "", MESSAGE_BLUE),
        ("Messages", "", MESSAGE_BLUE),
        ("Voicemail", "", MESSAGE_BLUE),
        ("Email", "", theme(ThemeColor::Warning)),
        ("Schedule", "", CALENDAR_PURPLE),
        ("Calendar", "", CALENDAR_PURPLE),
        ("News", "", theme(ThemeColor::Accent)),
        ("Alert", "", theme(ThemeColor::Accent)),
        ("Signal", "", theme(ThemeColor::Accent)),
    ];
    for (app, body, color) in kinds {
        assert_eq!(color_for(app, body), color.to_value(), "{app} {body}");
    }
}

#[test]
fn there_is_no_colour_past_the_end_of_the_list() {
    assert_eq!(
        NotificationList::new().read_element(SystemValue::NotificationColor, 0),
        NONE
    );
}

/// A colour's relative luminance, as the Web Content Accessibility Guidelines define it.
fn luminance(rgb565: u16) -> f64 {
    let channel = |value: u16, most: u16| {
        let fraction = value as f64 / most as f64;
        if fraction <= 0.03928 {
            fraction / 12.92
        } else {
            ((fraction + 0.055) / 1.055).powf(2.4)
        }
    };
    let red = channel(rgb565 >> 11, 31);
    let green = channel((rgb565 >> 5) & 63, 63);
    let blue = channel(rgb565 & 31, 31);
    0.2126 * red + 0.7152 * green + 0.0722 * blue
}

fn contrast(first: u16, second: u16) -> f64 {
    let (first, second) = (luminance(first), luminance(second));
    (first.max(second) + 0.05) / (first.min(second) + 0.05)
}

/// The least contrast the guidelines ask of an icon against what is behind it.
const LEAST_ICON_CONTRAST: f64 = 3.0;

#[test]
fn the_fixed_colours_read_on_every_preset_themes_background_and_surface() {
    for preset in PRESET_THEMES {
        for behind in [ThemeColor::Background, ThemeColor::Surface] {
            for color in [MESSAGE_BLUE, CALENDAR_PURPLE] {
                let drawn = color.resolve(&preset.theme);
                let measured = contrast(drawn, preset.theme.color(behind));
                assert!(
                    measured >= LEAST_ICON_CONTRAST,
                    "{color:?} on {} {behind:?}: {measured}",
                    preset.name
                );
            }
        }
    }
}
