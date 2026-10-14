// Files the user chooses in the file dialog: a path on a computer, a content:// link on Android.

use std::str::FromStr;

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::{DialogExt, FilePath};
use tauri_plugin_fs::FsExt;

// One kind of file the dialog offers, by its extensions; Android's picker has no such filters.
pub struct FileKind<'a> {
    pub name: &'a str,
    pub extensions: &'a [&'a str],
}

// Asks for one file; none when the dialog is cancelled.
pub async fn pick_file<R: Runtime>(
    app: &AppHandle<R>,
    title: &str,
    kinds: &[FileKind<'_>],
) -> Option<FilePath> {
    let (chosen_sender, chosen) = tokio::sync::oneshot::channel();
    let mut dialog = app.dialog().file().set_title(title);
    if cfg!(desktop) {
        for kind in kinds {
            dialog = dialog.add_filter(kind.name, kind.extensions);
        }
    }
    dialog.pick_file(move |file| {
        let _ = chosen_sender.send(file);
    });
    chosen.await.ok().flatten()
}

// The file's name as the user knows it, which Android looks up for a content:// link.
pub fn file_name<R: Runtime>(app: &AppHandle<R>, file: &FilePath) -> String {
    app.path().file_name(&file.to_string()).unwrap_or_default()
}

// Reads a chosen file again, by the text `FilePath` writes it as.
pub fn read_file<R: Runtime>(app: &AppHandle<R>, file: &str) -> Result<Vec<u8>, String> {
    let file_path = FilePath::from_str(file).unwrap_or_else(|never| match never {});
    app.fs()
        .read(file_path)
        .map_err(|error| format!("Could not read {file}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_read_and_named() {
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_fs::init())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let folder = tempfile::tempdir().unwrap();
        let note = folder.path().join("shopping.txt");
        std::fs::write(&note, "Milk").unwrap();

        let text = note.to_string_lossy().into_owned();
        assert_eq!(read_file(app.handle(), &text).unwrap(), b"Milk");
        assert_eq!(file_name(app.handle(), &FilePath::Path(note)), "shopping.txt");
        assert!(read_file(app.handle(), "/no/such/note.txt")
            .unwrap_err()
            .contains("/no/such/note.txt"));
    }
}
