extern crate std;

use std::{vec, vec::Vec};

use embassy_futures::block_on;
use neotime_flash_store::{ram_flash::RamFlash, FlashStore, Layout};
use neotime_runtime::writer::ImageWriter;

use super::*;

// ============================================================================
// A store in RAM, and images to install
// ============================================================================

type TestStore = FlashStore<RamFlash, 32, 64, 8>;

const LAYOUT: Layout = Layout {
    start_address: 0,
    file_sectors: 64,
    record_sectors: 8,
    log_sectors: 4,
};

fn file_system() -> TestStore {
    block_on(TestStore::mount(RamFlash::blank(LAYOUT.total_sectors()), LAYOUT)).unwrap()
}

fn write_raw(files: &mut TestStore, path: &str, bytes: &[u8]) {
    block_on(FileAccess::write_file(files, path, bytes)).unwrap();
}

/// The package manager with each call run to its end, so the tests read as before.
struct SyncPackages<'files>(Packages<'files, TestStore>);

impl SyncPackages<'_> {
    fn list(&mut self) -> Result<Vec<InstalledApp>, PackageError> {
        block_on(self.0.list())
    }

    fn enabled(&mut self) -> Result<Vec<InstalledApp>, PackageError> {
        block_on(self.0.enabled())
    }

    fn read_image(&mut self, id: &str) -> Result<Vec<u8>, PackageError> {
        block_on(self.0.read_image(id))
    }

    fn install(&mut self, id: &str, library_version: Option<u16>) -> Result<InstalledApp, PackageError> {
        block_on(self.0.install(id, library_version))
    }

    fn change_enabled(&mut self, id: &str, enabled: bool) -> Result<u32, PackageError> {
        block_on(self.0.change_enabled(id, enabled))
    }

    fn remove(&mut self, id: &str) -> Result<(), PackageError> {
        block_on(self.0.remove(id))
    }

    fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), PackageError> {
        block_on(self.0.set_enabled(id, enabled))
    }

    fn exists(&mut self, path: &str) -> bool {
        block_on(self.0.exists(path))
    }
}

fn packages(files: &mut TestStore) -> SyncPackages<'_> {
    SyncPackages(Packages::new(files))
}

fn image(id: &str, name: &str, kind: AppKind) -> Vec<u8> {
    ImageWriter::new(id, name, kind, 1).build()
}

fn put(files: &mut TestStore, id: &str, bytes: &[u8]) {
    write_raw(files, &app_file(id, IMAGE_FILE), bytes);
}

// ============================================================================
// Ids
// ============================================================================

#[test]
fn an_id_is_short_and_plain() {
    assert_eq!(check_id("timer"), Ok(()));
    assert_eq!(check_id("puzzle_15-b"), Ok(()));
    for bad in [
        "",
        "../x",
        "a/b",
        "spaced id",
        "é",
        &"x".repeat(MOST_ID_BYTES + 1),
    ] {
        assert_eq!(check_id(bad), Err(PackageError::BadId), "{bad}");
    }
}

#[test]
fn paths_sit_inside_the_apps_folder() {
    assert_eq!(app_folder("timer").as_str(), "/apps/timer");
    assert_eq!(app_file("timer", IMAGE_FILE).as_str(), "/apps/timer/app.ntapp");
}

// ============================================================================
// Installing
// ============================================================================

#[test]
fn an_app_built_for_a_newer_library_than_the_watch_has_is_refused() {
    let mut files = file_system();
    put(
        &mut files,
        "timer",
        &ImageWriter::new("timer", "Timer", AppKind::App, 3).build(),
    );
    let mut packages = packages(&mut files);

    assert_eq!(
        packages.install("timer", Some(2)),
        Err(PackageError::NeedsNewerLibrary)
    );
    assert!(!packages.list().unwrap()[0].enabled);
    assert!(packages.install("timer", Some(3)).is_ok());
    assert!(packages.install("timer", None).is_ok());
    assert_eq!(
        crate::library::library_version(&ImageWriter::new("shared", "Shared", AppKind::Library, 7).build()),
        Some(7)
    );
}

