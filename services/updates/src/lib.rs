//! The on-watch half of a firmware update: store the uploaded image, check it, and hand it to the bootloader.
//!
//! The Bluetooth layer feeds an image in with `begin`, `receive` and `finish`, whatever protocol
//! carried it. `UpdateReceiver` writes the firmware into the update slot through embassy-boot and
//! marks a swap; `RecoveryReceiver` writes the recovery firmware into its own slot. Both accept only an
//! image whose header is sound, says it runs as firmware, and whose CRC matches. A firmware must
//! call `confirm_running_firmware` once it works, or the bootloader rolls it back at the next reset,
//! and no new update is accepted before then. `change_slots` moves the external slots.

#![no_std]

use embassy_boot::{FirmwareState, FirmwareUpdater, FirmwareUpdaterConfig, FirmwareUpdaterError, State};
use embedded_storage_async::nor_flash::NorFlash;
use neotime_boot::{
    image_header::{ImageHeader, IMAGE_HEADER_SIZE},
    migration::{crc32, CRC32_INITIAL},
    slot_table::{write_slots, SlotTableError},
    SlotProblem, Slots, FIRMWARE,
};

// ============================================================================
// Errors
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateError {
    ImageTooLarge {
        image_size_in_bytes: u32,
        largest_size_in_bytes: u32,
    },
    NotStarted,
    MoreThanAnnounced {
        image_size_in_bytes: u32,
    },
    Incomplete {
        bytes_received: u32,
        image_size_in_bytes: u32,
    },
    /// The header is missing or damaged, or the image is linked to run somewhere other than `FIRMWARE`.
    NotFirmware,
    /// The header is sound but the image after it is cut short or damaged.
    ImageCrcDiffers,
    RunningFirmwareNotConfirmed,
    UnsoundSlots(SlotProblem),
    Flash,
}

impl From<FirmwareUpdaterError> for UpdateError {
    fn from(error: FirmwareUpdaterError) -> UpdateError {
        match error {
            FirmwareUpdaterError::BadState => UpdateError::RunningFirmwareNotConfirmed,
            FirmwareUpdaterError::Flash(_) | FirmwareUpdaterError::Signature(_) => UpdateError::Flash,
        }
    }
}

// ============================================================================
// Progress through one image, shared by both receivers
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ImageProgress {
    image_size_in_bytes: u32,
    bytes_received: u32,
}

impl ImageProgress {
    fn start(image_size_in_bytes: u32, largest_size_in_bytes: u32) -> Result<ImageProgress, UpdateError> {
        if image_size_in_bytes > largest_size_in_bytes {
            return Err(UpdateError::ImageTooLarge {
                image_size_in_bytes,
                largest_size_in_bytes,
            });
        }
        Ok(ImageProgress {
            image_size_in_bytes,
            bytes_received: 0,
        })
    }

    fn check_room_for(&self, chunk_length: usize) -> Result<(), UpdateError> {
        if self.bytes_received as usize + chunk_length > self.image_size_in_bytes as usize {
            return Err(UpdateError::MoreThanAnnounced {
                image_size_in_bytes: self.image_size_in_bytes,
            });
        }
        Ok(())
    }

    fn check_complete(&self) -> Result<(), UpdateError> {
        if self.bytes_received != self.image_size_in_bytes {
            return Err(UpdateError::Incomplete {
                bytes_received: self.bytes_received,
                image_size_in_bytes: self.image_size_in_bytes,
            });
        }
        Ok(())
    }
}

fn started(progress: &mut Option<ImageProgress>) -> Result<&mut ImageProgress, UpdateError> {
    progress.as_mut().ok_or(UpdateError::NotStarted)
}

// ============================================================================
// Checking a received image
// ============================================================================

const CRC_CHUNK_SIZE: usize = 256;

/// Holds the header to firmware rules and the image after it to the header's CRC.
async fn check_image<ReadError>(
    header_bytes: &[u8; IMAGE_HEADER_SIZE as usize],
    slot_size_in_bytes: u32,
    received_in_bytes: u32,
    mut read: impl AsyncFnMut(u32, &mut [u8]) -> Result<(), ReadError>,
) -> Result<(), UpdateError> {
    let header = ImageHeader::decode(header_bytes).ok_or(UpdateError::NotFirmware)?;
    if !header.runs_as_firmware() || header.total_size_in_bytes() > slot_size_in_bytes {
        return Err(UpdateError::NotFirmware);
    }
    if header.total_size_in_bytes() > received_in_bytes {
        return Err(UpdateError::ImageCrcDiffers);
    }

    let mut chunk = [0; CRC_CHUNK_SIZE];
    let mut crc = CRC32_INITIAL;
    let mut offset = 0;
    while offset < header.image_size_in_bytes {
        let length = (header.image_size_in_bytes - offset).min(CRC_CHUNK_SIZE as u32) as usize;
        read(IMAGE_HEADER_SIZE + offset, &mut chunk[..length])
            .await
            .map_err(|_| UpdateError::Flash)?;
        crc = crc32(crc, &chunk[..length]);
        offset += length as u32;
    }
    if crc != header.image_crc32 {
        return Err(UpdateError::ImageCrcDiffers);
    }
    Ok(())
}

