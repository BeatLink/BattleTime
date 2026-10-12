//! Nordic legacy DFU over Bluetooth: the protocol Gadgetbridge, ITD, Siglo and NeoTime's own tools
//! use to update InfiniTime, so they update NeoTime unchanged.
//!
//! The phone drives the exchange through two characteristics. It writes commands to the control
//! point and gets answers back as notifications; it streams the image through the packet
//! characteristic, 20 bytes at a time. `LegacyDfu` is the state machine, with no Bluetooth code in
//! it, and hands the bytes to `neotime-updates`; the `trouble` feature adds the GATT service.

#![no_std]

use heapless::Vec;
use neotime_updates::{ImageKind, UpdateError, UpdateTargets};

#[cfg(feature = "trouble")]
pub mod service;

// ============================================================================
// The protocol's numbers
// ============================================================================

pub mod command {
    pub const START_DFU: u8 = 1;
    pub const INITIALIZE_DFU: u8 = 2;
    pub const RECEIVE_FIRMWARE_IMAGE: u8 = 3;
    pub const VALIDATE_FIRMWARE: u8 = 4;
    pub const ACTIVATE_IMAGE_AND_RESET: u8 = 5;
    pub const RESET_SYSTEM: u8 = 6;
    pub const REPORT_RECEIVED_IMAGE_SIZE: u8 = 7;
    pub const SET_RECEIPT_INTERVAL: u8 = 8;
    pub const RESPONSE: u8 = 16;
    pub const PACKET_RECEIPT: u8 = 17;
}

pub mod result {
    pub const SUCCESS: u8 = 1;
    pub const INVALID_STATE: u8 = 2;
    pub const NOT_SUPPORTED: u8 = 3;
    pub const DATA_SIZE_EXCEEDS_LIMIT: u8 = 4;
    pub const CRC_ERROR: u8 = 5;
    pub const OPERATION_FAILED: u8 = 6;
}

pub mod image_type {
    pub const SOFTDEVICE: u8 = 0x01;
    pub const BOOTLOADER: u8 = 0x02;
    pub const APPLICATION: u8 = 0x04;
    /// NeoTime's own addition: the image goes into the recovery slot instead.
    pub const RECOVERY: u8 = 0x08;
}

/// The bootloader this protocol came from reports version 8, and companion apps look for it.
pub const PROTOCOL_VERSION: [u8; 2] = [8, 0];

const INIT_PACKET_END: u8 = 1;
const LARGEST_INIT_PACKET: usize = 64;
const LARGEST_PACKET: usize = 20;
/// Writes to the flash go in whole program pages, not 20 bytes at a time.
const WRITE_BUFFER_SIZE: usize = 256;
const CRC_READ_CHUNK: usize = 256;

// ============================================================================
// What the firmware must do after a packet
// ============================================================================

pub type Notification = Vec<u8, 8>;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Notifications to send on the control point, in order.
    pub notifications: Vec<Notification, 2>,
    pub reset_requested: bool,
}

impl Outcome {
    fn none() -> Outcome {
        Outcome::default()
    }

    fn response(procedure: u8, result: u8) -> Outcome {
        let mut outcome = Outcome::default();
        outcome.push(&[command::RESPONSE, procedure, result]);
        outcome
    }

    fn push(&mut self, bytes: &[u8]) {
        let _ = self.notifications.push(Vec::from_slice(bytes).unwrap());
    }

    /// Why the watch refused a step, read from the first response that was not a success.
    pub fn failure(&self) -> Option<Failure> {
        self.notifications
            .iter()
            .filter(|notification| notification.first() == Some(&command::RESPONSE))
            .find_map(|notification| Failure::from_result(*notification.get(2)?))
    }
}

// ============================================================================
// What the status page says about an update
// ============================================================================

/// The step an update is at, as the wearer sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Idle,
    /// The phone is sending the image's sizes and its init packet.
    Starting,
    Receiving,
    /// The whole image is in; the phone asks next for its checksum to be checked.
    Checking,
    /// The checksum matched; the phone asks next for the image to be installed.
    Installing,
}

/// Why a step was refused, in words that fit one line of the status page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    OutOfOrder,
    NotSupported,
    TooLarge,
    Damaged,
    StepFailed,
}

