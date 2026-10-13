//! The 256 bytes in front of every firmware image: what it is, where it runs, how long it is and its CRC.
//!
//! `tools/image_header.py` writes the header after the build. The bootloader reads it before it starts
//! or installs an image, and the firmware reads it before it accepts one, so an image built for another
//! layout, cut short or damaged is never run.

use embedded_storage::nor_flash::ReadNorFlash;

use crate::{
    migration::{crc32, CRC32_INITIAL},
    FIRMWARE, FIRMWARE_VECTOR_TABLE,
};

// ============================================================================
// Format
// ============================================================================

pub const IMAGE_HEADER_SIZE: u32 = 256;
/// "NTIM" in flash byte order.
pub const IMAGE_MAGIC: u32 = u32::from_le_bytes(*b"NTIM");
pub const IMAGE_HEADER_VERSION: u16 = 1;
pub const COMMIT_LENGTH: usize = 8;

const MAGIC_OFFSET: usize = 0;
const HEADER_VERSION_OFFSET: usize = 4;
const HEADER_SIZE_OFFSET: usize = 6;
const IMAGE_TYPE_OFFSET: usize = 8;
const VERSION_OFFSET: usize = 9;
const LOAD_ADDRESS_OFFSET: usize = 12;
const IMAGE_SIZE_OFFSET: usize = 16;
const IMAGE_CRC_OFFSET: usize = 20;
const BUILD_TIME_OFFSET: usize = 24;
const COMMIT_OFFSET: usize = 28;
const HEADER_CRC_OFFSET: usize = IMAGE_HEADER_SIZE as usize - 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageType {
    Firmware = 1,
    Recovery = 2,
    BootloaderUpdater = 3,
}

