extern crate std;

use std::{vec, vec::Vec as StdVec};

use embassy_futures::block_on;

use super::*;

// ============================================================================
// Fake targets
// ============================================================================

#[derive(Debug, PartialEq, Eq)]
enum Call {
    Begin(ImageKind, u32),
    Finish(ImageKind),
    Abort(ImageKind),
}

#[derive(Default)]
struct FakeTargets {
    calls: StdVec<Call>,
    stored: StdVec<u8>,
    chunk_sizes: StdVec<usize>,
    largest_size: u32,
    finish_fails: bool,
}

impl FakeTargets {
    fn new() -> FakeTargets {
        FakeTargets {
            largest_size: 400 * 1024,
            ..Default::default()
        }
    }
}

impl UpdateTargets for FakeTargets {
    async fn begin(&mut self, kind: ImageKind, image_size_in_bytes: u32) -> Result<(), UpdateError> {
        self.calls.push(Call::Begin(kind, image_size_in_bytes));
        if image_size_in_bytes > self.largest_size {
            return Err(UpdateError::ImageTooLarge {
                image_size_in_bytes,
                largest_size_in_bytes: self.largest_size,
            });
        }
        self.stored.clear();
        Ok(())
    }

    async fn receive(&mut self, _kind: ImageKind, chunk: &[u8]) -> Result<(), UpdateError> {
        self.chunk_sizes.push(chunk.len());
        self.stored.extend_from_slice(chunk);
        Ok(())
    }

    async fn read_back(
        &mut self,
        _kind: ImageKind,
        offset: u32,
        bytes: &mut [u8],
    ) -> Result<(), UpdateError> {
        let start = offset as usize;
        bytes.copy_from_slice(&self.stored[start..start + bytes.len()]);
        Ok(())
    }

    async fn finish(&mut self, kind: ImageKind) -> Result<(), UpdateError> {
        self.calls.push(Call::Finish(kind));
        if self.finish_fails {
            Err(UpdateError::NotFirmware)
        } else {
            Ok(())
        }
    }

    fn abort(&mut self, kind: ImageKind) {
        self.calls.push(Call::Abort(kind));
    }
}

// ============================================================================
// A phone, following the companion app's script
// ============================================================================

struct Phone {
    dfu: LegacyDfu<FakeTargets>,
    notifications: StdVec<StdVec<u8>>,
    reset_requested: bool,
}

impl Phone {
    fn new() -> Phone {
        Phone {
            dfu: LegacyDfu::new(FakeTargets::new()),
            notifications: StdVec::new(),
            reset_requested: false,
        }
    }

    fn command(&mut self, bytes: &[u8]) {
        let outcome = block_on(self.dfu.on_control_point(bytes));
        self.record(outcome);
    }

    fn packet(&mut self, bytes: &[u8]) {
        let outcome = block_on(self.dfu.on_packet(bytes));
        self.record(outcome);
    }

    fn record(&mut self, outcome: Outcome) {
        for notification in outcome.notifications {
            self.notifications.push(notification.to_vec());
        }
        self.reset_requested |= outcome.reset_requested;
    }

    fn take_notifications(&mut self) -> StdVec<StdVec<u8>> {
        core::mem::take(&mut self.notifications)
    }

    fn sizes_packet(application_size: u32) -> [u8; 12] {
        let mut sizes = [0; 12];
        sizes[8..].copy_from_slice(&application_size.to_le_bytes());
        sizes
    }

    fn init_packet(image: &[u8]) -> StdVec<u8> {
        let mut init = vec![
            0x52, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01, 0x00, 0xFE, 0xFF,
        ];
        init.extend_from_slice(&crc16(CRC16_INITIAL, image).to_le_bytes());
        init
    }

    /// Runs the whole exchange up to, and not including, the activate command.
    fn upload(&mut self, image_type: u8, image: &[u8], receipt_interval: u16) {
        self.command(&[command::START_DFU, image_type]);
        self.packet(&Self::sizes_packet(image.len() as u32));
        self.command(&[command::INITIALIZE_DFU, 0]);
        self.packet(&Self::init_packet(image));
        self.command(&[command::INITIALIZE_DFU, 1]);
        if receipt_interval > 0 {
            let [low, high] = receipt_interval.to_le_bytes();
            self.command(&[command::SET_RECEIPT_INTERVAL, low, high]);
        }
        self.command(&[command::RECEIVE_FIRMWARE_IMAGE]);
        for chunk in image.chunks(20) {
            self.packet(chunk);
        }
        self.command(&[command::VALIDATE_FIRMWARE]);
    }
}

fn image(length: usize) -> StdVec<u8> {
    (0..length).map(|index| (index * 13 + 7) as u8).collect()
}

