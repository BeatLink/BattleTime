//! Where everything lives in the PineTime's two flash chips, and how the recovery firmware is installed.
//!
//! The bootloader and the firmware both build on these numbers, so they must change together. The
//! running firmware sits in internal flash; updates and the recovery firmware wait in external flash
//! until the bootloader swaps them in with embassy-boot. Every image starts with an `ImageHeader`, and
//! where the external slots sit is read from the `slot_table`, which the firmware may change.

#![no_std]

use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};

// ============================================================================
// Regions
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub start: u32,
    pub size: u32,
}

impl Region {
    pub const fn end(self) -> u32 {
        self.start + self.size
    }

    pub const fn contains(self, address: u32) -> bool {
        address >= self.start && address < self.end()
    }
}

pub const PAGE_SIZE: u32 = 4096;

// ============================================================================
// Internal flash, 512 KB
// ============================================================================

pub const INTERNAL_FLASH_SIZE: u32 = 512 * 1024;

/// There is no Nordic master boot record; the bootloader is the first code to run.
pub const BOOTLOADER: Region = Region {
    start: 0x0000_0000,
    size: 20 * 1024,
};
/// Two pages that record where the external slots sit; see `slot_table`.
pub const SLOT_TABLE: Region = Region {
    start: BOOTLOADER.end(),
    size: 2 * PAGE_SIZE,
};
pub const BOOT_STATE: Region = Region {
    start: SLOT_TABLE.end(),
    size: PAGE_SIZE,
};
/// The running firmware, to the end of flash: its `ImageHeader`, its code from `FIRMWARE_VECTOR_TABLE`
/// on, then the shared library that apps use, where the header says.
pub const FIRMWARE: Region = Region {
    start: BOOT_STATE.end(),
    size: INTERNAL_FLASH_SIZE - BOOT_STATE.end(),
};
/// Where the firmware is linked to start; the bootloader jumps here.
pub const FIRMWARE_VECTOR_TABLE: u32 = FIRMWARE.start + image_header::IMAGE_HEADER_SIZE;

// ============================================================================
// External flash, 4 MB
// ============================================================================

pub const EXTERNAL_FLASH_SIZE: u32 = 4 * 1024 * 1024;

/// Where the update and recovery slots may sit: everything below the file system.
pub const SLOT_AREA: Region = Region {
    start: 0,
    size: FILE_SYSTEM.start,
};
pub const FILE_SYSTEM: Region = Region {
    start: 1024 * 1024,
    size: EXTERNAL_FLASH_SIZE - 1024 * 1024,
};

/// Where the external slots are when the slot table holds no sound record.
pub const DEFAULT_SLOTS: Slots = Slots {
    update: Region {
        start: 0x0000_0000,
        size: FIRMWARE.size + PAGE_SIZE,
    },
    // Clear of every slot the 388 KB layout used, so the move to this layout can write it first.
    recovery: Region {
        start: 0x000C_6000,
        size: SLOT_AREA.end() - 0x000C_6000,
    },
};

/// The two slots in external flash; the firmware's own slot in internal flash never moves, since it is linked there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slots {
    /// A new firmware waiting to be swapped in; after a swap it holds the replaced firmware one page up.
    pub update: Region,
    pub recovery: Region,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotProblem {
    NotWholePages,
    OutsideSlotArea,
    UpdateSlotNotFirmwareSizePlusOnePage,
    RecoverySlotTooSmall,
    RecoverySlotTooLarge,
    Overlapping,
}

impl Slots {
    /// Where the replaced firmware sits after the bootloader's swap: embassy-boot moves each page one page up.
    pub const fn previous_firmware(self) -> Region {
        Region {
            start: self.update.start + PAGE_SIZE,
            size: FIRMWARE.size,
        }
    }

