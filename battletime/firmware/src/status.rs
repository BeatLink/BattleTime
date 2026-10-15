//! Which mode the firmware is running in, and how the status page shows it: a background colour and plain words.

use battletime_boot::{
    status_colour, BootNote, UpdaterNote, BOOTLOADER, BOOT_NOTE_ADDRESS, UPDATER_NOTE_ADDRESS,
};
use embedded_graphics::pixelcolor::{raw::RawU16, Rgb565};
use embedded_storage::nor_flash::{ErrorType, NorFlashErrorKind, ReadNorFlash};

// ============================================================================
// Modes
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The firmware has confirmed itself and stays.
    Confirmed,
    /// A new firmware the bootloader just swapped in; it confirms after its first Bluetooth exchange.
    OnTrial,
    /// The previous firmware, back because the last update never confirmed itself.
    RolledBack,
    /// The recovery firmware, installed by holding the button.
    Recovery,
    /// Back after the bootloader updater wrote a new bootloader; the rollback that brought it back was meant.
    BootloaderUpdated,
    /// Back after the bootloader updater found the bootloader already current.
    BootloaderAlreadyCurrent,
}

impl Mode {
    pub fn background(self) -> Rgb565 {
        let colour = match self {
            Mode::Confirmed | Mode::BootloaderUpdated | Mode::BootloaderAlreadyCurrent => {
                status_colour::CONFIRMED
            }
            Mode::OnTrial => status_colour::ON_TRIAL,
            Mode::RolledBack => status_colour::ROLLED_BACK,
            Mode::Recovery => status_colour::RECOVERY,
        };
        Rgb565::from(RawU16::new(colour))
    }

    /// The firmware line's value, short enough to follow its label.
    pub fn word(self) -> &'static str {
        match self {
            Mode::Confirmed | Mode::BootloaderUpdated | Mode::BootloaderAlreadyCurrent => "OK",
            Mode::OnTrial => "On trial",
            Mode::RolledBack => "Rolled back",
            Mode::Recovery => "Recovery",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            Mode::Confirmed => "Idle",
            Mode::OnTrial => "Connect to keep it",
            Mode::RolledBack => "Previous one restored",
            Mode::Recovery => "Send a new firmware",
            Mode::BootloaderUpdated => "Bootloader updated",
            Mode::BootloaderAlreadyCurrent => "Bootloader up to date",
        }
    }
}

// ============================================================================
// What the bootloader noted
// ============================================================================

/// True when the bootloader installed the recovery firmware on its way to starting this one.
pub fn recovery_was_just_installed() -> bool {
    let word = unsafe { (BOOT_NOTE_ADDRESS as *const u32).read_volatile() };
    BootNote::from_word(word) == Some(BootNote::RecoveryInstalled)
}

/// Reads and clears the bootloader updater's note, so it speaks for one start only.
pub fn take_updater_note() -> Option<UpdaterNote> {
    let address = UPDATER_NOTE_ADDRESS as *mut u32;
    let note = UpdaterNote::from_word(unsafe { address.read_volatile() });
    unsafe { address.write_volatile(0) };
    note
}

/// The bootloader's pages, read with plain loads in assembly.
///
/// The bootloader starts at address 0, the null pointer to Rust, so reading it through a pointer or
/// the flash controller's driver is undefined behaviour, and the compiler drops the code after it.
pub struct BootloaderPages;

impl ErrorType for BootloaderPages {
    type Error = NorFlashErrorKind;
}

impl ReadNorFlash for BootloaderPages {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        if offset as usize + bytes.len() > BOOTLOADER.end() as usize {
            return Err(NorFlashErrorKind::OutOfBounds);
        }
        for (address, byte) in (offset as usize..).zip(bytes.iter_mut()) {
            let value: u32;
            unsafe {
                core::arch::asm!(
                    "ldrb {value}, [{address}]",
                    address = in(reg) address,
                    value = lateout(reg) value,
                    options(nostack, readonly, preserves_flags),
                );
            }
            *byte = value as u8;
        }
        Ok(())
    }

    fn capacity(&self) -> usize {
        BOOTLOADER.end() as usize
    }
}

/// The bootloader line's value: the first hex digits of its CRC-32, after "New" when the updater just wrote it.
pub fn bootloader_words(identity: Result<Option<u32>, ()>, mode: Mode) -> heapless::String<11> {
    let mut words = heapless::String::new();
    let _ = match identity {
        Ok(Some(crc)) if mode == Mode::BootloaderUpdated => {
            core::fmt::write(&mut words, format_args!("New {:07x}", crc >> 4))
        }
        Ok(Some(crc)) => core::fmt::write(&mut words, format_args!("OK {:07x}", crc >> 4)),
        Ok(None) => words.push_str("Not found").map_err(|_| core::fmt::Error),
        Err(()) => words.push_str("Unreadable").map_err(|_| core::fmt::Error),
    };
    words
}

/// A build made for the recovery slot is always the recovery firmware, however it was installed.
pub const IS_RECOVERY_BUILD: bool = cfg!(feature = "recovery");
