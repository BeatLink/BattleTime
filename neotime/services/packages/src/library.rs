//! The shared library: icons, fonts, the accelerometers' configuration and common functions, which every app can use.
//!
//! It ships inside the firmware image, after the code, where the image header says, so it is updated
//! with the firmware and never written on the watch.

use neotime_boot::image_header::{ImageHeader, IMAGE_HEADER_SIZE};

/// The library inside a firmware image, from the bytes of the image header on, or None when the image carries none.
pub fn firmware_library(image: &[u8]) -> Option<&[u8]> {
    let header_bytes = image.get(..IMAGE_HEADER_SIZE as usize)?.try_into().ok()?;
    let library = ImageHeader::decode(header_bytes)?.library_region(0)?;
    image.get(library.start as usize..library.end() as usize)
}

/// The version a library image records, read from its header without checking the image.
pub fn library_version(image: &[u8]) -> Option<u16> {
    image
        .get(8..10)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
}
