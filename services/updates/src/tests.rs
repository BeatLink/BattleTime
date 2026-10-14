extern crate std;

use std::{vec, vec::Vec};

use embassy_futures::block_on;
use embedded_storage_async::nor_flash::{ErrorType, NorFlashErrorKind, ReadNorFlash};
use neotime_boot::{
    image_header::{ImageType, Version},
    Region, DEFAULT_SLOTS, FIRMWARE_VECTOR_TABLE, PAGE_SIZE,
};

use super::*;

// ============================================================================
// Fake flash
// ============================================================================

struct FakeFlash<const WRITE_SIZE: usize> {
    memory: Vec<u8>,
    erase_count: usize,
}

impl<const WRITE_SIZE: usize> FakeFlash<WRITE_SIZE> {
    fn erased(size: u32) -> FakeFlash<WRITE_SIZE> {
        FakeFlash {
            memory: vec![0xFF; size as usize],
            erase_count: 0,
        }
    }

    fn bytes_at(&self, address: u32, length: usize) -> &[u8] {
        &self.memory[address as usize..address as usize + length]
    }
}

impl<const WRITE_SIZE: usize> ErrorType for FakeFlash<WRITE_SIZE> {
    type Error = NorFlashErrorKind;
}

impl<const WRITE_SIZE: usize> ReadNorFlash for FakeFlash<WRITE_SIZE> {
    const READ_SIZE: usize = 1;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        bytes.copy_from_slice(self.bytes_at(offset, bytes.len()));
        Ok(())
    }

    fn capacity(&self) -> usize {
        self.memory.len()
    }
}

impl<const WRITE_SIZE: usize> NorFlash for FakeFlash<WRITE_SIZE> {
    const WRITE_SIZE: usize = WRITE_SIZE;
    const ERASE_SIZE: usize = PAGE_SIZE as usize;

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        self.memory[from as usize..to as usize].fill(0xFF);
        self.erase_count += 1;
        Ok(())
    }

    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        assert_eq!(offset as usize % WRITE_SIZE, 0, "unaligned write");
        let start = offset as usize;
        for (memory_byte, new_byte) in self.memory[start..start + bytes.len()].iter_mut().zip(bytes) {
            assert_eq!(*memory_byte, 0xFF, "wrote over unerased flash");
            *memory_byte = *new_byte;
        }
        Ok(())
    }
}

type ExternalSlot = FakeFlash<1>;
type BootStatePage = FakeFlash<4>;

/// A whole image of `size_in_bytes`, header included, as `tools/image_header.py` writes it.
fn image_linked_at(load_address: u32, size_in_bytes: usize) -> Vec<u8> {
    let code: Vec<u8> = (0..size_in_bytes - IMAGE_HEADER_SIZE as usize)
        .map(|index| (index * 7) as u8)
        .collect();
    let header = ImageHeader {
        image_type: ImageType::Firmware,
        version: Version {
            major: 0,
            minor: 1,
            patch: 0,
        },
        load_address,
        image_size_in_bytes: code.len() as u32,
        image_crc32: crc32(CRC32_INITIAL, &code),
        build_time_in_seconds: 0,
        commit: [0; 8],
        library_offset: 0,
        library_size_in_bytes: 0,
    };
    [header.encode().to_vec(), code].concat()
}

fn firmware_image(size_in_bytes: usize) -> Vec<u8> {
    image_linked_at(FIRMWARE_VECTOR_TABLE, size_in_bytes)
}

fn boot_state_of(boot_state: &mut BootStatePage) -> State {
    let mut alignment = [0; 4];
    block_on(FirmwareState::new(boot_state, &mut alignment).get_state()).unwrap()
}

fn send_in_chunks<'buffer>(
    receiver: &mut UpdateReceiver<'buffer, &mut ExternalSlot, &mut BootStatePage>,
    image: &[u8],
    chunk_size: usize,
) {
    block_on(async {
        receiver.begin(image.len() as u32).unwrap();
        for chunk in image.chunks(chunk_size) {
            receiver.receive(chunk).await.unwrap();
        }
    });
}

