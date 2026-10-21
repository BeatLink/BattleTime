//! The packages characteristic: one JSON command per line from the phone, one JSON line back.
//!
//! The file itself arrives first, through the file transfer service; these commands then check it
//! and act on it. See docs/companion-protocol.md.

use alloc::string::String;
use core::fmt::Write;

use battletime_flash_store::FileAccess;
use battletime_runtime::{AppKind, APP_HEAP_BUDGET_BYTES};
use serde::Deserialize;

use crate::{check_id, AppId, PackageError, Packages};

#[derive(Deserialize)]
struct Command<'line> {
    c: &'line str,
    id: Option<&'line str>,
    on: Option<bool>,
    name: Option<&'line str>,
    call: Option<u32>,
}

/// The longest module name: the crash record keeps it after a four-byte tag.
pub const MOST_MODULE_NAME_BYTES: usize = 12;

/// A call of a native module in `/system/modules`, which the firmware runs and answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleCall {
    pub name: heapless::String<MOST_MODULE_NAME_BYTES>,
    pub call: u32,
}

/// What a command did, beyond its reply, that the shell should hear about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    None,
    /// An app was installed, removed, shown or hidden, or the launcher's groups changed.
    Apps,
    /// The temporary app with this id passed its checks, so the shell opens it.
    RunTemporary(AppId),
    /// The shell closes the open temporary app.
    StopTemporary,
    /// The firmware runs this module call and sends its answer with `module_reply`.
    RunModule(ModuleCall),
    /// The app passed the install's other checks; the shell verifies its code, then answers with `install_reply`.
    VerifyInstall {
        id: AppId,
        need_bytes: u32,
    },
}

/// Where a command's reply line goes, a piece at a time; the line ends with its newline.
#[allow(async_fn_in_trait)]
pub trait ReplySink {
    async fn send(&mut self, piece: String);
}

/// Runs one command line, sends its reply line, newline included, to `reply`, and says what changed; the shell sends `run`'s reply.
///
/// `library_version` is the version of the library inside the running firmware, or None when it carries none.
pub async fn handle_command<Files: FileAccess>(
    line: &[u8],
    files: &mut Files,
    library_version: Option<u16>,
    reply: &mut impl ReplySink,
) -> Change {
    let (answer, change) = answer_command(line, files, library_version, reply).await;
    if let Some(answer) = answer.filter(|answer| !answer.is_empty()) {
        reply.send(answer).await;
    }
    change
}

/// The reply line, or none when it was already sent in pieces, and what changed.
async fn answer_command<Files: FileAccess>(
    line: &[u8],
    files: &mut Files,
    library_version: Option<u16>,
    reply: &mut impl ReplySink,
) -> (Option<String>, Change) {
    let (answer, change) = match serde_json_core::from_slice::<Command>(line) {
        Err(_) => (error_reply("?", "could not read that command"), Change::None),
        Ok((command, _)) if command.c == "apps" => {
            send_apps_reply(&mut Packages::new(files), reply).await;
            return (None, Change::None);
        }
        Ok((command, _)) => run_command(command, files, library_version).await,
    };
    (Some(answer), change)
}