fn response(procedure: u8, result: u8) -> StdVec<u8> {
    vec![command::RESPONSE, procedure, result]
}

// ============================================================================
// CRC
// ============================================================================

#[test]
fn crc16_matches_the_standard_check_value() {
    assert_eq!(crc16(CRC16_INITIAL, b"123456789"), 0x29B1);
}

#[test]
fn crc16_can_be_computed_in_pieces() {
    let whole = image(1000);
    let in_pieces = whole.chunks(256).fold(CRC16_INITIAL, crc16);

    assert_eq!(in_pieces, crc16(CRC16_INITIAL, &whole));
}

// ============================================================================
// A whole update
// ============================================================================

#[test]
fn a_full_update_stores_the_image_and_asks_for_a_reset() {
    let firmware = image(5000);
    let mut phone = Phone::new();

    phone.upload(image_type::APPLICATION, &firmware, 0);
    let before_activate = phone.take_notifications();
    phone.command(&[command::ACTIVATE_IMAGE_AND_RESET]);

    assert_eq!(
        before_activate,
        [
            response(command::START_DFU, result::SUCCESS),
            response(command::INITIALIZE_DFU, result::SUCCESS),
            response(command::RECEIVE_FIRMWARE_IMAGE, result::SUCCESS),
            response(command::VALIDATE_FIRMWARE, result::SUCCESS),
        ]
    );
    assert!(phone.reset_requested);
    let targets = phone.dfu.release();
    assert_eq!(targets.stored, firmware);
    assert_eq!(
        targets.calls,
        [
            Call::Begin(ImageKind::Firmware, 5000),
            Call::Finish(ImageKind::Firmware)
        ]
    );
}

#[test]
fn image_bytes_reach_the_flash_in_large_pieces() {
    let firmware = image(1000);
    let mut phone = Phone::new();

    phone.upload(image_type::APPLICATION, &firmware, 0);

    assert_eq!(phone.dfu.release().chunk_sizes, [240, 240, 240, 240, 40]);
}

#[test]
fn receipts_come_every_interval_and_on_the_last_packet_too() {
    let firmware = image(200);
    let mut phone = Phone::new();

    phone.upload(image_type::APPLICATION, &firmware, 5);
    let notifications = phone.take_notifications();

    let receipts: StdVec<&StdVec<u8>> = notifications
        .iter()
        .filter(|n| n[0] == command::PACKET_RECEIPT)
        .collect();
    assert_eq!(
        receipts,
        [
            &vec![command::PACKET_RECEIPT, 100, 0, 0, 0],
            &vec![command::PACKET_RECEIPT, 200, 0, 0, 0]
        ]
    );
    let image_done = notifications
        .iter()
        .position(|n| n == &response(command::RECEIVE_FIRMWARE_IMAGE, result::SUCCESS))
        .unwrap();
    assert_eq!(
        notifications[image_done - 1],
        vec![command::PACKET_RECEIPT, 200, 0, 0, 0]
    );
}

#[test]
fn a_recovery_image_goes_to_the_recovery_slot() {
    let recovery = image(300);
    let mut phone = Phone::new();

    phone.upload(image_type::RECOVERY, &recovery, 0);
    phone.command(&[command::ACTIVATE_IMAGE_AND_RESET]);

    assert_eq!(
        phone.dfu.release().calls,
        [
            Call::Begin(ImageKind::Recovery, 300),
            Call::Finish(ImageKind::Recovery)
        ]
    );
}

// ============================================================================
// Refusals
// ============================================================================

#[test]
fn a_wrong_checksum_is_refused_and_the_upload_abandoned() {
    let firmware = image(500);
    let mut phone = Phone::new();

    phone.command(&[command::START_DFU, image_type::APPLICATION]);
    phone.packet(&Phone::sizes_packet(500));
    phone.command(&[command::INITIALIZE_DFU, 0]);
    phone.packet(&Phone::init_packet(&image(501)));
    phone.command(&[command::INITIALIZE_DFU, 1]);
    phone.command(&[command::RECEIVE_FIRMWARE_IMAGE]);
    for chunk in firmware.chunks(20) {
        phone.packet(chunk);
    }
    phone.command(&[command::VALIDATE_FIRMWARE]);

    assert_eq!(
        phone.take_notifications().last().unwrap(),
        &response(command::VALIDATE_FIRMWARE, result::CRC_ERROR)
    );
    assert!(!phone.dfu.is_updating());
    assert_eq!(
        phone.dfu.release().calls.last(),
        Some(&Call::Abort(ImageKind::Firmware))
    );
}

