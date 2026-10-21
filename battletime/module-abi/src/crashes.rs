//! How often each module has crashed the watch, kept in `/system/modules/crashes`, so one that keeps crashing is no longer run.
//!
//! A crash while a module runs is recorded with the module's name where the crash record names the
//! open app. The next start counts it here once, by the crash record's CRC, against the module file
//! that was there, so a new copy of the module starts again from no crashes.

use crate::NAME_BYTES;

/// After this many crashes the module is not run again until its file changes.
pub const MOST_CRASHES: u8 = 2;

/// The modules whose crashes are kept; the oldest is forgotten to make room.
pub const MOST_MODULES: usize = 8;

const ENTRY_BYTES: usize = NAME_BYTES + 4 + 1;
pub const FILE_BYTES: usize = 4 + 1 + MOST_MODULES * ENTRY_BYTES;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Entry {
    name: [u8; NAME_BYTES],
    module_crc: u32,
    crashes: u8,
}

/// The crashes of each module, and which crash record was counted last.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CrashCounts {
    last_counted_record: u32,
    entries: [Entry; MOST_MODULES],
    entry_count: usize,
}

fn name_bytes(name: &str) -> [u8; NAME_BYTES] {
    let mut bytes = [0; NAME_BYTES];
    let length = name.len().min(NAME_BYTES);
    bytes[..length].copy_from_slice(&name.as_bytes()[..length]);
    bytes
}

impl CrashCounts {
    /// The counts a file holds; a missing or damaged file counts no crashes.
    pub fn parse(bytes: &[u8]) -> CrashCounts {
        let mut counts = CrashCounts::default();
        if bytes.len() < 5 {
            return counts;
        }
        counts.last_counted_record = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let stored = (bytes[4] as usize).min(MOST_MODULES);
        for (place, entry) in bytes[5..]
            .as_chunks::<ENTRY_BYTES>()
            .0
            .iter()
            .take(stored)
            .enumerate()
        {
            let mut name = [0; NAME_BYTES];
            name.copy_from_slice(&entry[..NAME_BYTES]);
            let crc_bytes = &entry[NAME_BYTES..NAME_BYTES + 4];
            counts.entries[place] = Entry {
                name,
                module_crc: u32::from_le_bytes([crc_bytes[0], crc_bytes[1], crc_bytes[2], crc_bytes[3]]),
                crashes: entry[NAME_BYTES + 4],
            };
            counts.entry_count = place + 1;
        }
        counts
    }

    pub fn encode(&self) -> [u8; FILE_BYTES] {
        let mut bytes = [0; FILE_BYTES];
        bytes[..4].copy_from_slice(&self.last_counted_record.to_le_bytes());
        bytes[4] = self.entry_count as u8;
        for (entry, out) in self.entries[..self.entry_count]
            .iter()
            .zip(bytes[5..].as_chunks_mut::<ENTRY_BYTES>().0.iter_mut())
        {
            out[..NAME_BYTES].copy_from_slice(&entry.name);
            out[NAME_BYTES..NAME_BYTES + 4].copy_from_slice(&entry.module_crc.to_le_bytes());
            out[NAME_BYTES + 4] = entry.crashes;
        }
        bytes
    }

    /// Counts a crash of `name`, whose file's CRC is `module_crc`, unless the crash record `record_crc` was counted before; says whether it counted.
    pub fn count(&mut self, name: &str, module_crc: u32, record_crc: u32) -> bool {
        if record_crc == self.last_counted_record {
            return false;
        }
        self.last_counted_record = record_crc;
        let name = name_bytes(name);
        let found = self.entries[..self.entry_count]
            .iter()
            .position(|entry| entry.name == name);
        let place = match found {
            Some(place) => place,
            None if self.entry_count < MOST_MODULES => {
                self.entry_count += 1;
                self.entry_count - 1
            }
            None => {
                self.entries.copy_within(1.., 0);
                MOST_MODULES - 1
            }
        };
        let entry = &mut self.entries[place];
        let crashes = match found.is_some() && entry.module_crc == module_crc {
            true => entry.crashes.saturating_add(1),
            false => 1,
        };
        *entry = Entry {
            name,
            module_crc,
            crashes,
        };
        true
    }

    /// Whether the module of `name`, with this file, has crashed too often to run.
    pub fn is_stopped(&self, name: &str, module_crc: u32) -> bool {
        let name = name_bytes(name);
        self.entries[..self.entry_count].iter().any(|entry| {
            entry.name == name && entry.module_crc == module_crc && entry.crashes >= MOST_CRASHES
        })
    }
}

// ============================================================================
// Naming the module in the crash record
// ============================================================================

/// What the crash record's open-app id starts with while a module runs; its name follows.
pub const CRASH_TAG: &str = "mod:";

/// The open-app id a crash of the module `name` records, cut to fit the record's 16 bytes.
pub fn crash_tag(name: &str) -> [u8; NAME_BYTES] {
    let mut tag = [0; NAME_BYTES];
    let length = (CRASH_TAG.len() + name.len()).min(NAME_BYTES);
    tag[..CRASH_TAG.len()].copy_from_slice(CRASH_TAG.as_bytes());
    tag[CRASH_TAG.len()..length].copy_from_slice(&name.as_bytes()[..length - CRASH_TAG.len()]);
    tag
}

/// The module a crash record's open-app id names, if a module was running.
pub fn module_of_crash(open_app_id: &str) -> Option<&str> {
    open_app_id
        .strip_prefix(CRASH_TAG)
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
#[path = "crashes_tests.rs"]
mod crashes_tests;
