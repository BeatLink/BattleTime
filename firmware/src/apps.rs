//! The installed apps, read from `/apps` on the external flash, and the shared library in internal flash.
//!
//! Nothing is built into the firmware: the face, the launcher and every app are installed through
//! the companion app, and with no face the shell shows its help screen. An app's image is read
//! into the heap when it opens, and each app's icon is kept alone for the launcher.

extern crate alloc;

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ptr::NonNull;

use neotime_runtime::{AppImage, AppKind};

use crate::storage;

/// The launcher's id; the shell opens it on a swipe up from the face.
pub const LAUNCHER_ID: &str = "launcher";

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

pub struct Entry {
    pub id: String,
    pub name: String,
    has_icon: bool,
}

/// The enabled apps: the face, the launcher, and the apps the launcher lists.
pub struct InstalledApps {
    pub face: Option<String>,
    pub has_launcher: bool,
    /// What the launcher lists, in the package manager's order.
    pub listed: Vec<Entry>,
}

impl InstalledApps {
    pub const NONE: InstalledApps = InstalledApps {
        face: None,
        has_launcher: false,
        listed: Vec::new(),
    };

    pub fn read(files: &storage::Files) -> InstalledApps {
        let mut apps = InstalledApps::NONE;
        for app in storage_apps::enabled(files) {
            match app.kind {
                AppKind::Face if apps.face.is_none() => apps.face = Some(app.id),
                AppKind::App if app.id == LAUNCHER_ID => apps.has_launcher = true,
                AppKind::App => apps.listed.push(Entry {
                    id: app.id,
                    name: app.name,
                    has_icon: app.has_icon,
                }),
                _ => {}
            }
        }
        apps
    }

    pub fn place_of(&self, id: &str) -> Option<usize> {
        self.listed.iter().position(|entry| entry.id == id)
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
        let icons: Vec<Lent<[u8]>> = installed
            .listed
            .iter()
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

    pub fn enabled(files: &storage::Files) -> Vec<InstalledApp> {
        files
            .file_system()
            .and_then(|file_system| Packages::new(file_system).enabled().ok())
            .unwrap_or_default()
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

    pub fn enabled(_files: &storage::Files) -> Vec<InstalledApp> {
        Vec::new()
    }

    pub fn read_image(_files: &storage::Files, _id: &str) -> Option<Vec<u8>> {
        None
    }
}
