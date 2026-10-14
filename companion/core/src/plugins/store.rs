// Installed plugins on disk: one folder each, holding its files, whether it is on, and its settings.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::manifest::{is_package_path, is_plugin_id, Manifest};
use super::package::Package;

// The files the store keeps beside a plugin's own, which its package may not overwrite.
const STATE_FILE: &str = ".state.json";
const SETTINGS_FILE: &str = ".settings.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPlugin {
    pub manifest: Manifest,
    pub enabled: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct State {
    enabled: bool,
}

pub struct PluginStore {
    root: PathBuf,
}

impl PluginStore {
    pub fn new(root: impl Into<PathBuf>) -> PluginStore {
        PluginStore { root: root.into() }
    }

    fn folder(&self, id: &str) -> Result<PathBuf, String> {
        if !is_plugin_id(id) {
            return Err(format!("\"{id}\" is not a plugin id."));
        }
        Ok(self.root.join(id))
    }

    // ========================================================================
    // Installing and removing
    // ========================================================================

    // Replaces any earlier version, keeping its settings, and leaves the plugin turned on.
    pub fn install(&self, package: &Package) -> Result<InstalledPlugin, String> {
        let folder = self.folder(&package.manifest.id)?;
        let settings = self.settings(&package.manifest.id).unwrap_or_default();
        let staging = self.root.join(format!(".installing-{}", package.manifest.id));
        let _ = fs::remove_dir_all(&staging);
        for (path, contents) in &package.files {
            let target = staging.join(path);
            fs::create_dir_all(target.parent().unwrap_or(&staging)).map_err(describe)?;
            fs::write(&target, contents).map_err(describe)?;
        }
        write_json(&staging.join(STATE_FILE), &State { enabled: true })?;
        write_json(&staging.join(SETTINGS_FILE), &settings)?;
        let _ = fs::remove_dir_all(&folder);
        fs::rename(&staging, &folder).map_err(describe)?;
        Ok(InstalledPlugin {
            manifest: package.manifest.clone(),
            enabled: true,
        })
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let folder = self.folder(id)?;
        if !folder.exists() {
            return Err(format!("No plugin \"{id}\" is installed."));
        }
        fs::remove_dir_all(folder).map_err(describe)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        let folder = self.folder(id)?;
        if !folder.join("manifest.json").exists() {
            return Err(format!("No plugin \"{id}\" is installed."));
        }
        write_json(&folder.join(STATE_FILE), &State { enabled })
    }

    // ========================================================================
    // Reading
    // ========================================================================

    // Every installed plugin, by id; a folder that no longer reads as a plugin is skipped.
    pub fn list(&self) -> Vec<InstalledPlugin> {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut found: Vec<InstalledPlugin> = entries
            .flatten()
            .filter_map(|entry| self.installed(&entry.file_name().to_string_lossy()).ok())
            .collect();
        found.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
        found
    }

    pub fn installed(&self, id: &str) -> Result<InstalledPlugin, String> {
        let folder = self.folder(id)?;
        let text = fs::read_to_string(folder.join("manifest.json"))
            .map_err(|_| format!("No plugin \"{id}\" is installed."))?;
        let manifest = Manifest::parse(&text)?;
        let state: State = read_json(&folder.join(STATE_FILE)).unwrap_or_default();
        Ok(InstalledPlugin {
            manifest,
            enabled: state.enabled,
        })
    }

    // The whole package as installed, for running it or installing its watch apps again.
    pub fn package(&self, id: &str) -> Result<Package, String> {
        let installed = self.installed(id)?;
        let folder = self.folder(id)?;
        let mut files = BTreeMap::new();
        collect_files(&folder, &folder, &mut files)?;
        files.retain(|path, _| path != STATE_FILE && path != SETTINGS_FILE);
        let package = Package::from_files(files)?;
        debug_assert_eq!(package.manifest, installed.manifest);
        Ok(package)
    }

    // One file of the plugin's settings page, for the window to show.
    pub fn ui_file(&self, id: &str, path: &str) -> Result<Vec<u8>, String> {
        let installed = self.installed(id)?;
        let folder = installed
            .manifest
            .ui_folder()
            .ok_or_else(|| format!("\"{id}\" has no settings page."))?;
        let inside = if folder.is_empty() {
            path.to_string()
        } else {
            format!("{folder}/{path}")
        };
        if !is_package_path(&inside) {
            return Err(format!("{path} is not a file of the settings page."));
        }
        fs::read(self.folder(id)?.join(inside)).map_err(|_| format!("The settings page has no {path}."))
    }