async fn run_command<Files: FileAccess>(
    command: Command<'_>,
    files: &mut Files,
    library_version: Option<u16>,
) -> (String, Change) {
    let name = command.c;
    let mut packages = Packages::new(files);
    let outcome = match (name, command.id) {
        ("install", Some(id)) => {
            return match packages.install(id, library_version).await {
                Ok(app) => {
                    let apps = packages.list().await.unwrap_or_default();
                    let need = crate::checks::room_needed(&app, &apps);
                    match cfg!(feature = "verify-on-install") {
                        true => (ok_reply_with_need(name, need), Change::Apps),
                        false => (
                            String::new(),
                            Change::VerifyInstall {
                                id: app_id(id),
                                need_bytes: need,
                            },
                        ),
                    }
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
        // The shell answers once it has opened the app, or gone back when it would not open.
        ("run", Some(id)) => {
            return match check_id(id) {
                Ok(()) => (String::new(), Change::RunTemporary(app_id(id))),
                Err(error) => (error_reply_for(name, error), Change::None),
            };
        }
        ("stop", _) => return (ok_reply(name), Change::StopTemporary),
        ("module", _) => {
            return match module_call(command.name, command.call) {
                Some(call) => (String::new(), Change::RunModule(call)),
                None => (
                    error_reply(name, "a module name is 1 to 12 letters, digits, - and _"),
                    Change::None,
                ),
            };
        }
        ("install" | "remove" | "enable" | "run", None) => {
            return (error_reply(name, "give the app's id"), Change::None)
        }
        _ => return (error_reply(name, "unknown command"), Change::None),
    };
    match outcome {
        Ok(change) => (ok_reply(name), change),
        Err(error) => (error_reply(name, error.describe()), Change::None),
    }
}

/// The shell's answer to `run`: the heap the open app needs, or why it would not open.
pub fn run_reply(outcome: Result<u32, PackageError>) -> String {
    match outcome {
        Ok(need_bytes) => ok_reply_with_need("run", need_bytes),
        Err(error) => error_reply_for("run", error),
    }
}

fn module_call(name: Option<&str>, call: Option<u32>) -> Option<ModuleCall> {
    let name = name.filter(|name| check_id(name).is_ok())?;
    Some(ModuleCall {
        name: heapless::String::try_from(name).ok()?,
        call: call.unwrap_or(0),
    })
}

/// The shell's answer to `install` once it verified the app's code: the heap it needs, or why it was turned off.
pub fn install_reply(outcome: Result<u32, PackageError>) -> String {
    match outcome {
        Ok(need_bytes) => ok_reply_with_need("install", need_bytes),
        Err(error) => error_reply_for("install", error),
    }
}

/// The firmware's answer to `module`: what the call answered, or why it did not run.
pub fn module_reply(outcome: Result<i32, &str>) -> String {
    match outcome {
        Ok(answer) => {
            let mut reply = String::new();
            let _ = writeln!(reply, "{{\"c\":\"module\",\"ok\":1,\"r\":{answer}}}");
            reply
        }
        Err(message) => error_reply("module", message),
    }
}

/// An id that `check_id` passed, which always fits.
fn app_id(id: &str) -> AppId {
    AppId::try_from(id).unwrap_or_default()
}

/// Answers a command this build cannot run, such as in the recovery firmware or without a file system.
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
/// Sends the list a piece per app, so the reply never needs one block of heap that grows with the apps installed.
async fn send_apps_reply<Files: FileAccess>(packages: &mut Packages<'_, Files>, reply: &mut impl ReplySink) {
    let apps = match packages.list().await {
        Ok(apps) => apps,
        Err(error) => return reply.send(error_reply("apps", error.describe())).await,
    };
    let mut start = String::new();
    let _ = write!(start, "{{\"c\":\"apps\",\"room\":{APP_HEAP_BUDGET_BYTES},\"a\":[");
    reply.send(start).await;
    for (index, app) in apps.iter().enumerate() {
        let kind = if app.kind == AppKind::Face { "face" } else { "app" };
        let mut piece = String::new();
        let _ = write!(
            piece,
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
        reply.send(piece).await;
    }
    reply.send(String::from("]}\n")).await;
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

    use battletime_flash_store::{ram_flash::RamFlash, FlashStore, Layout};
    use battletime_runtime::writer::ImageWriter;
    use embassy_futures::block_on;

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

    /// Keeps every piece sent, to check how a reply was split as well as what it says.
    #[derive(Default)]
    struct Pieces(std::vec::Vec<String>);

    impl ReplySink for Pieces {
        async fn send(&mut self, piece: String) {
            self.0.push(piece);
        }
    }

    fn run_with(line: &str, files: &mut TestStore, library_version: Option<u16>) -> (String, Change, Pieces) {
        let mut pieces = Pieces::default();
        let change = block_on(handle_command(
            line.as_bytes(),
            files,
            library_version,
            &mut pieces,
        ));
        (pieces.0.concat(), change, pieces)
    }

    fn run(line: &str, files: &mut TestStore) -> (String, Change) {
        let (reply, change, _) = run_with(line, files, None);
        (reply, change)
    }

    #[test]
    fn module_asks_the_firmware_to_run_the_call() {
        let mut files = file_system();

        let (reply, change) = run(r#"{"c":"module","name":"test","call":2}"#, &mut files);

        assert_eq!(reply, "");
        let name = heapless::String::try_from("test").unwrap();
        assert_eq!(change, Change::RunModule(ModuleCall { name, call: 2 }));
    }

    #[test]
    fn module_refuses_a_name_that_is_missing_or_too_long() {
        let mut files = file_system();

        for line in [
            r#"{"c":"module"}"#,
            r#"{"c":"module","name":"a-name-of-thirteen"}"#,
        ] {
            let (reply, change) = run(line, &mut files);

            assert!(reply.contains(r#""err":"a module name"#), "{reply}");
            assert_eq!(change, Change::None);
        }
    }

    #[test]
    fn install_reply_gives_the_need_or_why_the_code_failed() {
        assert_eq!(
            install_reply(Ok(4096)),
            std::format!("{{\"c\":\"install\",\"ok\":1,\"need\":4096,\"room\":{APP_HEAP_BUDGET_BYTES}}}\n")
        );
        assert!(install_reply(Err(PackageError::CouldNotVerify)).contains("the verifier module"));
    }

    #[test]
    fn module_reply_gives_the_calls_answer_or_why_it_did_not_run() {
        assert_eq!(module_reply(Ok(-3)), "{\"c\":\"module\",\"ok\":1,\"r\":-3}\n");
        assert_eq!(
            module_reply(Err("the module is missing")),
            "{\"c\":\"module\",\"err\":\"the module is missing\"}\n"
        );
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
    #[cfg(not(feature = "verify-on-install"))]
    fn without_install_verification_the_shell_verifies_and_answers() {
        let mut files = file_system();
        put_app(&mut files, "timer", "Timer", AppKind::App);
        let need = block_on(crate::Packages::new(&mut files).list()).unwrap()[0].need_bytes;
        let need_bytes = need + battletime_runtime::memory_need::indexed_app_bytes("timer");

        let (reply, change) = run(r#"{"c":"install","id":"timer"}"#, &mut files);

        assert_eq!(reply, "");
        assert_eq!(
            change,
            Change::VerifyInstall {
                id: app_id("timer"),
                need_bytes
            }
        );
    }

    #[test]
    #[cfg(feature = "verify-on-install")]
    fn install_remove_and_enable_answer_and_report_a_change() {
        let mut files = file_system();
        put_app(&mut files, "timer", "Timer", AppKind::App);
        let need = block_on(crate::Packages::new(&mut files).list()).unwrap()[0].need_bytes;
        let need = need + battletime_runtime::memory_need::indexed_app_bytes("timer");
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
    fn an_app_for_a_newer_library_than_the_firmwares_is_refused() {
        let mut files = file_system();
        put(
            &mut files,
            &app_file("timer", IMAGE_FILE),
            &ImageWriter::new("timer", "Timer", AppKind::App, 3).build(),
        );

        let (reply, _, _) = run_with(r#"{"c":"install","id":"timer"}"#, &mut files, Some(2));

        assert!(reply.contains("newer shared library"));
    }

    #[test]
    fn the_app_list_goes_out_a_piece_per_app() {
        let mut files = file_system();
        put_app(&mut files, "timer", "Timer", AppKind::App);
        put_app(&mut files, "dice", "Dice", AppKind::App);
        put_app(&mut files, "analog", "Analog", AppKind::Face);

        let (reply, _, pieces) = run_with(r#"{"c":"apps"}"#, &mut files, None);

        assert_eq!(pieces.0.len(), 5, "{:?}", pieces.0);
        assert!(pieces.0[1].starts_with(r#"["analog","#), "{:?}", pieces.0);
        assert!(pieces.0[2].starts_with(r#",["dice","#), "{:?}", pieces.0);
        assert_eq!(pieces.0[4], "]}\n");
        assert_eq!(reply.matches('\n').count(), 1);
    }

    #[test]
    fn every_other_reply_is_one_piece() {
        let mut files = file_system();

        let (_, _, pieces) = run_with(r#"{"c":"remove","id":"nothing"}"#, &mut files, None);

        assert_eq!(pieces.0.len(), 1);
        assert!(pieces.0[0].ends_with('\n'));
    }

    #[test]
    fn run_asks_the_shell_to_open_a_temporary_app_and_leaves_the_reply_to_it() {
        let mut files = file_system();

        assert_eq!(
            run(r#"{"c":"run","id":"prompt"}"#, &mut files),
            (String::new(), Change::RunTemporary(app_id("prompt")))
        );
        assert_eq!(
            run(r#"{"c":"run","id":"bad id"}"#, &mut files),
            (
                String::from("{\"c\":\"run\",\"err\":\"an app id is letters, digits, - and _\"}\n"),
                Change::None
            )
        );
        assert!(run(r#"{"c":"run"}"#, &mut files).0.contains("give the app's id"));
    }

    #[test]
    fn the_shells_run_reply_gives_the_need_or_why_it_would_not_open() {
        assert_eq!(
            run_reply(Ok(9000)),
            std::format!("{{\"c\":\"run\",\"ok\":1,\"need\":9000,\"room\":{APP_HEAP_BUDGET_BYTES}}}\n")
        );
        assert_eq!(
            run_reply(Err(PackageError::NotInstalled)),
            "{\"c\":\"run\",\"err\":\"no such app\"}\n"
        );
        assert_eq!(
            run_reply(Err(PackageError::WouldNotOpen)),
            "{\"c\":\"run\",\"err\":\"the image is missing or would not open\"}\n"
        );
        let too_large = PackageError::Refused(crate::Refusal::TooLarge { need_bytes: 40_000 });
        assert!(run_reply(Err(too_large)).contains(r#""why":1,"need":40000"#));
    }

    #[test]
    fn stop_asks_the_shell_to_close_the_temporary_app() {
        let mut files = file_system();

        assert_eq!(
            run(r#"{"c":"stop"}"#, &mut files),
            (String::from("{\"c\":\"stop\",\"ok\":1}\n"), Change::StopTemporary)
        );
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
