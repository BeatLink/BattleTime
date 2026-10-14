//! The installed apps, read from `/apps` on the external flash, and the shared library in internal flash.
//!
//! Nothing is built into the firmware: the face, the launcher and every app are installed through
//! the companion app, and with no face the shell shows its help screen. An app's image is read
//! into the heap when it opens, and each app's icon is kept alone for the launcher.

extern crate alloc;

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ptr::NonNull;

use neotime_packages::launcher::{self, LauncherLook, ShownGroup, LAUNCHER_FILE};
pub use neotime_packages::LAUNCHER_ID;
use neotime_packages::{checks, InstalledApp, Refusal};
use neotime_runtime::resources::ImageReference;
use neotime_runtime::{AppImage, AppKind};

use crate::storage;

// ============================================================================
// Bytes the app host borrows
// ============================================================================

/// Bytes on the heap that an `AppHost<'static>` may borrow; `release` frees them.
///
/// The shell drops every host that borrowed them before it calls `release`.
pub struct Lent<T: ?Sized + 'static> {
    bytes: NonNull<T>,
}

impl<T: ?Sized> Lent<T> {
    fn new(boxed: Box<T>) -> Lent<T> {
        Lent {
            bytes: NonNull::from(Box::leak(boxed)),
        }
    }

    pub fn get(&self) -> &'static T {
        // The bytes stay put until `release`, which the shell calls only once no host borrows them.
        unsafe { self.bytes.as_ref() }
    }

    /// Frees the bytes; every host that borrowed them must already be gone.
    pub fn release(self) {
        drop(unsafe { Box::from_raw(self.bytes.as_ptr()) });
    }
}

// ============================================================================
// The list of installed apps
// ============================================================================

/// Every installed app and face, and which of them are on: the face, the launcher, and the apps the launcher lists.
pub struct InstalledApps {
    pub face: Option<String>,
    pub has_launcher: bool,
    /// Every installed package, turned on or off, in id order, as the Software app lists them.
    pub packages: Vec<InstalledApp>,
    /// The places in `packages` of the apps the launcher lists, group by group, as `/settings/launcher` arranges them.
    listed: Vec<u16>,
    /// How the launcher lays out its groups and draws their apps, from `/settings/launcher`.
    pub look: LauncherLook,
    /// The groups holding at least one listed app, each a run of `listed`.
    pub groups: Vec<ShownGroup>,
    /// `/settings/launcher` as the companion wrote it, which holds the groups' names.
    launcher_file: Vec<u8>,
}

impl InstalledApps {
    pub const NONE: InstalledApps = InstalledApps {
        face: None,
        has_launcher: false,
        packages: Vec::new(),
        listed: Vec::new(),
        look: LauncherLook { style: 0, view: 0 },
        groups: Vec::new(),
        launcher_file: Vec::new(),
    };

    pub fn read(files: &storage::Files) -> InstalledApps {
        let mut apps = InstalledApps::NONE;
        apps.packages = files.installed_apps_now();
        // The list stays for as long as the watch runs, so it keeps no spare room.
        apps.packages.shrink_to_fit();
        apps.sort_out(files);
        apps
    }

    /// Works out the face, the launcher and the listed apps from which packages are on, grouped as the launcher file says.
    fn sort_out(&mut self, files: &storage::Files) {
        self.face = None;
        self.has_launcher = false;
        self.listed.clear();
        for (place, app) in self.packages.iter().enumerate().filter(|(_, app)| app.enabled) {
            match app.kind {
                AppKind::Face if self.face.is_none() => self.face = Some(app.id.clone()),
                AppKind::App if app.id == LAUNCHER_ID => self.has_launcher = true,
                AppKind::App => self.listed.push(place as u16),
                _ => {}
            }
        }
        self.launcher_file = files.read_now(LAUNCHER_FILE).unwrap_or_default();
        let listed_apps: Vec<&InstalledApp> = self
            .listed
            .iter()
            .map(|&place| &self.packages[place as usize])
            .collect();
        let arrangement = launcher::arrange(&self.launcher_file, &listed_apps);
        drop(listed_apps);
        self.listed = arrangement
            .order
            .iter()
            .map(|&index| self.listed[index as usize])
            .collect();
        self.groups = arrangement.groups;
        self.groups.shrink_to_fit();
        self.look = arrangement.look;
    }

    /// A launcher group's icon: its shared library icon, or its first app's when the file gives none.
    pub fn group_icon(&self, place: usize) -> Option<ImageReference> {
        let group = self.groups.get(place)?;
        Some(match group.icon {
            Some(icon) => ImageReference::SharedLibrary(icon as u16),
            None => ImageReference::InstalledApp(group.first),
        })
    }

