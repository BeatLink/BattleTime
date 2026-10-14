use alloc::format;
use alloc::vec::Vec;

use super::*;

// ============================================================================
// Helpers
// ============================================================================

/// The listing of `path` over these files.
fn listing_of(path: &str, files: &[(&str, usize)]) -> FolderListing {
    let mut listing = FolderListing::new();
    assert!(listing.begin(path));
    for (file_path, length) in files {
        listing.add_file(file_path, *length);
    }
    listing
}

/// Each entry as its name, size and whether it is a folder, in the listing's order.
fn entries_of(listing: &FolderListing) -> Vec<(&str, Value, bool)> {
    (0..listing.entry_count() as Value)
        .map(|index| {
            (
                listing
                    .read_element_text(SystemValue::FolderEntryName, index)
                    .unwrap(),
                listing.read_element(SystemValue::FolderEntrySize, index).unwrap(),
                listing
                    .read_element(SystemValue::FolderEntryIsFolder, index)
                    .unwrap()
                    != 0,
            )
        })
        .collect()
}

const STORE: [(&str, usize); 6] = [
    ("/notes/note.txt", 90),
    ("/apps/timer/image", 4000),
    ("/apps/dice/image", 3000),
    ("/apps/dice/state", 12),
    ("/settings", 40),
    ("/a-file", 7),
];

// ============================================================================
// Paths
// ============================================================================

#[test]
fn a_path_is_written_plainly_from_the_top() {
    assert_eq!(normalise("").as_deref(), Some("/"));
    assert_eq!(normalise("/").as_deref(), Some("/"));
    assert_eq!(normalise("//apps/").as_deref(), Some("/apps"));
    assert_eq!(normalise("apps/./dice").as_deref(), Some("/apps/dice"));
    assert_eq!(normalise("/apps/dice/..").as_deref(), Some("/apps"));
    assert_eq!(normalise("/apps/..").as_deref(), Some("/"));
}

#[test]
fn going_up_from_the_top_stays_at_the_top() {
    assert_eq!(normalise("/..").as_deref(), Some("/"));
    assert_eq!(normalise("/../../apps").as_deref(), Some("/apps"));
}

#[test]
fn a_path_longer_than_a_listing_keeps_is_refused() {
    let longest = format!("/{}", "a".repeat(LONGEST_FOLDER_PATH_BYTES - 1));
    let too_long = format!("/{}", "a".repeat(LONGEST_FOLDER_PATH_BYTES));
    let mut listing = FolderListing::new();

    assert_eq!(normalise(&longest).as_deref(), Some(longest.as_str()));
    assert_eq!(normalise(&too_long), None);
    assert!(!listing.begin(&too_long));
    assert_eq!(listing.path(), "");
}

// ============================================================================
// Listing
// ============================================================================

#[test]
fn the_top_folder_lists_folders_first_then_files_each_by_name() {
    let listing = listing_of("/", &STORE);

    assert_eq!(listing.read_text(SystemValue::FolderPath), Some("/"));
    assert_eq!(listing.read(SystemValue::FolderEntryCount), Some(4));
    assert_eq!(
        entries_of(&listing),
        [
            ("apps", 7012, true),
            ("notes", 90, true),
            ("a-file", 7, false),
            ("settings", 40, false),
        ]
    );
}

#[test]
fn a_folder_inside_lists_only_what_lies_in_it() {
    let listing = listing_of("/apps/dice/../", &STORE);

    assert_eq!(listing.path(), "/apps");
    assert_eq!(
        entries_of(&listing),
        [("dice", 3012, true), ("timer", 4000, true)]
    );
}

#[test]
fn a_folder_of_files_gives_each_files_own_size() {
    let listing = listing_of("/apps/dice", &STORE);

    assert_eq!(
        entries_of(&listing),
        [("image", 3000, false), ("state", 12, false)]
    );
}

#[test]
fn a_file_whose_name_only_starts_like_the_folder_lies_outside_it() {
    let listing = listing_of("/apps", &[("/apps-old/image", 5), ("/apps/x", 1)]);

    assert_eq!(entries_of(&listing), [("x", 1, false)]);
}

#[test]
fn a_folder_with_no_files_lists_nothing() {
    let listing = listing_of("/missing", &STORE);

    assert_eq!(listing.read(SystemValue::FolderEntryCount), Some(0));
    assert_eq!(listing.read_element(SystemValue::FolderEntrySize, 0), Some(NONE));
    assert_eq!(
        listing.read_element_text(SystemValue::FolderEntryName, 0),
        Some("")
    );
}

#[test]
fn a_full_listing_leaves_out_the_rest() {
    let paths: Vec<_> = (0..MOST_FOLDER_ENTRIES + 3)
        .map(|index| format!("/f{index:03}"))
        .collect();
    let mut listing = FolderListing::new();
    listing.begin("/");
    for path in &paths {
        listing.add_file(path, 1);
    }

    assert_eq!(listing.entry_count(), MOST_FOLDER_ENTRIES);
    listing.add_file("/f000", 1);
    assert_eq!(listing.entry_count(), MOST_FOLDER_ENTRIES);
}

#[test]
fn names_beyond_the_name_room_are_left_out() {
    let mut listing = FolderListing::new();
    listing.begin("/");
    let name = "n".repeat(200);
    for index in 0..6 {
        listing.add_file(&format!("/{index}{name}"), 1);
    }

    assert_eq!(listing.entry_count(), FOLDER_NAME_BYTES / 201);
}

#[test]
fn clearing_forgets_the_folder() {
    let mut listing = listing_of("/", &STORE);

    listing.clear();

    assert_eq!(listing.path(), "");
    assert_eq!(listing.entry_count(), 0);
    listing.add_file("/settings", 40);
    assert_eq!(listing.entry_count(), 0);
}

#[test]
fn other_values_are_left_to_the_rest_of_the_system() {
    let listing = listing_of("/", &STORE);

    assert_eq!(listing.read(SystemValue::BatteryPercent), None);
    assert_eq!(listing.read_text(SystemValue::FirmwareVersion), None);
    assert_eq!(listing.read_element(SystemValue::TaskRuns, 0), None);
    assert_eq!(listing.read_element_text(SystemValue::TaskName, 0), None);
}
