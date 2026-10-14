extern crate std;

use alloc::{string::String, vec, vec::Vec};

use neotime_runtime::memory_need::{indexed_app_bytes, listed_app_bytes, APP_HEAP_BUDGET_BYTES};
use neotime_runtime::AppKind;

use super::*;
use crate::InstalledApp;

// ============================================================================
// Lists of apps
// ============================================================================

fn app(id: &str, kind: AppKind, enabled: bool, need_bytes: u32) -> InstalledApp {
    InstalledApp {
        id: String::from(id),
        name: String::from(id),
        kind,
        group: String::new(),
        has_icon: true,
        enabled,
        image_bytes: 1_000,
        need_bytes,
        lists_apps: id == LAUNCHER_ID,
        reads_app_list: id == LAUNCHER_ID,
        launcher_share_bytes: 100,
    }
}

fn watch(extra: Vec<InstalledApp>) -> Vec<InstalledApp> {
    let mut apps = vec![
        app("digital", AppKind::Face, true, 12_000),
        app(LAUNCHER_ID, AppKind::App, true, 8_000),
        app("timer", AppKind::App, true, 13_000),
    ];
    apps.extend(extra);
    apps
}

/// The shell's full list of apps, kept only while an app that reads it is open.
fn shell_list(apps: &[InstalledApp]) -> u32 {
    apps.iter().map(|app| listed_app_bytes(&app.id, &app.name)).sum()
}

/// The shell's index of app ids, which every open app has beside it.
fn shell_index(apps: &[InstalledApp]) -> u32 {
    apps.iter().map(|app| indexed_app_bytes(&app.id)).sum()
}

// ============================================================================
// The heap each app needs
// ============================================================================

#[test]
fn every_app_pays_for_the_shell_index_and_the_launcher_for_the_full_list_and_each_listed_app() {
    let apps = watch(vec![app("dice", AppKind::App, false, 9_000)]);

    assert_eq!(room_needed(&apps[2], &apps), 13_000 + shell_index(&apps));
    // The timer is listed, the face, the launcher itself and the turned-off dice are not.
    assert_eq!(
        room_needed(&apps[1], &apps),
        8_000 + shell_index(&apps) + shell_list(&apps) + 100
    );
}

#[test]
fn an_app_that_reads_the_app_list_pays_for_the_full_list_but_not_the_launcher_share() {
    let mut software = app("software", AppKind::App, true, 10_000);
    software.reads_app_list = true;
    let apps = watch(vec![software]);

    assert_eq!(
        room_needed(&apps[3], &apps),
        10_000 + shell_index(&apps) + shell_list(&apps)
    );
}

// ============================================================================
// Turning apps on
// ============================================================================

#[test]
fn an_app_that_fits_is_turned_on_and_its_need_given() {
    let apps = watch(vec![app("dice", AppKind::App, false, 9_000)]);

    assert_eq!(check_change(&apps, "dice", true), Ok(9_000 + shell_index(&apps)));
}

#[test]
fn an_app_needing_more_than_the_budget_is_refused() {
    let apps = watch(vec![app("huge", AppKind::App, false, APP_HEAP_BUDGET_BYTES)]);

    let refusal = check_change(&apps, "huge", true).unwrap_err();

    assert_eq!(
        refusal,
        Refusal::TooLarge {
            need_bytes: APP_HEAP_BUDGET_BYTES + shell_index(&apps)
        }
    );
    assert_eq!(refusal.code(), 1);
}

#[test]
fn an_app_is_refused_when_the_launcher_would_no_longer_fit() {
    let mut apps = watch(vec![app("dice", AppKind::App, false, 9_000)]);
    let room_left = APP_HEAP_BUDGET_BYTES - room_needed(&apps[1], &apps);
    // The dice's share of the launcher, its icon and name, is one byte more than the launcher has left.
    apps[3].launcher_share_bytes = room_left + 1;

    let refusal = check_change(&apps, "dice", true).unwrap_err();

    assert!(
        matches!(refusal, Refusal::LauncherWouldNotFit { need_bytes } if need_bytes > APP_HEAP_BUDGET_BYTES)
    );
    assert_eq!(refusal.code(), 2);
}

#[test]
fn a_face_never_grows_the_launcher() {
    let mut apps = watch(vec![app("analog", AppKind::Face, false, 9_000)]);
    apps[3].launcher_share_bytes = APP_HEAP_BUDGET_BYTES;

    assert!(check_change(&apps, "analog", true).is_ok());
}

#[test]
fn an_app_that_is_not_installed_passes_for_the_caller_to_report() {
    assert_eq!(check_change(&watch(vec![]), "nothing", true), Ok(0));
}

// ============================================================================
// Turning apps off
// ============================================================================

#[test]
fn turning_an_app_off_is_allowed() {
    let apps = watch(vec![]);
    assert_eq!(
        check_change(&apps, "timer", false),
        Ok(13_000 + shell_index(&apps))
    );
}

#[test]
fn the_launcher_and_the_last_face_stay_on() {
    let apps = watch(vec![]);
    assert_eq!(
        check_change(&apps, LAUNCHER_ID, false),
        Err(Refusal::LauncherStaysOn)
    );
    assert_eq!(
        check_change(&apps, "digital", false),
        Err(Refusal::LastFaceStaysOn)
    );

    let with_second_face = watch(vec![app("analog", AppKind::Face, true, 11_000)]);
    assert_eq!(
        check_change(&with_second_face, "digital", false),
        Ok(12_000 + shell_index(&with_second_face))
    );
}

#[test]
fn an_app_already_off_can_be_turned_off_again() {
    let apps = watch(vec![app("analog", AppKind::Face, false, 11_000)]);
    assert_eq!(
        check_change(&apps, "analog", false),
        Ok(11_000 + shell_index(&apps))
    );
}

#[test]
fn every_refusal_explains_itself_with_its_own_number() {
    let refusals = [
        Refusal::TooLarge { need_bytes: 1 },
        Refusal::LauncherWouldNotFit { need_bytes: 2 },
        Refusal::LauncherStaysOn,
        Refusal::LastFaceStaysOn,
        Refusal::IsOpen,
    ];
    for (place, refusal) in refusals.iter().enumerate() {
        assert_eq!(refusal.code() as usize, place + 1);
        assert!(!refusal.describe().is_empty());
        assert_eq!(refusal.need_bytes().is_some(), place < 2);
    }
}