// ============================================================================
// Firmware updates
// ============================================================================

#[test]
fn stores_an_image_received_in_chunks_and_marks_a_swap() {
    let mut update_slot = ExternalSlot::erased(DEFAULT_SLOTS.update.size);
    let mut boot_state = BootStatePage::erased(PAGE_SIZE);
    let mut alignment = [0; 4];
    let image = firmware_image(10_000);

    let mut receiver = UpdateReceiver::new(&mut update_slot, &mut boot_state, &mut alignment);
    send_in_chunks(&mut receiver, &image, 244);
    let result = block_on(receiver.finish());

    assert_eq!(result, Ok(()));
    assert_eq!(update_slot.bytes_at(0, image.len()), &image[..]);
    assert_eq!(boot_state_of(&mut boot_state), State::Swap);
}

#[test]
fn refuses_an_image_larger_than_the_firmware_slot() {
    let mut update_slot = ExternalSlot::erased(DEFAULT_SLOTS.update.size);
    let mut boot_state = BootStatePage::erased(PAGE_SIZE);
    let mut alignment = [0; 4];
    let mut receiver = UpdateReceiver::new(&mut update_slot, &mut boot_state, &mut alignment);

    let result = receiver.begin(FIRMWARE.size + 1);

    assert_eq!(
        result,
        Err(UpdateError::ImageTooLarge {
            image_size_in_bytes: FIRMWARE.size + 1,
            largest_size_in_bytes: FIRMWARE.size
        })
    );
}

#[test]
fn refuses_chunks_before_begin_and_beyond_the_announced_size() {
    let mut update_slot = ExternalSlot::erased(DEFAULT_SLOTS.update.size);
    let mut boot_state = BootStatePage::erased(PAGE_SIZE);
    let mut alignment = [0; 4];
    let mut receiver = UpdateReceiver::new(&mut update_slot, &mut boot_state, &mut alignment);

    let before_begin = block_on(receiver.receive(&[0; 16]));
    receiver.begin(16).unwrap();
    let too_much = block_on(receiver.receive(&[0; 17]));

    assert_eq!(before_begin, Err(UpdateError::NotStarted));
    assert_eq!(
        too_much,
        Err(UpdateError::MoreThanAnnounced {
            image_size_in_bytes: 16
        })
    );
}

#[test]
fn will_not_finish_an_incomplete_image() {
    let mut update_slot = ExternalSlot::erased(DEFAULT_SLOTS.update.size);
    let mut boot_state = BootStatePage::erased(PAGE_SIZE);
    let mut alignment = [0; 4];
    let image = firmware_image(1000);

    let mut receiver = UpdateReceiver::new(&mut update_slot, &mut boot_state, &mut alignment);
    let result = block_on(async {
        receiver.begin(1000).unwrap();
        receiver.receive(&image[..600]).await.unwrap();
        receiver.finish().await
    });

    assert_eq!(
        result,
        Err(UpdateError::Incomplete {
            bytes_received: 600,
            image_size_in_bytes: 1000
        })
    );
    assert_eq!(boot_state_of(&mut boot_state), State::Boot);
}

#[test]
fn will_not_swap_in_an_image_that_is_not_firmware() {
    let mut update_slot = ExternalSlot::erased(DEFAULT_SLOTS.update.size);
    let mut boot_state = BootStatePage::erased(PAGE_SIZE);
    let mut alignment = [0; 4];
    let not_firmware = vec![0x42; 2000];

    let mut receiver = UpdateReceiver::new(&mut update_slot, &mut boot_state, &mut alignment);
    send_in_chunks(&mut receiver, &not_firmware, 500);
    let result = block_on(receiver.finish());

    assert_eq!(result, Err(UpdateError::NotFirmware));
    assert_eq!(boot_state_of(&mut boot_state), State::Boot);
}