/// A firmware image as `tools/image_header.py --library` writes it: header, code, then the library.
fn firmware_image_with(library: &[u8]) -> Vec<u8> {
    use neotime_boot::image_header::{ImageHeader, ImageType, Version, IMAGE_HEADER_SIZE};
    let code = vec![0x5A; 100];
    let body = [code.clone(), library.to_vec()].concat();
    let header = ImageHeader {
        image_type: ImageType::Firmware,
        version: Version {
            major: 0,
            minor: 1,
            patch: 0,
        },
        load_address: neotime_boot::FIRMWARE_VECTOR_TABLE,
        image_size_in_bytes: body.len() as u32,
        image_crc32: 0,
        build_time_in_seconds: 0,
        commit: [0; 8],
        library_offset: if library.is_empty() {
            0
        } else {
            IMAGE_HEADER_SIZE + code.len() as u32
        },
        library_size_in_bytes: library.len() as u32,
    };
    [header.encode().to_vec(), body].concat()
}

#[test]
fn the_library_is_found_inside_the_firmware_image() {
    let library = ImageWriter::new("shared", "Shared", AppKind::Library, 2).build();
    let image = firmware_image_with(&library);

    let found = crate::library::firmware_library(&image);

    assert_eq!(found, Some(&library[..]));
    assert_eq!(found.and_then(crate::library::library_version), Some(2));
}

#[test]
fn a_firmware_without_a_library_or_a_header_has_none() {
    assert_eq!(crate::library::firmware_library(&firmware_image_with(&[])), None);
    assert_eq!(crate::library::firmware_library(&[0xFF; 512]), None);
    assert_eq!(crate::library::firmware_library(&[]), None);
}

#[test]
fn an_app_installs_and_is_listed_with_its_name() {
    let mut files = file_system();
    put(&mut files, "timer", &image("timer", "Timer", AppKind::App));
    let mut packages = packages(&mut files);

    let installed = packages.install("timer", None).unwrap();

    assert_eq!(installed.name, "Timer");
    assert_eq!(installed.kind, AppKind::App);
    assert!(installed.enabled);
    assert_eq!(packages.list().unwrap(), vec![installed]);
}

/// An app whose image alone is larger than the heap an open app may have.
fn huge_image(id: &str) -> Vec<u8> {
    let mut writer = ImageWriter::new(id, "Huge", AppKind::App, 1);
    writer.string(&"x".repeat(neotime_runtime::APP_HEAP_BUDGET_BYTES as usize));
    writer.build()
}

/// An app that fits the heap by its image, but not with its widgets and working memory too.
fn large_image(id: &str) -> Vec<u8> {
    let mut writer = ImageWriter::new(id, "Large", AppKind::App, 1);
    writer.string(&"x".repeat(neotime_runtime::APP_HEAP_BUDGET_BYTES as usize - 4_000));
    writer.widget(neotime_runtime::WidgetKind::Column, 0, &[]);
    writer.build()
}

#[test]
fn an_image_larger_than_the_heap_is_never_read_and_listed_as_off() {
    let mut files = file_system();
    let bytes = huge_image("huge");
    put(&mut files, "huge", &bytes);
    let mut packages = packages(&mut files);

    let refusal = Err(PackageError::Refused(Refusal::TooLarge {
        need_bytes: bytes.len() as u32,
    }));
    assert_eq!(packages.read_image("huge").map(|_| ()), refusal);
    assert_eq!(packages.install("huge", None).map(|_| ()), refusal);
    let listed = packages.list().unwrap();
    assert_eq!((listed[0].name.as_str(), listed[0].enabled), ("huge", false));
    assert!(packages.exists(&app_file("huge", DISABLED_FILE)));
}

#[test]
fn an_app_too_large_for_the_heap_stays_installed_but_turned_off() {
    let mut files = file_system();
    put(&mut files, "huge", &large_image("huge"));
    let mut packages = packages(&mut files);

    let result = packages.install("huge", None);

    assert!(matches!(
        result,
        Err(PackageError::Refused(Refusal::TooLarge { need_bytes })) if need_bytes > neotime_runtime::APP_HEAP_BUDGET_BYTES
    ));
    let listed = packages.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert!(!listed[0].enabled);
    assert_eq!(
        packages.change_enabled("huge", true),
        Err(PackageError::Refused(Refusal::TooLarge {
            need_bytes: checks::room_needed(&listed[0], &listed)
        }))
    );
}

#[test]
fn turning_an_app_on_and_off_gives_its_need() {
    let mut files = file_system();
    put(&mut files, "timer", &image("timer", "Timer", AppKind::App));
    let mut packages = packages(&mut files);
    let installed = packages.install("timer", None).unwrap();
    assert!(installed.need_bytes > 0);
    assert!(!installed.lists_apps);

    let listed = neotime_runtime::memory_need::indexed_app_bytes("timer");
    assert_eq!(
        packages.change_enabled("timer", false),
        Ok(installed.need_bytes + listed)
    );
    assert!(!packages.list().unwrap()[0].enabled);
    assert_eq!(
        packages.change_enabled("timer", true),
        Ok(installed.need_bytes + listed)
    );
    assert_eq!(
        packages.change_enabled("dice", true),
        Err(PackageError::NotInstalled)
    );
    assert_eq!(packages.change_enabled("../x", true), Err(PackageError::BadId));
}

