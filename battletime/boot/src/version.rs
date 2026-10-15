//! The version text a build carries: the package version from `Cargo.toml` and the git commit it was built from.
//!
//! A build script reads the commit from git, or from `BATTLETIME_GIT_COMMIT` where there is no `.git`,
//! and writes it with `write_version`, such as `0.1.0+1b19aed`, or `0.1.0+1b19aed*` when the tree
//! had changes nobody had committed. The bootloader carries its version and build date in a
//! `VersionRecord`, which the firmware finds by its magic bytes.

use core::fmt::{self, Write};

use embedded_storage::nor_flash::ReadNorFlash;

use crate::Region;

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

// ============================================================================
// Build date
// ============================================================================

const MILLISECONDS_PER_DAY: u64 = 24 * 60 * 60 * 1000;

/// Writes the UTC day of a moment as `YYYY-MM-DD`, counting from 1970-01-01.
pub fn write_date(writer: &mut impl Write, unix_milliseconds: u64) -> fmt::Result {
    let (year, month, day) = civil_date((unix_milliseconds / MILLISECONDS_PER_DAY) as i64);
    write!(writer, "{year:04}-{month:02}-{day:02}")
}

/// The year, month and day of a day counted from 1970-01-01, by Howard Hinnant's `civil_from_days`.
fn civil_date(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_from_march + 2) / 5 + 1) as u32;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

// ============================================================================
// The bootloader's version record
// ============================================================================

/// Starts the record the bootloader carries, so the firmware can find it among the bootloader's bytes.
pub const VERSION_RECORD_MAGIC: [u8; 8] = *b"NTBootV1";
/// Room for the version text, such as `0.1.0+1b19aed*`, padded with zero bytes.
pub const RECORD_VERSION_BYTES: usize = 24;
/// Room for the build date, `YYYY-MM-DD`.
pub const RECORD_DATE_BYTES: usize = 10;
pub const VERSION_RECORD_BYTES: usize = VERSION_RECORD_MAGIC.len() + RECORD_VERSION_BYTES + RECORD_DATE_BYTES;

/// The bootloader's version and build date, as its record holds them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VersionRecord {
    bytes: [u8; VERSION_RECORD_BYTES],
}

impl VersionRecord {
    /// Builds the record a build script writes into the bootloader; text too long is cut short.
    pub fn new(version: &str, date: &str) -> VersionRecord {
        let mut bytes = [0; VERSION_RECORD_BYTES];
        let (magic, rest) = bytes.split_at_mut(VERSION_RECORD_MAGIC.len());
        let (version_bytes, date_bytes) = rest.split_at_mut(RECORD_VERSION_BYTES);
        magic.copy_from_slice(&VERSION_RECORD_MAGIC);
        copy_cut_short(version_bytes, version);
        copy_cut_short(date_bytes, date);
        VersionRecord { bytes }
    }

    pub fn bytes(&self) -> [u8; VERSION_RECORD_BYTES] {
        self.bytes
    }

    pub fn version(&self) -> &str {
        let start = VERSION_RECORD_MAGIC.len();
        text_before_padding(&self.bytes[start..start + RECORD_VERSION_BYTES])
    }

    pub fn date(&self) -> &str {
        text_before_padding(&self.bytes[VERSION_RECORD_BYTES - RECORD_DATE_BYTES..])
    }

    /// Looks through `region` of the flash for a record, which starts on a 4-byte boundary, as the firmware reads the bootloader's.
    pub fn find<Flash: ReadNorFlash>(
        flash: &mut Flash,
        region: Region,
    ) -> Result<Option<VersionRecord>, Flash::Error> {
        let mut offset = region.start;
        while offset + VERSION_RECORD_BYTES as u32 <= region.end() {
            let mut magic = [0; VERSION_RECORD_MAGIC.len()];
            flash.read(offset, &mut magic)?;
            if magic == VERSION_RECORD_MAGIC {
                let mut bytes = [0; VERSION_RECORD_BYTES];
                flash.read(offset, &mut bytes)?;
                return Ok(Some(VersionRecord { bytes }));
            }
            offset += RECORD_ALIGNMENT;
        }
        Ok(None)
    }
}

/// A record starts on a word boundary, so the search reads one place in four.
pub const RECORD_ALIGNMENT: u32 = 4;

fn copy_cut_short(into: &mut [u8], text: &str) {
    let length = text.len().min(into.len());
    into[..length].copy_from_slice(&text.as_bytes()[..length]);
}

/// The text up to the first zero byte, or nothing when the bytes are not UTF-8.
fn text_before_padding(bytes: &[u8]) -> &str {
    let end = bytes.iter().position(|&byte| byte == 0).unwrap_or(bytes.len());
    core::str::from_utf8(&bytes[..end]).unwrap_or("")
}