    /// The rules the bootloader holds every slot table record to before it trusts one.
    pub const fn check(self) -> Result<(), SlotProblem> {
        let regions = [self.update, self.recovery];
        let mut index = 0;
        while index < regions.len() {
            let region = regions[index];
            if !region.start.is_multiple_of(PAGE_SIZE) || !region.size.is_multiple_of(PAGE_SIZE) {
                return Err(SlotProblem::NotWholePages);
            }
            let end = region.start as u64 + region.size as u64;
            if end > SLOT_AREA.end() as u64 {
                return Err(SlotProblem::OutsideSlotArea);
            }
            index += 1;
        }
        if self.update.size != FIRMWARE.size + PAGE_SIZE {
            return Err(SlotProblem::UpdateSlotNotFirmwareSizePlusOnePage);
        }
        if self.recovery.size < MINIMUM_RECOVERY_SLOT_SIZE {
            return Err(SlotProblem::RecoverySlotTooSmall);
        }
        if self.recovery.size > FIRMWARE.size {
            return Err(SlotProblem::RecoverySlotTooLarge);
        }
        if self.update.start < self.recovery.end() && self.recovery.start < self.update.end() {
            return Err(SlotProblem::Overlapping);
        }
        Ok(())
    }
}

/// A header and some code: anything smaller cannot hold a firmware.
pub const MINIMUM_RECOVERY_SLOT_SIZE: u32 = 16 * PAGE_SIZE;

// ============================================================================
// RAM and the watchdog
// ============================================================================

pub const RAM: Region = Region {
    start: 0x2000_0000,
    size: 64 * 1024,
};

/// The last words of RAM, where a running firmware writes `FIRMWARE_ALIVE_MAGIC` so an emulator can see it started.
pub const FIRMWARE_ALIVE_MARKER: Region = Region {
    start: RAM.end() - 16,
    size: 16,
};
pub const FIRMWARE_ALIVE_MAGIC: u32 = 0x4E45_4F54;

/// The firmware must start the watchdog with this same timeout, or it cannot take the running one over.
pub const WATCHDOG_TIMEOUT_IN_TICKS: u32 = 5 * WATCHDOG_TICKS_PER_SECOND;
pub const WATCHDOG_TICKS_PER_SECOND: u32 = 32768;

/// How long the button must stay held after a reset into the bootloader to open its menu.
pub const MENU_BUTTON_HOLD_IN_MILLISECONDS: u32 = 5000;
/// A press in the bootloader's menu held this long picks the option; a shorter one moves to the next.
pub const MENU_SELECT_HOLD_IN_MILLISECONDS: u32 = 1000;
/// The menu starts the firmware after this long without a press, so a watch is never left in it.
pub const MENU_TIMEOUT_IN_MILLISECONDS: u32 = 30_000;

// ============================================================================
// What the bootloader tells the firmware, and the status colours
// ============================================================================

/// The third word of `FIRMWARE_ALIVE_MARKER`, where the bootloader notes why it started the firmware this time.
pub const BOOT_NOTE_ADDRESS: u32 = FIRMWARE_ALIVE_MARKER.start + 8;
/// The top half tells a real note from whatever RAM held at power-on.
const BOOT_NOTE_MAGIC: u32 = 0xB007_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootNote {
    /// Nothing special: the firmware's own boot state says the rest.
    Normal = 1,
    /// The button hold installed the recovery firmware just now.
    RecoveryInstalled = 2,
}

impl BootNote {
    pub const fn to_word(self) -> u32 {
        BOOT_NOTE_MAGIC | self as u32
    }

    pub const fn from_word(word: u32) -> Option<BootNote> {
        match word {
            word if word == BootNote::Normal.to_word() => Some(BootNote::Normal),
            word if word == BootNote::RecoveryInstalled.to_word() => Some(BootNote::RecoveryInstalled),
            _ => None,
        }
    }
}

/// The fourth word of `FIRMWARE_ALIVE_MARKER`, where the bootloader updater leaves word for the firmware it hands back to.
pub const UPDATER_NOTE_ADDRESS: u32 = FIRMWARE_ALIVE_MARKER.start + 12;
const UPDATER_NOTE_MAGIC: u32 = 0xB10A_0000;

