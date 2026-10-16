// One companion per user on the desktop: a second launch asks the running one to show its window, then quits.
//
// The running companion listens on a socket in the user's runtime folder; a launch that can reach it is the second.

use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

// What a second launch writes to the first.
const SHOW_WINDOW: &[u8] = b"show\n";

// How long a second launch waits for the first to take its message.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(2);

// The socket's place: the runtime folder the session gives each user, or the app's data folder without one.
pub fn socket_path(runtime_folder: Option<PathBuf>, data_folder: &Path) -> PathBuf {
    runtime_folder
        .filter(|folder| folder.is_dir())
        .unwrap_or_else(|| data_folder.to_path_buf())
        .join("battletime-companion-window.sock")
}

pub enum Claim {
    // This is the only companion; it hears each later launch through the listener.
    First(UnixListener),
    // Another companion was running and has been asked to show its window.
    AlreadyRunning,
}

// Becomes the companion, or hands over to the one already running.
pub fn claim(path: &Path) -> std::io::Result<Claim> {
    if let Ok(mut running) = UnixStream::connect(path) {
        running.set_write_timeout(Some(ANSWER_TIMEOUT))?;
        running.write_all(SHOW_WINDOW)?;
        return Ok(Claim::AlreadyRunning);
    }
    // Nobody answered, so whatever is left there is from a companion that has gone.
    let _ = std::fs::remove_file(path);
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(Claim::First(listener))
}

// Calls `show` for every later launch, on a thread of its own, for the life of the app.
pub fn follow_launches(listener: UnixListener, show: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        for launch in listener.incoming().flatten() {
            let mut message = [0u8; SHOW_WINDOW.len()];
            let _ = launch.set_read_timeout(Some(ANSWER_TIMEOUT));
            if (&launch).read_exact(&mut message).is_ok() && message == SHOW_WINDOW {
                show();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn a_second_launch_asks_the_first_to_show_its_window() {
        let folder = tempfile::tempdir().unwrap();
        let path = socket_path(Some(folder.path().to_path_buf()), folder.path());
        let Claim::First(listener) = claim(&path).unwrap() else {
            panic!("the first launch found a companion running");
        };
        let (shown, heard) = mpsc::channel();
        follow_launches(listener, move || shown.send(()).unwrap());

        assert!(matches!(claim(&path).unwrap(), Claim::AlreadyRunning));
        assert!(heard.recv_timeout(Duration::from_secs(5)).is_ok());
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_socket_left_by_a_companion_that_has_gone_is_taken_over() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("battletime-companion-window.sock");
        drop(UnixListener::bind(&path).unwrap());

        assert!(matches!(claim(&path).unwrap(), Claim::First(_)));
    }

    #[test]
    fn without_a_runtime_folder_the_socket_goes_in_the_data_folder() {
        let data = Path::new("/data");
        assert_eq!(
            socket_path(None, data),
            data.join("battletime-companion-window.sock")
        );
        assert_eq!(
            socket_path(Some(PathBuf::from("/no/such/folder")), data),
            data.join("battletime-companion-window.sock")
        );
    }
}
