//! The packages characteristic's commands, run in their own task, and what an install, removal or library copy changed.
//!
//! The Bluetooth task's own frames are deep, so a command run inside it once pushed the verifier past
//! the end of the stack. Here a command starts near the top of the stack instead.

use alloc::{string::String, vec::Vec};

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal};
use neotime_packages::commands::Change;

use crate::{foreground, storage};

/// Raised after a packages command changes the installed apps or the shared library, so the shell
/// reads its app list again or lets go of the old library before using it.
pub static CHANGED: Signal<CriticalSectionRawMutex, Change> = Signal::new();

/// Command lines from the phone, without their newline, sent by the Bluetooth task.
pub static COMMANDS: Channel<CriticalSectionRawMutex, Vec<u8>, 2> = Channel::new();

/// Each command's reply line, which the Bluetooth task sends to the phone.
pub static REPLIES: Channel<CriticalSectionRawMutex, String, 2> = Channel::new();

/// Runs each command as it comes, then hands back its reply and raises `CHANGED` when it changed something.
#[embassy_executor::task]
pub async fn run_commands(files: &'static storage::Files) {
    loop {
        let line = COMMANDS.receive().await;
        foreground::wait_for_a_pause().await;
        let (reply, change) = files.run_packages_command(&line);
        REPLIES.send(reply).await;
        if change != Change::None {
            CHANGED.signal(change);
        }
    }
}