    // ========================================================================
    // Settings
    // ========================================================================

    pub fn settings(&self, id: &str) -> Result<Map<String, Value>, String> {
        Ok(read_json(&self.folder(id)?.join(SETTINGS_FILE)).unwrap_or_default())
    }

    pub fn save_settings(&self, id: &str, settings: &Map<String, Value>) -> Result<(), String> {
        let folder = self.folder(id)?;
        if !folder.exists() {
            return Err(format!("No plugin \"{id}\" is installed."));
        }
        write_json(&folder.join(SETTINGS_FILE), settings)
    }
}

// ============================================================================
// Files
// ============================================================================

fn describe(error: std::io::Error) -> String {
    error.to_string()
}

fn read_json<T: for<'a> Deserialize<'a>>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).map_err(describe)?;
    let staged = path.with_extension("json.new");
    fs::write(
        &staged,
        serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?,
    )
    .map_err(describe)?;
    fs::rename(&staged, path).map_err(describe)
}

fn collect_files(root: &Path, folder: &Path, files: &mut BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    for entry in fs::read_dir(folder).map_err(describe)?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files)?;
        } else if let Ok(inside) = path.strip_prefix(root) {
            let name = inside.to_string_lossy().replace('\\', "/");
            files.insert(name, fs::read(&path).map_err(describe)?);
        }
    }
    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::package::tests::zipped;
    use serde_json::json;

    fn demo(version: &str) -> Package {
        let manifest = format!(
            r#"{{"id":"demo","name":"Demo","version":"{version}","watch":["watch/demo.ntapp"],"ui":"ui/index.html"}}"#
        );
        Package::read(&zipped(&[
            ("manifest.json", manifest.as_bytes()),
            ("watch/demo.ntapp", b"NTAP"),
            ("ui/index.html", b"<p>page</p>"),
        ]))
        .unwrap()
    }

    #[test]
    fn an_installed_plugin_is_listed_turned_on_and_reads_back_whole() {
        let root = tempfile::tempdir().unwrap();
        let store = PluginStore::new(root.path());
        store.install(&demo("1")).unwrap();
        let listed = store.list();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].enabled);
        assert_eq!(store.package("demo").unwrap(), demo("1"));
        assert_eq!(store.ui_file("demo", "index.html").unwrap(), b"<p>page</p>");
    }

    #[test]
    fn an_update_keeps_the_settings_and_a_removal_drops_everything() {
        let root = tempfile::tempdir().unwrap();
        let store = PluginStore::new(root.path());
        store.install(&demo("1")).unwrap();
        store
            .save_settings("demo", &json!({"city": "Kingston"}).as_object().unwrap().clone())
            .unwrap();
        store.set_enabled("demo", false).unwrap();
        store.install(&demo("2")).unwrap();
        assert_eq!(store.installed("demo").unwrap().manifest.version, "2");
        assert_eq!(store.settings("demo").unwrap()["city"], "Kingston");
        store.remove("demo").unwrap();
        assert!(store.list().is_empty());
        assert!(store.remove("demo").unwrap_err().contains("No plugin"));
    }

    #[test]
    fn turning_off_is_remembered() {
        let root = tempfile::tempdir().unwrap();
        let store = PluginStore::new(root.path());
        store.install(&demo("1")).unwrap();
        store.set_enabled("demo", false).unwrap();
        assert!(!store.installed("demo").unwrap().enabled);
        assert!(store.set_enabled("other", true).is_err());
    }

    #[test]
    fn the_settings_page_cannot_reach_other_files() {
        let root = tempfile::tempdir().unwrap();
        let store = PluginStore::new(root.path());
        store.install(&demo("1")).unwrap();
        assert!(store
            .ui_file("demo", "../watch/demo.ntapp")
            .unwrap_err()
            .contains("not a file"));
        assert!(store
            .ui_file("demo", "missing.css")
            .unwrap_err()
            .contains("no missing.css"));
        assert!(store.ui_file("../x", "index.html").is_err());
    }

    #[test]
    fn a_broken_folder_is_skipped_and_nothing_is_installed_in_an_empty_store() {
        let root = tempfile::tempdir().unwrap();
        let store = PluginStore::new(root.path().join("missing"));
        assert!(store.list().is_empty());
        let store = PluginStore::new(root.path());
        fs::create_dir_all(root.path().join("junk")).unwrap();
        assert!(store.list().is_empty());
        assert!(store.save_settings("junk-two", &Map::new()).is_err());
    }
}
