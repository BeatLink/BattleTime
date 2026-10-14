// A plugin's manifest.json: who it is, what it carries and what it may do.

use serde::{Deserialize, Serialize};

// ============================================================================
// Limits
// ============================================================================

// The longest id; the watch's app channels allow 1 to 32 characters.
const MOST_ID_CHARACTERS: usize = 32;

// The phone-to-watch messages a plugin may ask to send. Settings, slots and the rest change the
// watch itself, so only the core sends those.
pub const PLUGIN_MESSAGE_TYPES: [&str; 7] = [
    "notify",
    "call",
    "musicinfo",
    "musicstate",
    "weather",
    "find",
    "vibrate",
];

// The synced watch data a plugin may ask for besides its own app's state, which it always has.
// `settings` also lets it change the watch's settings; the rest are read only.
pub const PLUGIN_DATA_KINDS: [&str; 5] = ["settings", "steps", "logs", "files", "apps"];

// ============================================================================
// The manifest
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    // The plugin's id, which is also its watch app's id and the channel that app speaks on.
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    // The watch app images the plugin carries, as paths inside the package.
    #[serde(default)]
    pub watch: Vec<String>,
    // The logic that runs in the companion, as a path inside the package.
    #[serde(default)]
    pub main: Option<String>,
    // The settings screen's page, as a path inside the package; the files beside it come along.
    #[serde(default)]
    pub ui: Option<String>,
    #[serde(default)]
    pub permissions: Permissions,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    // The message types the plugin may send to the watch, from PLUGIN_MESSAGE_TYPES.
    #[serde(default)]
    pub messages: Vec<String>,
    // The internet hosts the plugin may fetch from, over https only.
    #[serde(default)]
    pub hosts: Vec<String>,
    // The synced watch data the plugin may read, from PLUGIN_DATA_KINDS.
    #[serde(default)]
    pub data: Vec<String>,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest, String> {
        let manifest: Manifest =
            serde_json::from_str(text).map_err(|error| format!("manifest.json is not valid: {error}"))?;
        manifest.check()?;
        Ok(manifest)
    }

    pub fn check(&self) -> Result<(), String> {
        if !is_plugin_id(&self.id) {
            return Err(format!(
                "\"{}\" is not a plugin id: use 1 to {MOST_ID_CHARACTERS} lowercase letters, digits and hyphens.",
                self.id
            ));
        }
        if self.name.trim().is_empty() || self.version.trim().is_empty() {
            return Err("manifest.json needs a name and a version.".into());
        }
        for path in self.watch.iter().chain(&self.main).chain(&self.ui) {
            if !is_package_path(path) {
                return Err(format!("{path} is not a path inside the package."));
            }
        }
        if self.watch.iter().any(|path| !path.ends_with(".ntapp")) {
            return Err("Every watch entry must be a .ntapp image.".into());
        }
        for message in &self.permissions.messages {
            if !PLUGIN_MESSAGE_TYPES.contains(&message.as_str()) {
                return Err(format!(
                    "Plugins may not send \"{message}\" messages to the watch."
                ));
            }
        }
        for kind in &self.permissions.data {
            if !PLUGIN_DATA_KINDS.contains(&kind.as_str()) {
                return Err(format!("Plugins may not ask for \"{kind}\" data."));
            }
        }
        for host in &self.permissions.hosts {
            if !is_host_name(host) {
                return Err(format!("\"{host}\" is not a host name."));
            }
        }
        Ok(())
    }

    // The folder the settings page sits in, whose files all belong to the page.
    pub fn ui_folder(&self) -> Option<&str> {
        let page = self.ui.as_deref()?;
        Some(page.rsplit_once('/').map(|(folder, _)| folder).unwrap_or(""))
    }

    pub fn may_send(&self, message_type: &str) -> bool {
        self.permissions
            .messages
            .iter()
            .any(|allowed| allowed == message_type)
    }

    // A source is `settings`, `apps/<id>`, `steps/...` and so on, as the sync store names them.
    pub fn may_read(&self, source: &str) -> bool {
        if source == format!("apps/{}", self.id) {
            return true;
        }
        let kind = source.split('/').next().unwrap_or("");
        let whole = kind == source;
        self.permissions
            .data
            .iter()
            .any(|allowed| allowed == kind && (kind == "settings") == whole)
    }

    pub fn may_fetch(&self, host: &str) -> bool {
        self.permissions
            .hosts
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(host))
    }
}

