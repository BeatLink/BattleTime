//! Builds the external flash of a typical install and reports what each file costs in the flash store.
//!
//! Usage: `cargo run -p neotime-packages --example filesystem_use -- [--image OUT] [--no-theme] ID=FILE... /PATH=FILE...`
//!
//! `ID=FILE` installs an app or face image; `/PATH=FILE` copies any other file, such as a saved state.
//! `--no-theme` leaves out the all-black theme, so the built-in colours show.
//!
//! The store is mounted with the watch's own layout and sizes, so files up to 1 KB become records and
//! larger ones take whole sectors, as on the watch. Each step's cost is the file sectors it added.
//! `--image` also writes the whole 4 MB external flash, which the emulator loads with
//! `externalFlashMemory LoadBinary @OUT 0`. `tools/measure_memory.py` runs it with a typical install.

use embassy_futures::block_on;
use neotime_boot::{EXTERNAL_FLASH_SIZE, FILE_SYSTEM};
use neotime_flash_store::{ram_flash::RamFlash, FileAccess, FlashStore, Layout, SECTOR_BYTES};

// ============================================================================
// The store, as the watch builds it
// ============================================================================

/// The theme file holds one RGB565 colour per theme colour.
const THEME_FILE_BYTES: usize = 2 * neotime_runtime::ThemeColor::COUNT;

/// The watch's sizes: 128 files, 128 record keys and rings of up to 192 sectors.
type Store = FlashStore<RamFlash, 128, 128, 192>;

/// The 3 MB file system region: 2 MB of files, 256 KB of records and 768 KB of logs.
const LAYOUT: Layout = Layout {
    start_address: FILE_SYSTEM.start,
    file_sectors: 512,
    record_sectors: 64,
    log_sectors: 192,
};

// ============================================================================
// Measuring
// ============================================================================

/// One line of the report: a path, its bytes, and the file sectors its step added.
struct Step {
    path: String,
    bytes: usize,
    sectors: usize,
}

fn sectors_in_use(store: &Store) -> usize {
    store.file_sectors() - store.free_file_sectors()
}

fn write_measured(store: &mut Store, steps: &mut Vec<Step>, path: &str, bytes: &[u8]) {
    let before = sectors_in_use(store);
    block_on(FileAccess::write_file(store, path, bytes))
        .unwrap_or_else(|problem| panic!("{path}: {problem:?}"));
    steps.push(Step {
        path: path.into(),
        bytes: bytes.len(),
        sectors: sectors_in_use(store) - before,
    });
}

// ============================================================================
// Arguments
// ============================================================================

/// One thing to put on the file system.
#[derive(Debug, PartialEq, Eq)]
enum Entry {
    /// `ID=FILE`: an app or face image, installed as the package manager would.
    App { id: String, file: String },
    /// `/PATH=FILE`: any file, such as an app's saved state, copied as it is.
    File { path: String, file: String },
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    image_path: Option<String>,
    /// Leaves out the all-black theme, which stands in for the companion app's, so the built-in colours show.
    without_theme: bool,
    entries: Vec<Entry>,
}

fn read_options(arguments: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut options = Options::default();
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--image" => options.image_path = Some(arguments.next().ok_or("--image wants a path")?),
            "--no-theme" => options.without_theme = true,
            _ => {
                let (name, file) = argument
                    .split_once('=')
                    .ok_or_else(|| format!("{argument}: apps are ID=FILE and other files /PATH=FILE"))?;
                let (name, file) = (name.to_string(), file.to_string());
                options.entries.push(if name.starts_with('/') {
                    Entry::File { path: name, file }
                } else {
                    Entry::App { id: name, file }
                });
            }
        }
    }
    Ok(options)
}

// ============================================================================
// Building the store
// ============================================================================

fn main() {
    let options = read_options(std::env::args().skip(1)).unwrap_or_else(|problem| panic!("{problem}"));

    let flash = RamFlash::blank(EXTERNAL_FLASH_SIZE as usize / SECTOR_BYTES as usize);
    let mut store = block_on(Store::mount(flash, LAYOUT)).expect("the store mounts on blank flash");
    let mut steps = Vec::new();

    let mut settings = [0; neotime_settings::FILE_BYTES];
    let settings_bytes = neotime_settings::Settings::DEFAULT.write_file(&mut settings);
    write_measured(
        &mut store,
        &mut steps,
        "/settings/system",
        &settings[..settings_bytes],
    );
    if !options.without_theme {
        write_measured(&mut store, &mut steps, "/settings/theme", &[0; THEME_FILE_BYTES]);
    }

    for entry in &options.entries {
        match entry {
            Entry::File { path, file } => {
                let bytes = std::fs::read(file).unwrap_or_else(|_| panic!("cannot read {file}"));
                write_measured(&mut store, &mut steps, path, &bytes);
            }
            Entry::App { id, file } => {
                let image = std::fs::read(file).unwrap_or_else(|_| panic!("cannot read {file}"));
                let image_path = neotime_packages::app_file(id, neotime_packages::IMAGE_FILE);
                write_measured(&mut store, &mut steps, &image_path, &image);
                // The example writes no shared library, so any library version installs.
                block_on(neotime_packages::Packages::new(&mut store).install(id, None))
                    .unwrap_or_else(|problem| panic!("{id} did not install: {problem:?}"));
            }
        }
    }

    println!("sector_size\t{SECTOR_BYTES}");
    println!("file_sectors\t{}", LAYOUT.file_sectors);
    println!("record_sectors\t{}", LAYOUT.record_sectors);
    println!("log_sectors\t{}", LAYOUT.log_sectors);
    for step in &steps {
        println!("{}\t{}\t{}", step.path, step.bytes, step.sectors);
    }
    println!("file_sectors_in_use\t{}", sectors_in_use(&store));
    println!("record_bytes_in_use\t{}", store.record_bytes_in_use());

    if let Some(image_path) = options.image_path {
        std::fs::write(&image_path, store.into_flash().bytes).expect("the image is written");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(arguments: &[&str]) -> Vec<String> {
        arguments.iter().map(|argument| argument.to_string()).collect()
    }

    #[test]
    fn apps_files_and_flags_are_read_in_order() {
        let options = read_options(strings(&[
            "--no-theme",
            "launcher=out/launcher.ntapp",
            "--image",
            "flash.bin",
            "/apps/timer/state=backup/timer/state",
        ]))
        .unwrap();

        assert_eq!(
            options,
            Options {
                image_path: Some("flash.bin".into()),
                without_theme: true,
                entries: vec![
                    Entry::App {
                        id: "launcher".into(),
                        file: "out/launcher.ntapp".into()
                    },
                    Entry::File {
                        path: "/apps/timer/state".into(),
                        file: "backup/timer/state".into()
                    },
                ],
            }
        );
    }

    #[test]
    fn the_theme_is_written_unless_asked_not_to() {
        assert!(!read_options(strings(&[])).unwrap().without_theme);
    }

    #[test]
    fn a_bare_name_or_a_missing_image_path_is_refused() {
        assert!(read_options(strings(&["launcher"])).is_err());
        assert!(read_options(strings(&["--image"])).is_err());
    }
}
