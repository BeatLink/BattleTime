//! NeoTime's package manager: the apps and watch faces installed on the external flash.
//!
//! Each app keeps a folder, `/apps/<id>/`, holding `app.ntapp`, its compiled image, and `state`,
//! what it saved. An app is installed once its image passes the same checks the app host makes,
//! and its id matches the folder. A `disabled` marker in the folder hides it without deleting it.
//! The folders are the whole record, so there is no list to fall out of step with them.
//!
//! A temporary app's folder, `/temp/<id>/`, holds an image the phone runs without installing it,
//! beside the data files it reads; it is never listed, and its files stay until the phone deletes them.
//!
//! Turning an app on, installing it included, first checks that it and every other app turned on
//! still fit the heap; `checks` holds the rules.

#![no_std]

extern crate alloc;

use alloc::{boxed::Box, string::String, vec::Vec};
use core::fmt::Write;

use neotime_flash_store::{FileAccess, FileError};
use neotime_runtime::memory_need::{estimate_memory_need, launcher_share_bytes, APP_HEAP_BUDGET_BYTES};
use neotime_runtime::{verify_with, AppImage, AppKind, CheckedFunctions, ImageError};

pub mod checks;
pub mod commands;
pub mod faces;
pub mod image_reader;
pub mod launcher;
pub mod library;

pub use checks::{check_change, Refusal};

#[cfg(test)]
mod tests;

// ============================================================================
// Names inside an app's folder
// ============================================================================

/// Every app's folder is inside this one.
pub const APPS_DIRECTORY: &str = "/apps";

/// The compiled image, as `neotime-compiler` writes it.
pub const IMAGE_FILE: &str = "app.ntapp";
/// What the app saved; the shell writes it.
pub const STATE_FILE: &str = "state";
/// Present when the app is installed but hidden from the launcher.
pub const DISABLED_FILE: &str = "disabled";

/// The launcher's id; the shell opens it on a swipe up from the face, so it is never turned off.
pub const LAUNCHER_ID: &str = "launcher";

/// The longest id the package manager takes, which keeps every path within the store's 64 bytes.
pub const MOST_ID_BYTES: usize = 32;

/// A path inside `/apps`, long enough for any id and file name.
pub type AppPath = heapless::String<64>;

/// An app's id, held without the heap.
pub type AppId = heapless::String<MOST_ID_BYTES>;

/// Temporary apps' folders, `/temp/<id>/`: the phone writes an image and its data there and runs it without installing it.
pub const TEMPORARY_DIRECTORY: &str = "/temp";

// ============================================================================
// Types
// ============================================================================

/// One installed app, as the shell and the companion app see it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledApp {
    pub id: String,
    pub name: String,
    pub kind: AppKind,
    /// The launcher group the app declares, such as `games`, or empty when it declares none.
    pub group: String,
    pub has_icon: bool,
    pub enabled: bool,
    pub image_bytes: u32,
    /// The heap it needs while open, from `estimate_memory_need`.
    pub need_bytes: u32,
    /// Whether it shows the launcher's list of apps, which grows with every app turned on.
    pub lists_apps: bool,
    /// Whether it reads the installed apps or packages, so the shell keeps its full list while it is open.
    pub reads_app_list: bool,
    /// What it adds to the launcher while the launcher is open.
    pub launcher_share_bytes: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageError {
    /// The id is empty, too long, or holds something other than letters, digits, `-` and `_`.
    BadId,
    /// The folder or its image is missing.
    NotInstalled,
    /// The image fails the app host's checks.
    BadImage(ImageError),
    /// The image is the shared library, which ships inside the firmware instead.
    IsLibrary,
    /// The id inside the image is not the folder's name.
    IdMismatch,
    /// Turning the app on or off would leave an app without the heap it needs, or the watch without a launcher or face.
    Refused(Refusal),
    /// The app was built against a newer shared library than the watch has.
    NeedsNewerLibrary,
    /// The shell could not open the image: it is missing, fails the app checks, or needs a newer shared library.
    WouldNotOpen,
    /// The store refused.
    Storage(FileError),
}

