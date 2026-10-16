// The files every watch needs that are not apps, such as each accelerometer's configuration, which the firmware no
// longer carries in its shared library. The companion puts any a watch lacks; `npm run import-system-files` copies
// them into `core/system-files/` from the store folder `tools/build_store.py` fills.

use super::files::{FileClient, FileError};

// ============================================================================
// The files
// ============================================================================

pub struct SystemFile {
    pub path: &'static str,
    pub contents: &'static [u8],
}

// Each at its path on the watch, as docs/companion-protocol.md lists them.
pub const SYSTEM_FILES: [SystemFile; 2] = [
    SystemFile {
        path: "/system/accelerometer/bma421",
        contents: include_bytes!("../../system-files/accelerometer/bma421"),
    },
    SystemFile {
        path: "/system/accelerometer/bma425",
        contents: include_bytes!("../../system-files/accelerometer/bma425"),
    },
];

// ============================================================================
// Putting them on the watch
// ============================================================================

// Writes each system file the watch lacks or holds at another size, giving the paths it wrote.
pub async fn put_missing_system_files(files: &FileClient) -> Result<Vec<&'static str>, FileError> {
    let mut written = Vec::new();
    for file in &SYSTEM_FILES {
        let (folder, name) = file.path.rsplit_once('/').unwrap_or(("/", file.path));
        // A folder the watch has never had lists as missing, which means the file is too.
        let entries = files.list(folder).await.unwrap_or_default();
        let held = entries
            .iter()
            .any(|entry| entry.name == name && !entry.folder && entry.size as usize == file.contents.len());
        if !held {
            files.write(file.path, file.contents).await?;
            written.push(file.path);
        }
    }
    Ok(written)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::super::files::fake::FakeWatch;
    use super::*;

    #[test]
    fn every_copied_system_file_is_listed() {
        let copied = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/system-files/accelerometer"))
            .unwrap()
            .count();
        assert_eq!(copied, SYSTEM_FILES.len());
        assert!(SYSTEM_FILES.iter().all(|file| file.contents.len() == 6144));
    }

    #[tokio::test]
    async fn a_watch_without_them_gets_every_one() {
        let watch = FakeWatch::with(&[]);
        let written = put_missing_system_files(&watch.client()).await.unwrap();

        assert_eq!(
            written,
            ["/system/accelerometer/bma421", "/system/accelerometer/bma425"]
        );
        let files = watch.files.lock().unwrap();
        assert_eq!(files["/system/accelerometer/bma425"], SYSTEM_FILES[1].contents);
    }

    #[tokio::test]
    async fn a_watch_that_holds_them_gets_nothing() {
        let watch = FakeWatch::with(&[
            ("/system/accelerometer/bma421", SYSTEM_FILES[0].contents),
            ("/system/accelerometer/bma425", SYSTEM_FILES[1].contents),
        ]);
        let written = put_missing_system_files(&watch.client()).await.unwrap();

        assert!(written.is_empty());
        assert!(!watch.commands.lock().unwrap().contains(&0x20));
    }

    #[tokio::test]
    async fn a_file_of_another_size_is_written_again() {
        let watch = FakeWatch::with(&[
            ("/system/accelerometer/bma421", SYSTEM_FILES[0].contents),
            ("/system/accelerometer/bma425", b"cut short"),
        ]);
        let written = put_missing_system_files(&watch.client()).await.unwrap();

        assert_eq!(written, ["/system/accelerometer/bma425"]);
        assert_eq!(
            watch.files.lock().unwrap()["/system/accelerometer/bma425"],
            SYSTEM_FILES[1].contents
        );
    }
}