fn finish_firmware(image: &[u8]) -> (Result<(), UpdateError>, State) {
    let mut update_slot = ExternalSlot::erased(DEFAULT_SLOTS.update.size);
    let mut boot_state = BootStatePage::erased(PAGE_SIZE);
    let mut alignment = [0; 4];

    let mut receiver = UpdateReceiver::new(&mut update_slot, &mut boot_state, &mut alignment);
    send_in_chunks(&mut receiver, image, 500);
    let result = block_on(receiver.finish());
    (result, boot_state_of(&mut boot_state))
}

#[test]
fn will_not_swap_in_a_damaged_image() {
    let mut image = firmware_image(5000);
    image[3000] ^= 0x01;

    assert_eq!(
        finish_firmware(&image),
        (Err(UpdateError::ImageCrcDiffers), State::Boot)
    );
}

#[test]
fn will_not_swap_in_an_image_linked_for_the_first_layout() {
    let image = image_linked_at(0x5000, 5000);

    assert_eq!(
        finish_firmware(&image),
        (Err(UpdateError::NotFirmware), State::Boot)
    );
}

#[test]
fn will_not_swap_in_an_image_shorter_than_its_header_says() {
    let image = firmware_image(5000);

    assert_eq!(
        finish_firmware(&image[..4000]),
        (Err(UpdateError::ImageCrcDiffers), State::Boot)
    );
}

#[test]
fn will_not_swap_in_a_first_layout_image_without_a_header() {
    let mut image = vec![0x42; 2000];
    image[..8].copy_from_slice(&[0x00, 0x00, 0x01, 0x20, 0x01, 0x51, 0x00, 0x00]);

    assert_eq!(
        finish_firmware(&image),
        (Err(UpdateError::NotFirmware), State::Boot)
    );
}

#[test]
fn reads_back_what_was_stored() {
    let mut update_slot = ExternalSlot::erased(DEFAULT_SLOTS.update.size);
    let mut boot_state = BootStatePage::erased(PAGE_SIZE);
    let mut alignment = [0; 4];
    let image = firmware_image(5000);

    let mut receiver = UpdateReceiver::new(&mut update_slot, &mut boot_state, &mut alignment);
    send_in_chunks(&mut receiver, &image, 1000);
    let mut read_back = vec![0; 100];
    block_on(receiver.read_back(4090, &mut read_back)).unwrap();

    assert_eq!(read_back, &image[4090..4190]);
}

// ============================================================================
// Confirming the running firmware
// ============================================================================

#[test]
fn refuses_a_new_update_until_the_running_firmware_is_confirmed() {
    let mut update_slot = ExternalSlot::erased(DEFAULT_SLOTS.update.size);
    let mut boot_state = BootStatePage::erased(PAGE_SIZE);
    let mut alignment = [0; 4];
    let image = firmware_image(3000);
    let mut first_receiver = UpdateReceiver::new(&mut update_slot, &mut boot_state, &mut alignment);
    send_in_chunks(&mut first_receiver, &image, 3000);
    block_on(first_receiver.finish()).unwrap();

    let mut second_receiver = UpdateReceiver::new(&mut update_slot, &mut boot_state, &mut alignment);
    second_receiver.begin(3000).unwrap();
    let result = block_on(second_receiver.receive(&image));

    assert_eq!(result, Err(UpdateError::RunningFirmwareNotConfirmed));
}

#[test]
fn confirming_the_running_firmware_keeps_it() {
    let mut boot_state = BootStatePage::erased(PAGE_SIZE);
    let mut alignment = [0; 4];
    block_on(FirmwareState::new(&mut boot_state, &mut alignment).mark_updated()).unwrap();

    let result = block_on(confirm_running_firmware(&mut boot_state, &mut alignment));

    assert_eq!(result, Ok(()));
    assert_eq!(boot_state_of(&mut boot_state), State::Boot);
}

