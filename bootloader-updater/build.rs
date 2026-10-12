//! Links the updater where the firmware runs, and bakes in the bootloader it installs.
//!
//! The bootloader comes from NEOTIME_NEW_BOOTLOADER_BIN and is checked here, so a wrong or oversized
//! image fails the build rather than the watch.

use std::{env, fs, path::PathBuf};

use neotime_boot::{
    is_bootloader_vector_table,
    migration::{crc32, CRC32_INITIAL},
    BOOTLOADER, FIRMWARE, FIRMWARE_ALIVE_MARKER, RAM, VECTOR_TABLE_CHECK_SIZE,
};

fn main() {
    let output_directory = PathBuf::from(env::var("OUT_DIR").unwrap());

    // The bootloader swaps the updater in like any firmware, so it is linked like one.
    let memory_map = format!(
        "MEMORY\n{{\n  FLASH : ORIGIN = {:#010x}, LENGTH = {:#x}\n  RAM : ORIGIN = {:#010x}, LENGTH = {:#x}\n}}\n",
        FIRMWARE.start,
        FIRMWARE.size,
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

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../boot/src/lib.rs");
}
