// The companion's log: what the link, the daemon and the window did, for finding out why something failed.
//
// Lines go to Android's log (`adb logcat -s BattleTime`) or to the terminal, and to `logs/companion.log`
// in the app's data folder, which moves to `companion.old.log` once it passes a size limit.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use log::{Level, LevelFilter, Log, Metadata, Record};

// ============================================================================
// Settings
// ============================================================================

// The tag the lines carry in Android's log.
pub const ANDROID_TAG: &str = "BattleTime";

// How large the log file grows before it starts again, keeping one older file.
const MOST_FILE_BYTES: u64 = 2 * 1024 * 1024;

// The companion's own crates log everything down to debug; btleplug says what its platform did.
const OWN_CRATE_PREFIX: &str = "battletime";
const BTLEPLUG_PREFIX: &str = "btleplug";

// Gives the most detailed level kept from code in `target`, a module path such as `btleplug::bluez`.
pub fn level_for(target: &str) -> LevelFilter {
    if target.starts_with(OWN_CRATE_PREFIX) {
        LevelFilter::Debug
    } else if target.starts_with(BTLEPLUG_PREFIX) {
        LevelFilter::Info
    } else {
        LevelFilter::Warn
    }
}

// ============================================================================
// Starting
// ============================================================================

struct CompanionLogger {
    file: Mutex<Option<LogFile>>,
}

struct LogFile {
    path: PathBuf,
    file: File,
    written: u64,
}

static LOGGER: OnceLock<CompanionLogger> = OnceLock::new();

// Starts logging to Android's log or the terminal; safe to call any number of times.
pub fn start() {
    let mut first = false;
    let logger = LOGGER.get_or_init(|| {
        first = true;
        CompanionLogger {
            file: Mutex::new(None),
        }
    });
    if first && log::set_logger(logger).is_ok() {
        log::set_max_level(LevelFilter::Debug);
        log_panics();
    }
}

// Logs a panic before the usual report, since on Android nothing else would show it.
fn log_panics() {
    let earlier = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        let thread = std::thread::current();
        log::error!(
            "panic on thread {}: {panic}\n{}",
            thread.name().unwrap_or("unnamed"),
            std::backtrace::Backtrace::force_capture()
        );
        log::logger().flush();
        earlier(panic);
    }));
}

// Starts logging and writes the log to `logs/companion.log` under `data_dir` as well.
pub fn start_with_file(data_dir: &Path) {
    start();
    let Some(logger) = LOGGER.get() else { return };
    let mut slot = logger.file.lock().unwrap();
    if slot.is_some() {
        return;
    }
    match LogFile::open(&data_dir.join("logs")) {
        Ok(file) => {
            *slot = Some(file);
            drop(slot);
            log::info!("logging to {}", log_path(data_dir).display());
        }
        Err(error) => {
            drop(slot);
            log::warn!(
                "the log file under {} could not open: {error}",
                data_dir.display()
            );
        }
    }
}

// Where the log file is for an app whose data lives in `data_dir`.
pub fn log_path(data_dir: &Path) -> PathBuf {
    data_dir.join("logs").join("companion.log")
}

// ============================================================================
// Writing
// ============================================================================

impl Log for CompanionLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= level_for(metadata.target())
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let message = record.args().to_string();
        write_to_console(record.level(), record.target(), &message);
        if let Some(file) = self.file.lock().unwrap().as_mut() {
            file.write_line(&format_line(
                &chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
                record.level(),
                record.target(),
                &message,
            ));
        }
    }

    fn flush(&self) {
        if let Some(file) = self.file.lock().unwrap().as_mut() {
            let _ = file.file.flush();
        }
    }
}

// One line of the log file: when, how serious, which part of the companion, and what happened.
pub fn format_line(time: &str, level: Level, target: &str, message: &str) -> String {
    format!("{time} {level:<5} {target}: {message}\n")
}

impl LogFile {
    fn open(folder: &Path) -> std::io::Result<LogFile> {
        std::fs::create_dir_all(folder)?;
        let path = folder.join("companion.log");
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let written = file.metadata()?.len();
        Ok(LogFile { path, file, written })
    }

    fn write_line(&mut self, line: &str) {
        if self.written + line.len() as u64 > MOST_FILE_BYTES {
            self.start_again();
        }
        if self.file.write_all(line.as_bytes()).is_ok() {
            self.written += line.len() as u64;
        }
    }

    // Keeps the full file as the older one and begins an empty one.
    fn start_again(&mut self) {
        let older = self.path.with_file_name("companion.old.log");
        let _ = std::fs::rename(&self.path, older);
        if let Ok(file) = File::create(&self.path) {
            self.file = file;
            self.written = 0;
        }
    }
}

#[cfg(target_os = "android")]
fn write_to_console(level: Level, target: &str, message: &str) {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int};

    #[link(name = "log")]
    extern "C" {
        fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }

    // Android's priorities: 3 debug, 4 info, 5 warn, 6 error.
    let priority = match level {
        Level::Error => 6,
        Level::Warn => 5,
        Level::Info => 4,
        Level::Debug | Level::Trace => 3,
    };
    let text = format!("{target}: {message}").replace('\0', " ");
    let (Ok(tag), Ok(text)) = (CString::new(ANDROID_TAG), CString::new(text)) else {
        return;
    };
    unsafe {
        __android_log_write(priority, tag.as_ptr(), text.as_ptr());
    }
}

#[cfg(not(target_os = "android"))]
fn write_to_console(level: Level, target: &str, message: &str) {
    eprintln!("{level:<5} {target}: {message}");
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_companion_logs_its_own_debug_lines_but_only_warnings_from_other_crates() {
        assert_eq!(level_for("battletime_companion_core::ble"), LevelFilter::Debug);
        assert_eq!(level_for("battletime_companion_lib"), LevelFilter::Debug);
        assert_eq!(level_for("btleplug::droidplug::peripheral"), LevelFilter::Info);
        assert_eq!(level_for("rustls::client"), LevelFilter::Warn);
    }

    #[test]
    fn a_line_says_when_how_serious_where_and_what() {
        assert_eq!(
            format_line(
                "2026-10-21 04:00:00.000",
                Level::Warn,
                "battletime_companion_core::ble",
                "read failed"
            ),
            "2026-10-21 04:00:00.000 WARN  battletime_companion_core::ble: read failed\n"
        );
    }

    #[test]
    fn a_full_file_is_kept_as_the_older_one_and_begun_again() {
        let folder = tempfile::tempdir().unwrap();
        let mut file = LogFile::open(folder.path()).unwrap();
        file.written = MOST_FILE_BYTES - 2;
        file.write_line("first\n");
        file.write_line("second\n");
        assert_eq!(
            std::fs::read_to_string(folder.path().join("companion.log")).unwrap(),
            "first\nsecond\n"
        );
        assert!(folder.path().join("companion.old.log").exists());
    }
}
