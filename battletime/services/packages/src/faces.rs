//! Which installed watch face the shell shows: the one the `watch_face` setting names, or else the first face turned on,
//! and the face a step on or back from it reaches.
//!
//! The setting holds a face's number, made from its id by `face_number`, because settings hold only
//! numbers. The companion app works out the same number from an id to choose a face.

use battletime_runtime::AppKind;

use crate::InstalledApp;

// ============================================================================
// A face's number
// ============================================================================

/// What the `watch_face` setting holds before any face is chosen.
pub const NO_FACE_CHOSEN: i32 = 0;

const FNV_OFFSET_BASIS: u32 = 0x811C_9DC5;
const FNV_PRIME: u32 = 0x0100_0193;
const POSITIVE_BITS: u32 = 0x7FFF_FFFF;

/// The number the `watch_face` setting holds for an id: its 32-bit FNV-1a hash with the top bit cleared, never 0.
pub fn face_number(id: &str) -> i32 {
    let hash = id.bytes().fold(FNV_OFFSET_BASIS, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(FNV_PRIME)
    });
    match (hash & POSITIVE_BITS) as i32 {
        NO_FACE_CHOSEN => 1,
        number => number,
    }
}

// ============================================================================
// The face to show
// ============================================================================

/// The ids of the faces turned on, in the list's order, which is all the shell keeps to pick a face.
pub fn faces_turned_on(packages: &[InstalledApp]) -> impl Iterator<Item = &str> + Clone {
    packages
        .iter()
        .filter(|app| app.enabled && app.kind == AppKind::Face)
        .map(|app| app.id.as_str())
}

/// The face whose number is `chosen_number` among `face_ids`, or else the first of them.
pub fn face_to_show<'ids>(
    mut face_ids: impl Iterator<Item = &'ids str> + Clone,
    chosen_number: i32,
) -> Option<&'ids str> {
    face_ids
        .clone()
        .find(|id| face_number(id) == chosen_number)
        .or_else(|| face_ids.next())
}

/// The face `steps` places on from `shown` among `face_ids`, wrapping round, counting from the first when `shown` is not among them.
pub fn stepped_face<'ids>(
    face_ids: impl Iterator<Item = &'ids str> + Clone,
    shown: &str,
    steps: i32,
) -> Option<&'ids str> {
    let count = face_ids.clone().count() as i64;
    if count == 0 {
        return None;
    }
    let place = face_ids.clone().position(|id| id == shown).unwrap_or(0) as i64;
    let stepped_place = (place + i64::from(steps)).rem_euclid(count);
    face_ids.into_iter().nth(stepped_place as usize)
}

#[cfg(test)]
#[path = "faces_tests.rs"]
mod faces_tests;
