//! The installed apps, read from `/apps` on the external flash, and the shared library in internal flash.
//!
//! Nothing is built into the firmware: the face, the launcher and every app are installed through
//! the companion app, and with no face the shell shows its help screen. An app's image is read
//! into the heap when it opens, and each app's icon is kept alone for the launcher.

extern crate alloc;

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ptr::NonNull;

pub use neotime_packages::LAUNCHER_ID;
use neotime_packages::{checks, InstalledApp, Refusal};
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
    /// The places in `packages` of the apps the launcher lists, in the same order.
    listed: Vec<u16>,
}

impl InstalledApps {
    pub const NONE: InstalledApps = InstalledApps {
        face: None,
        has_launcher: false,
        packages: Vec::new(),
        listed: Vec::new(),
    };

    pub fn read(files: &storage::Files) -> InstalledApps {
        let mut apps = InstalledApps::NONE;
        apps.packages = storage_apps::all(files);
        // The list stays for as long as the watch runs, so it keeps no spare room.
        apps.packages.shrink_to_fit();
        apps.sort_out();
        apps
    }

    /// Works out the face, the launcher and the listed apps from which packages are on.
    fn sort_out(&mut self) {
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
            || !storage_apps::set_enabled(files, id, enabled)
        {
            return false;
        }
        self.packages[index].enabled = enabled;
        self.sort_out();
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
    pub fn read(files: &storage::Files, installed: &InstalledApps) -> LauncherIcons {
        let icons: Vec<Lent<[u8]>> = (0..installed.listed_count())
            .filter_map(|place| installed.listed(place))
            .map(|entry| {
                let icon = entry
                    .has_icon
                    .then(|| storage_apps::read_image(files, &entry.id))
                    .flatten()
                    .and_then(|image| icon_of(&image));
                Lent::new(icon.unwrap_or_default().into_boxed_slice())
            })
            .collect();
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
    let bytes = storage_apps::read_image(files, id)?;
    Some(Lent::new(bytes.into_boxed_slice()))
}

// ============================================================================
// The shared library in internal flash
// ============================================================================

/// The installed shared library, read where it lies in internal flash, or None before one is installed.
pub fn shared_library() -> Option<&'static [u8]> {
    let region = neotime_boot::SHARED_LIBRARY;
    let bytes = unsafe { core::slice::from_raw_parts(region.start as *const u8, region.size as usize) };
    neotime_packages::library::installed_library(bytes)
}

// ============================================================================
// The package manager, or nothing in the rehearsals, which keep no files
// ============================================================================

#[cfg(not(feature = "wasp-os-rehearsal"))]
mod storage_apps {
    use alloc::vec::Vec;

    use neotime_packages::{InstalledApp, Packages};

    use crate::storage;

    pub fn all(files: &storage::Files) -> Vec<InstalledApp> {
        files
            .file_system()
            .and_then(|file_system| Packages::new(file_system).list().ok())
            .unwrap_or_default()
    }

    pub fn set_enabled(files: &storage::Files, id: &str, enabled: bool) -> bool {
        files
            .file_system()
            .is_some_and(|file_system| Packages::new(file_system).set_enabled(id, enabled).is_ok())
    }

    pub fn read_image(files: &storage::Files, id: &str) -> Option<Vec<u8>> {
        Packages::new(files.file_system()?).read_image(id).ok()
    }
}

#[cfg(feature = "wasp-os-rehearsal")]
mod storage_apps {
    use alloc::vec::Vec;

    use neotime_packages::InstalledApp;

    use crate::storage;

    pub fn all(_files: &storage::Files) -> Vec<InstalledApp> {
        Vec::new()
    }

    pub fn set_enabled(_files: &storage::Files, _id: &str, _enabled: bool) -> bool {
        false
    }

    pub fn read_image(_files: &storage::Files, _id: &str) -> Option<Vec<u8>> {
        None
    }
}
