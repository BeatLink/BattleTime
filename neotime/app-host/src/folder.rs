//! One folder of the watch's storage as an app reads it through the `Folder` system values.
//!
//! The store keeps only files, each by its full path, so a folder exists only through the files
//! inside it. A listing is built by handing it every file's path and length: the first part of the
//! path after the folder names the entry, which is a folder when more of the path follows. A folder
//! entry's size is the sum of every file inside it. Entries are kept in order, folders first and
//! then files, each by name, up to `MOST_FOLDER_ENTRIES` of them.

use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::Ordering;

use neotime_runtime::memory_need::{FOLDER_NAME_BYTES, LONGEST_FOLDER_PATH_BYTES, MOST_FOLDER_ENTRIES};
use neotime_runtime::{SystemValue, Value, NONE};

// ============================================================================
// Paths
// ============================================================================

/// The top folder of the storage.
pub const TOP_FOLDER: &str = "/";

/// A folder path written plainly: from the top, without empty or `.` parts, with each `..` taking
/// the part before it away and never climbing above the top.
///
/// Gives none when the result is longer than a listing keeps.
pub fn normalise(path: &str) -> Option<String> {
    let mut normal = String::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => normal.truncate(normal.rfind('/').unwrap_or(0)),
            part => {
                normal.push('/');
                normal.push_str(part);
            }
        }
    }
    if normal.is_empty() {
        normal.push_str(TOP_FOLDER);
    }
    (normal.len() <= LONGEST_FOLDER_PATH_BYTES).then_some(normal)
}

// ============================================================================
// The listing
// ============================================================================

/// One entry of the folder: a file, or a folder standing for the files inside it.
#[derive(Clone, Copy, Debug)]
struct FolderEntry {
    size_in_bytes: u32,
    /// Where its name starts in `FolderListing::names`, and how many bytes it takes.
    name_start: u16,
    name_length: u8,
    is_folder: bool,
}

/// The entries of the folder an app last opened, with their names kept together in one string.
#[derive(Debug, Default)]
pub struct FolderListing {
    path: String,
    entries: Vec<FolderEntry>,
    names: String,
}

impl FolderListing {
    pub const fn new() -> FolderListing {
        FolderListing {
            path: String::new(),
            entries: Vec::new(),
            names: String::new(),
        }
    }

    /// Starts listing the folder at `path` afresh, written plainly; false when the path is too long, which leaves it empty.
    pub fn begin(&mut self, path: &str) -> bool {
        self.clear();
        let Some(normal) = normalise(path) else {
            return false;
        };
        self.path = normal;
        self.entries = Vec::with_capacity(MOST_FOLDER_ENTRIES);
        self.names = String::with_capacity(FOLDER_NAME_BYTES);
        true
    }

    /// Forgets the listing and gives its memory back.
    pub fn clear(&mut self) {
        *self = FolderListing::new();
    }

    /// The folder being listed, such as `/apps`, or nothing before `begin`.
    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Counts one file of the store, which is left out when it lies outside the folder or the listing is full.
    pub fn add_file(&mut self, file_path: &str, length_in_bytes: usize) {
        let Some(inside) = self.inside_part(file_path) else {
            return;
        };
        let (name, is_folder) = match inside.split_once('/') {
            Some((folder_name, _)) => (folder_name, true),
            None => (inside, false),
        };
        if name.is_empty() {
            return;
        }
        let size_in_bytes = u32::try_from(length_in_bytes).unwrap_or(u32::MAX);
        match self.place_of(name, is_folder) {
            Ok(found) => {
                let entry = &mut self.entries[found];
                entry.size_in_bytes = entry.size_in_bytes.saturating_add(size_in_bytes);
            }
            Err(place) => self.insert(place, name, is_folder, size_in_bytes),
        }
    }

    /// The part of a file's path after the folder's, or none when the file lies outside it.
    fn inside_part<'path>(&self, file_path: &'path str) -> Option<&'path str> {
        if self.path.is_empty() {
            return None;
        }
        let rest = file_path.strip_prefix(self.path.as_str())?;
        match self.path.ends_with('/') {
            true => Some(rest),
            false => rest.strip_prefix('/'),
        }
    }

    /// Where an entry is, or where it would go to keep folders first and then names in order.
    fn place_of(&self, name: &str, is_folder: bool) -> Result<usize, usize> {
        self.entries
            .binary_search_by(|entry| match (entry.is_folder, is_folder) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => self.name_of(entry).cmp(name),
            })
    }

    fn insert(&mut self, place: usize, name: &str, is_folder: bool, size_in_bytes: u32) {
        let fits = self.entries.len() < MOST_FOLDER_ENTRIES
            && self.names.len() + name.len() <= FOLDER_NAME_BYTES
            && name.len() <= u8::MAX as usize;
        if !fits {
            return;
        }
        let entry = FolderEntry {
            size_in_bytes,
            name_start: self.names.len() as u16,
            name_length: name.len() as u8,
            is_folder,
        };
        self.names.push_str(name);
        self.entries.insert(place, entry);
    }

    fn name_of(&self, entry: &FolderEntry) -> &str {
        let start = entry.name_start as usize;
        self.names
            .get(start..start + entry.name_length as usize)
            .unwrap_or("")
    }

    fn entry(&self, index: Value) -> Option<&FolderEntry> {
        self.entries.get(usize::try_from(index).ok()?)
    }

    // ------------------------------------------------------------------------
    // The system values
    // ------------------------------------------------------------------------

    /// The number of entries, or none for a value the listing does not answer.
    pub fn read(&self, value: SystemValue) -> Option<Value> {
        (value == SystemValue::FolderEntryCount).then_some(self.entries.len() as Value)
    }

    /// The folder's path, or none for a value the listing does not answer.
    pub fn read_text(&self, value: SystemValue) -> Option<&str> {
        (value == SystemValue::FolderPath).then_some(self.path.as_str())
    }

    /// An entry's size or whether it is a folder, `NONE` past the last entry.
    pub fn read_element(&self, value: SystemValue, index: Value) -> Option<Value> {
        let entry = self.entry(index);
        match value {
            SystemValue::FolderEntrySize => Some(entry.map_or(NONE, |entry| entry.size_in_bytes as Value)),
            SystemValue::FolderEntryIsFolder => Some(entry.map_or(NONE, |entry| entry.is_folder as Value)),
            _ => None,
        }
    }

    /// An entry's name, empty past the last entry.
    pub fn read_element_text(&self, value: SystemValue, index: Value) -> Option<&str> {
        if value != SystemValue::FolderEntryName {
            return None;
        }
        Some(self.entry(index).map_or("", |entry| self.name_of(entry)))
    }
}

#[cfg(test)]
#[path = "folder_tests.rs"]
mod folder_tests;