impl Failure {
    fn from_result(result: u8) -> Option<Failure> {
        match result {
            result::INVALID_STATE => Some(Failure::OutOfOrder),
            result::NOT_SUPPORTED => Some(Failure::NotSupported),
            result::DATA_SIZE_EXCEEDS_LIMIT => Some(Failure::TooLarge),
            result::CRC_ERROR => Some(Failure::Damaged),
            result::OPERATION_FAILED => Some(Failure::StepFailed),
            _ => None,
        }
    }

    pub fn words(self) -> &'static str {
        match self {
            Failure::OutOfOrder => "Steps out of order",
            Failure::NotSupported => "Image not supported",
            Failure::TooLarge => "Image too large",
            Failure::Damaged => "Checksum did not match",
            Failure::StepFailed => "Could not store image",
        }
    }
}

/// Writes how much of the image has arrived, such as `123 / 392 KB  31%`.
pub fn write_progress(
    writer: &mut impl core::fmt::Write,
    bytes_received: u32,
    image_size_in_bytes: u32,
) -> core::fmt::Result {
    let percent = (bytes_received as u64 * 100)
        .checked_div(image_size_in_bytes as u64)
        .unwrap_or(0);
    write!(
        writer,
        "{:>3} / {:>3} KB {:>3}%",
        bytes_received / 1024,
        image_size_in_bytes.div_ceil(1024),
        percent
    )
}

// ============================================================================
// CRC-16, the check the init packet carries
// ============================================================================

