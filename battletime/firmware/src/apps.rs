//! The installed apps, read from `/apps` on the external flash, and the shared library in internal flash.
//!
//! Nothing is built into the firmware: the face, the launcher and every app are installed through
//! the companion app, and with no face the shell shows its help screen. The face is the one the
//! `watch_face` setting names, or the first face turned on when that one is off or gone; the ids
//! of the faces turned on stay in RAM, so choosing another needs no list. An app's image is read
//! into the heap when it opens, and each app's icon is kept alone for the launcher.

extern crate alloc;

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ptr::NonNull;

use battletime_packages::faces::{face_number, face_to_show, faces_turned_on, stepped_face, NO_FACE_CHOSEN};
use battletime_packages::launcher::{self, LauncherLook, ShownGroup, LAUNCHER_FILE};
pub use battletime_packages::LAUNCHER_ID;
use battletime_packages::{checks, InstalledApp, Refusal};
use battletime_runtime::memory_need::reads_app_list;
use battletime_runtime::resources::ImageReference;
use battletime_runtime::{AppImage, AppKind};
use battletime_settings::Setting;

use crate::{settings, storage};

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
///
/// The full list is kept only while an app that reads it is open, such as the launcher or the Software
/// app. Otherwise only `face`, `has_launcher`, `look` and the listed apps' ids stay, a few bytes an app.
pub struct InstalledApps {
    pub face: Option<String>,
    /// The shown face's name, for apps to read as `watch_face_name`.
    pub face_name: String,
    /// The ids of the faces turned on, in id order, each ending in a newline, so a face is picked without the full list.
    face_ids: String,
    pub has_launcher: bool,
    /// The ids of the apps the launcher lists, in its order, each ending in a newline.
    listed_ids: String,
    /// Whether the full list is here, rather than only the ids.
    details: bool,
    /// Every installed package, turned on or off, in id order, as the Software app lists them; empty without the details.
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
        face_name: String::new(),
        face_ids: String::new(),
        has_launcher: false,
        listed_ids: String::new(),
        details: false,
        packages: Vec::new(),
        listed: Vec::new(),
        look: LauncherLook { style: 0, view: 0 },
        groups: Vec::new(),
        launcher_file: Vec::new(),
    };

    /// Reads the full list; `forget_details` keeps only the ids once no app reads it.
    pub fn read(files: &storage::Files) -> InstalledApps {
        let mut apps = InstalledApps::NONE;
        apps.packages = files.installed_apps_now();
        apps.packages.shrink_to_fit();
        apps.sort_out(files);
        apps.details = true;
        apps
    }

    /// Whether the full list is here, rather than only the ids.
    pub fn has_details(&self) -> bool {
        self.details
    }

    /// Frees everything but the face, the faces turned on, the launcher, the look and the listed apps' ids.
    pub fn forget_details(&mut self) {
        self.packages = Vec::new();
        self.listed = Vec::new();
        self.groups = Vec::new();
        self.launcher_file = Vec::new();
        self.details = false;
    }

    /// The number of the face shown, as the `watch_face` setting holds it, or 0 with no face on.
    pub fn face_number(&self) -> i32 {
        self.face.as_deref().map_or(NO_FACE_CHOSEN, face_number)
    }

    /// The number of the face `steps` places on from the one shown, wrapping round, for the `watch_face` setting.
    pub fn stepped_face_number(&self, steps: i32) -> Option<i32> {
        stepped_face(self.face_ids.lines(), self.face.as_deref().unwrap_or(""), steps).map(face_number)
    }

    /// Picks the face again after the `watch_face` setting changed, and reads the new face's name from its image.
    pub fn choose_face_again(&mut self, files: &storage::Files) {
        let face_before = self.face.take();
        self.pick_face();
        if self.face != face_before {
            self.face_name = self
                .face
                .as_deref()
                .and_then(|id| files.read_app_name_now(id))
                .unwrap_or_default();
        }
    }

    /// Picks the face the `watch_face` setting names among the faces turned on, or else the first of them.
    fn pick_face(&mut self) {
        let chosen_number = settings::current().get(Setting::WatchFace);
        self.face = face_to_show(self.face_ids.lines(), chosen_number).map(String::from);
    }

    /// Works out the face, the launcher and the listed apps from which packages are on, grouped as the launcher file says.
    fn sort_out(&mut self, files: &storage::Files) {
        self.face_ids.clear();
        for id in faces_turned_on(&self.packages) {
            self.face_ids.push_str(id);
            self.face_ids.push('\n');
        }
        self.face_ids.shrink_to_fit();
        self.pick_face();
        self.face_name = self
            .packages
            .iter()
            .find(|app| Some(&app.id) == self.face.as_ref())
            .map(|face| face.name.clone())
            .unwrap_or_default();
        self.has_launcher = false;
        self.listed.clear();
        for (place, app) in self.packages.iter().enumerate().filter(|(_, app)| app.enabled) {
            match app.kind {
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
        self.listed_ids.clear();
        for &place in &self.listed {
            self.listed_ids.push_str(&self.packages[place as usize].id);
            self.listed_ids.push('\n');
        }
        self.listed_ids.shrink_to_fit();
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
        self.listed_ids.lines().count()
    }

    /// The app at a place in the launcher's list, when the full list is here.
    pub fn listed(&self, place: usize) -> Option<&InstalledApp> {
        self.packages.get(*self.listed.get(place)? as usize)
    }

    /// The id of the app at a place in the launcher's list.
    pub fn listed_id(&self, place: usize) -> Option<&str> {
        self.listed_ids.lines().nth(place)
    }

    pub fn place_of(&self, id: &str) -> Option<usize> {
        self.listed_ids.lines().position(|listed| listed == id)
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
            || !files.set_app_enabled_soon(id, enabled)
        {
            return false;
        }
        self.packages[index].enabled = enabled;
        self.sort_out(files);
        true
    }
}

/// Reads the full list of apps when the app about to open reads it, and lets it go when it does not.
///
/// Called once the old app has closed, so the two never hold the heap at once.
#[inline(never)]
pub fn keep_list_for(installed: &mut InstalledApps, files: &storage::Files, image_bytes: &[u8]) {
    let wanted = AppImage::parse(image_bytes).is_ok_and(|image| reads_app_list(&image));
    match (wanted, installed.has_details()) {
        (true, false) => *installed = InstalledApps::read(files),
        (false, true) => installed.forget_details(),
        _ => {}
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
            let icon = if entry.has_icon {
                files.read_app_icon_now(&entry.id)
            } else {
                None
            };
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

/// The image of the app `id` in `directory`, `/apps` or `/temp`, read into the heap for an `AppHost` to borrow.
pub fn read_image(files: &storage::Files, directory: &str, id: &str) -> Option<Lent<[u8]>> {
    let bytes = files.read_app_image_now(directory, id)?;
    Some(Lent::new(bytes.into_boxed_slice()))
}

// ============================================================================
// The shared library inside the firmware image
// ============================================================================

/// The shared library carried after the running firmware's code, or None in a build without one.
pub fn shared_library() -> Option<&'static [u8]> {
    let region = battletime_boot::FIRMWARE;
    let image = unsafe { core::slice::from_raw_parts(region.start as *const u8, region.size as usize) };
    battletime_packages::library::firmware_library(image)
}
