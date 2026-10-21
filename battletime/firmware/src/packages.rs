//! The packages characteristic's commands, which the storage task runs, and what an install or removal changed.
//!
//! The Bluetooth task's own frames are deep, so a command run inside it once pushed the verifier past
//! the end of the stack. The storage task starts each command near the top of the stack instead.

use alloc::{string::String, vec::Vec};

use battletime_packages::commands::Change;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal};

/// Raised after a packages command changes the installed apps, so the shell reads its app list again.
pub static CHANGED: Signal<CriticalSectionRawMutex, Change> = Signal::new();

/// Raised when the phone runs or stops a temporary app, carrying `Change::RunTemporary` or `Change::StopTemporary`,
/// and after an install, carrying `Change::VerifyInstall` for the shell to verify the app's code.
pub static TEMPORARY: Signal<CriticalSectionRawMutex, Change> = Signal::new();

/// Command lines from the phone, without their newline, sent by the Bluetooth task.
pub static COMMANDS: Channel<CriticalSectionRawMutex, Vec<u8>, 2> = Channel::new();

/// Each command's reply line, which the Bluetooth task sends to the phone.
pub static REPLIES: Channel<CriticalSectionRawMutex, String, 2> = Channel::new();
