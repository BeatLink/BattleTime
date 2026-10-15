//! What the other tasks ask of the storage task, which alone runs the flash store's calls, so their futures are
//! kept once rather than in every task.
//!
//! The shell is the exception: it calls the store directly, without waiting, once no request is under way.

use alloc::boxed::Box;
use alloc::vec::Vec;

use battletime_slot_log::EndedSlot;
use battletime_steps::Recorded;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal};

/// The longest file service command: one write of the 128-byte packets the Bluetooth host uses.
pub const FILE_REQUEST_BYTES: usize = 128;
/// The longest small file another task writes, such as the time zone.
pub const SMALL_WRITE_BYTES: usize = 32;

/// The recovery firmware has no step counter, so it leaves some of what these carry unread.
#[cfg_attr(not(with_shell), allow(dead_code))]
pub enum StorageRequest {
    /// A file service command from the phone; its answers come back on `FILE_ANSWERS`.
    FileTransfer {
        request: FileRequest,
        room: usize,
    },
    /// A new connection forgets what the last one left unfinished.
    StartFileTransfer,
    /// Reads the remembered days and the trip count into the step history, then raises `STEPS_LOADED`.
    LoadSteps,
    /// Writes the slot a reading ended, with the trip count, and reads the days again after the clock jumped.
    StoreSteps(Recorded),
    SaveTrip(u32),
    /// Writes a six-minute slot that ended to the heart rate and battery logs; see `battletime_slot_log`.
    StoreSlotLogs {
        ended: EndedSlot,
        beats_per_minute: u8,
        battery: u8,
    },
    WriteSmallFile {
        path: &'static str,
        bytes: heapless::Vec<u8, SMALL_WRITE_BYTES>,
    },
}

pub static REQUESTS: Channel<CriticalSectionRawMutex, StorageRequest, 2> = Channel::new();
pub static FILE_ANSWERS: Channel<CriticalSectionRawMutex, Vec<Vec<u8>>, 1> = Channel::new();
pub static STEPS_LOADED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// A file service command on its way to the storage task, on the heap so the request channel stays small.
pub enum FileRequest {
    Copied(Box<[u8]>),
    /// The heap had no room for the copy, so only the command byte goes, for the storage task to refuse.
    Refused(u8),
}

impl FileRequest {
    /// Copies up to `FILE_REQUEST_BYTES` of a write, or keeps its command byte when the heap has no room.
    pub fn copy_of(bytes: &[u8]) -> FileRequest {
        let bytes = &bytes[..bytes.len().min(FILE_REQUEST_BYTES)];
        let mut copy = Vec::new();
        if copy.try_reserve_exact(bytes.len()).is_err() {
            return FileRequest::Refused(bytes.first().copied().unwrap_or(0));
        }
        copy.extend_from_slice(bytes);
        FileRequest::Copied(copy.into_boxed_slice())
    }
}

/// Sends a file service command to the storage task and waits for its answers.
pub async fn answer_file_transfer(request: FileRequest, room: usize) -> Vec<Vec<u8>> {
    REQUESTS
        .send(StorageRequest::FileTransfer { request, room })
        .await;
    FILE_ANSWERS.receive().await
}
