//! How the launcher groups the apps: the style it shows them in and the groups the companion set up.
//!
//! The companion writes `/settings/launcher`, which says where every app goes, so the watch only
//! follows it. It is bytes, so the firmware reads names in place and keeps no copy of them:
//!
//! | Bytes | Holds |
//! |---|---|
//! | 0 | The format version, 1. |
//! | 1 | The style: 0 one list, 1 folders, 2 sections, 3 pages. |
//! | 2 | The view: 0 rows, 1 a grid. |
//! | 3 | How many groups follow. |
//! | then, per group | Its shared library icon's number (255 for none), its id's length and id (for the companion; the watch skips it), its name's length and the name in UTF-8, how many apps it holds, then each app's id length and id. |
//!
//! The groups are in the launcher's order, and each group's apps in the order it shows them. The
//! companion places each app in the group its image declares until the wearer moves it, and writes
//! the file again whenever it installs one. Apps the file does not name, such as one installed
//! another way, go in a group of their own at the end, named Other, in order of their names. After the
//! groups the companion may add a byte of its own, how it sorts (0 by name, 1 the wearer's order), which
//! the watch ignores. A missing file, or one of
//! another version, shows one list; a group cut short ends the groups there.

use alloc::vec::Vec;

use crate::InstalledApp;

/// Where the companion writes the launcher's groups.
pub const LAUNCHER_FILE: &str = "/settings/launcher";

pub const LAUNCHER_FILE_VERSION: u8 = 1;

/// An icon's value for none.
pub const NONE_BYTE: u8 = 255;

/// The name of the group that takes the apps the file does not name.
pub const OTHER_GROUP_NAME: &str = "Other";

const HEADER_BYTES: usize = 4;

// ============================================================================
// Types
// ============================================================================

/// How the launcher lays out the groups: one list, folders, sections or pages, in the numbers apps read
/// from `launcher_style`; and how it draws a group's apps: rows or a grid, as `launcher_view`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LauncherLook {
    pub style: u8,
    pub view: u8,
}

/// One group the launcher shows, with the run of `order` that holds its apps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShownGroup {
    /// Where its name lies in the file, or none for the Other group.
    pub name: Option<(u16, u8)>,
    /// The shared library icon's number, or none to show its first app's.
    pub icon: Option<u8>,
    pub first: u16,
    pub count: u16,
}

/// The apps in the launcher's order and the groups that hold at least one of them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Arrangement {
    pub look: LauncherLook,
    /// Places in the list of apps given to `arrange`.
    pub order: Vec<u16>,
    pub groups: Vec<ShownGroup>,
}

// ============================================================================
// Reading the file
// ============================================================================

/// A group's name from the file, or Other's.
pub fn group_name<'file>(file: &'file [u8], group: &ShownGroup) -> &'file str {
    match group.name {
        Some((at, length)) => file
            .get(at as usize..at as usize + length as usize)
            .and_then(|name| core::str::from_utf8(name).ok())
            .unwrap_or(""),
        None => OTHER_GROUP_NAME,
    }
}

// ============================================================================
// Grouping the apps
// ============================================================================

/// Puts `apps` in the launcher's order, group by group, as the file says; with no usable file, one list.
///
/// A plain walk over the bytes and, for each id, over the apps: a watch holds a few dozen apps,
/// and this costs the firmware less flash than parsing into a list and sorting.
pub fn arrange(file: &[u8], apps: &[&InstalledApp]) -> Arrangement {
    let mut arrangement = Arrangement::default();
    let byte = |at: usize| file.get(at).copied();
    let mut groups_left = 0;
    if file.len() >= HEADER_BYTES && file[0] == LAUNCHER_FILE_VERSION {
        arrangement.look = LauncherLook {
            style: file[1],
            view: file[2],
        };
        groups_left = file[3];
    }
    let mut at = HEADER_BYTES;
    'groups: while groups_left > 0 {
        groups_left -= 1;
        let first = arrangement.order.len();
        let Some(icon) = byte(at) else {
            break;
        };
        at += 2 + byte(at + 1).unwrap_or(0) as usize;
        let Some(name_length) = byte(at) else {
            break;
        };
        let name_at = at + 1;
        at = name_at + name_length as usize;
        let Some(mut ids_left) = byte(at) else {
            break;
        };
        at += 1;
        while ids_left > 0 {
            ids_left -= 1;
            let id = byte(at).and_then(|length| file.get(at + 1..at + 1 + length as usize));
            let Some(id) = id else {
                // A group cut short ends the groups, and its apps go to Other.
                arrangement.order.truncate(first);
                break 'groups;
            };
            at += 1 + id.len();
            if let Some(place) = apps.iter().position(|app| app.id.as_bytes() == id) {
                push_once(&mut arrangement.order, place);
            }
        }
        if arrangement.order.len() > first {
            arrangement.groups.push(ShownGroup {
                name: Some((name_at as u16, name_length)),
                icon: (icon != NONE_BYTE).then_some(icon),
                first: first as u16,
                count: (arrangement.order.len() - first) as u16,
            });
        }
    }
    // Apps the file leaves out, or every app when there is no file, in order of their names.
    let first = arrangement.order.len();
    while let Some(place) = (0..apps.len())
        .filter(|&place| !arrangement.order.contains(&(place as u16)))
        .min_by(|&a, &b| by_name(apps[a], apps[b]))
    {
        arrangement.order.push(place as u16);
    }
    if arrangement.order.len() > first {
        arrangement.groups.push(ShownGroup {
            name: None,
            icon: None,
            first: first as u16,
            count: (arrangement.order.len() - first) as u16,
        });
    }
    arrangement
}

/// Orders two apps by name, ignoring case, then by id so equal names keep one order.
fn by_name(a: &InstalledApp, b: &InstalledApp) -> core::cmp::Ordering {
    let a_name = a.name.bytes().map(|byte| byte.to_ascii_lowercase());
    let b_name = b.name.bytes().map(|byte| byte.to_ascii_lowercase());
    a_name.cmp(b_name).then_with(|| a.id.cmp(&b.id))
}

fn push_once(order: &mut Vec<u16>, place: usize) {
    if !order.contains(&(place as u16)) {
        order.push(place as u16);
    }
}

#[cfg(test)]
#[path = "launcher_tests.rs"]
mod launcher_tests;
