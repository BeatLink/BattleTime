// The watch's launcher file, /settings/launcher, as src/protocol/launcher.ts writes it and
// neotime/services/packages/src/launcher.rs reads it.
//
// Byte 0 the format version (1), byte 1 the style, byte 2 the view, byte 3 the group count, then per
// group its icon's place in the shared library, its id, its name and its apps' ids in order, each text
// behind a length byte, and last one byte for how the companion sorts, which the watch ignores. This
// port reads the file back and writes it again with every installed app placed, as an install does.

use std::future::Future;

use super::app_image::ImageKind;
use super::link::GattLink;
use super::packages::{FileWriter, PackagesClient, PackagesError, WatchApp};

// ============================================================================
// The settings
// ============================================================================

pub const LAUNCHER_PATH: &str = "/settings/launcher";
pub const LAUNCHER_FILE_VERSION: u8 = 1;
pub const OTHER_GROUP_ID: &str = "other";

// The Other group as a watch starts with it, with the pine_logo icon, at place 61 counting from 0 in src/bundles/icon-names.ts.
const OTHER_GROUP_ICON: u8 = 61;

const MOST_TEXT_BYTES: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherGroup {
    pub id: String,
    pub name: String,
    // The icon's place in the shared library, or 255 for none.
    pub icon: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherSettings {
    pub style: u8,
    pub view: u8,
    // 0 by name, 1 the wearer's order.
    pub sort: u8,
    pub groups: Vec<LauncherGroup>,
    // Each placed app and its group, in the wearer's order.
    pub placements: Vec<(String, String)>,
}

impl LauncherSettings {
    fn custom_order(&self) -> bool {
        self.sort == 1
    }
}

// ============================================================================
// Reading and writing the file
// ============================================================================

// Reads a launcher file back, every app in it placed where it is; None when it is not one.
pub fn read_launcher_file(file: &[u8]) -> Option<LauncherSettings> {
    if file.len() < 4 || file[0] != LAUNCHER_FILE_VERSION {
        return None;
    }
    let mut settings = LauncherSettings {
        style: file[1],
        view: file[2],
        sort: 0,
        groups: Vec::new(),
        placements: Vec::new(),
    };
    let mut at = 4;
    let text = |at: &mut usize| -> Option<String> {
        let length = *file.get(*at)? as usize;
        let bytes = file.get(*at + 1..*at + 1 + length)?;
        *at += 1 + length;
        Some(String::from_utf8_lossy(bytes).into_owned())
    };
    // The groups before a cut stay, as on the watch.
    'groups: for _ in 0..file[3] {
        let Some(&icon) = file.get(at) else { break };
        at += 1;
        let (Some(id), Some(name)) = (text(&mut at), text(&mut at)) else {
            break;
        };
        let Some(&count) = file.get(at) else { break };
        at += 1;
        settings.groups.push(LauncherGroup {
            id: id.clone(),
            name,
            icon,
        });
        for _ in 0..count {
            let Some(app) = text(&mut at) else { break 'groups };
            settings.placements.push((app, id.clone()));
        }
    }
    settings.sort = file.get(at).copied().filter(|sort| *sort <= 1).unwrap_or(0);
    Some(settings)
}

// A length byte and the text, cut to whole characters within 255 bytes.
fn push_text(bytes: &mut Vec<u8>, text: &str) {
    let mut end = text.len().min(MOST_TEXT_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    bytes.push(end as u8);
    bytes.extend_from_slice(&text.as_bytes()[..end]);
}

fn by_name(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

// The groups in the launcher's order: by name with `other` last, or as the wearer put them.
fn ordered_groups(settings: &LauncherSettings) -> Vec<LauncherGroup> {
    let mut groups = settings.groups.clone();
    if !settings.custom_order() {
        groups.sort_by(|a, b| {
            (a.id == OTHER_GROUP_ID)
                .cmp(&(b.id == OTHER_GROUP_ID))
                .then_with(|| by_name(&a.name, &b.name))
        });
    }
    groups
}

// Each group with the apps the launcher shows in it, in order; `other` is added when it is missing.
pub fn group_apps<'app>(
    settings: &LauncherSettings,
    apps: &'app [WatchApp],
) -> Vec<(LauncherGroup, Vec<&'app WatchApp>)> {
    // Hidden apps keep their place too, so turning one back on puts it where it was.
    let listed: Vec<&WatchApp> = apps
        .iter()
        .filter(|app| app.kind == ImageKind::App && app.id != "launcher")
        .collect();
    let mut groups = ordered_groups(settings);
    if !groups.iter().any(|group| group.id == OTHER_GROUP_ID) {
        groups.push(LauncherGroup {
            id: OTHER_GROUP_ID.into(),
            name: "Other".into(),
            icon: OTHER_GROUP_ICON,
        });
    }
    let group_of = |app: &WatchApp| -> String {
        let wanted = settings
            .placements
            .iter()
            .find(|(placed, _)| *placed == app.id)
            .map(|(_, group)| group.clone())
            .unwrap_or_else(|| app.group.clone());
        if groups.iter().any(|group| group.id == wanted) {
            wanted
        } else {
            OTHER_GROUP_ID.into()
        }
    };
    // Apps the wearer has not ordered come after those they have, by name.
    let rank = |app: &WatchApp| -> usize {
        let placed = settings
            .placements
            .iter()
            .position(|(placed, _)| *placed == app.id);
        match placed {
            Some(place) if settings.custom_order() => place,
            _ => settings.placements.len(),
        }
    };
    groups
        .iter()
        .map(|group| {
            let mut members: Vec<&WatchApp> = listed
                .iter()
                .copied()
                .filter(|app| group_of(app) == group.id)
                .collect();
            members.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| by_name(&a.name, &b.name)));
            (group.clone(), members)
        })
        .collect()
}