#[test]
fn a_freshly_swapped_firmware_is_on_trial_until_confirmed() {
    let mut boot_state = BootStatePage::erased(PAGE_SIZE);
    let mut alignment = [0; 4];

    let before = block_on(running_firmware_is_on_trial(&mut boot_state, &mut alignment));
    block_on(FirmwareState::new(&mut boot_state, &mut alignment).mark_updated()).unwrap();
    let after_swap = block_on(running_firmware_is_on_trial(&mut boot_state, &mut alignment));
    block_on(confirm_running_firmware(&mut boot_state, &mut alignment)).unwrap();
    let after_confirm = block_on(running_firmware_is_on_trial(&mut boot_state, &mut alignment));

    assert_eq!(
        (before, after_swap, after_confirm),
        (Ok(false), Ok(true), Ok(false))
    );
}

#[test]
fn confirming_an_already_confirmed_firmware_writes_nothing() {
    let mut boot_state = BootStatePage::erased(PAGE_SIZE);
    let mut alignment = [0; 4];

    block_on(confirm_running_firmware(&mut boot_state, &mut alignment)).unwrap();

    assert_eq!(boot_state.erase_count, 0);
    assert!(boot_state.memory.iter().all(|byte| *byte == 0xFF));
}

/// The page as the bootloader leaves it after rolling an update back.
fn rolled_back_boot_state() -> BootStatePage {
    let mut boot_state = BootStatePage::erased(PAGE_SIZE);
    boot_state.memory[..4].fill(0xC0);
    boot_state
}

#[test]
fn confirming_after_a_rollback_keeps_the_rollback_mark() {
    let mut boot_state = rolled_back_boot_state();
    let mut alignment = [0; 4];

    block_on(confirm_running_firmware(&mut boot_state, &mut alignment)).unwrap();

    assert_eq!(boot_state.erase_count, 0);
    assert_eq!(
        block_on(running_firmware_was_rolled_back(&mut boot_state, &mut alignment)),
        Ok(true)
    );
}

#[test]
fn an_update_is_accepted_after_a_rollback_and_clears_the_mark() {
    let mut update_slot = ExternalSlot::erased(DEFAULT_SLOTS.update.size);
    let mut boot_state = rolled_back_boot_state();
    let mut alignment = [0; 4];
    let image = firmware_image(5000);

    let mut receiver = UpdateReceiver::new(&mut update_slot, &mut boot_state, &mut alignment);
    send_in_chunks(&mut receiver, &image, 1000);
    assert_eq!(block_on(receiver.finish()), Ok(()));

    assert_eq!(boot_state_of(&mut boot_state), State::Swap);
}

#[test]
fn forgetting_a_rollback_clears_its_mark_and_nothing_else() {
    let mut boot_state = rolled_back_boot_state();
    let mut alignment = [0; 4];
    block_on(forget_rollback(&mut boot_state, &mut alignment)).unwrap();
    assert_eq!(
        block_on(running_firmware_was_rolled_back(&mut boot_state, &mut alignment)),
        Ok(false)
    );

    let mut on_trial = BootStatePage::erased(PAGE_SIZE);
    block_on(FirmwareState::new(&mut on_trial, &mut alignment).mark_updated()).unwrap();
    block_on(forget_rollback(&mut on_trial, &mut alignment)).unwrap();
    assert_eq!(boot_state_of(&mut on_trial), State::Swap);
}

#[test]
fn a_fresh_watch_was_not_rolled_back() {
    let mut boot_state = BootStatePage::erased(PAGE_SIZE);
    let mut alignment = [0; 4];
    assert_eq!(
        block_on(running_firmware_was_rolled_back(&mut boot_state, &mut alignment)),
        Ok(false)
    );
}

// ============================================================================
// Recovery firmware
// ============================================================================

#[test]
fn stores_a_recovery_image_with_its_header_magic_written_last() {
    let mut recovery_slot = ExternalSlot::erased(DEFAULT_SLOTS.recovery.size);
    let image = firmware_image(9000);
    let mut receiver = RecoveryReceiver::new(&mut recovery_slot);

    block_on(receiver.begin(image.len() as u32)).unwrap();
    for chunk in image.chunks(3) {
        block_on(receiver.receive(chunk)).unwrap();
    }
    let header_start_before_finish = receiver.recovery_slot.bytes_at(0, 8).to_vec();
    let result = block_on(receiver.finish());

    assert_eq!(header_start_before_finish, [0xFF; 8]);
    assert_eq!(result, Ok(()));
    assert_eq!(recovery_slot.bytes_at(0, image.len()), &image[..]);
}

