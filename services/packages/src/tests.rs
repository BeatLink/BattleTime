extern crate std;

use std::{vec, vec::Vec};

use littlefs_rust::{Config, RamStorage};
use neotime_runtime::writer::ImageWriter;

use super::*;

// ============================================================================
// A file system in RAM, and images to install
// ============================================================================

const BLOCK_SIZE: u32 = 4096;
const BLOCK_COUNT: u32 = 64;

fn file_system() -> Filesystem<RamStorage> {
    let mut storage = RamStorage::new(BLOCK_SIZE, BLOCK_COUNT);
    let config = Config::new(BLOCK_SIZE, BLOCK_COUNT);
    Filesystem::format(&mut storage, &config).unwrap();
    let files = Filesystem::mount(storage, config)
        .map_err(|(error, _)| error)
        .unwrap();
    files.mkdir(APPS_DIRECTORY).unwrap();
    files
}

fn image(id: &str, name: &str, kind: AppKind) -> Vec<u8> {
    ImageWriter::new(id, name, kind, 1).build()
}

fn put(files: &Filesystem<RamStorage>, id: &str, bytes: &[u8]) {
    let _ = files.mkdir(&app_folder(id));
    files.write_file(&app_file(id, IMAGE_FILE), bytes).unwrap();
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
fn an_app_installs_and_is_listed_with_its_name() {
    let files = file_system();
    put(&files, "timer", &image("timer", "Timer", AppKind::App));
    let packages = Packages::new(&files);

    let installed = packages.install("timer").unwrap();

    assert_eq!(installed.name, "Timer");
    assert_eq!(installed.kind, AppKind::App);
    assert!(installed.enabled);
    assert_eq!(packages.list().unwrap(), vec![installed]);
}

#[test]
fn a_face_installs_too() {
    let files = file_system();
    put(&files, "analog", &image("analog", "Analog", AppKind::Face));
    assert_eq!(
        Packages::new(&files).install("analog").unwrap().kind,
        AppKind::Face
    );
}

#[test]
fn a_broken_image_is_refused_and_left_for_another_try() {
    let files = file_system();
    let mut bytes = image("timer", "Timer", AppKind::App);
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    put(&files, "timer", &bytes);
    let packages = Packages::new(&files);

    assert!(matches!(
        packages.install("timer"),
        Err(PackageError::BadImage(_))
    ));
    assert!(files.exists(&app_file("timer", IMAGE_FILE)));
    assert_eq!(packages.list().unwrap(), vec![]);
}

#[test]
fn the_shared_library_is_not_an_app() {
    let files = file_system();
    put(
        &files,
        "shared",
        &image("shared", "Shared library", AppKind::Library),
    );
    assert_eq!(
        Packages::new(&files).install("shared"),
        Err(PackageError::IsLibrary)
    );
}

#[test]
fn an_image_must_be_in_its_own_folder() {
    let files = file_system();
    put(&files, "timer", &image("dice", "Dice", AppKind::App));
    assert_eq!(
        Packages::new(&files).install("timer"),
        Err(PackageError::IdMismatch)
    );
}

#[test]
fn installing_without_an_image_says_so() {
    let files = file_system();
    assert_eq!(
        Packages::new(&files).install("timer"),
        Err(PackageError::NotInstalled)
    );
    assert_eq!(Packages::new(&files).install("../etc"), Err(PackageError::BadId));
}

#[test]
fn the_library_check_wants_a_library() {
    assert_eq!(
        check_library_image(&image("shared", "Shared", AppKind::Library)),
        Ok(())
    );
    assert_eq!(
        check_library_image(&image("timer", "Timer", AppKind::App)),
        Err(PackageError::NotLibrary)
    );
    assert!(matches!(
        check_library_image(&[1, 2, 3]),
        Err(PackageError::BadImage(_))
    ));
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
    let files = file_system();
    let bytes = image_with_bad_code("timer");
    assert!(AppImage::parse(&bytes).is_ok());
    put(&files, "timer", &bytes);
    let packages = Packages::new(&files);

    assert!(matches!(
        packages.install("timer"),
        Err(PackageError::BadImage(_))
    ));
    let ids: Vec<String> = packages.list().unwrap().into_iter().map(|app| app.id).collect();
    assert_eq!(ids, vec!["timer"]);
}

#[test]
fn the_list_is_in_id_order_and_skips_what_is_not_an_app() {
    let files = file_system();
    put(&files, "timer", &image("timer", "Timer", AppKind::App));
    put(&files, "dice", &image("dice", "Dice", AppKind::App));
    put(&files, "junk", &[0; 8]);
    files.write_file("/apps/loose-file", b"x").unwrap();

    let ids: Vec<String> = Packages::new(&files)
        .list()
        .unwrap()
        .into_iter()
        .map(|app| app.id)
        .collect();

    assert_eq!(ids, vec!["dice", "timer"]);
}

#[test]
fn a_hidden_app_stays_installed_but_leaves_the_launcher() {
    let files = file_system();
    put(&files, "timer", &image("timer", "Timer", AppKind::App));
    put(&files, "dice", &image("dice", "Dice", AppKind::App));
    let packages = Packages::new(&files);

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
    let files = file_system();
    put(&files, "timer", &image("timer", "Timer", AppKind::App));
    let packages = Packages::new(&files);
    packages.set_enabled("timer", false).unwrap();
    assert!(packages.install("timer").unwrap().enabled);
}

#[test]
fn removing_deletes_the_folder_and_the_saved_state() {
    let files = file_system();
    put(&files, "timer", &image("timer", "Timer", AppKind::App));
    files
        .write_file(&app_file("timer", STATE_FILE), &[1, 2, 3])
        .unwrap();
    let packages = Packages::new(&files);

    packages.remove("timer").unwrap();

    assert!(!files.exists(&app_folder("timer")));
    assert_eq!(packages.remove("timer"), Err(PackageError::NotInstalled));
}

#[test]
fn an_image_reads_back_for_the_app_host() {
    let files = file_system();
    let bytes = image("timer", "Timer", AppKind::App);
    put(&files, "timer", &bytes);
    let packages = Packages::new(&files);
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
        PackageError::NotLibrary,
        PackageError::IdMismatch,
        PackageError::Storage(Error::NoSpace),
        PackageError::Storage(Error::Io),
    ] {
        assert!(!error.describe().is_empty());
    }
    assert_eq!(PackageError::from(Error::Io), PackageError::Storage(Error::Io));
}
