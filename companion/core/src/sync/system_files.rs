// The files every watch needs that are not apps, such as each accelerometer's configuration and the shared library's
// icons and the text font, which the firmware no longer carries, the native modules and the shell's screens. The companion puts any a watch lacks; `npm run import-system-files` copies
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
pub const SYSTEM_FILES: [SystemFile; 9] = [
    SystemFile {
        path: "/system/accelerometer/bma421",
        contents: include_bytes!("../../system-files/accelerometer/bma421"),
    },
    SystemFile {
        path: "/system/accelerometer/bma425",
        contents: include_bytes!("../../system-files/accelerometer/bma425"),
    },
    SystemFile {
        path: "/system/font",
        contents: include_bytes!("../../system-files/font"),
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

/// Where the companion keeps the CRC-32 of each system file it put, since the file service lists only names and sizes.
pub const SYSTEM_MANIFEST_PATH: &str = "/system/manifest";

// The CRC-32 zlib uses, as the window's `crc32` does.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                0xEDB8_8320 ^ (crc >> 1)
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

// The manifest's line for one file: its CRC-32 in hex, its size and its path, as src/protocol/system-files.ts writes it.
fn manifest_line(file: &SystemFile) -> String {
    format!(
        "{:08x} {} {}",
        crc32(file.contents),
        file.contents.len(),
        file.path
    )
}

fn manifest_of(bundle: &[SystemFile]) -> String {
    bundle.iter().map(|file| manifest_line(file) + "\n").collect()
}

// Writes each system file the watch lacks or holds in another version, then the manifest, giving the paths it wrote.
//
// A file counts as held only while the manifest names it with the bundle's CRC and the listing with its size, so a
// watch without the manifest gets every file once.
pub async fn put_missing_system_files(files: &FileClient) -> Result<Vec<&'static str>, FileError> {
    put_from(files, &SYSTEM_FILES).await
}

async fn put_from(files: &FileClient, bundle: &'static [SystemFile]) -> Result<Vec<&'static str>, FileError> {
    let on_watch = files
        .read(SYSTEM_MANIFEST_PATH)
        .await
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default();
    let held: std::collections::HashSet<&str> = on_watch.lines().collect();
    let mut written = Vec::new();
    for file in bundle {
        let (folder, name) = file.path.rsplit_once('/').unwrap_or(("/", file.path));
        // A folder the watch has never had lists as missing, which means the file is too.
        let entries = files.list(folder).await.unwrap_or_default();
        let listed = entries
            .iter()
            .any(|entry| entry.name == name && !entry.folder && entry.size as usize == file.contents.len());
        if !listed || !held.contains(manifest_line(file).as_str()) {
            files.write(file.path, file.contents).await?;
            written.push(file.path);
        }
    }
    let manifest = manifest_of(bundle);
    if manifest != on_watch {
        files.write(SYSTEM_MANIFEST_PATH, manifest.as_bytes()).await?;
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
        assert_eq!(&SYSTEM_FILES[2].contents[..4], b"NTGF");
        assert_eq!(&SYSTEM_FILES[3].contents[..2], &(72u16).to_le_bytes());
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
                "/system/font",
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

    #[test]
    fn the_crc_is_zlibs() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    const SMALL_BUNDLE: [SystemFile; 2] = [
        SystemFile {
            path: "/system/accelerometer/bma421",
            contents: &[1, 2, 3],
        },
        SystemFile {
            path: "/system/accelerometer/bma425",
            contents: &[4, 5, 6, 7],
        },
    ];

    #[tokio::test]
    async fn a_file_changed_at_the_same_size_is_written_again_and_then_held() {
        let old: [SystemFile; 2] = [
            SystemFile {
                path: "/system/accelerometer/bma421",
                contents: &[1, 2, 3],
            },
            SystemFile {
                path: "/system/accelerometer/bma425",
                contents: &[4, 5, 6, 0],
            },
        ];
        let manifest = manifest_of(&old);
        let watch = FakeWatch::with(&[
            ("/system/accelerometer/bma421", &[1, 2, 3]),
            ("/system/accelerometer/bma425", &[4, 5, 6, 0]),
            (SYSTEM_MANIFEST_PATH, manifest.as_bytes()),
        ]);

        let client = watch.client();
        let first = put_from(&client, &SMALL_BUNDLE).await.unwrap();
        let second = put_from(&client, &SMALL_BUNDLE).await.unwrap();

        assert_eq!(first, ["/system/accelerometer/bma425"]);
        assert!(second.is_empty());
        assert_eq!(
            watch.files.lock().unwrap()[SYSTEM_MANIFEST_PATH],
            manifest_of(&SMALL_BUNDLE).as_bytes()
        );
    }

    #[tokio::test]
    async fn a_watch_without_the_manifest_gets_every_file_once() {
        let watch = FakeWatch::with(&[
            ("/system/accelerometer/bma421", &[1, 2, 3]),
            ("/system/accelerometer/bma425", &[4, 5, 6, 7]),
        ]);

        let client = watch.client();
        let first = put_from(&client, &SMALL_BUNDLE).await.unwrap();
        let second = put_from(&client, &SMALL_BUNDLE).await.unwrap();

        assert_eq!(first.len(), 2);
        assert!(second.is_empty());
    }

    #[tokio::test]
    async fn a_watch_that_holds_them_gets_nothing() {
        let manifest = manifest_of(&SYSTEM_FILES);
        let watch = FakeWatch::with(&[
            (SYSTEM_MANIFEST_PATH, manifest.as_bytes()),
            ("/system/accelerometer/bma421", SYSTEM_FILES[0].contents),
            ("/system/accelerometer/bma425", SYSTEM_FILES[1].contents),
            ("/system/font", SYSTEM_FILES[2].contents),
            ("/system/icons", SYSTEM_FILES[3].contents),
            ("/system/modules/p256", SYSTEM_FILES[4].contents),
            ("/system/modules/test", SYSTEM_FILES[5].contents),
            ("/system/modules/verifier", SYSTEM_FILES[6].contents),
            ("/system/shell/notifications/app.ntapp", SYSTEM_FILES[7].contents),
            ("/system/shell/quick-settings/app.ntapp", SYSTEM_FILES[8].contents),
        ]);
        let written = put_missing_system_files(&watch.client()).await.unwrap();

        assert!(written.is_empty());
        assert!(!watch.commands.lock().unwrap().contains(&0x20));
    }

    #[tokio::test]
    async fn a_file_of_another_size_is_written_again() {
        let manifest = manifest_of(&SYSTEM_FILES);
        let watch = FakeWatch::with(&[
            (SYSTEM_MANIFEST_PATH, manifest.as_bytes()),
            ("/system/accelerometer/bma421", SYSTEM_FILES[0].contents),
            ("/system/accelerometer/bma425", b"cut short"),
            ("/system/font", SYSTEM_FILES[2].contents),
            ("/system/icons", SYSTEM_FILES[3].contents),
            ("/system/modules/p256", SYSTEM_FILES[4].contents),
            ("/system/modules/test", SYSTEM_FILES[5].contents),
            ("/system/modules/verifier", SYSTEM_FILES[6].contents),
            ("/system/shell/notifications/app.ntapp", SYSTEM_FILES[7].contents),
            ("/system/shell/quick-settings/app.ntapp", SYSTEM_FILES[8].contents),
        ]);
        let written = put_missing_system_files(&watch.client()).await.unwrap();

        assert_eq!(written, ["/system/accelerometer/bma425"]);
        assert_eq!(
            watch.files.lock().unwrap()["/system/accelerometer/bma425"],
            SYSTEM_FILES[1].contents
        );
    }
}