impl PackageError {
    /// A short explanation for the companion app.
    pub fn describe(self) -> &'static str {
        match self {
            PackageError::BadId => "an app id is letters, digits, - and _",
            PackageError::NotInstalled => "no such app",
            PackageError::BadImage(_) => "the image fails the app checks",
            PackageError::IsLibrary => "the shared library ships inside the firmware",
            PackageError::IdMismatch => "the image's id is not the folder's name",
            PackageError::Refused(refusal) => refusal.describe(),
            PackageError::NeedsNewerLibrary => "the app needs a newer shared library; update the watch first",
            PackageError::WouldNotOpen => "the image is missing or would not open",
            PackageError::Storage(FileError::Full) => "no room left on the flash",
            PackageError::Storage(_) => "the flash store refused",
        }
    }
}

impl From<FileError> for PackageError {
    fn from(error: FileError) -> PackageError {
        PackageError::Storage(error)
    }
}

// ============================================================================
// Paths
// ============================================================================

pub fn check_id(id: &str) -> Result<(), PackageError> {
    let fits = !id.is_empty() && id.len() <= MOST_ID_BYTES;
    let plain = id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    if fits && plain {
        Ok(())
    } else {
        Err(PackageError::BadId)
    }
}

pub fn app_folder(id: &str) -> AppPath {
    let mut path = AppPath::new();
    let _ = write!(path, "{APPS_DIRECTORY}/{id}");
    path
}

pub fn app_file(id: &str, file: &str) -> AppPath {
    file_in(APPS_DIRECTORY, id, file)
}

/// A file in the app's folder inside `directory`, which is `APPS_DIRECTORY` or `TEMPORARY_DIRECTORY`.
pub fn file_in(directory: &str, id: &str, file: &str) -> AppPath {
    let mut path = AppPath::new();
    let _ = write!(path, "{directory}/{id}/{file}");
    path
}

pub fn temporary_file(id: &str, file: &str) -> AppPath {
    file_in(TEMPORARY_DIRECTORY, id, file)
}

// ============================================================================
// Checking images
// ============================================================================

/// The checks the app host makes before it runs an image.
fn check_image(bytes: &[u8]) -> Result<AppImage<'_>, PackageError> {
    let image = AppImage::parse(bytes).map_err(PackageError::BadImage)?;
    verify_with(&image, &mut checked_functions_on_the_heap()).map_err(PackageError::BadImage)?;
    // A listing trusts the stored facts, so they are checked against a fresh analysis once, here.
    let need = estimate_memory_need(&image);
    let facts = neotime_runtime::image::Facts {
        need_bytes: need.total_bytes,
        launcher_share_bytes: launcher_share_bytes(&image),
        lists_apps: need.lists_apps,
        reads_app_list: need.reads_app_list,
    };
    if image.facts != facts {
        return Err(PackageError::BadImage(ImageError::BadFacts));
    }
    Ok(image)
}

/// The verifier's 800-byte table, on the heap, since commands run with little stack to spare.
fn checked_functions_on_the_heap() -> Box<CheckedFunctions> {
    Box::new(CheckedFunctions::NONE)
}

/// Checks an app or face image for the folder `id`.
pub fn check_app_image(id: &str, bytes: &[u8]) -> Result<(), PackageError> {
    let image = check_image(bytes)?;
    if image.kind == AppKind::Library {
        return Err(PackageError::IsLibrary);
    }
    if image.string(image.id_string) != Some(id) {
        return Err(PackageError::IdMismatch);
    }
    Ok(())
}

// ============================================================================
// The package manager
// ============================================================================

/// An app whose image alone is larger than the heap an open app has, listed without reading it and always off.
fn too_large_to_read(id: &str, length: u32) -> InstalledApp {
    InstalledApp {
        id: String::from(id),
        name: String::from(id),
        kind: AppKind::App,
        group: String::new(),
        has_icon: false,
        enabled: false,
        image_bytes: length,
        need_bytes: length,
        lists_apps: false,
        reads_app_list: false,
        launcher_share_bytes: 0,
    }
}

pub struct Packages<'files, Files: FileAccess> {
    files: &'files mut Files,
}