// The file for the watch: the settings with every installed app placed.
pub fn launcher_file(settings: &LauncherSettings, apps: &[WatchApp]) -> Vec<u8> {
    let mut bytes = vec![
        LAUNCHER_FILE_VERSION,
        settings.style,
        settings.view,
        settings.groups.len() as u8,
    ];
    let grouped = group_apps(settings, apps);
    for group in ordered_groups(settings) {
        let members = grouped
            .iter()
            .find(|(found, _)| found.id == group.id)
            .map(|(_, members)| members.as_slice());
        let members = members.unwrap_or(&[]);
        let name = group.name.replace(['\r', '\n'], " ");
        let name = name.trim();
        bytes.push(group.icon);
        push_text(&mut bytes, &group.id);
        push_text(&mut bytes, if name.is_empty() { &group.id } else { name });
        bytes.push(members.len() as u8);
        for member in members {
            push_text(&mut bytes, &member.id);
        }
    }
    bytes.push(settings.sort);
    bytes
}

// ============================================================================
// After an install
// ============================================================================

// Reading as well as writing the watch's files, which placing new apps needs.
pub trait FileReader: Send + Sync {
    fn read(&self, path: &str) -> impl Future<Output = Result<Vec<u8>, String>> + Send;
}

impl FileReader for crate::sync::files::FileClient {
    async fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        crate::sync::files::FileClient::read(self, path)
            .await
            .map_err(|error| error.to_string())
    }
}

