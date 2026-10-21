//! Builds the external flash of a typical install and reports what each file costs in the flash store.
//!
//! Usage: `cargo run -p battletime-packages --example filesystem_use -- [--image OUT] [--no-theme] [--no-accelerometer] [--no-icons] [--no-modules] ID=FILE... /PATH=FILE...`
//! or `... -- --read IMAGE /PATH`
//!
//! `ID=FILE` installs an app or face image; `/PATH=FILE` copies any other file, such as a saved state.
//! `--no-theme` leaves out the all-black theme, so the built-in colours show. Both accelerometers'
//! configuration files, the shared library's icons and the native modules go in, as the companion
//! app puts them, unless `--no-accelerometer`, `--no-icons` or `--no-modules` leaves them out to
//! show a watch without them. The modules are built first with `tools/build_modules.sh`, which needs
//! `nix develop`; without the verifier module the watch opens no app.
//!
//! The store is mounted with the watch's own layout and sizes, so files up to 1 KB become records and
//! larger ones take whole sectors, as on the watch. Each step's cost is the file sectors it added.
//! `--image` also writes the whole 4 MB external flash, which the emulator loads with
//! `externalFlashMemory LoadBinary @OUT 0`. `tools/measure_memory.py` runs it with a typical install.
//!
//! `--read` mounts a whole 4 MB image, such as the emulator's `externalFlash SaveBinary` writes, and
//! prints the file at `/PATH` in hex, or `missing`.

use battletime_boot::{EXTERNAL_FLASH_SIZE, FILE_SYSTEM};
use battletime_flash_store::{ram_flash::RamFlash, FileAccess, FlashStore, Layout, SECTOR_BYTES};
use embassy_futures::block_on;

// ============================================================================
// The store, as the watch builds it
// ============================================================================

/// The theme file holds one RGB565 colour per theme colour.
const THEME_FILE_BYTES: usize = 2 * battletime_runtime::ThemeColor::COUNT;

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
    /// Leaves out the accelerometers' configuration files, so the watch counts no steps.
    without_accelerometer: bool,
    /// Leaves out the shared library's icons file, so library icons draw as placeholders.
    without_icons: bool,
    /// Leaves out the native modules, the verifier among them, so the watch opens no app.
    without_modules: bool,
    entries: Vec<Entry>,
    /// `--read IMAGE /PATH`: the image to mount and the file to print from it.
    read: Option<(String, String)>,
}