/// CRC-16/CCITT-FALSE, as Nordic's tools and bootloaders compute it.
pub fn crc16(initial: u16, bytes: &[u8]) -> u16 {
    let mut crc = initial;
    for byte in bytes {
        crc ^= (*byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

pub const CRC16_INITIAL: u16 = 0xFFFF;

// ============================================================================
// The state machine
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Idle,
    WaitingForSizes,
    WaitingForInitPacket,
    ReceivingInitPacket,
    ReadyForImage,
    ReceivingImage,
    ImageReceived,
    Validated,
}

pub struct LegacyDfu<Targets: UpdateTargets> {
    targets: Targets,
    state: State,
    kind: ImageKind,
    image_size_in_bytes: u32,
    bytes_received: u32,
    packets_received: u32,
    receipt_interval: u16,
    init_packet: Vec<u8, LARGEST_INIT_PACKET>,
    expected_crc: u16,
    write_buffer: Vec<u8, WRITE_BUFFER_SIZE>,
}

impl<Targets: UpdateTargets> LegacyDfu<Targets> {
    pub fn new(targets: Targets) -> Self {
        LegacyDfu {
            targets,
            state: State::Idle,
            kind: ImageKind::Firmware,
            image_size_in_bytes: 0,
            bytes_received: 0,
            packets_received: 0,
            receipt_interval: 0,
            init_packet: Vec::new(),
            expected_crc: 0,
            write_buffer: Vec::new(),
        }
    }

    pub fn is_updating(&self) -> bool {
        self.state != State::Idle
    }

    pub fn bytes_received(&self) -> u32 {
        self.bytes_received
    }

    pub fn image_size_in_bytes(&self) -> u32 {
        self.image_size_in_bytes
    }

    /// Called when the phone goes away or stays silent too long.
    pub fn abort(&mut self) {
        if self.state != State::Idle {
            self.targets.abort(self.kind);
        }
        self.state = State::Idle;
        self.bytes_received = 0;
        self.packets_received = 0;
        self.init_packet.clear();
        self.write_buffer.clear();
    }

    pub fn release(self) -> Targets {
        self.targets
    }

    pub fn activity(&self) -> Activity {
        match self.state {
            State::Idle => Activity::Idle,
            State::WaitingForSizes
            | State::WaitingForInitPacket
            | State::ReceivingInitPacket
            | State::ReadyForImage => Activity::Starting,
            State::ReceivingImage => Activity::Receiving,
            State::ImageReceived => Activity::Checking,
            State::Validated => Activity::Installing,
        }
    }

    /// The slot the current or last update was for.
    pub fn image_kind(&self) -> ImageKind {
        self.kind
    }

    // ------------------------------------------------------------------------
    // The control point
    // ------------------------------------------------------------------------

    pub async fn on_control_point(&mut self, data: &[u8]) -> Outcome {
        let Some((&procedure, parameters)) = data.split_first() else {
            return Outcome::none();
        };

        match procedure {
            command::START_DFU => self.start(parameters),
            command::INITIALIZE_DFU => self.initialize(parameters),
            command::SET_RECEIPT_INTERVAL => {
                self.receipt_interval = u16::from_le_bytes([
                    parameters.first().copied().unwrap_or(0),
                    parameters.get(1).copied().unwrap_or(0),
                ]);
                Outcome::none()
            }
            command::RECEIVE_FIRMWARE_IMAGE => self.begin_image(),
            command::VALIDATE_FIRMWARE => self.validate().await,
            command::ACTIVATE_IMAGE_AND_RESET => self.activate().await,
            command::RESET_SYSTEM => {
                self.abort();
                Outcome {
                    notifications: Vec::new(),
                    reset_requested: true,
                }
            }
            command::REPORT_RECEIVED_IMAGE_SIZE => {
                let mut outcome = Outcome::response(procedure, result::SUCCESS);
                let _ = outcome.notifications[0].extend_from_slice(&self.bytes_received.to_le_bytes());
                outcome
            }
            _ => Outcome::response(procedure, result::NOT_SUPPORTED),
        }
    }

    fn start(&mut self, parameters: &[u8]) -> Outcome {
        if self.state != State::Idle {
            return Outcome::response(command::START_DFU, result::INVALID_STATE);
        }
        self.kind = match parameters.first() {
            Some(&image_type::APPLICATION) => ImageKind::Firmware,
            Some(&image_type::RECOVERY) => ImageKind::Recovery,
            _ => return Outcome::response(command::START_DFU, result::NOT_SUPPORTED),
        };

        self.state = State::WaitingForSizes;
        Outcome::none()
    }

    fn initialize(&mut self, parameters: &[u8]) -> Outcome {
        match (self.state, parameters.first()) {
            (State::WaitingForInitPacket, Some(0)) | (State::ReceivingInitPacket, Some(0)) => {
                self.init_packet.clear();
                self.state = State::ReceivingInitPacket;
                Outcome::none()
            }
            (State::ReceivingInitPacket, Some(&INIT_PACKET_END)) => {
                let Some(crc_bytes) = self
                    .init_packet
                    .len()
                    .checked_sub(2)
                    .map(|start| &self.init_packet[start..])
                else {
                    return Outcome::response(command::INITIALIZE_DFU, result::OPERATION_FAILED);
                };
                self.expected_crc = u16::from_le_bytes([crc_bytes[0], crc_bytes[1]]);
                self.state = State::ReadyForImage;
                Outcome::response(command::INITIALIZE_DFU, result::SUCCESS)
            }
            _ => Outcome::response(command::INITIALIZE_DFU, result::INVALID_STATE),
        }
    }

    fn begin_image(&mut self) -> Outcome {
        if self.state != State::ReadyForImage {
            return Outcome::response(command::RECEIVE_FIRMWARE_IMAGE, result::INVALID_STATE);
        }
        self.state = State::ReceivingImage;
        self.bytes_received = 0;
        self.packets_received = 0;
        Outcome::none()
    }

    async fn validate(&mut self) -> Outcome {
        if self.state != State::ImageReceived {
            return Outcome::response(command::VALIDATE_FIRMWARE, result::INVALID_STATE);
        }

        let mut crc = CRC16_INITIAL;
        let mut chunk = [0; CRC_READ_CHUNK];
        let mut offset = 0;
        while offset < self.image_size_in_bytes {
            let length = (self.image_size_in_bytes - offset).min(CRC_READ_CHUNK as u32) as usize;
            if self
                .targets
                .read_back(self.kind, offset, &mut chunk[..length])
                .await
                .is_err()
            {
                self.abort();
                return Outcome::response(command::VALIDATE_FIRMWARE, result::OPERATION_FAILED);
            }
            crc = crc16(crc, &chunk[..length]);
            offset += length as u32;
        }

        if crc != self.expected_crc {
            self.abort();
            return Outcome::response(command::VALIDATE_FIRMWARE, result::CRC_ERROR);
        }
        self.state = State::Validated;
        Outcome::response(command::VALIDATE_FIRMWARE, result::SUCCESS)
    }

    async fn activate(&mut self) -> Outcome {
        if self.state != State::Validated {
            return Outcome::response(command::ACTIVATE_IMAGE_AND_RESET, result::INVALID_STATE);
        }

        let finished = self.targets.finish(self.kind).await;
        self.state = State::Idle;
        match finished {
            Ok(()) => Outcome {
                notifications: Vec::new(),
                reset_requested: true,
            },
            Err(_) => Outcome::response(command::ACTIVATE_IMAGE_AND_RESET, result::OPERATION_FAILED),
        }
    }

    // ------------------------------------------------------------------------
    // The packet characteristic
    // ------------------------------------------------------------------------

    pub async fn on_packet(&mut self, data: &[u8]) -> Outcome {
        match self.state {
            State::WaitingForSizes => self.take_sizes(data).await,
            State::ReceivingInitPacket => {
                if self.init_packet.extend_from_slice(data).is_err() {
                    self.abort();
                    return Outcome::response(command::INITIALIZE_DFU, result::DATA_SIZE_EXCEEDS_LIMIT);
                }
                Outcome::none()
            }
            State::ReceivingImage => self.take_image_bytes(data).await,
            _ => Outcome::none(),
        }
    }

    /// The first packet carries three sizes: SoftDevice, bootloader and application, each 4 bytes.
    async fn take_sizes(&mut self, data: &[u8]) -> Outcome {
        if data.len() < 12 {
            self.abort();
            return Outcome::response(command::START_DFU, result::OPERATION_FAILED);
        }
        let softdevice_size = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let bootloader_size = u32::from_le_bytes(data[4..8].try_into().unwrap());
        let application_size = u32::from_le_bytes(data[8..12].try_into().unwrap());
        if softdevice_size != 0 || bootloader_size != 0 || application_size == 0 {
            self.abort();
            return Outcome::response(command::START_DFU, result::NOT_SUPPORTED);
        }

        match self.targets.begin(self.kind, application_size).await {
            Ok(()) => {
                self.image_size_in_bytes = application_size;
                self.state = State::WaitingForInitPacket;
                Outcome::response(command::START_DFU, result::SUCCESS)
            }
            Err(UpdateError::ImageTooLarge { .. }) => {
                self.abort();
                Outcome::response(command::START_DFU, result::DATA_SIZE_EXCEEDS_LIMIT)
            }
            Err(_) => {
                self.abort();
                Outcome::response(command::START_DFU, result::OPERATION_FAILED)
            }
        }
    }

    async fn take_image_bytes(&mut self, data: &[u8]) -> Outcome {
        let data = &data[..data.len().min(LARGEST_PACKET)];
        if self.bytes_received + data.len() as u32 > self.image_size_in_bytes {
            self.abort();
            return Outcome::response(command::RECEIVE_FIRMWARE_IMAGE, result::DATA_SIZE_EXCEEDS_LIMIT);
        }

        if self.write_buffer.extend_from_slice(data).is_err() {
            if self.flush().await.is_err() {
                return self.fail_image();
            }
            self.write_buffer.extend_from_slice(data).unwrap();
        }
        self.bytes_received += data.len() as u32;
        self.packets_received += 1;

        let mut outcome = Outcome::none();
        if self.receipt_interval > 0 && self.packets_received.is_multiple_of(self.receipt_interval as u32) {
            let mut receipt = Vec::new();
            let _ = receipt.push(command::PACKET_RECEIPT);
            let _ = receipt.extend_from_slice(&self.bytes_received.to_le_bytes());
            let _ = outcome.notifications.push(receipt);
        }

        if self.bytes_received == self.image_size_in_bytes {
            if self.flush().await.is_err() {
                return self.fail_image();
            }
            self.state = State::ImageReceived;
            outcome.push(&[
                command::RESPONSE,
                command::RECEIVE_FIRMWARE_IMAGE,
                result::SUCCESS,
            ]);
        }
        outcome
    }

    async fn flush(&mut self) -> Result<(), UpdateError> {
        if self.write_buffer.is_empty() {
            return Ok(());
        }
        let result = self.targets.receive(self.kind, &self.write_buffer).await;
        self.write_buffer.clear();
        result
    }

    fn fail_image(&mut self) -> Outcome {
        self.abort();
        Outcome::response(command::RECEIVE_FIRMWARE_IMAGE, result::OPERATION_FAILED)
    }
}

#[cfg(test)]
mod tests;
