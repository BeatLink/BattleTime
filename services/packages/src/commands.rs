//! The packages characteristic: one JSON command per line from the phone, one JSON line back.
//!
//! The file itself arrives first, through the file transfer service; these commands then check it
//! and act on it. See docs/companion-protocol.md.

use alloc::string::String;
use core::fmt::Write;

use neotime_flash_store::FileAccess;
use neotime_runtime::{AppKind, APP_HEAP_BUDGET_BYTES};
use serde::Deserialize;

use crate::{PackageError, Packages};

#[derive(Deserialize)]
struct Command<'line> {
    c: &'line str,
    id: Option<&'line str>,
    on: Option<bool>,
}

/// What a command did, beyond its reply, that the shell should hear about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    None,
    /// An app was installed, removed, shown or hidden, or the launcher's groups changed.
    Apps,
}

/// Runs one command line and returns the reply line, newline included, and what changed.
///
/// `library_version` is the version of the library inside the running firmware, or None when it carries none.
pub async fn handle_command<Files: FileAccess>(
    line: &[u8],
    files: &mut Files,
    library_version: Option<u16>,
) -> (String, Change) {
    let Ok((command, _)) = serde_json_core::from_slice::<Command>(line) else {
        return (error_reply("?", "could not read that command"), Change::None);
    };
    let name = command.c;
    if name == "install_library" {
        return (error_reply(name, PackageError::IsLibrary.describe()), Change::None);
    }
    let mut packages = Packages::new(files);
    let outcome = match (name, command.id) {
        ("apps", _) => return (apps_reply(&mut packages).await, Change::None),
        ("install", Some(id)) => {
            return match packages.install(id, library_version).await {
                Ok(app) => {
                    let apps = packages.list().await.unwrap_or_default();
                    let need = crate::checks::room_needed(&app, &apps);
                    (ok_reply_with_need(name, need), Change::Apps)
                }
                Err(error) => (error_reply_for(name, error), Change::None),
            };
        }
        ("launcher", _) => return (ok_reply(name), Change::Apps),
        ("remove", Some(id)) => packages.remove(id).await.map(|_| Change::Apps),
        ("enable", Some(id)) => {
            return match packages.change_enabled(id, command.on.unwrap_or(true)).await {
                Ok(need) => (ok_reply_with_need(name, need), Change::Apps),
                Err(error) => (error_reply_for(name, error), Change::None),
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

/// Answers a command this build cannot run, such as in a rehearsal with no file system.
pub fn refuse_command(line: &[u8], message: &str) -> (String, Change) {
    let name = serde_json_core::from_slice::<Command>(line).map_or("?", |(command, _)| command.c);
    (error_reply(name, message), Change::None)
}

fn ok_reply(name: &str) -> String {
    let mut reply = String::new();
    let _ = writeln!(reply, "{{\"c\":\"{}\",\"ok\":1}}", escaped(name));
    reply
}

/// An answer that also gives the app's heap need and the most an app may have.
fn ok_reply_with_need(name: &str, need_bytes: u32) -> String {
    let mut reply = String::new();
    let _ = writeln!(
        reply,
        "{{\"c\":\"{}\",\"ok\":1,\"need\":{need_bytes},\"room\":{APP_HEAP_BUDGET_BYTES}}}",
        escaped(name)
    );
    reply
}

/// An error, with the refusal's number and the heap need when the checks refused it.
fn error_reply_for(name: &str, error: PackageError) -> String {
    let PackageError::Refused(refusal) = error else {
        return error_reply(name, error.describe());
    };
    let mut reply = String::new();
    let _ = write!(
        reply,
        "{{\"c\":\"{}\",\"err\":\"{}\",\"why\":{}",
        escaped(name),
        escaped(refusal.describe()),
        refusal.code()
    );
    if let Some(need_bytes) = refusal.need_bytes() {
        let _ = write!(reply, ",\"need\":{need_bytes},\"room\":{APP_HEAP_BUDGET_BYTES}");
    }
    reply.push_str("}\n");
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

/// `{"c":"apps","room":budget,"a":[[id,name,kind,enabled,bytes,need,group],...]}`, kind being "app" or "face",
/// need the heap the app needs while open among the others and group the launcher group it declares.
async fn apps_reply<Files: FileAccess>(packages: &mut Packages<'_, Files>) -> String {
    let apps = match packages.list().await {
        Ok(apps) => apps,
        Err(error) => return error_reply("apps", error.describe()),
    };
    let mut reply = String::new();
    let _ = write!(reply, "{{\"c\":\"apps\",\"room\":{APP_HEAP_BUDGET_BYTES},\"a\":[");
    for (index, app) in apps.iter().enumerate() {
        let kind = if app.kind == AppKind::Face { "face" } else { "app" };
        let _ = write!(
            reply,
            "{}[\"{}\",\"{}\",\"{}\",{},{},{},\"{}\"]",
            if index == 0 { "" } else { "," },
            escaped(&app.id),
            escaped(&app.name),
            kind,
            app.enabled as u8,
            app.image_bytes,
            crate::checks::room_needed(app, &apps),
            escaped(&app.group)
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

    use embassy_futures::block_on;
    use neotime_flash_store::{ram_flash::RamFlash, FlashStore, Layout};
    use neotime_runtime::writer::ImageWriter;

    use super::*;
    use crate::{app_file, IMAGE_FILE};

    type TestStore = FlashStore<RamFlash, 32, 64, 8>;

    fn store_of(file_sectors: usize) -> TestStore {
        let layout = Layout {
            start_address: 0,
            file_sectors,
            record_sectors: 8,
            log_sectors: 4,
        };
        block_on(TestStore::mount(RamFlash::blank(layout.total_sectors()), layout)).unwrap()
    }

    fn file_system() -> TestStore {
        store_of(64)
    }

    fn put(files: &mut TestStore, path: &str, bytes: &[u8]) {
        block_on(FileAccess::write_file(files, path, bytes)).unwrap();
    }

    fn put_app(files: &mut TestStore, id: &str, name: &str, kind: AppKind) {
        put(
            files,
            &app_file(id, IMAGE_FILE),
            &ImageWriter::new(id, name, kind, 1).build(),
        );
    }

    fn run(line: &str, files: &mut TestStore) -> (String, Change) {
        block_on(handle_command(line.as_bytes(), files, None))
    }

    #[test]
    fn apps_lists_each_app_with_its_kind_and_state() {
        let mut files = file_system();
        put_app(&mut files, "timer", "Timer", AppKind::App);
        put_app(&mut files, "analog", "Analog \"classic\"", AppKind::Face);
        block_on(Packages::new(&mut files).set_enabled("timer", false)).unwrap();

        let (reply, change) = run(r#"{"c":"apps"}"#, &mut files);

        assert!(
            reply.starts_with(&std::format!(
                r#"{{"c":"apps","room":{APP_HEAP_BUDGET_BYTES},"a":[["analog","Analog \"classic\"","face",1,"#
            )),
            "{reply}"
        );
        assert!(reply.contains(r#"["timer","Timer","app",0,"#));
        assert!(reply.ends_with("]}\n"));
        assert_eq!(change, Change::None);
    }

    #[test]
    fn apps_gives_each_apps_declared_group() {
        let mut files = file_system();
        let mut writer = ImageWriter::new("snake", "Snake", AppKind::App, 1);
        writer.set_group("games");
        put(&mut files, &app_file("snake", IMAGE_FILE), &writer.build());
        put_app(&mut files, "timer", "Timer", AppKind::App);

        let (reply, _) = run(r#"{"c":"apps"}"#, &mut files);

        assert!(
            reply.contains(r#"["snake","Snake","app",1,"#) && reply.contains(r#","games"],"#),
            "{reply}"
        );
        assert!(reply.ends_with(",\"\"]]}\n"), "{reply}");
    }

    #[test]
    fn launcher_answers_and_reports_that_the_apps_changed() {
        let mut files = file_system();

        assert_eq!(
            run(r#"{"c":"launcher"}"#, &mut files),
            (String::from("{\"c\":\"launcher\",\"ok\":1}\n"), Change::Apps)
        );
    }

    #[test]
    fn install_remove_and_enable_answer_and_report_a_change() {
        let mut files = file_system();
        put_app(&mut files, "timer", "Timer", AppKind::App);
        let need = block_on(crate::Packages::new(&mut files).list()).unwrap()[0].need_bytes;
        let need = need + neotime_runtime::memory_need::listed_app_bytes("timer", "Timer");
        assert_eq!(
            run(r#"{"c":"install","id":"timer"}"#, &mut files),
            (
                std::format!(
                    "{{\"c\":\"install\",\"ok\":1,\"need\":{need},\"room\":{APP_HEAP_BUDGET_BYTES}}}\n"
                ),
                Change::Apps
            )
        );
        assert_eq!(
            run(r#"{"c":"enable","id":"timer","on":false}"#, &mut files).1,
            Change::Apps
        );
        assert!(!block_on(Packages::new(&mut files).list()).unwrap()[0].enabled);
        assert_eq!(run(r#"{"c":"remove","id":"timer"}"#, &mut files).1, Change::Apps);
        assert!(block_on(Packages::new(&mut files).list()).unwrap().is_empty());
    }

    #[test]
    fn refusals_give_their_number_and_the_need() {
        let mut files = file_system();
        put_app(&mut files, "launcher", "Apps", AppKind::App);
        let mut huge = ImageWriter::new("huge", "Huge", AppKind::App, 1);
        huge.string(&"x".repeat(40_000));
        put(&mut files, &app_file("huge", IMAGE_FILE), &huge.build());

        let (reply, change) = run(r#"{"c":"install","id":"huge"}"#, &mut files);
        assert!(
            reply.starts_with(
                r#"{"c":"install","err":"it needs more memory than an app can have","why":1,"need":"#
            ),
            "{reply}"
        );
        assert!(
            reply.ends_with(&std::format!(",\"room\":{APP_HEAP_BUDGET_BYTES}}}\n")),
            "{reply}"
        );
        assert_eq!(change, Change::None);

        assert_eq!(
            run(r#"{"c":"enable","id":"launcher","on":false}"#, &mut files).0,
            "{\"c\":\"enable\",\"err\":\"the launcher stays on\",\"why\":3}\n"
        );
        assert_eq!(
            run(r#"{"c":"enable","id":"nothing"}"#, &mut files).0,
            "{\"c\":\"enable\",\"err\":\"no such app\"}\n"
        );
    }

    #[test]
    fn failures_say_why_in_the_reply() {
        let mut files = file_system();
        assert_eq!(
            run(r#"{"c":"install","id":"nothing"}"#, &mut files).0,
            "{\"c\":\"install\",\"err\":\"no such app\"}\n"
        );
        assert!(run(r#"{"c":"install"}"#, &mut files)
            .0
            .contains("give the app's id"));
        assert!(run(r#"{"c":"format"}"#, &mut files).0.contains("unknown command"));
        assert!(run("not json", &mut files).0.contains("could not read"));
    }

    #[test]
    fn install_library_says_the_library_ships_inside_the_firmware() {
        let mut files = file_system();

        let (reply, change) = run(r#"{"c":"install_library"}"#, &mut files);

        assert!(reply.contains("ships inside the firmware"));
        assert_eq!(change, Change::None);
    }

    #[test]
    fn an_app_for_a_newer_library_than_the_firmwares_is_refused() {
        let mut files = file_system();
        put(
            &mut files,
            &app_file("timer", IMAGE_FILE),
            &ImageWriter::new("timer", "Timer", AppKind::App, 3).build(),
        );

        let (reply, _) = block_on(handle_command(br#"{"c":"install","id":"timer"}"#, &mut files, Some(2)));

        assert!(reply.contains("newer shared library"));
    }

    #[test]
    fn a_refusal_names_the_command() {
        assert_eq!(
            refuse_command(br#"{"c":"apps"}"#, "no files").0,
            "{\"c\":\"apps\",\"err\":\"no files\"}\n"
        );
        assert_eq!(refuse_command(b"junk", "no files").1, Change::None);
    }

    #[test]
    fn names_are_sent_as_ascii() {
        assert_eq!(escaped("caf\u{e9} \u{1f600}"), "caf\\u00e9 \\ud83d\\ude00");
        assert_eq!(escaped("a\\b"), "a\\\\b");
    }
}
