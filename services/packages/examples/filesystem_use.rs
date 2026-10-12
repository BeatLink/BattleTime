//! Builds the external flash of a typical install and reports what each file costs on littlefs.
//!
//! Usage: `cargo run -p neotime-packages --example filesystem_use -- [--image OUT] [--no-theme] ID=FILE... /PATH=FILE...`
//!
//! `ID=FILE` installs an app or face image; `/PATH=FILE` copies any other file, such as a saved state,
//! making its folders. `--no-theme` leaves out the all-black theme, so the built-in colours show.
//!
//! The file system is formatted by `neotime_storage::prepare_file_system` with the watch's own
//! settings, so block size, inline files and metadata pairs behave as on the watch. Each step's cost
//! is the change in blocks in use, as littlefs counts them. `--image` also writes the whole 4 MB
//! external flash, which the emulator loads with `externalFlashMemory LoadBinary @OUT 0`.
//! `tools/measure_memory.py` runs it with the apps the firmware builds in.

use embedded_storage::nor_flash::{ErrorType, NorFlash, NorFlashErrorKind, ReadNorFlash};
use littlefs_rust::Filesystem;
use neotime_storage::{FileSystemFlash, BLOCK_COUNT, BLOCK_SIZE};

// ============================================================================
// A NOR flash chip in RAM
// ============================================================================

const EXTERNAL_FLASH_SIZE: usize = 4 * 1024 * 1024;
/// The theme file holds one RGB565 colour per theme colour.
const THEME_FILE_BYTES: usize = 2 * neotime_runtime::ThemeColor::COUNT;

/// Erasing sets bits and writing can only clear them, as on the XT25F32B.
struct FlashInRam(Vec<u8>);

impl ErrorType for FlashInRam {
    type Error = NorFlashErrorKind;
}

impl ReadNorFlash for FlashInRam {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        let start = offset as usize;
        bytes.copy_from_slice(&self.0[start..start + bytes.len()]);
        Ok(())
    }

    fn capacity(&self) -> usize {
        self.0.len()
    }
}

impl NorFlash for FlashInRam {
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = BLOCK_SIZE as usize;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        self.0[from as usize..to as usize].fill(0xFF);
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        for (index, byte) in bytes.iter().enumerate() {
            self.0[offset as usize + index] &= byte;
        }
        Ok(())
    }
}

type Files = Filesystem<FileSystemFlash<FlashInRam>>;

// ============================================================================
// Measuring
// ============================================================================

/// One line of the report: a path, its bytes, and the blocks its step added.
struct Step {
    path: String,
    bytes: usize,
    blocks: u32,
}

fn blocks_in_use(files: &Files) -> u32 {
    files.fs_size().expect("littlefs counts its blocks")
}

/// Runs one change and records the blocks it added.
fn measure(files: &Files, steps: &mut Vec<Step>, path: &str, bytes: usize, change: impl FnOnce(&Files)) {
    let before = blocks_in_use(files);
    change(files);
    steps.push(Step {
        path: path.into(),
        bytes,
        blocks: blocks_in_use(files) - before,
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

/// The folders a file's path passes through, outermost first.
fn folders_of(path: &str) -> Vec<String> {
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let mut folders = Vec::new();
    let mut folder = String::new();
    for part in parts.iter().take(parts.len().saturating_sub(1)) {
        folder = format!("{folder}/{part}");
        folders.push(folder.clone());
    }
    folders
}

// ============================================================================
// Building the file system
// ============================================================================

fn main() {
    let options = read_options(std::env::args().skip(1)).unwrap_or_else(|problem| panic!("{problem}"));

    let flash = FileSystemFlash::new(FlashInRam(vec![0xFF; EXTERNAL_FLASH_SIZE]));
    let Ok(prepared) = neotime_storage::prepare_file_system(flash) else {
        panic!("the file system did not format");
    };
    let files = prepared.file_system;
    let mut steps = vec![Step {
        path: "(format and top-level folders)".into(),
        bytes: 0,
        blocks: blocks_in_use(&files),
    }];

    let mut settings = [0; neotime_settings::FILE_BYTES];
    let settings_bytes = neotime_settings::Settings::DEFAULT.write_file(&mut settings);
    measure(&files, &mut steps, "/settings/system", settings_bytes, |files| {
        files
            .write_file("/settings/system", &settings[..settings_bytes])
            .unwrap();
    });
    if !options.without_theme {
        measure(&files, &mut steps, "/settings/theme", THEME_FILE_BYTES, |files| {
            files
                .write_file("/settings/theme", &[0; THEME_FILE_BYTES])
                .unwrap();
        });
    }

    for entry in &options.entries {
        match entry {
            Entry::File { path, file } => {
                let bytes = std::fs::read(file).unwrap_or_else(|_| panic!("cannot read {file}"));
                for folder in folders_of(path) {
                    if !files.exists(&folder) {
                        files.mkdir(&folder).unwrap();
                    }
                }
                measure(&files, &mut steps, path, bytes.len(), |files| {
                    files.write_file(path, &bytes).unwrap();
                });
            }
            Entry::App { id, file } => {
                let image = std::fs::read(file).unwrap_or_else(|_| panic!("cannot read {file}"));
                let folder = neotime_packages::app_folder(id);
                let image_path = neotime_packages::app_file(id, neotime_packages::IMAGE_FILE);
                measure(&files, &mut steps, &format!("{folder}/"), 0, |files| {
                    files.mkdir(&folder).unwrap();
                });
                measure(&files, &mut steps, &image_path, image.len(), |files| {
                    files.write_file(&image_path, &image).unwrap();
                });
                neotime_packages::Packages::new(&files)
                    .install(id)
                    .unwrap_or_else(|problem| panic!("{id} did not install: {problem:?}"));
            }
        }
    }

    println!("block_size\t{BLOCK_SIZE}");
    println!("block_count\t{BLOCK_COUNT}");
    for step in &steps {
        println!("{}\t{}\t{}", step.path, step.bytes, step.blocks);
    }
    println!("in_use\t{}", blocks_in_use(&files));

    if let Some(image_path) = options.image_path {
        let Ok(flash) = files.unmount() else {
            panic!("the file system did not unmount");
        };
        std::fs::write(&image_path, flash.release().0).expect("the image is written");
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

    #[test]
    fn a_file_makes_every_folder_above_it() {
        assert_eq!(folders_of("/apps/timer/state"), vec!["/apps", "/apps/timer"]);
        assert_eq!(folders_of("/state"), Vec::<String>::new());
    }
}
