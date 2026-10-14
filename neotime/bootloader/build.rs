//! Writes the linker's memory map from the shared flash layout, so the two cannot drift apart.
//!
//! The last words of RAM are left out, so the bootloader's stack does not overwrite the firmware's alive marker.

use std::{env, fs, path::PathBuf};

use neotime_boot::{BOOTLOADER, FIRMWARE_ALIVE_MARKER, RAM};

fn main() {
    let output_directory = PathBuf::from(env::var("OUT_DIR").unwrap());

    let memory_map = format!(
        "MEMORY\n{{\n  FLASH : ORIGIN = {:#010x}, LENGTH = {:#x}\n  RAM : ORIGIN = {:#010x}, LENGTH = {:#x}\n}}\n",
        BOOTLOADER.start,
        BOOTLOADER.size,
        RAM.start,
        RAM.size - FIRMWARE_ALIVE_MARKER.size,
    );
    fs::write(output_directory.join("memory.x"), memory_map).unwrap();

    println!("cargo:rustc-link-search={}", output_directory.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../boot/src/lib.rs");
}
