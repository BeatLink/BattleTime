//! Writes the linker's memory map from the shared flash layout, so the two cannot drift apart.

use std::{env, fs, path::PathBuf};

use neotime_boot::{BOOTLOADER, RAM};

fn main() {
    let output_directory = PathBuf::from(env::var("OUT_DIR").unwrap());

    let memory_map = format!(
        "MEMORY\n{{\n  FLASH : ORIGIN = {:#010x}, LENGTH = {:#x}\n  RAM : ORIGIN = {:#010x}, LENGTH = {:#x}\n}}\n",
        BOOTLOADER.start, BOOTLOADER.size, RAM.start, RAM.size,
    );
    fs::write(output_directory.join("memory.x"), memory_map).unwrap();

    println!("cargo:rustc-link-search={}", output_directory.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../boot/src/lib.rs");
}
