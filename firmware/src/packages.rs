//! The packages characteristic's commands, which the storage task runs, and what an install, removal or library copy
//! changed.
//!
//! The Bluetooth task's own frames are deep, so a command run inside it once pushed the verifier past
//! the end of the stack. The storage task starts each command near the top of the stack instead.

use alloc::{string::String, vec::Vec};

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal};
use neotime_packages::commands::Change;

/// Raised after a packages command changes the installed apps or the shared library, so the shell
/// reads its app list again or lets go of the old library before using it.
pub static CHANGED: Signal<CriticalSectionRawMutex, Change> = Signal::new();

/// Command lines from the phone, without their newline, sent by the Bluetooth task.
pub static COMMANDS: Channel<CriticalSectionRawMutex, Vec<u8>, 2> = Channel::new();

/// Each command's reply line, which the Bluetooth task sends to the phone.
pub static REPLIES: Channel<CriticalSectionRawMutex, String, 2> = Channel::new();