// ============================================================================
// Firmware updates
// ============================================================================

/// `update_slot` is the `FIRMWARE_UPDATE` region of the external flash and `boot_state` the `BOOT_STATE` page.
pub struct UpdateReceiver<'buffer, UpdateSlot: NorFlash, BootState: NorFlash> {
    updater: FirmwareUpdater<'buffer, UpdateSlot, BootState>,
    progress: Option<ImageProgress>,
}

impl<'buffer, UpdateSlot: NorFlash, BootState: NorFlash> UpdateReceiver<'buffer, UpdateSlot, BootState> {
    /// `state_alignment_buffer` must be at least the boot state flash's write size.
    pub fn new(
        update_slot: UpdateSlot,
        boot_state: BootState,
        state_alignment_buffer: &'buffer mut [u8],
    ) -> Self {
        let config = FirmwareUpdaterConfig {
            dfu: update_slot,
            state: boot_state,
        };
        UpdateReceiver {
            updater: FirmwareUpdater::new(config, state_alignment_buffer),
            progress: None,
        }
    }

    pub fn begin(&mut self, image_size_in_bytes: u32) -> Result<(), UpdateError> {
        self.progress = Some(ImageProgress::start(image_size_in_bytes, FIRMWARE.size)?);
        Ok(())
    }

    /// Chunks must arrive in order; each erases its flash sectors just before writing them.
    pub async fn receive(&mut self, chunk: &[u8]) -> Result<(), UpdateError> {
        let progress = started(&mut self.progress)?;
        progress.check_room_for(chunk.len())?;

        self.updater
            .write_firmware(progress.bytes_received as usize, chunk)
            .await?;
        progress.bytes_received += chunk.len() as u32;
        Ok(())
    }

    /// Lets the protocol above check its own checksum over what was stored.
    pub async fn read_back(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), UpdateError> {
        self.updater.read_dfu(offset, bytes).await?;
        Ok(())
    }

    /// Forgets a half-received image; the next `begin` starts over.
    pub fn abort(&mut self) {
        self.progress = None;
    }

    /// Checks the image's header and CRC, then asks the bootloader to swap it in at the next reset.
    pub async fn finish(&mut self) -> Result<(), UpdateError> {
        let progress = *started(&mut self.progress)?;
        progress.check_complete()?;

        let mut header_bytes = [0; IMAGE_HEADER_SIZE as usize];
        self.updater.read_dfu(0, &mut header_bytes).await?;
        let updater = &mut self.updater;
        check_image(
            &header_bytes,
            FIRMWARE.size,
            progress.bytes_received,
            async |offset, bytes: &mut [u8]| updater.read_dfu(offset, bytes).await,
        )
        .await?;

        self.updater.mark_updated().await?;
        self.progress = None;
        Ok(())
    }
}

/// True right after the bootloader swapped this firmware in, until it is confirmed or rolled back.
pub async fn running_firmware_is_on_trial<BootState: NorFlash>(
    boot_state: BootState,
    state_alignment_buffer: &mut [u8],
) -> Result<bool, UpdateError> {
    let mut firmware_state = FirmwareState::new(boot_state, state_alignment_buffer);
    Ok(firmware_state.get_state().await? == State::Swap)
}

/// Tells the bootloader a firmware on trial works, so it is kept; a rollback's mark is left, so the screen can still say so.
pub async fn confirm_running_firmware<BootState: NorFlash>(
    boot_state: BootState,
    state_alignment_buffer: &mut [u8],
) -> Result<(), UpdateError> {
    let mut firmware_state = FirmwareState::new(boot_state, state_alignment_buffer);
    if firmware_state.get_state().await? == State::Swap {
        firmware_state.mark_booted().await?;
    }
    Ok(())
}

/// Clears a rollback's mark, for when the rollback was meant, as after the bootloader updater.
pub async fn forget_rollback<BootState: NorFlash>(
    boot_state: BootState,
    state_alignment_buffer: &mut [u8],
) -> Result<(), UpdateError> {
    let mut firmware_state = FirmwareState::new(boot_state, state_alignment_buffer);
    if firmware_state.get_state().await? == State::Revert {
        firmware_state.mark_booted().await?;
    }
    Ok(())
}

