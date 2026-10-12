//! The packages characteristic: one JSON command per line from the phone, one JSON line back.
//!
//! The file itself arrives first, through the file transfer service; these commands then check it
//! and act on it. See docs/companion-protocol.md.

use alloc::string::String;
use core::fmt::Write;

use littlefs_rust::{Filesystem, Storage};
use neotime_runtime::AppKind;
use serde::Deserialize;

use crate::{library::LibraryError, Packages};

/// Where the phone puts a new shared library before `install_library`.
pub const LIBRARY_UPLOAD_PATH: &str = "/resources/shared.ntlib";

#[derive(Deserialize)]
struct Command<'line> {
    c: &'line str,
    id: Option<&'line str>,
    on: Option<bool>,
}

/// Copies a library file into internal flash and checks it there; the firmware gives it the flash controller.
pub trait LibraryInstaller {
    fn install<S: Storage>(&mut self, files: &Filesystem<S>, path: &str) -> Result<(), LibraryError>;
}

/// What a command did, beyond its reply, that the shell should hear about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    None,
    /// An app was installed, removed, shown or hidden.
    Apps,
    /// A new shared library is in internal flash.
    Library,
}

/// Runs one command line and returns the reply line, newline included, and what changed.
pub fn handle_command<S: Storage>(
    line: &[u8],
    files: &Filesystem<S>,
    library: &mut impl LibraryInstaller,
) -> (String, Change) {
    let Ok((command, _)) = serde_json_core::from_slice::<Command>(line) else {
        return (error_reply("?", "could not read that command"), Change::None);
    };
    let packages = Packages::new(files);
    let name = command.c;
    let outcome = match (name, command.id) {
        ("apps", _) => return (apps_reply(&packages), Change::None),
        ("install", Some(id)) => packages.install(id).map(|_| Change::Apps),
        ("remove", Some(id)) => packages.remove(id).map(|_| Change::Apps),
        ("enable", Some(id)) => packages
            .set_enabled(id, command.on.unwrap_or(true))
            .map(|_| Change::Apps),
        ("install_library", _) => {
            return match library.install(files, LIBRARY_UPLOAD_PATH) {
                Ok(()) => (ok_reply(name), Change::Library),
                Err(error) => (error_reply(name, error.describe()), Change::None),
            };
        }
        ("install" | "remove" | "enable", None) => {
            return (error_reply(name, "give the app's id"), Change::None)
        }
        _ => return (error_reply(name, "unknown command"), Change::None),
    };
    match outcome {
        Ok(change) => (ok_reply(name), change),
        Err(error) => (error_reply(name, error.describe()), Change::None),
    }
}

fn ok_reply(name: &str) -> String {
    let mut reply = String::new();
    let _ = writeln!(reply, "{{\"c\":\"{}\",\"ok\":1}}", escaped(name));
    reply
}

fn error_reply(name: &str, message: &str) -> String {
    let mut reply = String::new();
    let _ = writeln!(
        reply,
        "{{\"c\":\"{}\",\"err\":\"{}\"}}",
        escaped(name),
        escaped(message)
    );
    reply
}

/// `{"c":"apps","a":[[id,name,kind,enabled,bytes],...]}`, kind being "app" or "face".
fn apps_reply<S: Storage>(packages: &Packages<S>) -> String {
    let apps = match packages.list() {
        Ok(apps) => apps,
        Err(error) => return error_reply("apps", error.describe()),
    };
    let mut reply = String::from("{\"c\":\"apps\",\"a\":[");
    for (index, app) in apps.iter().enumerate() {
        let kind = if app.kind == AppKind::Face { "face" } else { "app" };
        let _ = write!(
            reply,
            "{}[\"{}\",\"{}\",\"{}\",{},{}]",
            if index == 0 { "" } else { "," },
            escaped(&app.id),
            escaped(&app.name),
            kind,
            app.enabled as u8,
            app.image_bytes
        );
    }
    reply.push_str("]}\n");
    reply
}