impl ImageType {
    const fn from_byte(byte: u8) -> Option<ImageType> {
        match byte {
            1 => Some(ImageType::Firmware),
            2 => Some(ImageType::Recovery),
            3 => Some(ImageType::BootloaderUpdater),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageHeader {
    pub image_type: ImageType,
    pub version: Version,
    /// Where the image's vector table must sit, which is where it was linked to run.
    pub load_address: u32,
    /// The bytes after the header.
    pub image_size_in_bytes: u32,
    pub image_crc32: u32,
    pub build_time_in_seconds: u32,
    /// The short commit hash in ASCII, `*` after it for a build with uncommitted changes, zero padded.
    pub commit: [u8; COMMIT_LENGTH],
}

// ============================================================================
// Reading and writing the bytes
// ============================================================================

impl ImageHeader {
    pub fn encode(&self) -> [u8; IMAGE_HEADER_SIZE as usize] {
        let mut bytes = [0; IMAGE_HEADER_SIZE as usize];
        put_word(&mut bytes, MAGIC_OFFSET, IMAGE_MAGIC);
        bytes[HEADER_VERSION_OFFSET..HEADER_VERSION_OFFSET + 2]
            .copy_from_slice(&IMAGE_HEADER_VERSION.to_le_bytes());
        bytes[HEADER_SIZE_OFFSET..HEADER_SIZE_OFFSET + 2]
            .copy_from_slice(&(IMAGE_HEADER_SIZE as u16).to_le_bytes());
        bytes[IMAGE_TYPE_OFFSET] = self.image_type as u8;
        bytes[VERSION_OFFSET..VERSION_OFFSET + 3].copy_from_slice(&[
            self.version.major,
            self.version.minor,
            self.version.patch,
        ]);
        put_word(&mut bytes, LOAD_ADDRESS_OFFSET, self.load_address);
        put_word(&mut bytes, IMAGE_SIZE_OFFSET, self.image_size_in_bytes);
        put_word(&mut bytes, IMAGE_CRC_OFFSET, self.image_crc32);
        put_word(&mut bytes, BUILD_TIME_OFFSET, self.build_time_in_seconds);
        bytes[COMMIT_OFFSET..COMMIT_OFFSET + COMMIT_LENGTH].copy_from_slice(&self.commit);
        let header_crc = crc32(CRC32_INITIAL, &bytes[..HEADER_CRC_OFFSET]);
        put_word(&mut bytes, HEADER_CRC_OFFSET, header_crc);
        bytes
    }

    /// Gives none for anything but a whole, unchanged header of this version.
    pub fn decode(bytes: &[u8; IMAGE_HEADER_SIZE as usize]) -> Option<ImageHeader> {
        let header_is_sound = word(bytes, MAGIC_OFFSET) == IMAGE_MAGIC
            && bytes[HEADER_VERSION_OFFSET..HEADER_VERSION_OFFSET + 2] == IMAGE_HEADER_VERSION.to_le_bytes()
            && bytes[HEADER_SIZE_OFFSET..HEADER_SIZE_OFFSET + 2] == (IMAGE_HEADER_SIZE as u16).to_le_bytes()
            && word(bytes, HEADER_CRC_OFFSET) == crc32(CRC32_INITIAL, &bytes[..HEADER_CRC_OFFSET]);
        if !header_is_sound {
            return None;
        }

        let mut commit = [0; COMMIT_LENGTH];
        commit.copy_from_slice(&bytes[COMMIT_OFFSET..COMMIT_OFFSET + COMMIT_LENGTH]);
        Some(ImageHeader {
            image_type: ImageType::from_byte(bytes[IMAGE_TYPE_OFFSET])?,
            version: Version {
                major: bytes[VERSION_OFFSET],
                minor: bytes[VERSION_OFFSET + 1],
                patch: bytes[VERSION_OFFSET + 2],
            },
            load_address: word(bytes, LOAD_ADDRESS_OFFSET),
            image_size_in_bytes: word(bytes, IMAGE_SIZE_OFFSET),
            image_crc32: word(bytes, IMAGE_CRC_OFFSET),
            build_time_in_seconds: word(bytes, BUILD_TIME_OFFSET),
            commit,
        })
    }

    /// Header and image together, as they sit in a slot.
    pub const fn total_size_in_bytes(&self) -> u32 {
        IMAGE_HEADER_SIZE.saturating_add(self.image_size_in_bytes)
    }

    /// Linked to run from `FIRMWARE` and small enough to fit there.
    pub const fn runs_as_firmware(&self) -> bool {
        self.load_address == FIRMWARE_VECTOR_TABLE && self.total_size_in_bytes() <= FIRMWARE.size
    }
}

fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn put_word(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

// ============================================================================
// Images in flash
// ============================================================================

const CRC_CHUNK_SIZE: usize = 256;

pub fn read_image_header<Flash: ReadNorFlash>(
    flash: &mut Flash,
    image_start: u32,
) -> Result<Option<ImageHeader>, Flash::Error> {
    let mut bytes = [0; IMAGE_HEADER_SIZE as usize];
    flash.read(image_start, &mut bytes)?;
    Ok(ImageHeader::decode(&bytes))
}

/// Reads the image after the header and compares its CRC with the header's.
pub fn image_crc_matches<Flash: ReadNorFlash>(
    flash: &mut Flash,
    image_start: u32,
    header: &ImageHeader,
) -> Result<bool, Flash::Error> {
    let mut chunk = [0; CRC_CHUNK_SIZE];
    let mut crc = CRC32_INITIAL;
    let code_start = image_start + IMAGE_HEADER_SIZE;
    let mut offset = 0;
    while offset < header.image_size_in_bytes {
        let length = (header.image_size_in_bytes - offset).min(CRC_CHUNK_SIZE as u32) as usize;
        flash.read(code_start + offset, &mut chunk[..length])?;
        crc = crc32(crc, &chunk[..length]);
        offset += length as u32;
    }
    Ok(crc == header.image_crc32)
}

/// The header of a whole firmware image that fits `slot_size_in_bytes`, or none.
pub fn installable_image<Flash: ReadNorFlash>(
    flash: &mut Flash,
    image_start: u32,
    slot_size_in_bytes: u32,
) -> Result<Option<ImageHeader>, Flash::Error> {
    let Some(header) = read_image_header(flash, image_start)? else {
        return Ok(None);
    };
    let fits = header.runs_as_firmware() && header.total_size_in_bytes() <= slot_size_in_bytes;
    if !fits || !image_crc_matches(flash, image_start, &header)? {
        return Ok(None);
    }
    Ok(Some(header))
}