// ============================================================================
// Names
// ============================================================================

pub fn is_plugin_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MOST_ID_CHARACTERS
        && !id.starts_with('-')
        && !id.ends_with('-')
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

// A relative path with no way out of the package.
pub fn is_package_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn is_host_name(host: &str) -> bool {
    !host.is_empty()
        && host.split('.').all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{
        "id": "weather-demo", "name": "Weather demo", "version": "1.0.0",
        "watch": ["watch/weather-demo.ntapp"], "main": "phone/main.js", "ui": "phone/ui/index.html",
        "permissions": { "messages": ["weather"], "hosts": ["api.open-meteo.com"] }
    }"#;

    #[test]
    fn a_complete_manifest_parses() {
        let manifest = Manifest::parse(GOOD).unwrap();
        assert_eq!(manifest.id, "weather-demo");
        assert_eq!(manifest.ui_folder(), Some("phone/ui"));
        assert!(manifest.may_send("weather"));
        assert!(!manifest.may_send("vibrate"));
        assert!(manifest.may_fetch("API.open-meteo.com"));
        assert!(!manifest.may_fetch("example.com"));
    }

    #[test]
    fn a_plugin_reads_its_own_app_and_only_the_data_it_names() {
        let manifest = Manifest::parse(
            r#"{"id":"steps","name":"Steps","version":"1","permissions":{"data":["steps","settings"]}}"#,
        )
        .unwrap();
        for source in ["apps/steps", "steps/2026/10-14", "steps/trip", "settings"] {
            assert!(manifest.may_read(source), "{source}");
        }
        for source in [
            "apps/alarm",
            "logs/heart/2026/10-14",
            "steps",
            "settings/x",
            "files/settings/launcher",
        ] {
            assert!(!manifest.may_read(source), "{source}");
        }
        assert!(Manifest::parse(
            r#"{"id":"x","name":"X","version":"1","permissions":{"data":["everything"]}}"#
        )
        .unwrap_err()
        .contains("\"everything\" data"));
    }

    #[test]
    fn a_bare_manifest_needs_only_its_names() {
        let manifest = Manifest::parse(r#"{"id":"x","name":"X","version":"1"}"#).unwrap();
        assert!(manifest.watch.is_empty() && manifest.main.is_none() && manifest.ui_folder().is_none());
    }

    #[test]
    fn bad_ids_are_refused() {
        for id in ["", "Upper", "under_score", "-lead", "trail-", &"a".repeat(33)] {
            assert!(!is_plugin_id(id), "{id}");
        }
        assert!(is_plugin_id("a-1"));
    }

    #[test]
    fn paths_cannot_leave_the_package() {
        for path in ["/etc/passwd", "../x", "a/../b", "a//b", "a\\b", "./a", ""] {
            assert!(!is_package_path(path), "{path}");
        }
        assert!(is_package_path("phone/ui/index.html"));
    }

    #[test]
    fn each_mistake_is_named() {
        let cases = [
            (r#"{"id":"x","name":"X","version":"1","extra":1}"#, "not valid"),
            (r#"{"id":"X","name":"X","version":"1"}"#, "not a plugin id"),
            (r#"{"id":"x","name":" ","version":"1"}"#, "needs a name"),
            (
                r#"{"id":"x","name":"X","version":"1","main":"../m.js"}"#,
                "not a path",
            ),
            (
                r#"{"id":"x","name":"X","version":"1","watch":["w.bin"]}"#,
                ".ntapp",
            ),
            (
                r#"{"id":"x","name":"X","version":"1","permissions":{"messages":["slots"]}}"#,
                "\"slots\"",
            ),
            (
                r#"{"id":"x","name":"X","version":"1","permissions":{"hosts":["a b"]}}"#,
                "host name",
            ),
        ];
        for (text, expected) in cases {
            let error = Manifest::parse(text).unwrap_err();
            assert!(error.contains(expected), "{text}: {error}");
        }
    }
}
