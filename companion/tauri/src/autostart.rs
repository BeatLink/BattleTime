// Starting the companion with the computer, hidden, through the desktop's autostart folder.

use std::path::{Path, PathBuf};

const ENTRY_NAME: &str = "battletime-companion.desktop";

// The program as it was installed; a Nix build's own binary sits behind a wrapper, so it is started by name.
pub fn program() -> String {
    match std::env::current_exe() {
        Ok(path) if !path.starts_with("/nix/store") => path.to_string_lossy().into_owned(),
        _ => "battletime-companion".into(),
    }
}

// The user's autostart folder, under $XDG_CONFIG_HOME or ~/.config.
pub fn autostart_folder() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|folder| folder.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config.join("autostart"))
}

pub fn autostart_entry(program: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=BattleTime Companion\n\
         Comment=Keeps your BattleTime watch connected\nExec={program} --background\n\
         Icon=battletime-companion\nX-GNOME-Autostart-enabled=true\n"
    )
}

// Adds or removes the autostart entry, writing it only when it would change.
pub fn set_autostart(folder: &Path, enabled: bool, program: &str) -> std::io::Result<()> {
    let entry = folder.join(ENTRY_NAME);
    if !enabled {
        return match std::fs::remove_file(&entry) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    }
    let wanted = autostart_entry(program);
    if std::fs::read_to_string(&entry).ok().as_deref() == Some(wanted.as_str()) {
        return Ok(());
    }
    std::fs::create_dir_all(folder)?;
    std::fs::write(entry, wanted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turning_it_on_writes_an_entry_that_starts_hidden_and_off_removes_it() {
        let folder = tempfile::tempdir().unwrap();
        let autostart = folder.path().join("autostart");

        set_autostart(&autostart, true, "/opt/companion").unwrap();
        let written = std::fs::read_to_string(autostart.join(ENTRY_NAME)).unwrap();
        assert!(written.contains("Exec=/opt/companion --background\n"));

        set_autostart(&autostart, false, "/opt/companion").unwrap();
        assert!(!autostart.join(ENTRY_NAME).exists());
        set_autostart(&autostart, false, "/opt/companion").unwrap();
    }
}
