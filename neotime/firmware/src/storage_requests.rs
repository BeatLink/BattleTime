//! What the other tasks ask of the storage task, which alone runs the flash store's calls, so their futures are
//! kept once rather than in every task.
//!
//! The shell is the exception: it calls the store directly, without waiting, once no request is under way.

use alloc::vec::Vec;

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal};
use neotime_steps::Recorded;

/// The longest file service command: one write of the 128-byte packets the Bluetooth host uses.
pub const FILE_REQUEST_BYTES: usize = 128;
/// The longest small file another task writes, such as the time zone.
pub const SMALL_WRITE_BYTES: usize = 32;

/// The rehearsals keep no store and the recovery firmware no step counter, so they leave some of what these carry unread.
#[cfg_attr(not(with_shell), allow(dead_code))]
pub enum StorageRequest {
    /// A file service command from the phone; its answers come back on `FILE_ANSWERS`.
    FileTransfer {
        request: heapless::Vec<u8, FILE_REQUEST_BYTES>,
        room: usize,
    },
    /// A new connection forgets what the last one left unfinished.
    StartFileTransfer,
    /// Reads the remembered days and the trip count into the step history, then raises `STEPS_LOADED`.
    LoadSteps,
    /// Writes the slot a reading ended, with the trip count, and reads the days again after the clock jumped.
    StoreSteps(Recorded),
    SaveTrip(u32),
    WriteSmallFile {
        path: &'static str,
        bytes: heapless::Vec<u8, SMALL_WRITE_BYTES>,
    },
}

pub static REQUESTS: Channel<CriticalSectionRawMutex, StorageRequest, 2> = Channel::new();
pub static FILE_ANSWERS: Channel<CriticalSectionRawMutex, Vec<Vec<u8>>, 1> = Channel::new();
pub static STEPS_LOADED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Sends a file service command to the storage task and waits for its answers.
pub async fn answer_file_transfer(request: &[u8], room: usize) -> Vec<Vec<u8>> {
    let mut copied = heapless::Vec::new();
    let _ = copied.extend_from_slice(&request[..request.len().min(FILE_REQUEST_BYTES)]);
    REQUESTS
        .send(StorageRequest::FileTransfer {
            request: copied,
            room,
        })
        .await;
    FILE_ANSWERS.receive().await
}
