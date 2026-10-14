//! The GATT service for TrouBLE: InfiniTime's file service UUID with Adafruit's two characteristics.

use trouble_host::prelude::*;

use crate::PROTOCOL_VERSION;

/// The longest command the transfer characteristic takes: the firmware's 124-byte MTU less the 3-byte header.
pub const LONGEST_COMMAND: usize = 121;

const VERSION_BYTES: [u8; 4] = PROTOCOL_VERSION.to_le_bytes();

/// The phone writes commands to the transfer characteristic and gets its answers back as notifications.
#[gatt_service(uuid = "febb")]
pub struct FileTransferService {
    #[characteristic(uuid = "adaf0100-4669-6c65-5472-616e73666572", read, value = &VERSION_BYTES)]
    pub version: &'static [u8; 4],
    #[characteristic(uuid = "adaf0200-4669-6c65-5472-616e73666572", write, write_without_response, notify, value = [0; LONGEST_COMMAND])]
    pub transfer: [u8; LONGEST_COMMAND],
}
