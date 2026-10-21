//! The packages characteristic's commands, which the storage task runs, and what an install or removal changed.
//!
//! The Bluetooth task's own frames are deep, so a command run inside it once pushed the verifier past
//! the end of the stack. The storage task starts each command near the top of the stack instead.

use alloc::{collections::VecDeque, string::String, vec::Vec};
use core::cell::RefCell;

use battletime_packages::commands::Change;
use embassy_sync::{
    blocking_mutex::{raw::CriticalSectionRawMutex, Mutex},
    channel::Channel,
    signal::Signal,
};

/// Raised after a packages command changes the installed apps, so the shell reads its app list again.
pub static CHANGED: Signal<CriticalSectionRawMutex, Change> = Signal::new();

/// Raised when the phone runs or stops a temporary app, carrying `Change::RunTemporary` or `Change::StopTemporary`,
/// and after an install, carrying `Change::VerifyInstall` for the shell to verify the app's code.
pub static TEMPORARY: Signal<CriticalSectionRawMutex, Change> = Signal::new();

/// Command lines from the phone, without their newline, sent by the Bluetooth task.
pub static COMMANDS: Channel<CriticalSectionRawMutex, Vec<u8>, 2> = Channel::new();

// ============================================================================
// Replies
// ============================================================================

/// Each command's reply, a piece at a time, waiting for the Bluetooth task to send it to the phone.
///
/// A queue no task waits to put into: the storage task holds the store while it answers, and the
/// Bluetooth task may be waiting on the storage task for a file request, so a full channel here
/// once left both, and the shell behind them, waiting for good.
static REPLIES: Mutex<CriticalSectionRawMutex, RefCell<VecDeque<String>>> =
    Mutex::new(RefCell::new(VecDeque::new()));
static REPLY_READY: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Puts a piece of a reply after those waiting, without waiting itself.
pub fn send_reply(piece: String) {
    REPLIES.lock(|replies| replies.borrow_mut().push_back(piece));
    REPLY_READY.signal(());
}

/// The next piece of a reply, once there is one.
pub async fn next_reply() -> String {
    loop {
        if let Some(piece) = REPLIES.lock(|replies| replies.borrow_mut().pop_front()) {
            return piece;
        }
        REPLY_READY.wait().await;
    }
}

/// Forgets the replies a phone that went away left unsent.
pub fn clear_replies() {
    REPLIES.lock(|replies| replies.borrow_mut().clear());
    REPLY_READY.reset();
}
