//! Builds the external flash of a typical install and reports what each file costs on littlefs.
//!
//! Usage: `cargo run -p neotime-packages --example filesystem_use -- [--image OUT] ID=FILE...`
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

fn main() {
    let mut arguments: Vec<String> = std::env::args().skip(1).collect();
    let image_path = match arguments.iter().position(|argument| argument == "--image") {
        Some(index) => {
            arguments.remove(index);
            Some(arguments.remove(index))
        }
        None => None,
    };

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
    measure(&files, &mut steps, "/settings/theme", THEME_FILE_BYTES, |files| {
        files
            .write_file("/settings/theme", &[0; THEME_FILE_BYTES])
            .unwrap();
    });

    for pair in &arguments {
        let (id, file) = pair.split_once('=').expect("apps are given as ID=FILE");
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

    println!("block_size\t{BLOCK_SIZE}");
    println!("block_count\t{BLOCK_COUNT}");
    for step in &steps {
        println!("{}\t{}\t{}", step.path, step.bytes, step.blocks);
    }
    println!("in_use\t{}", blocks_in_use(&files));

    if let Some(image_path) = image_path {
        let Ok(flash) = files.unmount() else {
            panic!("the file system did not unmount");
        };
        std::fs::write(&image_path, flash.release().0).expect("the image is written");
    }
}