#[test]
fn softdevice_and_bootloader_images_are_not_supported() {
    let mut phone = Phone::new();

    phone.command(&[command::START_DFU, image_type::SOFTDEVICE]);
    phone.command(&[command::START_DFU, image_type::BOOTLOADER]);

    assert_eq!(
        phone.take_notifications(),
        [
            response(command::START_DFU, result::NOT_SUPPORTED),
            response(command::START_DFU, result::NOT_SUPPORTED)
        ]
    );
}

#[test]
fn an_image_too_large_for_the_slot_is_refused_at_the_start() {
    let mut phone = Phone::new();

    phone.command(&[command::START_DFU, image_type::APPLICATION]);
    phone.packet(&Phone::sizes_packet(401 * 1024));

    assert_eq!(
        phone.take_notifications(),
        [response(command::START_DFU, result::DATA_SIZE_EXCEEDS_LIMIT)]
    );
    assert!(!phone.dfu.is_updating());
}

#[test]
fn commands_out_of_order_get_invalid_state() {
    let mut phone = Phone::new();

    phone.command(&[command::VALIDATE_FIRMWARE]);
    phone.command(&[command::ACTIVATE_IMAGE_AND_RESET]);
    phone.command(&[command::RECEIVE_FIRMWARE_IMAGE]);

    assert_eq!(
        phone.take_notifications(),
        [
            response(command::VALIDATE_FIRMWARE, result::INVALID_STATE),
            response(command::ACTIVATE_IMAGE_AND_RESET, result::INVALID_STATE),
            response(command::RECEIVE_FIRMWARE_IMAGE, result::INVALID_STATE),
        ]
    );
}

#[test]
fn more_bytes_than_announced_abandon_the_upload() {
    let mut phone = Phone::new();

    phone.command(&[command::START_DFU, image_type::APPLICATION]);
    phone.packet(&Phone::sizes_packet(30));
    phone.command(&[command::INITIALIZE_DFU, 0]);
    phone.packet(&Phone::init_packet(&image(30)));
    phone.command(&[command::INITIALIZE_DFU, 1]);
    phone.command(&[command::RECEIVE_FIRMWARE_IMAGE]);
    phone.packet(&[0; 20]);
    phone.packet(&[0; 20]);

    assert_eq!(
        phone.take_notifications().last().unwrap(),
        &response(command::RECEIVE_FIRMWARE_IMAGE, result::DATA_SIZE_EXCEEDS_LIMIT)
    );
    assert!(!phone.dfu.is_updating());
}

#[test]
fn a_finish_that_fails_is_reported_instead_of_resetting() {
    let firmware = image(100);
    let mut phone = Phone::new();
    phone.dfu.targets.finish_fails = true;

    phone.upload(image_type::APPLICATION, &firmware, 0);
    phone.take_notifications();
    phone.command(&[command::ACTIVATE_IMAGE_AND_RESET]);

    assert_eq!(
        phone.take_notifications(),
        [response(
            command::ACTIVATE_IMAGE_AND_RESET,
            result::OPERATION_FAILED
        )]
    );
    assert!(!phone.reset_requested);
}

#[test]
fn losing_the_phone_abandons_the_upload() {
    let mut phone = Phone::new();
    phone.command(&[command::START_DFU, image_type::APPLICATION]);
    phone.packet(&Phone::sizes_packet(1000));

    phone.dfu.abort();

    assert!(!phone.dfu.is_updating());
    assert_eq!(
        phone.dfu.release().calls,
        [
            Call::Begin(ImageKind::Firmware, 1000),
            Call::Abort(ImageKind::Firmware)
        ]
    );
}

#[test]
fn reset_system_is_obeyed_at_any_time() {
    let mut phone = Phone::new();

    phone.command(&[command::RESET_SYSTEM]);

    assert!(phone.reset_requested);
}

#[test]
fn reports_the_received_size_on_request() {
    let mut phone = Phone::new();
    phone.command(&[command::START_DFU, image_type::APPLICATION]);
    phone.packet(&Phone::sizes_packet(1000));
    phone.command(&[command::INITIALIZE_DFU, 0]);
    phone.packet(&Phone::init_packet(&image(1000)));
    phone.command(&[command::INITIALIZE_DFU, 1]);
    phone.command(&[command::RECEIVE_FIRMWARE_IMAGE]);
    phone.packet(&[0; 20]);
    phone.take_notifications();

    phone.command(&[command::REPORT_RECEIVED_IMAGE_SIZE]);

    assert_eq!(
        phone.take_notifications(),
        [vec![
            command::RESPONSE,
            command::REPORT_RECEIVED_IMAGE_SIZE,
            result::SUCCESS,
            20,
            0,
            0,
            0
        ]]
    );
}
