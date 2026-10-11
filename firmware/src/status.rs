//! Which mode the firmware is running in, and how the status page shows it: a background colour and two lines of plain words.

use embedded_graphics::pixelcolor::{raw::RawU16, Rgb565};
use neotime_boot::{status_colour, BootNote, BOOT_NOTE_ADDRESS};

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
}

impl Mode {
    pub fn background(self) -> Rgb565 {
        let colour = match self {
            Mode::Confirmed => status_colour::CONFIRMED,
            Mode::OnTrial => status_colour::ON_TRIAL,
            Mode::RolledBack => status_colour::ROLLED_BACK,
            Mode::Recovery => status_colour::RECOVERY,
        };
        Rgb565::from(RawU16::new(colour))
    }

    pub fn title(self) -> &'static str {
        match self {
            Mode::Confirmed => "Firmware OK",
            Mode::OnTrial => "New firmware on trial",
            Mode::RolledBack => "Update failed",
            Mode::Recovery => "Recovery firmware",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            Mode::Confirmed => "",
            Mode::OnTrial => "Connect to keep it",
            Mode::RolledBack => "Previous one restored",
            Mode::Recovery => "Send a new firmware",
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

/// A build made for the recovery slot is always the recovery firmware, however it was installed.
pub const IS_RECOVERY_BUILD: bool = cfg!(feature = "recovery");