#[test]
fn recovery_read_back_shows_the_held_header_start() {
    let mut recovery_slot = ExternalSlot::erased(DEFAULT_SLOTS.recovery.size);
    let image = firmware_image(300);
    let mut receiver = RecoveryReceiver::new(&mut recovery_slot);

    block_on(async {
        receiver.begin(image.len() as u32).await.unwrap();
        receiver.receive(&image).await.unwrap();
    });
    let mut read_back = vec![0; 300];
    block_on(receiver.read_back(0, &mut read_back)).unwrap();
    let mut tail = vec![0; 10];
    block_on(receiver.read_back(4, &mut tail)).unwrap();

    assert_eq!(read_back, image);
    assert_eq!(tail, &image[4..14]);
}

#[test]
fn receivers_route_each_image_kind_to_its_slot() {
    let mut update_slot = ExternalSlot::erased(DEFAULT_SLOTS.update.size);
    let mut boot_state = BootStatePage::erased(PAGE_SIZE);
    let mut recovery_slot = ExternalSlot::erased(DEFAULT_SLOTS.recovery.size);
    let mut alignment = [0; 4];
    let firmware = firmware_image(500);
    let recovery = firmware_image(700);

    let mut receivers = Receivers {
        firmware: UpdateReceiver::new(&mut update_slot, &mut boot_state, &mut alignment),
        recovery: RecoveryReceiver::new(&mut recovery_slot),
    };
    block_on(async {
        receivers.begin(ImageKind::Recovery, 700).await.unwrap();
        receivers.receive(ImageKind::Recovery, &recovery).await.unwrap();
        receivers.finish(ImageKind::Recovery).await.unwrap();
        receivers.begin(ImageKind::Firmware, 500).await.unwrap();
        receivers.receive(ImageKind::Firmware, &firmware).await.unwrap();
        receivers.finish(ImageKind::Firmware).await.unwrap();
    });

    assert_eq!(update_slot.bytes_at(0, 500), &firmware[..]);
    assert_eq!(recovery_slot.bytes_at(0, 700), &recovery[..]);
    assert_eq!(boot_state_of(&mut boot_state), State::Swap);
}

#[test]
fn starting_a_recovery_upload_invalidates_the_old_image_at_once() {
    let mut recovery_slot = ExternalSlot::erased(DEFAULT_SLOTS.recovery.size);
    recovery_slot.memory[..8].copy_from_slice(&firmware_image(300)[..8]);

    let mut receiver = RecoveryReceiver::new(&mut recovery_slot);
    block_on(receiver.begin(5000)).unwrap();

    assert_eq!(recovery_slot.bytes_at(0, 8), [0xFF; 8]);
}

#[test]
fn will_not_finish_a_recovery_image_that_is_not_firmware() {
    let mut recovery_slot = ExternalSlot::erased(DEFAULT_SLOTS.recovery.size);
    let mut receiver = RecoveryReceiver::new(&mut recovery_slot);

    block_on(async {
        receiver.begin(100).await.unwrap();
        receiver.receive(&[0x42; 100]).await.unwrap();
    });
    let result = block_on(receiver.finish());

    assert_eq!(result, Err(UpdateError::NotFirmware));
    assert_eq!(recovery_slot.bytes_at(0, 8), [0xFF; 8]);
}

#[test]
fn a_recovery_slot_takes_its_size_from_the_slot_table() {
    let mut recovery_slot = ExternalSlot::erased(64 * 1024);
    let mut receiver = RecoveryReceiver::new(&mut recovery_slot);

    let result = block_on(receiver.begin(64 * 1024 + 1));

    assert_eq!(
        result,
        Err(UpdateError::ImageTooLarge {
            image_size_in_bytes: 64 * 1024 + 1,
            largest_size_in_bytes: 64 * 1024
        })
    );
}