#[test]
fn a_face_installs_too() {
    let mut files = file_system();
    put(&mut files, "analog", &image("analog", "Analog", AppKind::Face));
    assert_eq!(
        packages(&mut files).install("analog", None).unwrap().kind,
        AppKind::Face
    );
}

#[test]
fn a_broken_image_is_refused_turned_off_and_left_for_another_try() {
    let mut files = file_system();
    let mut bytes = image("timer", "Timer", AppKind::App);
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    put(&mut files, "timer", &bytes);
    let mut packages = packages(&mut files);

    assert!(matches!(
        packages.install("timer", None),
        Err(PackageError::BadImage(_))
    ));
    assert!(packages.exists(&app_file("timer", IMAGE_FILE)));
    assert!(!packages.list().unwrap()[0].enabled);
}

#[test]
fn the_shared_library_is_not_an_app() {
    let mut files = file_system();
    put(
        &mut files,
        "shared",
        &image("shared", "Shared library", AppKind::Library),
    );
    assert_eq!(
        packages(&mut files).install("shared", None),
        Err(PackageError::IsLibrary)
    );
}

#[test]
fn an_image_must_be_in_its_own_folder() {
    let mut files = file_system();
    put(&mut files, "timer", &image("dice", "Dice", AppKind::App));
    assert_eq!(
        packages(&mut files).install("timer", None),
        Err(PackageError::IdMismatch)
    );
}

#[test]
fn installing_without_an_image_says_so() {
    let mut files = file_system();
    assert_eq!(
        packages(&mut files).install("timer", None),
        Err(PackageError::NotInstalled)
    );
    assert_eq!(
        packages(&mut files).install("../etc", None),
        Err(PackageError::BadId)
    );
}

// ============================================================================
// Listing, hiding and removing
// ============================================================================

/// An image that parses, so its CRC is right, but whose code fails verification.
fn image_with_bad_code(id: &str) -> Vec<u8> {
    let mut writer = ImageWriter::new(id, "Broken", AppKind::App, 1);
    writer.expression(vec![]);
    writer.build()
}

#[test]
fn installing_verifies_the_code_but_listing_only_parses() {
    let mut files = file_system();
    let bytes = image_with_bad_code("timer");
    assert!(AppImage::parse(&bytes).is_ok());
    put(&mut files, "timer", &bytes);
    let mut packages = packages(&mut files);

    assert!(matches!(
        packages.install("timer", None),
        Err(PackageError::BadImage(_))
    ));
    let ids: Vec<String> = packages.list().unwrap().into_iter().map(|app| app.id).collect();
    assert_eq!(ids, vec!["timer"]);
}

#[test]
fn the_list_is_in_id_order_and_skips_what_is_not_an_app() {
    let mut files = file_system();
    put(&mut files, "timer", &image("timer", "Timer", AppKind::App));
    put(&mut files, "dice", &image("dice", "Dice", AppKind::App));
    put(&mut files, "junk", &[0; 8]);
    write_raw(&mut files, "/apps/loose-file", b"x");

    let ids: Vec<String> = packages(&mut files)
        .list()
        .unwrap()
        .into_iter()
        .map(|app| app.id)
        .collect();

    assert_eq!(ids, vec!["dice", "timer"]);
}

#[test]
fn a_hidden_app_stays_installed_but_leaves_the_launcher() {
    let mut files = file_system();
    put(&mut files, "timer", &image("timer", "Timer", AppKind::App));
    put(&mut files, "dice", &image("dice", "Dice", AppKind::App));
    let mut packages = packages(&mut files);

    packages.set_enabled("dice", false).unwrap();
    let enabled: Vec<String> = packages
        .enabled()
        .unwrap()
        .into_iter()
        .map(|app| app.id)
        .collect();
    assert_eq!(enabled, vec!["timer"]);
    assert_eq!(packages.list().unwrap().len(), 2);

    packages.set_enabled("dice", true).unwrap();
    assert_eq!(packages.enabled().unwrap().len(), 2);
    assert_eq!(
        packages.set_enabled("nothing", true),
        Err(PackageError::NotInstalled)
    );
}