fn read_options(arguments: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut options = Options::default();
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--image" => options.image_path = Some(arguments.next().ok_or("--image wants a path")?),
            "--no-theme" => options.without_theme = true,
            "--no-accelerometer" => options.without_accelerometer = true,
            "--no-icons" => options.without_icons = true,
            "--no-modules" => options.without_modules = true,
            "--read" => {
                let image = arguments.next().ok_or("--read wants an image")?;
                let path = arguments.next().ok_or("--read wants a path")?;
                options.read = Some((image, path));
            }
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
    if let Some((image, path)) = &options.read {
        println!("{}", read_from_image(image, path));
        return;
    }

    let flash = RamFlash::blank(EXTERNAL_FLASH_SIZE as usize / SECTOR_BYTES as usize);
    let mut store = block_on(Store::mount(flash, LAYOUT)).expect("the store mounts on blank flash");
    let mut steps = Vec::new();

    let mut settings = [0; battletime_settings::FILE_BYTES];
    let settings_bytes = battletime_settings::Settings::DEFAULT.write_file(&mut settings);
    write_measured(
        &mut store,
        &mut steps,
        "/settings/system",
        &settings[..settings_bytes],
    );
    if !options.without_theme {
        write_measured(&mut store, &mut steps, "/settings/theme", &[0; THEME_FILE_BYTES]);
    }
    // Both chips' files, as the companion app puts them, since a watch has either.
    if !options.without_accelerometer {
        for (path, file) in [
            (
                battletime_accelerometer::BMA421_CONFIGURATION_PATH,
                battletime_accelerometer::BMA421_CONFIGURATION_FILE,
            ),
            (
                battletime_accelerometer::BMA425_CONFIGURATION_PATH,
                battletime_accelerometer::BMA425_CONFIGURATION_FILE,
            ),
        ] {
            write_measured(&mut store, &mut steps, path, file);
        }
    }
    if !options.without_icons {
        let icons =
            battletime_compiler::library::build_icons_file().unwrap_or_else(|problem| panic!("{problem}"));
        write_measured(
            &mut store,
            &mut steps,
            battletime_runtime::library_icons::LIBRARY_ICONS_PATH,
            &icons,
        );
    }

    if !options.without_modules {
        for (name, bytes) in built_modules() {
            write_measured(&mut store, &mut steps, &format!("/system/modules/{name}"), &bytes);
        }
    }

    for entry in &options.entries {
        match entry {
            Entry::File { path, file } => {
                let bytes = std::fs::read(file).unwrap_or_else(|_| panic!("cannot read {file}"));
                write_measured(&mut store, &mut steps, path, &bytes);
            }
            Entry::App { id, file } => {
                let image = std::fs::read(file).unwrap_or_else(|_| panic!("cannot read {file}"));
                let image_path = battletime_packages::app_file(id, battletime_packages::IMAGE_FILE);
                write_measured(&mut store, &mut steps, &image_path, &image);
                // The example writes no shared library, so any library version installs.
                block_on(battletime_packages::Packages::new(&mut store).install(id, None))
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

/// Each native module by name, built for the watch by `tools/build_modules.sh`.
fn built_modules() -> Vec<(String, Vec<u8>)> {
    let battletime = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let built = std::process::Command::new(battletime.join("../tools/build_modules.sh"))
        .status()
        .is_ok_and(|status| status.success());
    assert!(
        built,
        "tools/build_modules.sh failed; run inside nix develop, or pass --no-modules"
    );
    let mut modules: Vec<(String, Vec<u8>)> = std::fs::read_dir(battletime.join("dist/modules"))
        .expect("the modules were built")
        .map(|entry| entry.expect("the folder lists").path())
        .map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read(&path).expect("a module reads"))
        })
        .collect();
    modules.sort();
    modules
}

// ============================================================================
// Reading an image back
// ============================================================================

/// The file at `path` in the image in hex, or `missing`.
fn read_from_image(image: &str, path: &str) -> String {
    let bytes = std::fs::read(image).unwrap_or_else(|_| panic!("cannot read {image}"));
    let mut store = block_on(Store::mount(RamFlash { bytes, erases: 0 }, LAYOUT)).expect("the image mounts");
    let Some(length) = block_on(FileAccess::file_length(&mut store, path)).expect("the store reads") else {
        return "missing".into();
    };
    let mut file = vec![0; length];
    block_on(FileAccess::read_file(&mut store, path, 0, &mut file)).expect("the store reads");
    file.iter().map(|byte| format!("{byte:02x}")).collect()
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
            "--no-accelerometer",
            "--no-icons",
            "--no-modules",
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
                without_accelerometer: true,
                without_icons: true,
                without_modules: true,
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
                read: None,
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
        assert!(read_options(strings(&["--read", "flash.bin"])).is_err());
    }

    #[test]
    fn a_file_reads_back_from_an_image_in_hex() {
        let flash = RamFlash::blank(EXTERNAL_FLASH_SIZE as usize / SECTOR_BYTES as usize);
        let mut store = block_on(Store::mount(flash, LAYOUT)).unwrap();
        block_on(FileAccess::write_file(&mut store, "/settings/system", &[1, 0xab])).unwrap();
        let image = std::env::temp_dir().join(format!("filesystem_use-{}.bin", std::process::id()));
        std::fs::write(&image, store.into_flash().bytes).unwrap();
        let image = image.to_str().unwrap();

        assert_eq!(read_from_image(image, "/settings/system"), "01ab");
        assert_eq!(read_from_image(image, "/settings/theme"), "missing");
        std::fs::remove_file(image).unwrap();
    }
}
