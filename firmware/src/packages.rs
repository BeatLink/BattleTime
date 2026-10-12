//! The packages characteristic's link to the shell: what an install, removal or library copy changed.

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use neotime_packages::commands::Change;

/// Raised by the Bluetooth task after a packages command changes the installed apps or the shared
/// library, so the shell reads its app list again or lets go of the old library before using it.
pub static CHANGED: Signal<CriticalSectionRawMutex, Change> = Signal::new();
