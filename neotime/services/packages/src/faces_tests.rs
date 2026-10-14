extern crate std;

use alloc::{string::String, vec, vec::Vec};

use neotime_runtime::AppKind;

use super::*;
use crate::InstalledApp;

// ============================================================================
// Lists of apps
// ============================================================================

fn app(id: &str, kind: AppKind, enabled: bool) -> InstalledApp {
    InstalledApp {
        id: String::from(id),
        name: String::from(id),
        kind,
        group: String::new(),
        has_icon: true,
        enabled,
        image_bytes: 1_000,
        need_bytes: 9_000,
        lists_apps: false,
        reads_app_list: false,
        launcher_share_bytes: 100,
    }
}

fn watch() -> Vec<InstalledApp> {
    vec![
        app("analog", AppKind::Face, true),
        app("casio", AppKind::Face, false),
        app("digital", AppKind::Face, true),
        app("timer", AppKind::App, true),
    ]
}

fn shown_id(packages: &[InstalledApp], chosen_number: i32) -> Option<&str> {
    face_to_show(faces_turned_on(packages), chosen_number)
}

fn stepped_id<'apps>(packages: &'apps [InstalledApp], shown: &str, steps: i32) -> Option<&'apps str> {
    stepped_face(faces_turned_on(packages), shown, steps)
}

// ============================================================================
// Numbers
// ============================================================================

#[test]
fn a_face_number_is_the_fnv_hash_of_its_id_kept_positive() {
    assert_eq!(face_number("analog"), 0x3114_E811);
    assert_eq!(face_number(""), 0x011C_9DC5);
    assert!(face_number("digital") > 0);
    assert_ne!(face_number("analog"), face_number("digital"));
}

// ============================================================================
// Choosing
// ============================================================================

#[test]
fn the_chosen_face_shows() {
    assert_eq!(shown_id(&watch(), face_number("digital")), Some("digital"));
}

#[test]
fn with_no_face_chosen_the_first_face_on_shows() {
    assert_eq!(shown_id(&watch(), NO_FACE_CHOSEN), Some("analog"));
}

#[test]
fn a_chosen_face_turned_off_or_removed_falls_back_to_the_first_face_on() {
    assert_eq!(shown_id(&watch(), face_number("casio")), Some("analog"));
    assert_eq!(shown_id(&watch(), face_number("word-clock")), Some("analog"));
}

#[test]
fn an_app_with_the_chosen_number_is_never_shown_as_the_face() {
    assert_eq!(shown_id(&watch(), face_number("timer")), Some("analog"));
}

#[test]
fn a_watch_with_no_face_on_shows_none() {
    let apps = vec![
        app("casio", AppKind::Face, false),
        app("timer", AppKind::App, true),
    ];

    assert_eq!(shown_id(&apps, face_number("casio")), None);
}

// ============================================================================
// Stepping
// ============================================================================

#[test]
fn only_faces_turned_on_count() {
    let apps = watch();

    assert_eq!(faces_turned_on(&apps).collect::<Vec<_>>(), ["analog", "digital"]);
}

#[test]
fn a_step_on_from_the_last_face_wraps_round_to_the_first() {
    assert_eq!(stepped_id(&watch(), "analog", 1), Some("digital"));
    assert_eq!(stepped_id(&watch(), "digital", 1), Some("analog"));
}

#[test]
fn a_step_back_from_the_first_face_wraps_round_to_the_last() {
    assert_eq!(stepped_id(&watch(), "analog", -1), Some("digital"));
    assert_eq!(stepped_id(&watch(), "analog", -4), Some("analog"));
}

#[test]
fn stepping_from_a_face_not_on_counts_from_the_first() {
    assert_eq!(stepped_id(&watch(), "casio", 1), Some("digital"));
}

#[test]
fn stepping_with_no_face_on_finds_none() {
    let apps = vec![app("timer", AppKind::App, true)];

    assert_eq!(stepped_id(&apps, "", 1), None);
}
