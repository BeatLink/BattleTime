// The files every watch needs that are not apps, such as each accelerometer's configuration and the shared library's
// icons, which the firmware no longer carries in its shared library, the native modules and the shell's screens. The companion puts any a watch lacks; `npm run import-system-files` copies
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
pub const SYSTEM_FILES: [SystemFile; 8] = [
    SystemFile {
        path: "/system/accelerometer/bma421",
        contents: include_bytes!("../../system-files/accelerometer/bma421"),
    },
    SystemFile {
        path: "/system/accelerometer/bma425",
        contents: include_bytes!("../../system-files/accelerometer/bma425"),
    },
    SystemFile {
        path: "/system/icons",
        contents: include_bytes!("../../system-files/icons"),
    },
    SystemFile {
        path: "/system/modules/p256",
        contents: include_bytes!("../../system-files/modules/p256"),
    },
    SystemFile {
        path: "/system/modules/test",
        contents: include_bytes!("../../system-files/modules/test"),
    },
    SystemFile {
        path: "/system/modules/verifier",
        contents: include_bytes!("../../system-files/modules/verifier"),
    },
    SystemFile {
        path: "/system/shell/notifications/app.ntapp",
        contents: include_bytes!("../../system-files/shell/notifications/app.ntapp"),
    },
    SystemFile {
        path: "/system/shell/quick-settings/app.ntapp",
        contents: include_bytes!("../../system-files/shell/quick-settings/app.ntapp"),
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

    fn count_files(folder: &std::path::Path) -> usize {
        std::fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .map(|path| if path.is_dir() { count_files(&path) } else { 1 })
            .sum()
    }

    #[test]
    fn every_copied_system_file_is_listed() {
        let folder = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/system-files"));
        assert_eq!(count_files(folder), SYSTEM_FILES.len());
        for file in &SYSTEM_FILES {
            let copied = std::fs::read(folder.join(file.path.strip_prefix("/system/").unwrap())).unwrap();
            assert_eq!(copied, file.contents, "{}", file.path);
        }
        assert_eq!(SYSTEM_FILES[0].contents.len(), 6144);
        assert_eq!(&SYSTEM_FILES[2].contents[..2], &(72u16).to_le_bytes());
    }

    #[tokio::test]
    async fn a_watch_without_them_gets_every_one() {
        let watch = FakeWatch::with(&[]);
        let written = put_missing_system_files(&watch.client()).await.unwrap();

        assert_eq!(
            written,
            [
                "/system/accelerometer/bma421",
                "/system/accelerometer/bma425",
                "/system/icons",
                "/system/modules/p256",
                "/system/modules/test",
                "/system/modules/verifier",
                "/system/shell/notifications/app.ntapp",
                "/system/shell/quick-settings/app.ntapp"
            ]
        );
        let files = watch.files.lock().unwrap();
        assert_eq!(files["/system/accelerometer/bma425"], SYSTEM_FILES[1].contents);
    }

    #[tokio::test]
    async fn a_watch_that_holds_them_gets_nothing() {
        let watch = FakeWatch::with(&[
            ("/system/accelerometer/bma421", SYSTEM_FILES[0].contents),
            ("/system/accelerometer/bma425", SYSTEM_FILES[1].contents),
            ("/system/icons", SYSTEM_FILES[2].contents),
            ("/system/modules/p256", SYSTEM_FILES[3].contents),
            ("/system/modules/test", SYSTEM_FILES[4].contents),
            ("/system/modules/verifier", SYSTEM_FILES[5].contents),
            ("/system/shell/notifications/app.ntapp", SYSTEM_FILES[6].contents),
            ("/system/shell/quick-settings/app.ntapp", SYSTEM_FILES[7].contents),
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
            ("/system/icons", SYSTEM_FILES[2].contents),
            ("/system/modules/p256", SYSTEM_FILES[3].contents),
            ("/system/modules/test", SYSTEM_FILES[4].contents),
            ("/system/modules/verifier", SYSTEM_FILES[5].contents),
            ("/system/shell/notifications/app.ntapp", SYSTEM_FILES[6].contents),
            ("/system/shell/quick-settings/app.ntapp", SYSTEM_FILES[7].contents),
        ]);
        let written = put_missing_system_files(&watch.client()).await.unwrap();

        assert_eq!(written, ["/system/accelerometer/bma425"]);
        assert_eq!(
            watch.files.lock().unwrap()["/system/accelerometer/bma425"],
            SYSTEM_FILES[1].contents
        );
    }
}
