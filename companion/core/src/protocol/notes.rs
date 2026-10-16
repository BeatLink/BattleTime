// The note the watch's notes app shows: a text file kept at /notes/note.txt, and the font it is drawn in.
//
// docs/companion-protocol.md describes both files, and `src/protocol/notes.ts` is the window's copy of
// this code. They travel over the file service, and each is written only when the watch does not
// already hold it.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::sync::files::FileClient;

// ============================================================================
// The files
// ============================================================================

pub const NOTE_DIRECTORY: &str = "/notes";
pub const NOTE_PATH: &str = "/notes/note.txt";
pub const NOTE_FONT_PATH: &str = "/notes/font";

// A bigger note takes minutes to send, and the watch shows only about 2,000 lines of one.
pub const LARGEST_NOTE_BYTES: usize = 64 * 1024;

const FINGERPRINT_OFFSET_BASIS: u32 = 0x811c_9dc5;
const FINGERPRINT_PRIME: u32 = 0x0100_0193;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteFont {
    Small,
    #[default]
    Normal,
    Medium,
    Large,
}

impl NoteFont {
    pub fn as_str(self) -> &'static str {
        match self {
            NoteFont::Small => "small",
            NoteFont::Normal => "normal",
            NoteFont::Medium => "medium",
            NoteFont::Large => "large",
        }
    }

    pub fn bytes(self) -> &'static [u8] {
        self.as_str().as_bytes()
    }
}

// The refusal for a note the watch cannot take, worded as the window words it.
pub fn note_too_large(bytes: usize) -> String {
    format!(
        "The note is {} kB; the watch takes up to {} kB.",
        bytes.div_ceil(1024),
        LARGEST_NOTE_BYTES / 1024
    )
}

// ============================================================================
// What the watch holds
// ============================================================================

// What was last sent to a watch, so an unchanged note is not sent again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteOnWatch {
    pub watch_id: String,
    pub fingerprint: String,
    pub font: NoteFont,
}

// A short stand-in for the note's contents: its length and a 32-bit FNV-1a hash.
pub fn fingerprint(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(FINGERPRINT_OFFSET_BASIS, |hash, &byte| {
        (hash ^ u32::from(byte)).wrapping_mul(FINGERPRINT_PRIME)
    });
    format!("{}:{hash:08x}", bytes.len())
}

// ============================================================================
// Sending
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSyncResult {
    pub on_watch: NoteOnWatch,
    pub sent_note: bool,
    pub sent_font: bool,
}

// The sizes of the files in /notes on the watch, or none when the folder is missing.
async fn sizes_on_watch(files: &FileClient) -> HashMap<String, usize> {
    let Ok(entries) = files.list(NOTE_DIRECTORY).await else {
        return HashMap::new();
    };
    entries
        .into_iter()
        .filter(|entry| !entry.folder)
        .map(|entry| (format!("{NOTE_DIRECTORY}/{}", entry.name), entry.size as usize))
        .collect()
}

