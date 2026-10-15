//! What the firmware's and the bootloader's build scripts share: the version text from git and the build date.

extern crate std;

use std::{env, println, process::Command, string::String, vec, vec::Vec};

use crate::version::{read_commit, write_date, write_version, BuildCommit};

// ============================================================================
// Version
// ============================================================================

/// Set by the Nix flake, or by hand, where the source has no `.git` to read the commit from.
const COMMIT_VARIABLE: &str = "BATTLETIME_GIT_COMMIT";

/// The package version and the commit, from git or else the variable, such as `0.1.0+1b19aed`.
///
/// It also tells cargo to run the build script again when the commit or the variable changes.
pub fn version_text() -> String {
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
    println!("cargo:rerun-if-env-changed={COMMIT_VARIABLE}");
    rerun_when_the_commit_moves();
    version
}

/// The time the build script runs, in milliseconds since 1970.
pub fn build_time_in_milliseconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

/// The UTC day of a build time, such as `2026-10-13`.
pub fn build_date(build_time_in_milliseconds: u64) -> String {
    let mut date = String::new();
    write_date(&mut date, build_time_in_milliseconds).unwrap();
    date
}

// ============================================================================
// Git
// ============================================================================

/// Runs git in the crate's folder and gives what it printed, or none when git or the repository is missing.
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

/// Reruns the build script when HEAD, the branch it points to or the index changes, as git keeps them.
fn rerun_when_the_commit_moves() {
    let mut watched_names: Vec<String> = vec![
        String::from("HEAD"),
        String::from("index"),
        String::from("packed-refs"),
    ];
    if let Some(branch) = run_git(&["symbolic-ref", "--quiet", "HEAD"]) {
        watched_names.push(String::from(branch.trim()));
    }
    for name in watched_names {
        if let Some(path) = run_git(&["rev-parse", "--path-format=absolute", "--git-path", &name]) {
            println!("cargo:rerun-if-changed={}", path.trim());
        }
    }
}