/// The updater runs on trial and never confirms, so the firmware before it comes back as a rollback; this says the rollback was meant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdaterNote {
    /// A new bootloader was written and checked.
    BootloaderWritten = 1,
    /// The bootloader already matched the one the updater carries, so nothing was written.
    BootloaderAlreadyCurrent = 2,
}

impl UpdaterNote {
    pub const fn to_word(self) -> u32 {
        UPDATER_NOTE_MAGIC | self as u32
    }

    pub const fn from_word(word: u32) -> Option<UpdaterNote> {
        match word {
            word if word == UpdaterNote::BootloaderWritten.to_word() => Some(UpdaterNote::BootloaderWritten),
            word if word == UpdaterNote::BootloaderAlreadyCurrent.to_word() => {
                Some(UpdaterNote::BootloaderAlreadyCurrent)
            }
            _ => None,
        }
    }
}

/// Screen backgrounds as RGB565, one for each mode, so the mode shows from across the room.
pub mod status_colour {
    /// The bootloader at work: installing, rolling back or waiting for the recovery hold.
    pub const BOOTLOADER: u16 = 0x00EB;
    /// A firmware that has confirmed itself.
    pub const CONFIRMED: u16 = 0x0000;
    /// A new firmware that has not confirmed itself yet; the next reset rolls it back.
    pub const ON_TRIAL: u16 = 0x6A00;
    /// The previous firmware, back because an update never confirmed itself.
    pub const ROLLED_BACK: u16 = 0x6800;
    /// The recovery firmware.
    pub const RECOVERY: u16 = 0x480C;
}

// ============================================================================
// Layout checks
// ============================================================================

const BOOT_STATE_WRITE_SIZE: u32 = 4;
const FIRMWARE_PAGES: u32 = FIRMWARE.size / PAGE_SIZE;
/// The Cortex-M4 wants the vector table aligned to a power of two at least its own size, 220 bytes here.
const VECTOR_TABLE_ALIGNMENT: u32 = 256;

const _: () = {
    let regions = [BOOTLOADER, SLOT_TABLE, BOOT_STATE, FIRMWARE, FILE_SYSTEM];
    let mut index = 0;
    while index < regions.len() {
        assert!(
            regions[index].start.is_multiple_of(PAGE_SIZE) && regions[index].size.is_multiple_of(PAGE_SIZE)
        );
        index += 1;
    }

    assert!(FIRMWARE.end() == INTERNAL_FLASH_SIZE);
    assert!(FILE_SYSTEM.end() == EXTERNAL_FLASH_SIZE);
    assert!(FIRMWARE_VECTOR_TABLE.is_multiple_of(VECTOR_TABLE_ALIGNMENT));
    assert!(DEFAULT_SLOTS.check().is_ok());

    // These are the rules embassy-boot checks at boot; breaking them here fails the build instead.
    assert!(DEFAULT_SLOTS.update.size >= FIRMWARE.size + PAGE_SIZE);
    assert!(2 + 4 * FIRMWARE_PAGES <= BOOT_STATE.size / BOOT_STATE_WRITE_SIZE);
};

// ============================================================================
// Bootloader image check
// ============================================================================

pub const VECTOR_TABLE_CHECK_SIZE: usize = 8;

/// Checks the vector table at the start of a bootloader image: a stack pointer in RAM and a reset handler in `BOOTLOADER`.
pub fn is_bootloader_vector_table(vector_table_start: [u8; VECTOR_TABLE_CHECK_SIZE]) -> bool {
    let [stack_pointer, reset_handler] = [&vector_table_start[..4], &vector_table_start[4..]]
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()));
    let stack_pointer_is_in_ram = stack_pointer > RAM.start && stack_pointer <= RAM.end();
    let reset_handler_is_thumb_code = reset_handler & 1 == 1;
    let reset_handler_is_in_bootloader = BOOTLOADER.contains(reset_handler & !1);

    stack_pointer_is_in_ram && reset_handler_is_thumb_code && reset_handler_is_in_bootloader
}

