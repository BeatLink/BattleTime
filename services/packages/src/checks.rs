//! Whether an app may be turned on or off: every app turned on must still fit the heap while it is
//! open, and the watch must keep its launcher and a face.
//!
//! The rules work on the list `Packages::list` gives, so the package manager, the shell and the
//! test harness all check the same way. docs/memory-map.md explains the numbers.

use neotime_runtime::memory_need::{listed_app_bytes, APP_HEAP_BUDGET_BYTES};
use neotime_runtime::AppKind;

use crate::{InstalledApp, LAUNCHER_ID};

// ============================================================================
// Refusals
// ============================================================================

/// Why turning an app on or off is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The app needs more heap than an open app may have.
    TooLarge { need_bytes: u32 },
    /// With the app turned on, the launcher's list would grow past the heap.
    LauncherWouldNotFit { need_bytes: u32 },
    /// The launcher is how every other app is reached, so it stays on.
    LauncherStaysOn,
    /// The watch shows a face when nothing else is open, so the last one stays on.
    LastFaceStaysOn,
    /// The app is open on the watch, so it cannot be turned off from inside it.
    IsOpen,
}

impl Refusal {
    /// The number `PackageRefusal` gives an app, 0 meaning no refusal.
    pub fn code(self) -> u8 {
        match self {
            Refusal::TooLarge { .. } => 1,
            Refusal::LauncherWouldNotFit { .. } => 2,
            Refusal::LauncherStaysOn => 3,
            Refusal::LastFaceStaysOn => 4,
            Refusal::IsOpen => 5,
        }
    }

    /// A short explanation for the companion app.
    pub fn describe(self) -> &'static str {
        match self {
            Refusal::TooLarge { .. } => "it needs more memory than an app can have",
            Refusal::LauncherWouldNotFit { .. } => "the launcher would run out of memory",
            Refusal::LauncherStaysOn => "the launcher stays on",
            Refusal::LastFaceStaysOn => "the last watch face stays on",
            Refusal::IsOpen => "it is open on the watch",
        }
    }

    /// The heap the app that would not fit needs, when that is the reason.
    pub fn need_bytes(self) -> Option<u32> {
        match self {
            Refusal::TooLarge { need_bytes } | Refusal::LauncherWouldNotFit { need_bytes } => {
                Some(need_bytes)
            }
            _ => None,
        }
    }
}

// ============================================================================
// The heap an app needs among the others
// ============================================================================

/// What an app needs while open beside the list of installed apps the shell keeps, and for a
/// launcher the names and icons of every app turned on, plus the largest image it reads them from.
pub fn room_needed(app: &InstalledApp, apps: &[InstalledApp]) -> u32 {
    room_needed_with(app, apps, None)
}

/// Like `room_needed`, with the app `turned_on` counted as on whatever its marker says.
fn room_needed_with(app: &InstalledApp, apps: &[InstalledApp], turned_on: Option<&str>) -> u32 {
    let shell_list: u32 = apps
        .iter()
        .map(|other| listed_app_bytes(&other.id, &other.name))
        .sum();
    let mut need = app.need_bytes + shell_list;
    if app.lists_apps {
        let listed = apps.iter().filter(|other| is_listed(other, turned_on));
        let shares: u32 = listed.clone().map(|other| other.launcher_share_bytes).sum();
        let largest_image = listed.map(|other| other.image_bytes).max().unwrap_or(0);
        need += shares + largest_image;
    }
    need
}

/// Whether an app is on, counting `turned_on` as on.
fn is_on(app: &InstalledApp, turned_on: Option<&str>) -> bool {
    app.enabled || Some(app.id.as_str()) == turned_on
}

/// Whether the launcher lists an app: on, not a face, and not the launcher itself.
fn is_listed(app: &InstalledApp, turned_on: Option<&str>) -> bool {
    is_on(app, turned_on) && app.kind == AppKind::App && app.id != LAUNCHER_ID
}

// ============================================================================
// The check
// ============================================================================

/// Checks turning the app `id` on or off among `apps`, and gives the heap it needs once changed.
///
/// An id that is not in the list passes, so the caller reports it as not installed.
pub fn check_change(apps: &[InstalledApp], id: &str, enabled: bool) -> Result<u32, Refusal> {
    let Some(app) = apps.iter().find(|app| app.id == id) else {
        return Ok(0);
    };
    if !enabled {
        let other_faces_on = apps
            .iter()
            .any(|other| other.id != id && other.enabled && other.kind == AppKind::Face);
        return match app {
            _ if app.enabled && app.id == LAUNCHER_ID => Err(Refusal::LauncherStaysOn),
            _ if app.enabled && app.kind == AppKind::Face && !other_faces_on => Err(Refusal::LastFaceStaysOn),
            _ => Ok(room_needed(app, apps)),
        };
    }

    let turned_on = Some(id);
    let own_need = room_needed_with(app, apps, turned_on);
    if own_need > APP_HEAP_BUDGET_BYTES {
        return Err(Refusal::TooLarge { need_bytes: own_need });
    }
    // Only a launcher's need grows with the app turned on; the shell's list already counts every installed app.
    if is_listed(app, turned_on) {
        let launchers = apps
            .iter()
            .filter(|other| other.enabled && other.id != id && other.lists_apps);
        for launcher in launchers {
            let need = room_needed_with(launcher, apps, turned_on);
            if need > APP_HEAP_BUDGET_BYTES {
                return Err(Refusal::LauncherWouldNotFit { need_bytes: need });
            }
        }
    }
    Ok(own_need)
}

#[cfg(test)]
#[path = "checks_tests.rs"]
mod checks_tests;