#[test]
fn will_not_finish_a_damaged_recovery_image() {
    let mut recovery_slot = ExternalSlot::erased(DEFAULT_SLOTS.recovery.size);
    let mut image = firmware_image(3000);
    image[2000] ^= 0x80;
    let mut receiver = RecoveryReceiver::new(&mut recovery_slot);

    block_on(async {
        receiver.begin(image.len() as u32).await.unwrap();
        receiver.receive(&image).await.unwrap();
    });
    let result = block_on(receiver.finish());

    assert_eq!(result, Err(UpdateError::ImageCrcDiffers));
    assert_eq!(recovery_slot.bytes_at(0, 8), [0xFF; 8]);
}

// ============================================================================
// Moving the external slots
// ============================================================================

struct SlotTablePage {
    memory: Vec<u8>,
}

impl embedded_storage::nor_flash::ErrorType for SlotTablePage {
    type Error = NorFlashErrorKind;
}

impl embedded_storage::nor_flash::ReadNorFlash for SlotTablePage {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        bytes.copy_from_slice(&self.memory[offset as usize..offset as usize + bytes.len()]);
        Ok(())
    }

    fn capacity(&self) -> usize {
        self.memory.len()
    }
}

impl embedded_storage::nor_flash::NorFlash for SlotTablePage {
    const WRITE_SIZE: usize = 4;
    const ERASE_SIZE: usize = PAGE_SIZE as usize;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        self.memory[from as usize..to as usize].fill(0xFF);
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        for (memory_byte, new_byte) in self.memory[offset as usize..].iter_mut().zip(bytes) {
            *memory_byte &= new_byte;
        }
        Ok(())
    }
}

fn smaller_recovery_slot() -> Slots {
    Slots {
        recovery: Region {
            start: 0x7_9000,
            size: 172 * 1024,
        },
        ..DEFAULT_SLOTS
    }
}

fn empty_slot_table() -> SlotTablePage {
    SlotTablePage {
        memory: vec![0xFF; neotime_boot::SLOT_TABLE.size as usize],
    }
}

#[test]
fn slots_move_while_the_running_firmware_is_confirmed() {
    let mut table = empty_slot_table();
    let mut boot_state = BootStatePage::erased(PAGE_SIZE);
    let mut alignment = [0; 4];

    let result = block_on(change_slots(
        &mut table,
        &mut boot_state,
        &mut alignment,
        smaller_recovery_slot(),
    ));

    assert_eq!(result, Ok(()));
    assert_eq!(
        neotime_boot::slot_table::read_slots(&mut table),
        Ok(smaller_recovery_slot())
    );
}

#[test]
fn slots_stay_put_while_a_firmware_is_on_trial() {
    let mut table = empty_slot_table();
    let mut boot_state = BootStatePage::erased(PAGE_SIZE);
    let mut alignment = [0; 4];
    block_on(FirmwareState::new(&mut boot_state, &mut alignment).mark_updated()).unwrap();

    let result = block_on(change_slots(
        &mut table,
        &mut boot_state,
        &mut alignment,
        smaller_recovery_slot(),
    ));

    assert_eq!(result, Err(UpdateError::RunningFirmwareNotConfirmed));
    assert_eq!(
        neotime_boot::slot_table::read_slots(&mut table),
        Ok(DEFAULT_SLOTS)
    );
}

#[test]
fn unsound_slots_are_refused() {
    let mut table = empty_slot_table();
    let mut boot_state = BootStatePage::erased(PAGE_SIZE);
    let mut alignment = [0; 4];
    let overlapping = Slots {
        recovery: Region {
            start: 0x1000,
            size: 172 * 1024,
        },
        ..DEFAULT_SLOTS
    };

    let result = block_on(change_slots(
        &mut table,
        &mut boot_state,
        &mut alignment,
        overlapping,
    ));

    assert_eq!(result, Err(UpdateError::UnsoundSlots(SlotProblem::Overlapping)));
}
