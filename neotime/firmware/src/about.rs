//! The facts the system app's About pages show about the bootloader and the chips, kept as numbers
//! for the app to format.
//!
//! The parts that talk to a chip note its ID as they start it. The bootloader's version record is
//! looked for only when an app first reads it: the read asks for it, and the shell's loop looks
//! through the bootloader once, then tells the app it changed.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

use embassy_sync::once_lock::OnceLock;
use neotime_boot::{version::VersionRecord, BOOTLOADER};
use neotime_runtime::{Value, NONE};

// ============================================================================
// What start-up notes
// ============================================================================

/// The bootloader's CRC-32, as start-up reads it for the status page.
static BOOTLOADER_CRC: AtomicU32 = AtomicU32::new(0);
static BOOTLOADER_CRC_READ: AtomicBool = AtomicBool::new(false);

/// The JEDEC ID's three bytes, highest first, or 0 when the chip never answered.
static FLASH_ID: AtomicU32 = AtomicU32::new(0);

/// Each stays 0 until its chip answers.
static TOUCH_CHIP_ID: AtomicU8 = AtomicU8::new(0);
static TOUCH_FIRMWARE_VERSION: AtomicU8 = AtomicU8::new(0);
static ACCELEROMETER_CHIP_ID: AtomicU8 = AtomicU8::new(0);

pub fn note_bootloader_crc(crc: u32) {
    BOOTLOADER_CRC.store(crc, Ordering::Relaxed);
    BOOTLOADER_CRC_READ.store(true, Ordering::Relaxed);
}

pub fn note_flash_id(manufacturer: u8, memory_type: u8, capacity: u8) {
    FLASH_ID.store(
        u32::from_be_bytes([0, manufacturer, memory_type, capacity]),
        Ordering::Relaxed,
    );
}

pub fn note_touch_chip(chip_id: u8, firmware_version: u8) {
    TOUCH_CHIP_ID.store(chip_id, Ordering::Relaxed);
    TOUCH_FIRMWARE_VERSION.store(firmware_version, Ordering::Relaxed);
}

pub fn note_accelerometer_chip(chip_id: u8) {
    ACCELEROMETER_CHIP_ID.store(chip_id, Ordering::Relaxed);
}

// ============================================================================
// What apps read
// ============================================================================

/// The bootloader's CRC-32, or none when start-up could not read the bootloader.
pub fn bootloader_crc() -> Value {
    if BOOTLOADER_CRC_READ.load(Ordering::Relaxed) {
        Value::from(BOOTLOADER_CRC.load(Ordering::Relaxed))
    } else {
        NONE
    }
}

/// The address the watch advertises with, highest byte first, as one number.
pub fn bluetooth_address() -> Value {
    crate::device_address()
        .iter()
        .rev()
        .fold(0, |address, &byte| address << 8 | Value::from(byte))
}

pub fn flash_id() -> Value {
    Value::from(FLASH_ID.load(Ordering::Relaxed))
}

pub fn touch_chip_id() -> Value {
    Value::from(TOUCH_CHIP_ID.load(Ordering::Relaxed))
}

pub fn touch_firmware_version() -> Value {
    Value::from(TOUCH_FIRMWARE_VERSION.load(Ordering::Relaxed))
}

pub fn accelerometer_chip_id() -> Value {
    Value::from(ACCELEROMETER_CHIP_ID.load(Ordering::Relaxed))
}

// ============================================================================
// The bootloader's version record, looked for when first read
// ============================================================================

/// The record, or none when the bootloader carries none; unset until the shell's loop has looked.
static RECORD: OnceLock<Option<VersionRecord>> = OnceLock::new();
/// Set when an app read the record before the loop looked, so the loop looks.
static RECORD_WANTED: AtomicBool = AtomicBool::new(false);

/// The bootloader's version, or nothing when it has no record or the loop has not looked yet.
pub fn bootloader_version() -> &'static str {
    record().map_or("", VersionRecord::version)
}

pub fn bootloader_build_date() -> &'static str {
    record().map_or("", VersionRecord::date)
}

fn record() -> Option<&'static VersionRecord> {
    match RECORD.try_get() {
        Some(record) => record.as_ref(),
        None => {
            RECORD_WANTED.store(true, Ordering::Relaxed);
            None
        }
    }
}

/// Looks for the record if an app asked for it; true when it looked, so the shell tells the app.
pub fn find_record_if_wanted() -> bool {
    if !RECORD_WANTED.swap(false, Ordering::Relaxed) || RECORD.try_get().is_some() {
        return false;
    }
    let found = VersionRecord::find(&mut crate::status::BootloaderPages, BOOTLOADER)
        .ok()
        .flatten();
    let _ = RECORD.init(found);
    true
}
