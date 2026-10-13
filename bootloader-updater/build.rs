//! Links the updater where the firmware runs, and bakes in the bootloader it installs.
//!
//! The bootloader comes from NEOTIME_NEW_BOOTLOADER_BIN and is checked here, so a wrong or oversized
//! image fails the build rather than the watch. Built with `--features from-first-layout`, it is linked
//! where the 16 KB bootloader started firmware, and also carries the recovery firmware from
//! NEOTIME_RECOVERY_BIN, which the new bootloader installs on its first start.

use std::{env, fs, path::PathBuf};

use neotime_boot::{
    image_header::{ImageHeader, IMAGE_HEADER_SIZE},
    is_bootloader_vector_table,
    migration::{crc32, CRC32_INITIAL},
    BOOTLOADER, DEFAULT_SLOTS, FIRMWARE, FIRMWARE_ALIVE_MARKER, FIRMWARE_VECTOR_TABLE, RAM,
    VECTOR_TABLE_CHECK_SIZE,
};

/// Where the 16 KB bootloader started the firmware, and how much room it had.
const FIRST_LAYOUT_FIRMWARE: (u32, u32) = (0x5000, 400 * 1024);

fn main() {
    let output_directory = PathBuf::from(env::var("OUT_DIR").unwrap());

    // The bootloader swaps the updater in like any firmware, so it is linked like one. Under the first
    // layout its code starts at `FIRMWARE`, past the slot table and boot state pages, so the new
    // bootloader finds those pages zeroed rather than holding the updater's code.
    let from_first_layout = env::var_os("CARGO_FEATURE_FROM_FIRST_LAYOUT").is_some();
    let (flash_start, flash_size, code_start) = if from_first_layout {
        let (start, size) = FIRST_LAYOUT_FIRMWARE;
        (start, size, format!("_stext = {:#010x};\n", FIRMWARE.start))
    } else {
        (
            FIRMWARE_VECTOR_TABLE,
            FIRMWARE.end() - FIRMWARE_VECTOR_TABLE,
            String::new(),
        )
    };
    let memory_map = format!(
        "MEMORY\n{{\n  FLASH : ORIGIN = {:#010x}, LENGTH = {:#x}\n  RAM : ORIGIN = {:#010x}, LENGTH = {:#x}\n}}\n{code_start}",
        flash_start,
        flash_size,
        RAM.start,
        FIRMWARE_ALIVE_MARKER.start - RAM.start,
    );
    fs::write(output_directory.join("memory.x"), memory_map).unwrap();
    println!("cargo:rustc-link-search={}", output_directory.display());

    println!("cargo:rerun-if-env-changed=NEOTIME_NEW_BOOTLOADER_BIN");
    let path = PathBuf::from(
        env::var_os("NEOTIME_NEW_BOOTLOADER_BIN")
            .expect("NEOTIME_NEW_BOOTLOADER_BIN must name the bootloader .bin"),
    );
    println!("cargo:rerun-if-changed={}", path.display());
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));

    assert!(
        bytes.len() <= BOOTLOADER.size as usize,
        "the bootloader is {} bytes, more than its {} byte region",
        bytes.len(),
        BOOTLOADER.size
    );
    let start: [u8; VECTOR_TABLE_CHECK_SIZE] = bytes[..VECTOR_TABLE_CHECK_SIZE].try_into().unwrap();
    assert!(
        is_bootloader_vector_table(start),
        "{} does not start with a bootloader's vector table",
        path.display()
    );

    // Padded to whole words, which is what the flash controller writes.
    let padded_length = bytes.len().div_ceil(4) * 4;
    let mut padded = bytes.clone();
    padded.resize(padded_length, 0xFF);
    let padded_path = output_directory.join("new-bootloader.padded");
    fs::write(&padded_path, &padded).unwrap();

    let source = format!(
        "pub const LENGTH_IN_BYTES: usize = {};\n\
         pub const CRC32: u32 = {:#010x};\n\
         #[repr(align(4))]\nstruct Aligned([u8; {padded_length}]);\n\
         static ALIGNED: Aligned = Aligned(*include_bytes!({:?}));\n\
         /// The bootloader padded to whole words with 0xFF.\n\
         pub static PADDED: &[u8] = &ALIGNED.0;\n",
        bytes.len(),
        crc32(CRC32_INITIAL, &bytes),
        padded_path,
    );
    fs::write(output_directory.join("new_bootloader.rs"), source).unwrap();

    println!("cargo:rerun-if-env-changed=NEOTIME_RECOVERY_BIN");
    if from_first_layout {
        let path = PathBuf::from(
            env::var_os("NEOTIME_RECOVERY_BIN").expect("NEOTIME_RECOVERY_BIN must name the recovery .bin"),
        );
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = fs::read(&path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        check_recovery(&bytes, &path);

        let padded_length = bytes.len().div_ceil(4) * 4;
        let mut padded = bytes.clone();
        padded.resize(padded_length, 0xFF);
        let padded_path = output_directory.join("recovery.padded");
        fs::write(&padded_path, &padded).unwrap();
        let source = format!(
            "pub const LENGTH_IN_BYTES: usize = {};\n\
             #[repr(align(4))]\nstruct Aligned([u8; {padded_length}]);\n\
             static ALIGNED: Aligned = Aligned(*include_bytes!({:?}));\n\
             /// The recovery firmware, header first, padded to whole words with 0xFF.\n\
             pub static PADDED: &[u8] = &ALIGNED.0;\n",
            bytes.len(),
            padded_path,
        );
        fs::write(output_directory.join("recovery.rs"), source).unwrap();
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../boot/src/lib.rs");
}

/// A recovery image the new bootloader will install: a sound header, linked for `FIRMWARE`, whole, and fitting its slot.
fn check_recovery(bytes: &[u8], path: &std::path::Path) {
    let header_bytes: [u8; IMAGE_HEADER_SIZE as usize] = bytes
        .get(..IMAGE_HEADER_SIZE as usize)
        .and_then(|header| header.try_into().ok())
        .unwrap_or_else(|| panic!("{} is too short for an image header", path.display()));
    let header = ImageHeader::decode(&header_bytes).unwrap_or_else(|| {
        panic!(
            "{} has no sound image header; run tools/image_header.py",
            path.display()
        )
    });
    assert!(
        header.runs_as_firmware(),
        "{} is not linked to run from FIRMWARE",
        path.display()
    );
    assert!(
        header.total_size_in_bytes() as usize == bytes.len()
            && crc32(CRC32_INITIAL, &bytes[IMAGE_HEADER_SIZE as usize..]) == header.image_crc32,
        "{} does not match its header's length and CRC",
        path.display()
    );
    assert!(
        header.total_size_in_bytes() <= DEFAULT_SLOTS.recovery.size,
        "{} does not fit the recovery slot",
        path.display()
    );
}
