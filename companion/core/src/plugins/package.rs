// Reading a .ntplugin file: a zip holding manifest.json and the files it names.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use super::manifest::{is_package_path, Manifest};

// ============================================================================
// Limits
// ============================================================================

// The most a whole plugin may unpack to, so a hostile zip cannot fill the disk.
const MOST_UNPACKED_BYTES: u64 = 8 * 1024 * 1024;

// ============================================================================
// The package
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub manifest: Manifest,
    // Every file the plugin keeps, by its path inside the package, manifest.json included.
    pub files: BTreeMap<String, Vec<u8>>,
}

impl Package {
    pub fn read(bytes: &[u8]) -> Result<Package, String> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| format!("This is not a plugin file: {error}"))?;
        let mut files = BTreeMap::new();
        let mut unpacked = 0u64;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().map_err(|error| error.to_string())?.to_string();
            if !is_package_path(&name) {
                return Err(format!("The plugin holds a file outside itself: {name}"));
            }
            unpacked += entry.size();
            if unpacked > MOST_UNPACKED_BYTES {
                return Err("The plugin unpacks to more than 8 MB.".into());
            }
            let mut contents = Vec::with_capacity(entry.size() as usize);
            entry
                .read_to_end(&mut contents)
                .map_err(|error| error.to_string())?;
            files.insert(name, contents);
        }
        Package::from_files(files)
    }

    // Keeps the manifest, the files it names and the settings page's folder, and checks each is there.
    pub fn from_files(mut files: BTreeMap<String, Vec<u8>>) -> Result<Package, String> {
        let manifest_text = files
            .get("manifest.json")
            .ok_or("The plugin has no manifest.json.")?;
        let manifest = Manifest::parse(&String::from_utf8_lossy(manifest_text))?;
        for path in manifest.watch.iter().chain(&manifest.main).chain(&manifest.ui) {
            if !files.contains_key(path) {
                return Err(format!(
                    "manifest.json names {path}, which the plugin does not hold."
                ));
            }
        }
        let ui_folder = manifest.ui_folder().map(|folder| format!("{folder}/"));
        files.retain(|path, _| {
            path == "manifest.json"
                || manifest.watch.contains(path)
                || manifest.main.as_ref() == Some(path)
                || ui_folder
                    .as_ref()
                    .is_some_and(|folder| folder == "/" || path.starts_with(folder.as_str()))
        });
        Ok(Package { manifest, files })
    }

    pub fn main_source(&self) -> Option<String> {
        let path = self.manifest.main.as_ref()?;
        Some(String::from_utf8_lossy(self.files.get(path)?).into_owned())
    }

    // The watch app images, each with its file name.
    pub fn watch_apps(&self) -> Vec<(String, Vec<u8>)> {
        self.manifest
            .watch
            .iter()
            .filter_map(|path| {
                let name = path.rsplit('/').next().unwrap_or(path).to_string();
                Some((name, self.files.get(path)?.clone()))
            })
            .collect()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    // Zips the given files, for tests here and in the store and host.
    pub fn zipped(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut bytes);
        for (name, contents) in files {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap();
        bytes.into_inner()
    }

    const MANIFEST: &[u8] = br#"{"id":"demo","name":"Demo","version":"1","watch":["watch/demo.ntapp"],
        "main":"phone/main.js","ui":"phone/ui/index.html"}"#;

    #[test]
    fn a_package_keeps_what_its_manifest_names() {
        let package = Package::read(&zipped(&[
            ("manifest.json", MANIFEST),
            ("watch/demo.ntapp", b"NTAP"),
            ("phone/main.js", b"battletime.on('connect', () => {});"),
            ("phone/ui/index.html", b"<p>hi</p>"),
            ("phone/ui/style.css", b"p{}"),
            ("notes.txt", b"left out"),
        ]))
        .unwrap();
        assert_eq!(
            package.files.keys().collect::<Vec<_>>(),
            [
                "manifest.json",
                "phone/main.js",
                "phone/ui/index.html",
                "phone/ui/style.css",
                "watch/demo.ntapp"
            ]
        );
        assert_eq!(
            package.watch_apps(),
            vec![("demo.ntapp".to_string(), b"NTAP".to_vec())]
        );
        assert!(package.main_source().unwrap().contains("connect"));
    }

    #[test]
    fn a_missing_file_or_manifest_is_named() {
        assert!(Package::read(&zipped(&[("manifest.json", MANIFEST)]))
            .unwrap_err()
            .contains("watch/demo.ntapp"));
        assert!(Package::read(&zipped(&[("x", b"")]))
            .unwrap_err()
            .contains("no manifest.json"));
        assert!(Package::read(b"not a zip")
            .unwrap_err()
            .contains("not a plugin file"));
    }

    #[test]
    fn an_entry_outside_the_package_is_refused() {
        let bytes = zipped(&[
            ("manifest.json", br#"{"id":"x","name":"X","version":"1"}"#),
            ("../evil", b""),
        ]);
        assert!(Package::read(&bytes).unwrap_err().contains("outside itself"));
    }

    #[test]
    fn a_settings_page_at_the_top_keeps_every_file() {
        let package = Package::from_files(BTreeMap::from([
            (
                "manifest.json".to_string(),
                br#"{"id":"x","name":"X","version":"1","ui":"index.html"}"#.to_vec(),
            ),
            ("index.html".to_string(), b"".to_vec()),
            ("app.js".to_string(), b"".to_vec()),
        ]))
        .unwrap();
        assert_eq!(package.files.len(), 3);
        assert_eq!(package.main_source(), None);
    }
}