impl<'files, Files: FileAccess> Packages<'files, Files> {
    pub fn new(files: &'files mut Files) -> Packages<'files, Files> {
        Packages { files }
    }

    /// Every installed app in id order, which is the order the launcher shows them in.
    /// A folder whose image is missing, damaged or not its own is left out. Installing verified
    /// the code and the app host verifies it again before running it, so listing only parses each image.
    pub async fn list(&mut self) -> Result<Vec<InstalledApp>, PackageError> {
        let mut ids: Vec<String> = Vec::new();
        let mut prefix = String::from(APPS_DIRECTORY);
        prefix.push('/');
        self.files
            .for_each_file(&prefix, &mut |path| {
                let folder_and_file = path[prefix.len()..].split_once('/');
                if let Some((id, IMAGE_FILE)) = folder_and_file {
                    if check_id(id).is_ok() {
                        ids.push(String::from(id));
                    }
                }
            })
            .await?;
        let mut apps = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(app) = self.describe(&id).await {
                apps.push(app);
            }
        }
        // Ids are unique, so this gives the stable order without the stable sort's 4 KB buffer on the stack.
        apps.sort_unstable_by(|a, b| a.id.cmp(&b.id));
        Ok(apps)
    }

    /// The enabled apps, which the shell offers as `installed_apps`.
    pub async fn enabled(&mut self) -> Result<Vec<InstalledApp>, PackageError> {
        let mut apps = self.list().await?;
        apps.retain(|app| app.enabled);
        Ok(apps)
    }

    async fn exists(&mut self, path: &str) -> bool {
        matches!(self.files.file_length(path).await, Ok(Some(_)))
    }

    async fn read_whole(&mut self, path: &str) -> Option<Vec<u8>> {
        let length = self.files.file_length(path).await.ok()??;
        let mut bytes = alloc::vec![0; length];
        self.files.read_file(path, 0, &mut bytes).await.ok()??;
        Some(bytes)
    }

    /// An app as a listing gives it, read from its image's header, strings and facts without reading the image whole.
    async fn describe(&mut self, id: &str) -> Option<InstalledApp> {
        let length = self.image_length(id).await?;
        if length > APP_HEAP_BUDGET_BYTES {
            return Some(too_large_to_read(id, length));
        }
        let path = app_file(id, IMAGE_FILE);
        let outline = image_reader::outline(self.files, &path).await?;
        if outline.kind == AppKind::Library
            || image_reader::id(self.files, &path, &outline).await.as_deref() != Some(id)
        {
            return None;
        }
        let facts = image_reader::facts(self.files, &path, &outline).await?;
        Some(InstalledApp {
            id: String::from(id),
            name: image_reader::name(self.files, &path, &outline).await?,
            kind: outline.kind,
            group: image_reader::group(self.files, &path, &outline).await?,
            has_icon: outline.has_icon,
            enabled: !self.exists(&app_file(id, DISABLED_FILE)).await,
            image_bytes: length,
            need_bytes: facts.need_bytes,
            lists_apps: facts.lists_apps,
            reads_app_list: facts.reads_app_list,
            launcher_share_bytes: facts.launcher_share_bytes,
        })
    }

    /// An installed app's icon, read on its own, or none when it has none.
    pub async fn read_icon(&mut self, id: &str) -> Option<Vec<u8>> {
        check_id(id).ok()?;
        image_reader::icon(self.files, &app_file(id, IMAGE_FILE)).await
    }

    /// An installed app's name, read without the rest of its image.
    pub async fn read_name(&mut self, id: &str) -> Option<String> {
        check_id(id).ok()?;
        let path = app_file(id, IMAGE_FILE);
        let outline = image_reader::outline(self.files, &path).await?;
        image_reader::name(self.files, &path, &outline).await
    }

    /// Whether the app has subscriptions that run while it is closed, read without the rest of its image.
    pub async fn runs_while_closed(&mut self, id: &str) -> bool {
        check_id(id).is_ok()
            && image_reader::runs_while_closed(self.files, &app_file(id, IMAGE_FILE)).await == Some(true)
    }

    /// The app's image, to load into RAM for the app host.
    pub async fn read_image(&mut self, id: &str) -> Result<Vec<u8>, PackageError> {
        self.read_image_in(APPS_DIRECTORY, id).await
    }