/// Whether the bootloader rolled back the last update; true until the next update is marked.
pub async fn running_firmware_was_rolled_back<BootState: NorFlash>(
    boot_state: BootState,
    state_alignment_buffer: &mut [u8],
) -> Result<bool, UpdateError> {
    let mut firmware_state = FirmwareState::new(boot_state, state_alignment_buffer);
    Ok(firmware_state.get_state().await? == State::Revert)
}

// ============================================================================
// Recovery firmware
// ============================================================================

/// The first bytes of the header, which hold its magic; written last, they are what makes the bootloader see an image.
const HELD_BYTE_COUNT: usize = 8;

/// `recovery_slot` is the slot table's recovery region of the external flash, written a byte at a time.
pub struct RecoveryReceiver<RecoverySlot: NorFlash> {
    recovery_slot: RecoverySlot,
    progress: Option<ImageProgress>,
    last_erased_sector: Option<u32>,
    held_header_start: [u8; HELD_BYTE_COUNT],
}

impl<RecoverySlot: NorFlash> RecoveryReceiver<RecoverySlot> {
    pub fn new(recovery_slot: RecoverySlot) -> Self {
        assert_eq!(
            RecoverySlot::WRITE_SIZE,
            1,
            "the recovery slot must take writes of any length"
        );
        RecoveryReceiver {
            recovery_slot,
            progress: None,
            last_erased_sector: None,
            held_header_start: [0; HELD_BYTE_COUNT],
        }
    }

    /// Erases the start of the slot at once, so a half-written image is never taken for a whole one.
    pub async fn begin(&mut self, image_size_in_bytes: u32) -> Result<(), UpdateError> {
        let progress = ImageProgress::start(image_size_in_bytes, self.slot_size_in_bytes())?;

        self.erase_sector(0).await?;
        self.progress = Some(progress);
        Ok(())
    }

    /// The start of the header is held back and written by `finish`, since it is what makes the bootloader see an image.
    pub async fn receive(&mut self, chunk: &[u8]) -> Result<(), UpdateError> {
        let progress = *started(&mut self.progress)?;
        progress.check_room_for(chunk.len())?;

        let mut written = 0;
        while written < chunk.len() {
            let offset = progress.bytes_received + written as u32;
            let sector = offset / RecoverySlot::ERASE_SIZE as u32;
            if self.last_erased_sector != Some(sector) {
                self.erase_sector(sector).await?;
            }
            let room_left_in_sector = (sector + 1) * RecoverySlot::ERASE_SIZE as u32 - offset;
            let piece = &chunk[written..written + (room_left_in_sector as usize).min(chunk.len() - written)];

            let held_byte_count = (HELD_BYTE_COUNT as u32)
                .saturating_sub(offset)
                .min(piece.len() as u32);
            if held_byte_count > 0 {
                let held_range = offset as usize..(offset + held_byte_count) as usize;
                self.held_header_start[held_range].copy_from_slice(&piece[..held_byte_count as usize]);
            }

            let rest_of_piece = &piece[held_byte_count as usize..];
            if !rest_of_piece.is_empty() {
                self.recovery_slot
                    .write(offset + held_byte_count, rest_of_piece)
                    .await
                    .map_err(|_| UpdateError::Flash)?;
            }
            written += piece.len();
        }

        started(&mut self.progress)?.bytes_received += chunk.len() as u32;
        Ok(())
    }

    /// Checks the image's header and CRC, then writes the held bytes that make it whole.
    pub async fn finish(&mut self) -> Result<(), UpdateError> {
        let progress = *started(&mut self.progress)?;
        progress.check_complete()?;

        let mut header_bytes = [0; IMAGE_HEADER_SIZE as usize];
        self.read_back(0, &mut header_bytes).await?;
        let slot_size_in_bytes = self.slot_size_in_bytes();
        let recovery_slot = &mut self.recovery_slot;
        check_image(
            &header_bytes,
            slot_size_in_bytes,
            progress.bytes_received,
            async |offset, bytes: &mut [u8]| recovery_slot.read(offset, bytes).await,
        )
        .await?;

        self.recovery_slot
            .write(0, &self.held_header_start)
            .await
            .map_err(|_| UpdateError::Flash)?;
        self.progress = None;
        Ok(())
    }

    /// Reads the image as it will be once finished, with the held-back bytes in place.
    pub async fn read_back(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), UpdateError> {
        self.recovery_slot
            .read(offset, bytes)
            .await
            .map_err(|_| UpdateError::Flash)?;

        let held_end = HELD_BYTE_COUNT as u32;
        if offset < held_end {
            let held_byte_count = (held_end - offset).min(bytes.len() as u32) as usize;
            let held_range = offset as usize..offset as usize + held_byte_count;
            bytes[..held_byte_count].copy_from_slice(&self.held_header_start[held_range]);
        }
        Ok(())
    }

