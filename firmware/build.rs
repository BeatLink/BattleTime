//! Writes the linker's memory map from the shared flash layout, so the firmware starts where the bootloader expects it.
//!
//! The last words of RAM are left out, for the alive marker. It also sets the version the firmware reports.

use std::{env, fs, path::PathBuf, process::Command};

use neotime_boot::{
    version::{read_commit, write_version, BuildCommit},
    FIRMWARE, FIRMWARE_ALIVE_MARKER, RAM,
};

const WASP_OS_APP_START: u32 = 0x19000;
/// wasp-os's bootloader starts at 0x78000 and keeps seven pages of app data below it.
const WASP_OS_APP_END: u32 = 0x78000 - 7 * 0x1000;
/// Above the SoftDevice's RAM and wasp-os's bootloader's retained block at 0x200039c0, where the rehearsal keeps breadcrumbs.
const WASP_OS_APP_RAM_START: u32 = 0x2000_4000;

fn main() {
    let output_directory = PathBuf::from(env::var("OUT_DIR").unwrap());

    // The rehearsal runs as a wasp-os app: from where wasp-os's bootloader starts apps up to the data it
    // keeps below itself, and above the first RAM, which Nordic's MBR still uses.
    let (flash_start, flash_size, ram_start) = if env::var_os("CARGO_FEATURE_WASP_OS_REHEARSAL").is_some() {
        (
            WASP_OS_APP_START,
            WASP_OS_APP_END - WASP_OS_APP_START,
            WASP_OS_APP_RAM_START,
        )
    } else {
        (FIRMWARE.start, FIRMWARE.size, RAM.start)
    };
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
    let build_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    println!("cargo:rustc-env=NEOTIME_BUILD_TIME_MS={build_time}");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_WASP_OS_REHEARSAL");

    // The rehearsals and the recovery firmware leave out the shell and everything only it uses.
    println!("cargo:rustc-check-cfg=cfg(with_shell)");
    let rehearsal = env::var_os("CARGO_FEATURE_WASP_OS_REHEARSAL").is_some();
    let recovery = env::var_os("CARGO_FEATURE_RECOVERY").is_some();
    if !rehearsal && !recovery {
        println!("cargo:rustc-cfg=with_shell");
    }
    println!("cargo:rerun-if-changed=../boot/src/lib.rs");

    set_firmware_version();
}

// ============================================================================
// Version
// ============================================================================

/// Set by the Nix flake, or by hand, where the source has no `.git` to read the commit from.
const COMMIT_VARIABLE: &str = "NEOTIME_GIT_COMMIT";

/// Gives the firmware `NEOTIME_FIRMWARE_VERSION`: the package version and the commit, from git or else the variable.
fn set_firmware_version() {
    let package_version = env::var("CARGO_PKG_VERSION").unwrap();
    let git_commit = run_git(&["rev-parse", "HEAD"]);
    let variable_commit = env::var(COMMIT_VARIABLE).ok();
    let commit = match git_commit.as_deref().and_then(read_commit) {
        Some(commit) => Some(BuildCommit {
            has_uncommitted_changes: tree_has_uncommitted_changes(),
            ..commit
        }),
        None => variable_commit.as_deref().and_then(read_commit),
    };

    let mut version = String::new();
    write_version(&mut version, &package_version, commit).unwrap();
    println!("cargo:rustc-env=NEOTIME_FIRMWARE_VERSION={version}");
    println!("cargo:rerun-if-env-changed={COMMIT_VARIABLE}");
    rerun_when_the_commit_moves();
}

/// Runs git in the firmware's folder and gives what it printed, or none when git or the repository is missing.
fn run_git(arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(env::var("CARGO_MANIFEST_DIR").ok()?)
        .args(arguments)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn tree_has_uncommitted_changes() -> bool {
    run_git(&["status", "--porcelain", "--untracked-files=no"])
        .is_some_and(|changes| !changes.trim().is_empty())
}

/// Reruns this script when HEAD, the branch it points to or the index changes, as git keeps them.
fn rerun_when_the_commit_moves() {
    let mut watched_names = vec![
        String::from("HEAD"),
        String::from("index"),
        String::from("packed-refs"),
    ];
    if let Some(branch) = run_git(&["symbolic-ref", "--quiet", "HEAD"]) {
        watched_names.push(branch.trim().to_owned());
    }
    for name in watched_names {
        if let Some(path) = run_git(&["rev-parse", "--path-format=absolute", "--git-path", &name]) {
            println!("cargo:rerun-if-changed={}", path.trim());
        }
    }
}