/// JSON string contents, ASCII only, as the protocol asks.
fn escaped(text: &str) -> String {
    let mut out = String::new();
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(character),
            _ => {
                let mut units = [0u16; 2];
                for unit in character.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{:04x}", unit);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::{string::ToString, vec::Vec};

    use embedded_storage::nor_flash::{ErrorType, NorFlash, NorFlashErrorKind, ReadNorFlash};
    use littlefs_rust::{Config, RamStorage};
    use neotime_boot::{INTERNAL_FLASH_SIZE, PAGE_SIZE, SHARED_LIBRARY};
    use neotime_runtime::writer::ImageWriter;

    use super::*;
    use crate::{app_file, app_folder, library, IMAGE_FILE};

    fn file_system() -> Filesystem<RamStorage> {
        let mut storage = RamStorage::new(4096, 64);
        let config = Config::new(4096, 64);
        Filesystem::format(&mut storage, &config).unwrap();
        let files = Filesystem::mount(storage, config)
            .map_err(|(error, _)| error)
            .unwrap();
        files.mkdir("/apps").unwrap();
        files.mkdir("/resources").unwrap();
        files
    }

    fn big_file_system() -> Filesystem<RamStorage> {
        let mut storage = RamStorage::new(4096, 128);
        let config = Config::new(4096, 128);
        Filesystem::format(&mut storage, &config).unwrap();
        let files = Filesystem::mount(storage, config)
            .map_err(|(error, _)| error)
            .unwrap();
        files.mkdir("/resources").unwrap();
        files
    }

    fn put_app(files: &Filesystem<RamStorage>, id: &str, name: &str, kind: AppKind) {
        files.mkdir(&app_folder(id)).unwrap();
        files
            .write_file(
                &app_file(id, IMAGE_FILE),
                &ImageWriter::new(id, name, kind, 1).build(),
            )
            .unwrap();
    }

    /// The whole internal flash, counting which pages were erased.
    struct FakeInternalFlash {
        memory: Vec<u8>,
        erased: Vec<(u32, u32)>,
    }

    impl FakeInternalFlash {
        fn new() -> FakeInternalFlash {
            FakeInternalFlash {
                memory: std::vec![0xAA; INTERNAL_FLASH_SIZE as usize],
                erased: Vec::new(),
            }
        }
    }

    impl ErrorType for FakeInternalFlash {
        type Error = NorFlashErrorKind;
    }

    impl ReadNorFlash for FakeInternalFlash {
        const READ_SIZE: usize = 1;
        fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
            bytes.copy_from_slice(&self.memory[offset as usize..offset as usize + bytes.len()]);
            Ok(())
        }
        fn capacity(&self) -> usize {
            self.memory.len()
        }
    }

    impl NorFlash for FakeInternalFlash {
        const WRITE_SIZE: usize = 4;
        const ERASE_SIZE: usize = PAGE_SIZE as usize;
        fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
            self.erased.push((from, to));
            self.memory[from as usize..to as usize].fill(0xFF);
            Ok(())
        }
        fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
            assert_eq!(bytes.len() % 4, 0);
            assert_eq!(offset % 4, 0);
            for (at, byte) in bytes.iter().enumerate() {
                assert_eq!(self.memory[offset as usize + at], 0xFF, "written without erasing");
                self.memory[offset as usize + at] = *byte;
            }
            Ok(())
        }
    }

    struct Installer(FakeInternalFlash);

    impl LibraryInstaller for Installer {
        fn install<S: Storage>(&mut self, files: &Filesystem<S>, path: &str) -> Result<(), LibraryError> {
            library::install_library_from_file(&mut self.0, files, path, |flash, length| {
                let start = (SHARED_LIBRARY.start + library::LIBRARY_HEADER_BYTES) as usize;
                crate::check_library_image(&flash.memory[start..start + length as usize])
            })
        }
    }

    fn run(line: &str, files: &Filesystem<RamStorage>) -> (String, Change) {
        handle_command(line.as_bytes(), files, &mut Installer(FakeInternalFlash::new()))
    }

    #[test]
    fn apps_lists_each_app_with_its_kind_and_state() {
        let files = file_system();
        put_app(&files, "timer", "Timer", AppKind::App);
        put_app(&files, "analog", "Analog \"classic\"", AppKind::Face);
        Packages::new(&files).set_enabled("timer", false).unwrap();

        let (reply, change) = run(r#"{"c":"apps"}"#, &files);

        assert!(
            reply.starts_with(r#"{"c":"apps","a":[["analog","Analog \"classic\"","face",1,"#),
            "{reply}"
        );
        assert!(reply.contains(r#"["timer","Timer","app",0,"#));
        assert!(reply.ends_with("]}\n"));
        assert_eq!(change, Change::None);
    }

    #[test]
    fn install_remove_and_enable_answer_and_report_a_change() {
        let files = file_system();
        put_app(&files, "timer", "Timer", AppKind::App);
        assert_eq!(
            run(r#"{"c":"install","id":"timer"}"#, &files),
            (ok_reply("install"), Change::Apps)
        );
        assert_eq!(
            run(r#"{"c":"enable","id":"timer","on":false}"#, &files).1,
            Change::Apps
        );
        assert!(!Packages::new(&files).list().unwrap()[0].enabled);
        assert_eq!(run(r#"{"c":"remove","id":"timer"}"#, &files).1, Change::Apps);
        assert!(Packages::new(&files).list().unwrap().is_empty());
    }

    #[test]
    fn failures_say_why_in_the_reply() {
        let files = file_system();
        assert_eq!(
            run(r#"{"c":"install","id":"nothing"}"#, &files).0,
            "{\"c\":\"install\",\"err\":\"no such app\"}\n"
        );
        assert!(run(r#"{"c":"install"}"#, &files).0.contains("give the app's id"));
        assert!(run(r#"{"c":"format"}"#, &files).0.contains("unknown command"));
        assert!(run("not json", &files).0.contains("could not read"));
    }

    #[test]
    fn the_library_goes_into_its_region_behind_a_header() {
        let files = file_system();
        let image = ImageWriter::new("shared", "Shared library", AppKind::Library, 1).build();
        files.write_file(LIBRARY_UPLOAD_PATH, &image).unwrap();
        let mut installer = Installer(FakeInternalFlash::new());

        let (reply, change) = handle_command(br#"{"c":"install_library"}"#, &files, &mut installer);

        assert_eq!((reply, change), (ok_reply("install_library"), Change::Library));
        let flash = &installer.0;
        let region = &flash.memory[SHARED_LIBRARY.start as usize..];
        assert_eq!(library::installed_library(region), Some(&image[..]));
        // Only pages inside the region were erased, and nothing before it changed.
        for (from, to) in &flash.erased {
            assert!(*from >= SHARED_LIBRARY.start && *to <= SHARED_LIBRARY.end());
        }
        assert!(flash.memory[..SHARED_LIBRARY.start as usize]
            .iter()
            .all(|byte| *byte == 0xAA));
    }

    #[test]
    fn the_library_must_be_uploaded_and_be_a_library() {
        let files = file_system();
        assert!(run(r#"{"c":"install_library"}"#, &files)
            .0
            .contains("no such app"));
        files
            .write_file(
                LIBRARY_UPLOAD_PATH,
                &ImageWriter::new("timer", "Timer", AppKind::App, 1).build(),
            )
            .unwrap();
        assert!(run(r#"{"c":"install_library"}"#, &files)
            .0
            .contains("not the shared library"));
    }

    #[test]
    fn a_library_too_large_for_the_region_is_refused_before_any_erase() {
        let files = big_file_system();
        files
            .write_file(
                LIBRARY_UPLOAD_PATH,
                &std::vec![0; library::MOST_LIBRARY_BYTES as usize + 1],
            )
            .unwrap();
        let mut installer = Installer(FakeInternalFlash::new());
        assert_eq!(
            installer.install(&files, LIBRARY_UPLOAD_PATH),
            Err(LibraryError::TooLarge)
        );
        assert!(installer.0.erased.is_empty());
    }

    #[test]
    fn a_library_failing_its_checks_in_place_gets_no_header() {
        let files = file_system();
        let mut image = ImageWriter::new("shared", "Shared library", AppKind::Library, 1).build();
        let last = image.len() - 1;
        image[last] ^= 0xFF;
        files.write_file(LIBRARY_UPLOAD_PATH, &image).unwrap();
        let mut installer = Installer(FakeInternalFlash::new());

        let result = installer.install(&files, LIBRARY_UPLOAD_PATH);

        assert!(matches!(result, Err(LibraryError::Image(_))));
        let region = &installer.0.memory[SHARED_LIBRARY.start as usize..];
        assert_eq!(library::installed_library(region), None);
    }

    #[test]
    fn a_library_larger_than_one_copy_piece_arrives_whole() {
        let files = big_file_system();
        let mut writer = ImageWriter::new("shared", "Shared library", AppKind::Library, 1);
        let pixels: Vec<u16> = (0..3000).map(|index| (index % 251) as u16).collect();
        writer.image_resource(
            neotime_runtime::resources::ImageFormat::Mask,
            50,
            60,
            &pixels,
        );
        let image = writer.build();
        files.write_file(LIBRARY_UPLOAD_PATH, &image).unwrap();
        let mut installer = Installer(FakeInternalFlash::new());

        installer.install(&files, LIBRARY_UPLOAD_PATH).unwrap();

        let region = &installer.0.memory[SHARED_LIBRARY.start as usize..];
        assert_eq!(library::installed_library(region), Some(&image[..]));
    }

    #[test]
    fn an_empty_or_unmarked_region_holds_no_library() {
        assert_eq!(library::installed_library(&[0xFF; 64]), None);
        assert_eq!(library::installed_library(&[]), None);
        let mut short = std::vec![0u8; 16];
        short[0..4].copy_from_slice(&library::LIBRARY_MAGIC);
        short[4..8].copy_from_slice(&100u32.to_le_bytes());
        assert_eq!(library::installed_library(&short), None);
        for error in [
            LibraryError::TooLarge,
            LibraryError::Flash,
            LibraryError::Image(crate::PackageError::BadId),
        ] {
            assert!(!error.describe().to_string().is_empty());
        }
    }

    #[test]
    fn names_are_sent_as_ascii() {
        assert_eq!(escaped("caf\u{e9} \u{1f600}"), "caf\\u00e9 \\ud83d\\ude00");
        assert_eq!(escaped("a\\b"), "a\\\\b");
    }
}
