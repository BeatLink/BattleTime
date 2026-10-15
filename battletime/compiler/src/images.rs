//! An app's own pictures: PNG files read when compiling and turned into the runtime's image formats.
//!
//! Artwork in one colour, or only an alpha channel, becomes a mask, so the widget's colour draws it
//! and it follows the theme. Anything else becomes a full-colour image; the runtime has no
//! transparency there, so partly clear pixels are blended onto black. docs/compiler.md has the rules.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::OnceLock;

use battletime_runtime::resources::{ImageFormat, ImageReference, LARGEST_IMAGE_SIDE};
use battletime_runtime::theme::rgb565;
use oxc_ast::ast::Expression;
use oxc_span::{GetSpan, Span};

use crate::compiler::Compiler;
use crate::constants::Constant;
use crate::diagnostics::{fail, Compiled, Diagnostic};
use crate::scope::Scope;

// ============================================================================
// Decoded images
// ============================================================================

/// One picture as the image writer takes it: every pixel, left to right and top to bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub format: ImageFormat,
    pub width: u16,
    pub height: u16,
    /// RGB565 colours for a colour image, coverage from 0 to 255 for a mask.
    pub pixels: Vec<u16>,
}

/// One pixel after decoding, in 8-bit red, green, blue and alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rgba {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
}

// ============================================================================
// Reading PNG files
// ============================================================================

/// Reads a PNG file from disk.
pub fn read_png_file(path: &Path) -> Result<Picture, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
    decode_png(&bytes).map_err(|problem| format!("`{}` {problem}", path.display()))
}

/// Decodes PNG bytes into a mask when the artwork has one colour, and a colour image otherwise.
pub fn decode_png(bytes: &[u8]) -> Result<Picture, String> {
    let (width, height, pixels) = decode_rgba(bytes)?;
    let largest = LARGEST_IMAGE_SIDE as u32;
    if width == 0 || height == 0 || width > largest || height > largest {
        return Err(format!(
            "is {width} by {height} pixels, and an image may be at most {largest} on each side"
        ));
    }
    let width = width as u16;
    let height = height as u16;
    if is_one_colour(&pixels) {
        return Ok(Picture {
            format: ImageFormat::Mask,
            width,
            height,
            pixels: pixels.iter().map(|pixel| pixel.alpha as u16).collect(),
        });
    }
    let blend = |channel: u8, alpha: u8| ((channel as u16 * alpha as u16 + 127) / 255) as u8;
    Ok(Picture {
        format: ImageFormat::Color,
        width,
        height,
        pixels: pixels
            .iter()
            .map(|pixel| {
                rgb565(
                    blend(pixel.red, pixel.alpha),
                    blend(pixel.green, pixel.alpha),
                    blend(pixel.blue, pixel.alpha),
                )
            })
            .collect(),
    })
}

/// Decodes a PNG's alpha channel alone as a mask, whatever its colours.
pub fn decode_png_alpha(bytes: &[u8]) -> Result<Picture, String> {
    let (width, height, pixels) = decode_rgba(bytes)?;
    let largest = LARGEST_IMAGE_SIDE as u32;
    if width == 0 || height == 0 || width > largest || height > largest {
        return Err(format!(
            "is {width} by {height} pixels, and an image may be at most {largest} on each side"
        ));
    }
    Ok(Picture {
        format: ImageFormat::Mask,
        width: width as u16,
        height: height as u16,
        pixels: pixels.iter().map(|pixel| pixel.alpha as u16).collect(),
    })
}

/// Decodes a PNG of any colour type and depth into 8-bit RGBA pixels.
fn decode_rgba(bytes: &[u8]) -> Result<(u32, u32, Vec<Rgba>), String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|error| format!("is not a PNG the compiler can read: {error}"))?;
    let Some(size) = reader.output_buffer_size() else {
        return Err("is too large to decode".to_string());
    };
    let mut buffer = vec![0; size];
    let frame = reader
        .next_frame(&mut buffer)
        .map_err(|error| format!("is not a PNG the compiler can read: {error}"))?;
    let data = &buffer[..frame.buffer_size()];
    let pixel_count = (frame.width * frame.height) as usize;
    let rows = data.chunks_exact(frame.line_size).take(frame.height as usize);
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err("kept its palette after decoding".to_string()),
    };
    let mut pixels = Vec::with_capacity(pixel_count);
    for row in rows {
        for pixel in row.chunks_exact(channels).take(frame.width as usize) {
            pixels.push(match channels {
                1 => Rgba {
                    red: pixel[0],
                    green: pixel[0],
                    blue: pixel[0],
                    alpha: 255,
                },
                2 => Rgba {
                    red: pixel[0],
                    green: pixel[0],
                    blue: pixel[0],
                    alpha: pixel[1],
                },
                3 => Rgba {
                    red: pixel[0],
                    green: pixel[1],
                    blue: pixel[2],
                    alpha: 255,
                },
                _ => Rgba {
                    red: pixel[0],
                    green: pixel[1],
                    blue: pixel[2],
                    alpha: pixel[3],
                },
            });
        }
    }
    if pixels.len() != pixel_count {
        return Err("ends before its last row".to_string());
    }
    Ok((frame.width, frame.height, pixels))
}

