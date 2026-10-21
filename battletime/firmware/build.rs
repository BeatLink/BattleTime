//! Writes the linker's memory map from the shared flash layout, so the firmware starts where the bootloader expects it.
//!
//! The last words of RAM are left out, for the alive marker. It also sets the version and build date the firmware reports.

use std::{env, fs, path::PathBuf};

use battletime_boot::{
    build_script::{build_date, build_time_in_milliseconds, version_text},
    FIRMWARE, FIRMWARE_ALIVE_MARKER, FIRMWARE_VECTOR_TABLE, RAM,
};

fn main() {
    let output_directory = PathBuf::from(env::var("OUT_DIR").unwrap());

    // The image header sits in front, so the code starts after it.
    let (flash_start, flash_size, ram_start) = (
        FIRMWARE_VECTOR_TABLE,
        FIRMWARE.end() - FIRMWARE_VECTOR_TABLE,
        RAM.start,
    );
    let memory_map = format!(
        "MEMORY\n{{\n  FLASH : ORIGIN = {:#010x}, LENGTH = {:#x}\n  RAM : ORIGIN = {:#010x}, LENGTH = {:#x}\n}}\n",
        flash_start,
        flash_size,
        ram_start,
        FIRMWARE_ALIVE_MARKER.start - ram_start,
    );
    fs::write(output_directory.join("memory.x"), memory_map).unwrap();

    println!("cargo:rustc-link-search={}", output_directory.display());
    println!("cargo:rerun-if-changed=build.rs");

    // The clock starts from the build time until a phone sets it.
    let build_time = build_time_in_milliseconds();
    println!("cargo:rustc-env=BATTLETIME_BUILD_TIME_MS={build_time}");

    // The recovery firmware leaves out the shell and everything only it uses.
    println!("cargo:rustc-check-cfg=cfg(with_shell)");
    let recovery = env::var_os("CARGO_FEATURE_RECOVERY").is_some();
    if !recovery {
        println!("cargo:rustc-cfg=with_shell");
    }
    println!("cargo:rerun-if-changed=../boot/src/lib.rs");

    // InfiniTime's own services, each turned on by its feature in the shell's builds only.
    for (feature, name) in INFINITIME_SERVICES {
        println!("cargo:rustc-check-cfg=cfg({name})");
        if !recovery && env::var_os(feature).is_some() {
            println!("cargo:rustc-cfg={name}");
        }
    }

    set_firmware_version(build_time);
}

/// Each InfiniTime service's feature, as cargo passes it to this script, and the cfg it turns on.
const INFINITIME_SERVICES: [(&str, &str); 5] = [
    ("CARGO_FEATURE_WEATHER_SERVICE", "weather_service"),
    ("CARGO_FEATURE_MUSIC_SERVICE", "music_service"),
    ("CARGO_FEATURE_NAVIGATION_SERVICE", "navigation_service"),
    ("CARGO_FEATURE_ALERT_SERVICE", "alert_service"),
    ("CARGO_FEATURE_MOTION_SERVICE", "motion_service"),
];

// ============================================================================
// Version
// ============================================================================

/// Gives the firmware `BATTLETIME_FIRMWARE_VERSION`, the package version and the commit, and `BATTLETIME_BUILD_DATE`.
fn set_firmware_version(build_time_in_milliseconds: u64) {
    println!("cargo:rustc-env=BATTLETIME_FIRMWARE_VERSION={}", version_text());
    println!(
        "cargo:rustc-env=BATTLETIME_BUILD_DATE={}",
        build_date(build_time_in_milliseconds)
    );
}