// When the watch has launcher groups, writes the file again so every new app sits where its image says.
pub async fn place_new_apps<Link: GattLink>(
    files: &(impl FileReader + FileWriter),
    packages: &PackagesClient<'_, Link>,
) -> Result<(), PackagesError> {
    let Ok(file) = files.read(LAUNCHER_PATH).await else {
        return Ok(());
    };
    let Some(settings) = read_launcher_file(&file) else {
        return Ok(());
    };
    let apps = packages.listing().await?.apps;
    files
        .write(LAUNCHER_PATH, &launcher_file(&settings, &apps))
        .await
        .map_err(|error| PackagesError {
            message: error,
            why: None,
            need: None,
            room: None,
        })?;
    packages.reload_launcher().await
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str, group: &str) -> WatchApp {
        WatchApp {
            id: id.into(),
            name: name.into(),
            kind: ImageKind::App,
            enabled: true,
            bytes: 100,
            need: None,
            group: group.into(),
        }
    }

    // The file as src/protocol/launcher.ts writes it: folders, list view, Tools then Other, sorted by name.
    fn file_with_tools_group() -> Vec<u8> {
        let mut bytes = vec![1, 1, 0, 2];
        bytes.push(4);
        push_text(&mut bytes, "tools");
        push_text(&mut bytes, "Tools");
        bytes.push(1);
        push_text(&mut bytes, "timer");
        bytes.push(OTHER_GROUP_ICON);
        push_text(&mut bytes, "other");
        push_text(&mut bytes, "Other");
        bytes.push(0);
        bytes.push(0);
        bytes
    }

    #[test]
    fn a_file_reads_back_with_its_groups_and_placements() {
        let settings = read_launcher_file(&file_with_tools_group()).unwrap();
        assert_eq!((settings.style, settings.view, settings.sort), (1, 0, 0));
        assert_eq!(
            settings
                .groups
                .iter()
                .map(|group| group.id.as_str())
                .collect::<Vec<_>>(),
            ["tools", "other"]
        );
        assert_eq!(settings.placements, [("timer".to_string(), "tools".to_string())]);
    }

    #[test]
    fn a_new_app_lands_in_the_group_its_image_names_and_an_unknown_one_in_other() {
        let settings = read_launcher_file(&file_with_tools_group()).unwrap();
        let apps = [
            app("timer", "Timer", "tools"),
            app("buzz-back", "Buzz back", "tools"),
            app("odd", "Odd", "nowhere"),
            app("launcher", "Launcher", ""),
        ];
        let written = read_launcher_file(&launcher_file(&settings, &apps)).unwrap();
        let in_group = |group: &str| -> Vec<String> {
            written
                .placements
                .iter()
                .filter(|(_, placed)| placed == group)
                .map(|(app, _)| app.clone())
                .collect()
        };
        assert_eq!(in_group("tools"), ["buzz-back", "timer"]);
        assert_eq!(in_group("other"), ["odd"]);
        assert_eq!(written.groups, settings.groups);
    }

    #[test]
    fn a_file_that_is_not_one_reads_as_none_and_a_cut_one_keeps_what_came_before() {
        assert_eq!(read_launcher_file(&[2, 0, 0, 0]), None);
        assert_eq!(read_launcher_file(&[1, 0]), None);
        let mut cut = file_with_tools_group();
        cut.truncate(26);
        let settings = read_launcher_file(&cut).unwrap();
        assert_eq!(settings.groups.len(), 1);
    }

    #[test]
    fn long_text_is_cut_on_a_character_boundary() {
        let mut bytes = Vec::new();
        push_text(&mut bytes, &"é".repeat(200));
        assert_eq!(bytes[0], 254);
        assert!(std::str::from_utf8(&bytes[1..]).is_ok());
    }

    #[test]
    fn the_wearers_order_keeps_groups_and_apps_where_they_put_them() {
        let mut settings = read_launcher_file(&file_with_tools_group()).unwrap();
        settings.sort = 1;
        settings.groups.reverse();
        settings.placements = vec![("zebra".into(), "tools".into()), ("apple".into(), "tools".into())];
        let apps = [app("apple", "Apple", "tools"), app("zebra", "Zebra", "tools")];
        let written = read_launcher_file(&launcher_file(&settings, &apps)).unwrap();
        assert_eq!(written.groups[0].id, "other");
        assert_eq!(
            written
                .placements
                .iter()
                .map(|(app, _)| app.as_str())
                .collect::<Vec<_>>(),
            ["zebra", "apple"]
        );
        assert_eq!(written.sort, 1);
    }
}