#[test]
fn installing_again_shows_a_hidden_app() {
    let mut files = file_system();
    put(&mut files, "timer", &image("timer", "Timer", AppKind::App));
    let mut packages = packages(&mut files);
    packages.set_enabled("timer", false).unwrap();
    assert!(packages.install("timer", None).unwrap().enabled);
}

#[test]
fn removing_deletes_the_folder_and_the_saved_state() {
    let mut files = file_system();
    put(&mut files, "timer", &image("timer", "Timer", AppKind::App));
    write_raw(&mut files, &app_file("timer", STATE_FILE), &[1, 2, 3]);
    let mut packages = packages(&mut files);

    packages.remove("timer").unwrap();

    assert!(!packages.exists(&app_file("timer", STATE_FILE)));
    assert!(!packages.exists(&app_file("timer", IMAGE_FILE)));
    assert_eq!(packages.remove("timer"), Err(PackageError::NotInstalled));
}

#[test]
fn an_image_reads_back_for_the_app_host() {
    let mut files = file_system();
    let bytes = image("timer", "Timer", AppKind::App);
    put(&mut files, "timer", &bytes);
    let mut packages = packages(&mut files);
    assert_eq!(packages.read_image("timer").unwrap(), bytes);
    assert_eq!(packages.read_image("dice"), Err(PackageError::NotInstalled));
}

#[test]
fn every_error_explains_itself() {
    for error in [
        PackageError::BadId,
        PackageError::NotInstalled,
        PackageError::BadImage(ImageError::TooShort),
        PackageError::IsLibrary,
        PackageError::IdMismatch,
        PackageError::WouldNotOpen,
        PackageError::Refused(Refusal::LauncherStaysOn),
        PackageError::Storage(FileError::Full),
        PackageError::Storage(FileError::Damaged),
    ] {
        assert!(!error.describe().is_empty());
    }
    assert_eq!(
        PackageError::from(FileError::Damaged),
        PackageError::Storage(FileError::Damaged)
    );
}

// ============================================================================
// Reading an image's parts without reading it whole
// ============================================================================

#[test]
fn a_listing_takes_its_numbers_from_the_stored_facts_and_its_texts_from_the_strings() {
    let mut files = file_system();
    let mut writer = ImageWriter::new("dice", "Dice", AppKind::App, 1);
    writer.set_group("games");
    let bytes = writer.build();
    let facts = neotime_runtime::AppImage::parse(&bytes).unwrap().facts;
    put(&mut files, "dice", &bytes);

    let listed = packages(&mut files).list().unwrap();

    assert_eq!(listed[0].name, "Dice");
    assert_eq!(listed[0].group, "games");
    assert_eq!(listed[0].need_bytes, facts.need_bytes);
    assert_eq!(listed[0].launcher_share_bytes, facts.launcher_share_bytes);
    assert_eq!(listed[0].image_bytes, bytes.len() as u32);
}

#[test]
fn the_icon_is_read_on_its_own() {
    let mut files = file_system();
    let mut writer = ImageWriter::new("dice", "Dice", AppKind::App, 1);
    let pixels: Vec<u16> = (0..64).collect();
    writer.image_resource(neotime_runtime::resources::ImageFormat::Mask, 8, 8, &pixels);
    writer.set_has_icon();
    let bytes = writer.build();
    let expected = neotime_runtime::AppImage::parse(&bytes)
        .unwrap()
        .resource_bytes(0)
        .unwrap()
        .to_vec();
    put(&mut files, "dice", &bytes);
    put(&mut files, "timer", &image("timer", "Timer", AppKind::App));
    let mut packages = Packages::new(&mut files);

    assert_eq!(block_on(packages.read_icon("dice")), Some(expected));
    assert_eq!(block_on(packages.read_icon("timer")), None);
    assert_eq!(block_on(packages.read_icon("nothing")), None);
}

#[test]
fn an_image_whose_stored_facts_differ_from_its_contents_is_refused() {
    let mut files = file_system();
    let mut bytes = image("timer", "Timer", AppKind::App);
    // The facts section is the image's last 12 bytes; its first word is the heap the app needs.
    let facts_start = bytes.len() - 12;
    bytes[facts_start] ^= 1;
    let crc = neotime_runtime::image::crc32(&bytes[16..]);
    bytes[12..16].copy_from_slice(&crc.to_le_bytes());
    put(&mut files, "timer", &bytes);

    assert_eq!(
        packages(&mut files).install("timer", None),
        Err(PackageError::BadImage(ImageError::BadFacts))
    );
}