/// Names the installed bootloader by a CRC-32 of the `BOOTLOADER` region, or gives none when it holds no bootloader.
///
/// The erased tail is left off, so the number matches `zlib.crc32` of the bootloader's `.bin` file.
pub fn bootloader_identity<Flash: ReadNorFlash>(flash: &mut Flash) -> Result<Option<u32>, Flash::Error> {
    let mut vector_table_start = [0; VECTOR_TABLE_CHECK_SIZE];
    flash.read(BOOTLOADER.start, &mut vector_table_start)?;
    if !is_bootloader_vector_table(vector_table_start) {
        return Ok(None);
    }

    let mut chunk = [0; IDENTITY_CHUNK_SIZE];
    let mut image_end = BOOTLOADER.end();
    while image_end > BOOTLOADER.start {
        let chunk_start = image_end - (image_end - BOOTLOADER.start).min(IDENTITY_CHUNK_SIZE as u32);
        let bytes = &mut chunk[..(image_end - chunk_start) as usize];
        flash.read(chunk_start, bytes)?;
        match bytes.iter().rposition(|&byte| byte != ERASED_BYTE) {
            Some(last) => {
                image_end = chunk_start + last as u32 + 1;
                break;
            }
            None => image_end = chunk_start,
        }
    }

    let mut crc = migration::CRC32_INITIAL;
    let mut offset = BOOTLOADER.start;
    while offset < image_end {
        let bytes = &mut chunk[..(image_end - offset).min(IDENTITY_CHUNK_SIZE as u32) as usize];
        flash.read(offset, bytes)?;
        crc = migration::crc32(crc, bytes);
        offset += bytes.len() as u32;
    }
    Ok(Some(crc))
}

/// What a flash byte reads as after an erase.
const ERASED_BYTE: u8 = 0xFF;
const IDENTITY_CHUNK_SIZE: usize = 256;

/// True when `FIRMWARE` holds a sound header for an image linked to run there; the image itself is not read.
pub fn firmware_is_runnable<Flash: ReadNorFlash>(internal_flash: &mut Flash) -> Result<bool, Flash::Error> {
    Ok(image_header::read_image_header(internal_flash, FIRMWARE.start)?
        .is_some_and(|header| header.runs_as_firmware()))
}

// ============================================================================
// Installing the recovery or the previous firmware
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageCopy {
    Copied,
    NoImage,
}

const COPY_CHUNK_SIZE: usize = 1024;

/// Copies the image in `source` into the update slot, so the next swap installs it like any update.
///
/// Only a whole image with a matching CRC is copied, and the update slot is left alone otherwise.
pub fn copy_image_into_update_slot<Flash: NorFlash>(
    external_flash: &mut Flash,
    slots: Slots,
    source: Region,
) -> Result<ImageCopy, Flash::Error> {
    let Some(header) = image_header::installable_image(external_flash, source.start, source.size)? else {
        return Ok(ImageCopy::NoImage);
    };

    external_flash.erase(slots.update.start, slots.update.end())?;

    let mut chunk = [0; COPY_CHUNK_SIZE];
    for offset in (0..header.total_size_in_bytes()).step_by(COPY_CHUNK_SIZE) {
        let length = (header.total_size_in_bytes() - offset).min(COPY_CHUNK_SIZE as u32) as usize;
        external_flash.read(source.start + offset, &mut chunk[..length])?;
        external_flash.write(slots.update.start + offset, &chunk[..length])?;
    }

    Ok(ImageCopy::Copied)
}