/// Whether every pixel that shows at all has the same colour, so alpha alone carries the shape.
fn is_one_colour(pixels: &[Rgba]) -> bool {
    let mut visible = pixels.iter().filter(|pixel| pixel.alpha > 0);
    let Some(first) = visible.next() else {
        return true;
    };
    visible.all(|pixel| (pixel.red, pixel.green, pixel.blue) == (first.red, first.green, first.blue))
}

// ============================================================================
// Placing
// ============================================================================

/// Puts a mask in the middle of a larger square of clear pixels.
pub fn centre_mask(picture: &Picture, side: u16) -> Picture {
    let mut pixels = vec![0u16; side as usize * side as usize];
    let left = (side.saturating_sub(picture.width) / 2) as usize;
    let top = (side.saturating_sub(picture.height) / 2) as usize;
    for y in 0..(picture.height.min(side) as usize) {
        for x in 0..(picture.width.min(side) as usize) {
            pixels[(top + y) * side as usize + left + x] = picture.pixels[y * picture.width as usize + x];
        }
    }
    Picture {
        format: ImageFormat::Mask,
        width: side,
        height: side,
        pixels,
    }
}

// ============================================================================
// The app's images
// ============================================================================

impl<'a> Compiler<'a> {
    /// define_app's `icon`: a PNG of the app's own or a shared library icon, written as image 0.
    pub fn add_app_icon(&mut self, icon: &'a Expression<'a>, root: &Rc<Scope<'a>>) -> Compiled<()> {
        let span = icon.span();
        if !self.image_numbers.is_empty() {
            return fail(span, "internal compiler error: the icon must be the first image");
        }
        match self.evaluate_constant(icon, root) {
            Some(Constant::Icon(value)) => {
                // The launcher reads image 0, which names the library icon rather than copying it.
                let Some(ImageReference::SharedLibrary(index)) = ImageReference::from_value(value as i32) else {
                    return fail(span, "internal compiler error: an icon is not a shared library image");
                };
                library_icon_picture(index as usize).map_err(|problem| Diagnostic::error(span, problem))?;
                self.writer.library_image_reference(index);
                self.image_numbers.push((PathBuf::new(), 0));
            }
            Some(Constant::Text(path)) => {
                self.app_image(&path, span)?;
            }
            _ => return fail(span, "`icon` is a PNG file, such as \"./icon.png\", or a shared library icon such as `icons.timer`"),
        }
        self.writer.set_has_icon();
        Ok(())
    }

    /// The image number of one of the app's own PNG files, adding it the first time it is used.
    pub fn app_image(&mut self, path: &str, span: Span) -> Compiled<u16> {
        let Some(directory) = &self.source_directory else {
            return fail(
                span,
                format!("`{path}` cannot be found: images are read relative to the source file, so compile the file itself"),
            );
        };
        let full_path = directory.join(path);
        let full_path = full_path.canonicalize().unwrap_or(full_path);
        if let Some((_, number)) = self
            .image_numbers
            .iter()
            .find(|(existing, _)| *existing == full_path)
        {
            return Ok(*number);
        }
        let picture = read_png_file(&full_path).map_err(|problem| Diagnostic::error(span, problem))?;
        let number =
            self.writer
                .image_resource(picture.format, picture.width, picture.height, &picture.pixels);
        self.image_numbers.push((full_path, number));
        Ok(number)
    }
}

/// One shared library icon's picture, drawn once and kept for every later app.
fn library_icon_picture(index: usize) -> Result<Picture, String> {
    static ICONS: OnceLock<Result<Vec<Picture>, String>> = OnceLock::new();
    let icons = ICONS
        .get_or_init(crate::library::draw_every_icon)
        .as_ref()
        .map_err(Clone::clone)?;
    icons
        .get(index)
        .cloned()
        .ok_or_else(|| format!("the shared library has no icon {index}"))
}

#[cfg(test)]
#[path = "images_tests.rs"]
mod images_tests;
