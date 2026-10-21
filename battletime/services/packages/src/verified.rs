//! The images the watch has verified, kept in `/system/verified`, so it opens them without checking their code again.
//!
//! The watch's firmware carries no verifier: the shell runs the verifier module once per image, after
//! an install or before the first open, and records the image here by its CRC and length, which
//! name its whole contents. The record belongs to one verifier module, by its CRC, so a module with
//! new rules checks every image again, while a firmware update that keeps the module checks none.

use alloc::vec::Vec;

pub const VERIFIED_PATH: &str = "/system/verified";

/// The images kept; the one recorded longest ago is forgotten to make room.
pub const MOST_IMAGES: usize = 128;

const HEADER_BYTES: usize = 8;
const ENTRY_BYTES: usize = 8;

/// An image by what names its contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageMark {
    pub crc: u32,
    pub length: u32,
}

impl ImageMark {
    /// The mark of an image whose header `AppImage::parse` checked: its CRC field and its length.
    pub fn of(image_bytes: &[u8]) -> Option<ImageMark> {
        let crc = image_bytes.get(12..16)?;
        Some(ImageMark {
            crc: u32::from_le_bytes([crc[0], crc[1], crc[2], crc[3]]),
            length: image_bytes.len() as u32,
        })
    }

    fn encode(&self) -> [u8; ENTRY_BYTES] {
        let mut bytes = [0; ENTRY_BYTES];
        bytes[..4].copy_from_slice(&self.crc.to_le_bytes());
        bytes[4..].copy_from_slice(&self.length.to_le_bytes());
        bytes
    }
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Whether the record, as the file holds it, names this image for the verifier `verifier_id`.
pub fn holds(file: &[u8], verifier_id: u32, mark: ImageMark) -> bool {
    if file.len() < HEADER_BYTES || read_u32(file, 0) != verifier_id {
        return false;
    }
    let wanted = mark.encode();
    file[HEADER_BYTES..]
        .as_chunks::<ENTRY_BYTES>()
        .0
        .contains(&wanted)
}

/// The record with `mark` added, newest last; one for another verifier starts afresh.
pub fn with_added(file: &[u8], verifier_id: u32, mark: ImageMark) -> Vec<u8> {
    let kept: &[[u8; ENTRY_BYTES]] = match file.len() >= HEADER_BYTES && read_u32(file, 0) == verifier_id {
        true => file[HEADER_BYTES..].as_chunks::<ENTRY_BYTES>().0,
        false => &[],
    };
    let wanted = mark.encode();
    let others: Vec<&[u8; ENTRY_BYTES]> = kept.iter().filter(|entry| **entry != wanted).collect();
    let skipped = (others.len() + 1).saturating_sub(MOST_IMAGES);
    let mut out = Vec::with_capacity(HEADER_BYTES + ENTRY_BYTES * MOST_IMAGES);
    out.extend_from_slice(&verifier_id.to_le_bytes());
    out.extend_from_slice(&[0; HEADER_BYTES - 4]);
    for entry in &others[skipped..] {
        out.extend_from_slice(*entry);
    }
    out.extend_from_slice(&wanted);
    out
}

#[cfg(test)]
#[path = "verified_tests.rs"]
mod verified_tests;
