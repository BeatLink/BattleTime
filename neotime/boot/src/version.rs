//! The version text a build carries: the package version from `Cargo.toml` and the git commit it was built from.
//!
//! A build script reads the commit from git, or from `NEOTIME_GIT_COMMIT` where there is no `.git`,
//! and writes it with `write_version`, such as `0.1.0+1b19aed`, or `0.1.0+1b19aed*` when the tree
//! had changes nobody had committed.

use core::fmt::{self, Write};

// ============================================================================
// Commits
// ============================================================================

/// Hex digits of the commit hash a version shows, as `git rev-parse --short` and Nix's `shortRev` give.
pub const SHORT_COMMIT_LENGTH: usize = 7;

/// Shown in place of the commit when a build could not learn it.
pub const UNKNOWN_COMMIT: &str = "unknown";

/// Marks a commit built with changes on top of it.
const CHANGED_MARK: char = '*';

/// The suffix Nix's `dirtyShortRev` puts after the hash.
const DIRTY_SUFFIX: &str = "-dirty";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildCommit<'text> {
    pub short_hash: &'text str,
    pub has_uncommitted_changes: bool,
}

/// Reads a commit hash of any length, with or without Nix's `-dirty` suffix; anything else gives none.
pub fn read_commit(text: &str) -> Option<BuildCommit<'_>> {
    let text = text.trim();
    let (hash, has_uncommitted_changes) = match text.strip_suffix(DIRTY_SUFFIX) {
        Some(hash) => (hash, true),
        None => (text, false),
    };

    let is_hash = hash.len() >= SHORT_COMMIT_LENGTH && hash.bytes().all(|byte| byte.is_ascii_hexdigit());
    if !is_hash {
        return None;
    }
    Some(BuildCommit {
        short_hash: &hash[..SHORT_COMMIT_LENGTH],
        has_uncommitted_changes,
    })
}

// ============================================================================
// Version text
// ============================================================================

/// Writes the package version and the commit, joined by a `+` as build details are in semantic versioning.
pub fn write_version(
    writer: &mut impl Write,
    package_version: &str,
    commit: Option<BuildCommit<'_>>,
) -> fmt::Result {
    match commit {
        Some(commit) => {
            write!(writer, "{package_version}+{}", commit.short_hash)?;
            if commit.has_uncommitted_changes {
                writer.write_char(CHANGED_MARK)?;
            }
            Ok(())
        }
        None => write!(writer, "{package_version}+{UNKNOWN_COMMIT}"),
    }
}
