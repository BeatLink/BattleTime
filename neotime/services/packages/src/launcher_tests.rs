extern crate std;

use alloc::{string::String, vec, vec::Vec};

use neotime_runtime::AppKind;

use super::*;
use crate::InstalledApp;

// ============================================================================
// Helpers
// ============================================================================

fn app(id: &str) -> InstalledApp {
    InstalledApp {
        id: String::from(id),
        name: String::from(id),
        kind: AppKind::App,
        group: String::new(),
        has_icon: true,
        enabled: true,
        image_bytes: 1_000,
        need_bytes: 8_000,
        lists_apps: false,
        reads_app_list: false,
        launcher_share_bytes: 100,
    }
}

/// A launcher file with a style, a view and each group's icon, name and apps.
fn file(style: u8, view: u8, groups: &[(u8, &str, &[&str])]) -> Vec<u8> {
    let mut bytes = vec![LAUNCHER_FILE_VERSION, style, view, groups.len() as u8];
    for (icon, name, ids) in groups {
        let id = name.to_lowercase();
        bytes.extend([*icon, id.len() as u8]);
        bytes.extend(id.as_bytes());
        bytes.push(name.len() as u8);
        bytes.extend(name.as_bytes());
        bytes.push(ids.len() as u8);
        for id in *ids {
            bytes.push(id.len() as u8);
            bytes.extend(id.as_bytes());
        }
    }
    bytes
}

type Shown = Vec<(String, Option<u8>, Vec<String>)>;

/// Each shown group as its name, its icon and its apps' ids.
fn shown(file: &[u8], apps: &[InstalledApp]) -> Shown {
    let references: Vec<&InstalledApp> = apps.iter().collect();
    let arrangement = arrange(file, &references);
    arrangement
        .groups
        .iter()
        .map(|group| {
            let run = &arrangement.order[group.first as usize..(group.first + group.count) as usize];
            (
                String::from(group_name(file, group)),
                group.icon,
                run.iter().map(|&place| apps[place as usize].id.clone()).collect(),
            )
        })
        .collect()
}

fn expect(groups: &[(&str, Option<u8>, &[&str])]) -> Shown {
    groups
        .iter()
        .map(|(name, icon, ids)| {
            (
                String::from(*name),
                *icon,
                ids.iter().map(|id| String::from(*id)).collect(),
            )
        })
        .collect()
}

fn apps(ids: &[&str]) -> Vec<InstalledApp> {
    ids.iter().map(|id| app(id)).collect()
}

const GAMES_AND_TIME: [(u8, &str, &[&str]); 2] =
    [(17, "Games", &["snake", "dice"]), (10, "Time apps", &["alarm"])];

// ============================================================================
// Grouping
// ============================================================================

#[test]
fn apps_go_in_their_groups_in_the_files_order_and_the_rest_in_other() {
    let file = file(1, 1, &GAMES_AND_TIME);
    let installed = apps(&["alarm", "dice", "hello", "snake"]);

    assert_eq!(
        shown(&file, &installed),
        expect(&[
            ("Games", Some(17), &["snake", "dice"]),
            ("Time apps", Some(10), &["alarm"]),
            ("Other", None, &["hello"]),
        ])
    );
    assert_eq!(arrange(&file, &[]).look, LauncherLook { style: 1, view: 1 });
}

#[test]
fn ids_of_apps_not_installed_and_empty_groups_are_skipped() {
    let file = file(
        1,
        0,
        &[(NONE_BYTE, "Empty", &["gone"]), (3, "Games", &["gone", "dice"])],
    );

    assert_eq!(
        shown(&file, &apps(&["dice"])),
        expect(&[("Games", Some(3), &["dice"])])
    );
}

#[test]
fn a_group_without_an_icon_shows_its_first_apps() {
    let file = file(1, 0, &[(NONE_BYTE, "Mine", &["dice"])]);

    assert_eq!(
        shown(&file, &apps(&["dice"])),
        expect(&[("Mine", None, &["dice"])])
    );
}

#[test]
fn an_app_named_twice_shows_only_where_it_is_first_named() {
    let file = file(1, 0, &[(1, "A", &["dice"]), (2, "B", &["dice", "snake"])]);

    assert_eq!(
        shown(&file, &apps(&["dice", "snake"])),
        expect(&[("A", Some(1), &["dice"]), ("B", Some(2), &["snake"])])
    );
}

#[test]
fn with_no_file_every_app_is_in_one_group_in_the_order_given() {
    let installed = apps(&["alarm", "dice"]);
    let arrangement = arrange(&[], &installed.iter().collect::<Vec<_>>());

    assert_eq!(arrangement.order, vec![0, 1]);
    assert_eq!(arrangement.look, LauncherLook::default());
    assert_eq!(
        shown(&[], &installed),
        expect(&[("Other", None, &["alarm", "dice"])])
    );
}

#[test]
fn a_file_of_another_version_is_one_list() {
    let mut other = file(1, 1, &GAMES_AND_TIME);
    other[0] = 2;

    assert_eq!(arrange(&other, &[]).look, LauncherLook::default());
    assert_eq!(
        shown(&other, &apps(&["snake"])),
        expect(&[("Other", None, &["snake"])])
    );
}

#[test]
fn a_cut_short_file_keeps_the_groups_before_the_cut() {
    let whole = file(1, 0, &GAMES_AND_TIME);
    let cut = &whole[..whole.len() - 2];

    assert_eq!(
        shown(cut, &apps(&["alarm", "snake"])),
        expect(&[("Games", Some(17), &["snake"]), ("Other", None, &["alarm"])])
    );
}

#[test]
fn a_group_whose_ids_are_cut_short_sends_its_apps_to_other() {
    let whole = file(1, 0, &GAMES_AND_TIME);
    let cut = &whole[..whole.len() - 30];

    assert_eq!(
        shown(cut, &apps(&["dice", "snake"])),
        expect(&[("Other", None, &["dice", "snake"])])
    );
}

#[test]
fn a_name_that_is_not_utf8_reads_as_empty() {
    let mut bytes = file(1, 0, &[(1, "AB", &["dice"])]);
    bytes[9] = 0xFF;
    let installed = apps(&["dice"]);
    let arrangement = arrange(&bytes, &installed.iter().collect::<Vec<_>>());

    assert_eq!(group_name(&bytes, &arrangement.groups[0]), "");
}

#[test]
fn no_apps_make_no_groups() {
    let file = file(1, 0, &GAMES_AND_TIME);

    assert_eq!(arrange(&file, &[]).groups, vec![]);
}