    /// A launcher group's name, read from the launcher file.
    pub fn group_name(&self, place: usize) -> &str {
        self.groups
            .get(place)
            .map_or("", |group| launcher::group_name(&self.launcher_file, group))
    }

    /// How many apps the launcher lists.
    pub fn listed_count(&self) -> usize {
        self.listed.len()
    }

    /// The app at a place in the launcher's list.
    pub fn listed(&self, place: usize) -> Option<&InstalledApp> {
        self.packages.get(*self.listed.get(place)? as usize)
    }

    pub fn place_of(&self, id: &str) -> Option<usize> {
        (0..self.listed.len()).find(|&place| self.listed(place).is_some_and(|app| app.id == id))
    }

    /// The heap a package needs while open among the others.
    pub fn memory_of(&self, index: usize) -> Option<u32> {
        Some(checks::room_needed(self.packages.get(index)?, &self.packages))
    }

    /// Why turning a package the other way would be refused, 0 when it would not; the open app cannot be turned off.
    pub fn refusal_of(&self, index: usize, open_id: &str) -> u8 {
        let Some(app) = self.packages.get(index) else {
            return 0;
        };
        if app.enabled && app.id == open_id {
            return Refusal::IsOpen.code();
        }
        checks::check_change(&self.packages, &app.id, !app.enabled).map_or_else(Refusal::code, |_| 0)
    }

    /// Turns a package on or off once the checks allow it, on the flash and in this list.
    pub fn set_enabled(&mut self, files: &storage::Files, id: &str, enabled: bool) -> bool {
        let Some(index) = self.packages.iter().position(|app| app.id == id) else {
            return false;
        };
        if checks::check_change(&self.packages, id, enabled).is_err()
            || !files.set_app_enabled_now(id, enabled)
        {
            return false;
        }
        self.packages[index].enabled = enabled;
        self.sort_out(files);
        true
    }
}

// ============================================================================
// The launcher's icons
// ============================================================================

/// Each listed app's icon, read only while the launcher is open, so other apps have that RAM.
pub struct LauncherIcons {
    icons: Vec<Lent<[u8]>>,
    list: Lent<[&'static [u8]]>,
}

impl LauncherIcons {
    // Kept out of `Shell::open`, whose frame stays on the stack while the app's handlers run.
    #[inline(never)]
    pub fn read(files: &storage::Files, installed: &InstalledApps) -> LauncherIcons {
        let mut icons: Vec<Lent<[u8]>> = Vec::new();
        for entry in (0..installed.listed_count()).filter_map(|place| installed.listed(place)) {
            let image = if entry.has_icon {
                files.read_app_image_now(&entry.id)
            } else {
                None
            };
            let icon = image.and_then(|image| icon_of(&image));
            icons.push(Lent::new(icon.unwrap_or_default().into_boxed_slice()));
        }
        let list: Vec<&'static [u8]> = icons.iter().map(Lent::get).collect();
        LauncherIcons {
            icons,
            list: Lent::new(list.into_boxed_slice()),
        }
    }

    /// The icons in the launcher's order, for `AppHost::set_installed_app_icons`.
    pub fn list(&self) -> &'static [&'static [u8]] {
        self.list.get()
    }

    /// Frees the icons; the launcher's host must already be gone.
    pub fn release(self) {
        self.list.release();
        for icon in self.icons {
            icon.release();
        }
    }
}

/// An image's icon alone, which is its first picture when it says it has one.
fn icon_of(image_bytes: &[u8]) -> Option<Vec<u8>> {
    let image = AppImage::parse(image_bytes).ok()?;
    let icon = image.has_icon.then(|| image.resource_bytes(0)).flatten()?;
    Some(icon.to_vec())
}

/// An app's image, read into the heap for an `AppHost` to borrow.
pub fn read_image(files: &storage::Files, id: &str) -> Option<Lent<[u8]>> {
    let bytes = files.read_app_image_now(id)?;
    Some(Lent::new(bytes.into_boxed_slice()))
}

// ============================================================================
// The shared library inside the firmware image
// ============================================================================

/// The shared library carried after the running firmware's code, or None in a build without one.
pub fn shared_library() -> Option<&'static [u8]> {
    let region = neotime_boot::FIRMWARE;
    let image = unsafe { core::slice::from_raw_parts(region.start as *const u8, region.size as usize) };
    neotime_packages::library::firmware_library(image)
}