    /// The image of the app `id` in `directory`, `APPS_DIRECTORY` or `TEMPORARY_DIRECTORY`, to load into RAM for the app host.
    pub async fn read_image_in(&mut self, directory: &str, id: &str) -> Result<Vec<u8>, PackageError> {
        check_id(id)?;
        let path = file_in(directory, id, IMAGE_FILE);
        let length = self.files.file_length(&path).await.ok().flatten();
        let length = length.ok_or(PackageError::NotInstalled)? as u32;
        if length > APP_HEAP_BUDGET_BYTES {
            return Err(PackageError::Refused(Refusal::TooLarge { need_bytes: length }));
        }
        self.read_whole(&path).await.ok_or(PackageError::NotInstalled)
    }

    /// The image's length, from the store, without reading it.
    async fn image_length(&mut self, id: &str) -> Option<u32> {
        let length = self.files.file_length(&app_file(id, IMAGE_FILE)).await.ok()??;
        Some(length as u32)
    }

    /// Checks the image already written to `/apps/<id>/app.ntapp` and makes the app visible.
    /// On a bad image the folder is left as it was, so the phone can try again. An app that would not
    /// fit the heap stays installed but turned off, and the refusal says why. So does an app built
    /// against a newer shared library than `library_version`, the installed one's, since it would not
    /// open; with no library installed yet, as while a watch is set up, any version is taken.
    pub async fn install(
        &mut self,
        id: &str,
        library_version: Option<u16>,
    ) -> Result<InstalledApp, PackageError> {
        check_id(id)?;
        let bytes = match self.read_image(id).await {
            Err(refused @ PackageError::Refused(_)) => {
                self.set_enabled(id, false).await?;
                return Err(refused);
            }
            other => other?,
        };
        // A listing reads only an image's header, strings and facts, so a refused image is turned off to keep it out of the launcher.
        if let Err(refusal) = check_app_image(id, &bytes) {
            drop(bytes);
            self.set_enabled(id, false).await?;
            return Err(refusal);
        }
        let needed = AppImage::parse(&bytes)
            .map_err(PackageError::BadImage)?
            .shared_library_version;
        drop(bytes);
        if library_version.is_some_and(|installed| needed > installed) {
            self.set_enabled(id, false).await?;
            return Err(PackageError::NeedsNewerLibrary);
        }
        let apps = self.list().await?;
        if let Err(refusal) = check_change(&apps, id, true) {
            self.set_enabled(id, false).await?;
            return Err(PackageError::Refused(refusal));
        }
        self.set_enabled(id, true).await?;
        self.describe(id).await.ok_or(PackageError::NotInstalled)
    }

    /// Turns an installed app on or off once the checks allow it, and returns the heap it needs.
    pub async fn change_enabled(&mut self, id: &str, enabled: bool) -> Result<u32, PackageError> {
        check_id(id)?;
        let apps = self.list().await?;
        let need = check_change(&apps, id, enabled).map_err(PackageError::Refused)?;
        self.set_enabled(id, enabled).await?;
        Ok(need)
    }

    /// Deletes every file in the app's folder, its saved state included.
    pub async fn remove(&mut self, id: &str) -> Result<(), PackageError> {
        check_id(id)?;
        let mut prefix = app_folder(id);
        let _ = prefix.push('/');
        let mut paths: Vec<String> = Vec::new();
        self.files
            .for_each_file(&prefix, &mut |path| paths.push(String::from(path)))
            .await?;
        if paths.is_empty() {
            return Err(PackageError::NotInstalled);
        }
        for path in paths {
            self.files.delete_file(&path).await?;
        }
        Ok(())
    }

    /// Shows or hides an installed app without deleting it.
    pub async fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), PackageError> {
        check_id(id)?;
        if !self.exists(&app_file(id, IMAGE_FILE)).await {
            return Err(PackageError::NotInstalled);
        }
        let marker = app_file(id, DISABLED_FILE);
        match (enabled, self.exists(&marker).await) {
            (true, true) => self.files.delete_file(&marker).await?,
            (false, false) => self.files.write_file(&marker, &[]).await?,
            _ => {}
        }
        Ok(())
    }
}
