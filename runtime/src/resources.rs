//! Images stored in an app image or in the shared library, compressed with run-length encoding.
//!
//! A full-colour image is runs of RGB565 colours. A mask is runs of coverage, from 0 for clear to
//! 255 for solid, and is drawn in whatever colour the widget gives, which is how icons follow the
//! theme. The runtime never decompresses a whole image; drawing reads the runs row by row.

use crate::image::{AppImage, ImageError};

// ============================================================================
// Layout
// ============================================================================

const RESOURCE_HEADER_BYTES: usize = 6;
const COLOR_RUN_BYTES: usize = 3;
const MASK_RUN_BYTES: usize = 2;

/// The largest an image may be on either side: the screen's size.
pub const LARGEST_IMAGE_SIDE: u16 = 240;

/// Marks an image property that names a shared library image rather than one of the app's own.
const SHARED_LIBRARY_FLAG: i32 = 0x8000;
const INSTALLED_APP_FLAG: i32 = 0x10000;

// ============================================================================
// Images
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    /// Runs of (count, RGB565 colour): 3 bytes each.
    Color,
    /// Runs of (count, coverage): 2 bytes each, drawn in the widget's colour.
    Mask,
}

/// One image, read in place.
#[derive(Clone, Copy, Debug)]
pub struct ImageResource<'image> {
    pub format: ImageFormat,
    pub width: u16,
    pub height: u16,
    runs: &'image [u8],
}

/// A run of identical pixels: a colour for a colour image, a coverage for a mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub count: u8,
    pub value: u16,
}

impl<'image> ImageResource<'image> {
    pub fn parse(bytes: &'image [u8]) -> Option<ImageResource<'image>> {
        let format = match *bytes.first()? {
            0 => ImageFormat::Color,
            1 => ImageFormat::Mask,
            _ => return None,
        };
        Some(ImageResource {
            format,
            width: u16::from_le_bytes([*bytes.get(2)?, *bytes.get(3)?]),
            height: u16::from_le_bytes([*bytes.get(4)?, *bytes.get(5)?]),
            runs: bytes.get(RESOURCE_HEADER_BYTES..)?,
        })
    }

    fn run_bytes(&self) -> usize {
        match self.format {
            ImageFormat::Color => COLOR_RUN_BYTES,
            ImageFormat::Mask => MASK_RUN_BYTES,
        }
    }

    /// The image's runs, left to right and top to bottom; a run may continue onto the next row.
    pub fn runs(&self) -> impl Iterator<Item = Run> + 'image {
        let format = self.format;
        self.runs
            .chunks_exact(self.run_bytes())
            .map(move |run| match format {
                ImageFormat::Color => Run {
                    count: run[0],
                    value: u16::from_le_bytes([run[1], run[2]]),
                },
                ImageFormat::Mask => Run {
                    count: run[0],
                    value: run[1] as u16,
                },
            })
    }

    /// Whether the runs cover exactly every pixel, with no empty run and no stray bytes.
    pub fn is_well_formed(&self) -> bool {
        let sides_fit =
            (1..=LARGEST_IMAGE_SIDE).contains(&self.width) && (1..=LARGEST_IMAGE_SIDE).contains(&self.height);
        let whole_runs = self.runs.len().is_multiple_of(self.run_bytes());
        let mut covered: u32 = 0;
        for run in self.runs() {
            if run.count == 0 {
                return false;
            }
            covered += run.count as u32;
        }
        sides_fit && whole_runs && covered == self.width as u32 * self.height as u32
    }
}

/// Which image an `Image` or `Icon` property names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageReference {
    /// One of the app's own images, by number.
    App(u16),
    /// One of the shared library's images, such as an icon, by number.
    SharedLibrary(u16),
    /// The launcher icon of an installed app, by its place in `installed_apps`.
    InstalledApp(u16),
}

impl ImageReference {
    pub fn from_value(value: i32) -> Option<ImageReference> {
        match value {
            0..SHARED_LIBRARY_FLAG => Some(ImageReference::App(value as u16)),
            SHARED_LIBRARY_FLAG..=0xFFFF => Some(ImageReference::SharedLibrary(
                (value - SHARED_LIBRARY_FLAG) as u16,
            )),
            INSTALLED_APP_FLAG..=0x1FFFF => {
                Some(ImageReference::InstalledApp((value - INSTALLED_APP_FLAG) as u16))
            }
            _ => None,
        }
    }

    pub fn to_value(self) -> i32 {
        match self {
            ImageReference::App(index) => index as i32,
            ImageReference::SharedLibrary(index) => SHARED_LIBRARY_FLAG + index as i32,
            ImageReference::InstalledApp(index) => INSTALLED_APP_FLAG + index as i32,
        }
    }
}

// ============================================================================
// Checking
// ============================================================================

/// Checks that every image in the image decodes to exactly its size.
pub fn verify_resources(image: &AppImage) -> Result<(), ImageError> {
    for index in 0..image.resource_count() {
        let resource = image.resource(index).ok_or(ImageError::BadResource(index))?;
        if !resource.is_well_formed() {
            return Err(ImageError::BadResource(index));
        }
    }
    Ok(())
}