    pub fn abort(&mut self) {
        self.progress = None;
    }

    pub fn release(self) -> RecoverySlot {
        self.recovery_slot
    }

    fn slot_size_in_bytes(&self) -> u32 {
        self.recovery_slot.capacity() as u32
    }

    async fn erase_sector(&mut self, sector: u32) -> Result<(), UpdateError> {
        let sector_start = sector * RecoverySlot::ERASE_SIZE as u32;
        self.recovery_slot
            .erase(sector_start, sector_start + RecoverySlot::ERASE_SIZE as u32)
            .await
            .map_err(|_| UpdateError::Flash)?;
        self.last_erased_sector = Some(sector);
        Ok(())
    }
}

// ============================================================================
// Moving the external slots
// ============================================================================

/// Writes a new slot table record, refused while a swap is pending or a firmware is on trial.
///
/// Moving a slot does not move what is in it: the image the old slot held is gone, so send the
/// recovery firmware again after moving its slot.
pub async fn change_slots<SlotTable: embedded_storage::nor_flash::NorFlash, BootState: NorFlash>(
    slot_table: &mut SlotTable,
    boot_state: BootState,
    state_alignment_buffer: &mut [u8],
    slots: Slots,
) -> Result<(), UpdateError> {
    let mut firmware_state = FirmwareState::new(boot_state, state_alignment_buffer);
    if firmware_state.get_state().await? == State::Swap {
        return Err(UpdateError::RunningFirmwareNotConfirmed);
    }

    write_slots(slot_table, slots).map_err(|error| match error {
        SlotTableError::Unsound(problem) => UpdateError::UnsoundSlots(problem),
        SlotTableError::Flash(_) | SlotTableError::ReadBackDiffers => UpdateError::Flash,
    })
}

// ============================================================================
// Both receivers behind one door, for the upload protocol
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageKind {
    Firmware,
    Recovery,
}

/// What an upload protocol needs from the watch, whichever slot an image is for.
#[allow(async_fn_in_trait)]
pub trait UpdateTargets {
    async fn begin(&mut self, kind: ImageKind, image_size_in_bytes: u32) -> Result<(), UpdateError>;
    async fn receive(&mut self, kind: ImageKind, chunk: &[u8]) -> Result<(), UpdateError>;
    async fn read_back(&mut self, kind: ImageKind, offset: u32, bytes: &mut [u8]) -> Result<(), UpdateError>;
    async fn finish(&mut self, kind: ImageKind) -> Result<(), UpdateError>;
    fn abort(&mut self, kind: ImageKind);
}

pub struct Receivers<'buffer, UpdateSlot: NorFlash, BootState: NorFlash, RecoverySlot: NorFlash> {
    pub firmware: UpdateReceiver<'buffer, UpdateSlot, BootState>,
    pub recovery: RecoveryReceiver<RecoverySlot>,
}

impl<UpdateSlot: NorFlash, BootState: NorFlash, RecoverySlot: NorFlash> UpdateTargets
    for Receivers<'_, UpdateSlot, BootState, RecoverySlot>
{
    async fn begin(&mut self, kind: ImageKind, image_size_in_bytes: u32) -> Result<(), UpdateError> {
        match kind {
            ImageKind::Firmware => self.firmware.begin(image_size_in_bytes),
            ImageKind::Recovery => self.recovery.begin(image_size_in_bytes).await,
        }
    }

    async fn receive(&mut self, kind: ImageKind, chunk: &[u8]) -> Result<(), UpdateError> {
        match kind {
            ImageKind::Firmware => self.firmware.receive(chunk).await,
            ImageKind::Recovery => self.recovery.receive(chunk).await,
        }
    }

    async fn read_back(&mut self, kind: ImageKind, offset: u32, bytes: &mut [u8]) -> Result<(), UpdateError> {
        match kind {
            ImageKind::Firmware => self.firmware.read_back(offset, bytes).await,
            ImageKind::Recovery => self.recovery.read_back(offset, bytes).await,
        }
    }

    async fn finish(&mut self, kind: ImageKind) -> Result<(), UpdateError> {
        match kind {
            ImageKind::Firmware => self.firmware.finish().await,
            ImageKind::Recovery => self.recovery.finish().await,
        }
    }

    fn abort(&mut self, kind: ImageKind) {
        match kind {
            ImageKind::Firmware => self.firmware.abort(),
            ImageKind::Recovery => self.recovery.abort(),
        }
    }
}

#[cfg(test)]
mod tests;