/// Moves the previous firmware down a page to the start of the update slot, so the next swap installs it.
///
/// Each page is erased only after the page above it, which holds its new contents, is still intact,
/// so the copy can go upwards in place. A power cut part way loses the previous firmware but leaves
/// the running one untouched.
pub fn copy_previous_into_update_slot<Flash: NorFlash>(
    external_flash: &mut Flash,
    slots: Slots,
) -> Result<ImageCopy, Flash::Error> {
    let previous = slots.previous_firmware();
    if image_header::installable_image(external_flash, previous.start, previous.size)?.is_none() {
        return Ok(ImageCopy::NoImage);
    }

    let mut chunk = [0; COPY_CHUNK_SIZE];
    for page_start in (slots.update.start..previous.end()).step_by(PAGE_SIZE as usize) {
        external_flash.erase(page_start, page_start + PAGE_SIZE)?;
        for offset in (0..PAGE_SIZE).step_by(COPY_CHUNK_SIZE) {
            external_flash.read(page_start + PAGE_SIZE + offset, &mut chunk)?;
            let chunk_is_erased = chunk.iter().all(|byte| *byte == ERASED_BYTE);
            if !chunk_is_erased {
                external_flash.write(page_start + offset, &chunk)?;
            }
        }
    }
    let last_page = slots.update.end() - PAGE_SIZE;
    external_flash.erase(last_page, slots.update.end())?;

    Ok(ImageCopy::Copied)
}

// ============================================================================
// Moving the recovery slot
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryMove {
    /// The slot keeps its start, and the image there still fits.
    KeptInPlace,
    /// The image was copied to the new slot and read back whole.
    Copied,
    /// The old slot held no whole image, so there was nothing to keep.
    NoImage,
    /// The image is larger than the new slot.
    ImageTooLarge,
    /// The new slot overlaps the old one at another start, so the copy would overwrite its own source.
    Overlapping,
    /// The copy did not read back whole.
    CopyDamaged,
}

impl RecoveryMove {
    /// True when the slot table may now name the new slot.
    pub const fn may_switch(self) -> bool {
        matches!(
            self,
            RecoveryMove::KeptInPlace | RecoveryMove::Copied | RecoveryMove::NoImage
        )
    }
}

/// Small, since the firmware runs this on its Bluetooth task's stack.
const MOVE_CHUNK_SIZE: usize = 256;

/// Carries the recovery image from `from` to `to` before the slot table names `to`, so moving the slot never loses it.
pub fn move_recovery<Flash: NorFlash>(
    external_flash: &mut Flash,
    from: Region,
    to: Region,
) -> Result<RecoveryMove, Flash::Error> {
    let Some(header) = image_header::installable_image(external_flash, from.start, from.size)? else {
        return Ok(RecoveryMove::NoImage);
    };
    if header.total_size_in_bytes() > to.size {
        return Ok(RecoveryMove::ImageTooLarge);
    }
    if to.start == from.start {
        return Ok(RecoveryMove::KeptInPlace);
    }
    if to.start < from.end() && from.start < to.end() {
        return Ok(RecoveryMove::Overlapping);
    }

    let length = header.total_size_in_bytes();
    let erase_end = to.start + length.div_ceil(PAGE_SIZE) * PAGE_SIZE;
    external_flash.erase(to.start, erase_end)?;
    let mut chunk = [0; MOVE_CHUNK_SIZE];
    for offset in (0..length).step_by(MOVE_CHUNK_SIZE) {
        let piece = &mut chunk[..(length - offset).min(MOVE_CHUNK_SIZE as u32) as usize];
        external_flash.read(from.start + offset, piece)?;
        external_flash.write(to.start + offset, piece)?;
    }

    match image_header::installable_image(external_flash, to.start, to.size)? {
        Some(_) => Ok(RecoveryMove::Copied),
        None => Ok(RecoveryMove::CopyDamaged),
    }
}

#[cfg(feature = "build-script")]
pub mod build_script;
pub mod image_header;
pub mod migration;
pub mod slot_table;
pub mod version;

#[cfg(test)]
mod tests;