// Writes the note and its font, each only when it differs from `last` or, with `check`, from the files listed on the watch.
pub async fn sync_note(
    files: &FileClient,
    note: &[u8],
    font: NoteFont,
    watch_id: &str,
    last: Option<&NoteOnWatch>,
    check: bool,
) -> Result<NoteSyncResult, String> {
    if note.len() > LARGEST_NOTE_BYTES {
        return Err(note_too_large(note.len()));
    }
    let print = fingerprint(note);
    let same_watch = last.filter(|last| last.watch_id == watch_id);
    let sizes = if check {
        Some(sizes_on_watch(files).await)
    } else {
        None
    };
    let size_matches = |path: &str, length: usize| {
        sizes
            .as_ref()
            .is_none_or(|sizes| sizes.get(path) == Some(&length))
    };
    let note_there =
        same_watch.is_some_and(|last| last.fingerprint == print) && size_matches(NOTE_PATH, note.len());
    let font_there =
        same_watch.is_some_and(|last| last.font == font) && size_matches(NOTE_FONT_PATH, font.bytes().len());

    if !note_there {
        files
            .write(NOTE_PATH, note)
            .await
            .map_err(|error| error.to_string())?;
    }
    if !font_there {
        files
            .write(NOTE_FONT_PATH, font.bytes())
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(NoteSyncResult {
        on_watch: NoteOnWatch {
            watch_id: watch_id.to_string(),
            fingerprint: print,
            font,
        },
        sent_note: !note_there,
        sent_font: !font_there,
    })
}

// Takes the note off the watch, so its notes app asks for one again.
pub async fn remove_note(files: &FileClient) -> Result<(), String> {
    files.remove(NOTE_PATH).await.map_err(|error| error.to_string())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::files::fake::FakeWatch;

    fn sent_to(watch_id: &str, note: &[u8], font: NoteFont) -> NoteOnWatch {
        NoteOnWatch {
            watch_id: watch_id.into(),
            fingerprint: fingerprint(note),
            font,
        }
    }

    #[test]
    fn the_fingerprint_is_the_length_and_an_fnv_hash_as_the_window_writes_it() {
        assert_eq!(fingerprint(b""), "0:811c9dc5");
        assert_eq!(fingerprint(b"a"), "1:e40c292c");
        assert_eq!(fingerprint(b"hello"), "5:4f9f2cab");
    }

    #[test]
    fn a_note_too_large_is_measured_in_whole_kilobytes() {
        assert_eq!(
            note_too_large(65 * 1024 + 1),
            "The note is 66 kB; the watch takes up to 64 kB."
        );
    }

    #[tokio::test]
    async fn a_first_sync_writes_the_note_and_its_font() {
        let watch = FakeWatch::with(&[]);
        let client = watch.client();

        let result = sync_note(&client, b"hello", NoteFont::Large, "AA", None, true)
            .await
            .unwrap();

        assert!(result.sent_note && result.sent_font);
        assert_eq!(result.on_watch, sent_to("AA", b"hello", NoteFont::Large));
        let files = watch.files.lock().unwrap();
        assert_eq!(files[NOTE_PATH], b"hello");
        assert_eq!(files[NOTE_FONT_PATH], b"large");
    }

    #[tokio::test]
    async fn an_unchanged_note_is_not_sent_again() {
        let watch = FakeWatch::with(&[(NOTE_PATH, b"hello"), (NOTE_FONT_PATH, b"normal")]);
        let client = watch.client();
        let last = sent_to("AA", b"hello", NoteFont::Normal);

        let checked = sync_note(&client, b"hello", NoteFont::Normal, "AA", Some(&last), true)
            .await
            .unwrap();
        let trusted = sync_note(&client, b"hello", NoteFont::Normal, "AA", Some(&last), false)
            .await
            .unwrap();

        assert!(!checked.sent_note && !checked.sent_font);
        assert!(!trusted.sent_note && !trusted.sent_font);
    }

    #[tokio::test]
    async fn only_the_part_that_changed_is_sent() {
        let watch = FakeWatch::with(&[]);
        let client = watch.client();
        let last = sent_to("AA", b"hello", NoteFont::Normal);

        let result = sync_note(&client, b"hello", NoteFont::Small, "AA", Some(&last), false)
            .await
            .unwrap();

        assert!(!result.sent_note && result.sent_font);
        assert!(!watch.files.lock().unwrap().contains_key(NOTE_PATH));
    }

    #[tokio::test]
    async fn a_note_removed_on_the_watch_is_sent_again_when_checked() {
        let watch = FakeWatch::with(&[(NOTE_FONT_PATH, b"normal")]);
        let client = watch.client();
        let last = sent_to("AA", b"hello", NoteFont::Normal);

        let result = sync_note(&client, b"hello", NoteFont::Normal, "AA", Some(&last), true)
            .await
            .unwrap();

        assert!(result.sent_note && !result.sent_font);
    }

    #[tokio::test]
    async fn another_watch_gets_everything() {
        let watch = FakeWatch::with(&[(NOTE_PATH, b"hello"), (NOTE_FONT_PATH, b"normal")]);
        let client = watch.client();
        let last = sent_to("AA", b"hello", NoteFont::Normal);

        let result = sync_note(&client, b"hello", NoteFont::Normal, "BB", Some(&last), false)
            .await
            .unwrap();

        assert!(result.sent_note && result.sent_font);
        assert_eq!(result.on_watch.watch_id, "BB");
    }

    #[tokio::test]
    async fn a_note_too_large_never_reaches_the_watch() {
        let watch = FakeWatch::with(&[]);
        let client = watch.client();
        let note = vec![b'x'; LARGEST_NOTE_BYTES + 1];

        let error = sync_note(&client, &note, NoteFont::Normal, "AA", None, true)
            .await
            .unwrap_err();

        assert_eq!(error, note_too_large(note.len()));
        assert!(watch.commands.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn removing_the_note_leaves_its_font() {
        let watch = FakeWatch::with(&[(NOTE_PATH, b"hello"), (NOTE_FONT_PATH, b"normal")]);
        let client = watch.client();

        remove_note(&client).await.unwrap();

        let files = watch.files.lock().unwrap();
        assert!(!files.contains_key(NOTE_PATH));
        assert!(files.contains_key(NOTE_FONT_PATH));
    }

    #[test]
    fn what_was_sent_reads_and_writes_the_windows_shape() {
        let on_watch = sent_to("AA", b"hi", NoteFont::Medium);
        let value = serde_json::to_value(&on_watch).unwrap();

        assert_eq!(
            value,
            serde_json::json!({ "watchId": "AA", "fingerprint": fingerprint(b"hi"), "font": "medium" })
        );
        assert_eq!(serde_json::from_value::<NoteOnWatch>(value).unwrap(), on_watch);
    }
}
