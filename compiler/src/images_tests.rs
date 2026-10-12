//! Tests for reading an app's PNG files and placing them, both alone and through a compiled app.

use std::path::{Path, PathBuf};

use neotime_runtime::resources::{ImageFormat, LARGEST_IMAGE_SIDE};
use neotime_runtime::theme::rgb565;
use neotime_runtime::AppImage;

use super::{centre_mask, decode_png, decode_png_alpha, library_icon_picture, read_png_file, Picture};
use crate::compile_source_in;
use crate::library::ICON_SIDE;

// ============================================================================
// Helpers
// ============================================================================

/// Encodes raw pixel bytes as a PNG of the given colour type and bit depth.
fn encode_png(
    width: u32,
    height: u32,
    color_type: png::ColorType,
    bit_depth: png::BitDepth,
    palette: Option<Vec<u8>>,
    data: &[u8],
) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(color_type);
        encoder.set_depth(bit_depth);
        if let Some(palette) = palette {
            encoder.set_palette(palette);
        }
        let mut writer = encoder.write_header().expect("a PNG header");
        writer.write_image_data(data).expect("PNG data");
    }
    bytes
}

/// Encodes 8-bit RGBA pixels as a PNG.
fn rgba_png(width: u32, height: u32, data: &[u8]) -> Vec<u8> {
    encode_png(
        width,
        height,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        None,
        data,
    )
}

/// A fresh folder of its own under the system's temporary folder.
fn temporary_folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("neotime-images-tests-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("a temporary folder");
    folder
}

/// An app whose icon and main screen are the given source fragments.
fn app_with(icon: &str, screen_body: &str) -> String {
    format!(
        r#"
import {{ define_app }} from "neotime";
import {{ icons }} from "neotime/icons";
import {{ Image, Screen }} from "neotime/widgets";

function Main() {{
  return <Screen>{screen_body}</Screen>;
}}

export default define_app({{ id: "test", name: "Test", {icon} screens: {{ main: Main }} }});
"#
    )
}

/// Compiles an app whose images are read from `folder`, failing the test on any error.
fn compile_in(source: &str, folder: &Path) -> Vec<u8> {
    let output = compile_source_in(source, Some(folder.to_path_buf()));
    let errors: Vec<String> = output.errors().map(|error| error.message.clone()).collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    output.image.expect("an image")
}

/// The first error from compiling an app whose images are read from `folder`, if any.
fn first_error_in(source: &str, folder: Option<&Path>) -> String {
    let output = compile_source_in(source, folder.map(Path::to_path_buf));
    let message = output.errors().next().expect("an error").message.clone();
    message
}

/// Every pixel of one of an image's resources, unpacked from its runs.
fn resource_pixels(image: &AppImage, index: u16) -> (ImageFormat, u16, u16, Vec<u16>) {
    let resource = image.resource(index).expect("a resource");
    let mut pixels = Vec::new();
    for run in resource.runs() {
        pixels.extend(std::iter::repeat_n(run.value, run.count as usize));
    }
    (resource.format, resource.width, resource.height, pixels)
}

// ============================================================================
// Decoding
// ============================================================================

#[test]
fn a_one_colour_png_becomes_a_mask_of_its_alpha() {
    let bytes = rgba_png(2, 1, &[200, 10, 30, 255, 200, 10, 30, 64]);

    let picture = decode_png(&bytes).unwrap();

    assert_eq!(
        picture,
        Picture {
            format: ImageFormat::Mask,
            width: 2,
            height: 1,
            pixels: vec![255, 64],
        }
    );
}

#[test]
fn clear_pixels_do_not_count_as_a_second_colour() {
    let bytes = rgba_png(2, 1, &[255, 255, 255, 255, 0, 0, 0, 0]);

    let picture = decode_png(&bytes).unwrap();

    assert_eq!(picture.format, ImageFormat::Mask);
    assert_eq!(picture.pixels, vec![255, 0]);
}

#[test]
fn a_fully_clear_png_becomes_an_empty_mask() {
    let bytes = rgba_png(1, 2, &[10, 20, 30, 0, 40, 50, 60, 0]);

    let picture = decode_png(&bytes).unwrap();

    assert_eq!(picture.format, ImageFormat::Mask);
    assert_eq!(picture.pixels, vec![0, 0]);
}

#[test]
fn a_two_colour_png_becomes_a_colour_image() {
    let bytes = rgba_png(2, 1, &[255, 0, 0, 255, 0, 0, 255, 255]);

    let picture = decode_png(&bytes).unwrap();

    assert_eq!(picture.format, ImageFormat::Color);
    assert_eq!(picture.pixels, vec![rgb565(255, 0, 0), rgb565(0, 0, 255)]);
}

#[test]
fn partly_clear_colour_pixels_are_blended_onto_black() {
    let bytes = rgba_png(2, 1, &[255, 255, 255, 128, 0, 255, 0, 255]);

    let picture = decode_png(&bytes).unwrap();

    assert_eq!(picture.format, ImageFormat::Color);
    assert_eq!(picture.pixels, vec![rgb565(128, 128, 128), rgb565(0, 255, 0)]);
}

#[test]
fn a_grey_png_of_one_shade_is_a_solid_mask() {
    let bytes = encode_png(
        2,
        1,
        png::ColorType::Grayscale,
        png::BitDepth::Eight,
        None,
        &[90, 90],
    );

    let picture = decode_png(&bytes).unwrap();

    assert_eq!(picture.format, ImageFormat::Mask);
    assert_eq!(picture.pixels, vec![255, 255]);
}

#[test]
fn a_grey_png_of_two_shades_is_a_colour_image() {
    let bytes = encode_png(
        2,
        1,
        png::ColorType::Grayscale,
        png::BitDepth::Eight,
        None,
        &[0, 255],
    );

    let picture = decode_png(&bytes).unwrap();

    assert_eq!(picture.format, ImageFormat::Color);
    assert_eq!(picture.pixels, vec![rgb565(0, 0, 0), rgb565(255, 255, 255)]);
}

#[test]
fn a_grey_png_with_alpha_keeps_its_alpha_as_the_mask() {
    let bytes = encode_png(
        2,
        1,
        png::ColorType::GrayscaleAlpha,
        png::BitDepth::Eight,
        None,
        &[255, 30, 255, 220],
    );

    let picture = decode_png(&bytes).unwrap();

    assert_eq!(picture.format, ImageFormat::Mask);
    assert_eq!(picture.pixels, vec![30, 220]);
}

#[test]
fn a_sixteen_bit_rgb_png_is_cut_to_eight_bits() {
    let bytes = encode_png(
        2,
        1,
        png::ColorType::Rgb,
        png::BitDepth::Sixteen,
        None,
        &[0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF, 0, 0],
    );

    let picture = decode_png(&bytes).unwrap();

    assert_eq!(picture.format, ImageFormat::Color);
    assert_eq!(picture.pixels, vec![rgb565(255, 0, 0), rgb565(0, 255, 0)]);
}

#[test]
fn a_palette_png_is_expanded_to_its_colours() {
    let palette = vec![255, 0, 0, 0, 0, 255];
    let bytes = encode_png(
        3,
        1,
        png::ColorType::Indexed,
        png::BitDepth::Eight,
        Some(palette),
        &[1, 0, 1],
    );

    let picture = decode_png(&bytes).unwrap();

    assert_eq!(picture.format, ImageFormat::Color);
    assert_eq!(
        picture.pixels,
        vec![rgb565(0, 0, 255), rgb565(255, 0, 0), rgb565(0, 0, 255)]
    );
}

#[test]
fn a_png_wider_than_the_screen_is_refused() {
    let width = LARGEST_IMAGE_SIDE as u32 + 1;
    let bytes = encode_png(
        width,
        1,
        png::ColorType::Grayscale,
        png::BitDepth::Eight,
        None,
        &vec![0; width as usize],
    );

    let problem = decode_png(&bytes).unwrap_err();

    assert_eq!(
        problem,
        "is 241 by 1 pixels, and an image may be at most 240 on each side"
    );
}

#[test]
fn bytes_that_are_not_a_png_are_refused() {
    let problem = decode_png(b"not a picture").unwrap_err();

    assert!(
        problem.starts_with("is not a PNG the compiler can read"),
        "{problem}"
    );
}

#[test]
fn a_png_cut_short_in_its_pixels_is_refused() {
    let bytes = rgba_png(4, 4, &[7; 64]);
    let cut = &bytes[..bytes.len() - 20];

    let problem = decode_png(cut).unwrap_err();

    assert!(
        problem.starts_with("is not a PNG the compiler can read"),
        "{problem}"
    );
}

#[test]
fn decoding_the_alpha_alone_ignores_the_colours() {
    let bytes = rgba_png(2, 1, &[255, 0, 0, 10, 0, 0, 255, 250]);

    let picture = decode_png_alpha(&bytes).unwrap();

    assert_eq!(picture.format, ImageFormat::Mask);
    assert_eq!((picture.width, picture.height), (2, 1));
    assert_eq!(picture.pixels, vec![10, 250]);
}

#[test]
fn decoding_the_alpha_of_a_too_tall_png_is_refused() {
    let height = LARGEST_IMAGE_SIDE as u32 + 10;
    let bytes = encode_png(
        1,
        height,
        png::ColorType::Grayscale,
        png::BitDepth::Eight,
        None,
        &vec![0; height as usize],
    );

    let problem = decode_png_alpha(&bytes).unwrap_err();

    assert!(problem.starts_with("is 1 by 250 pixels"), "{problem}");
}

#[test]
fn decoding_the_alpha_of_bytes_that_are_not_a_png_is_refused() {
    let problem = decode_png_alpha(&[0, 1, 2, 3]).unwrap_err();

    assert!(
        problem.starts_with("is not a PNG the compiler can read"),
        "{problem}"
    );
}

// ============================================================================
// Reading files
// ============================================================================

#[test]
fn reading_a_png_file_decodes_it() {
    let folder = temporary_folder("read");
    let path = folder.join("dot.png");
    std::fs::write(&path, rgba_png(1, 1, &[1, 2, 3, 99])).unwrap();

    let picture = read_png_file(&path);

    std::fs::remove_dir_all(&folder).unwrap();
    assert_eq!(picture.unwrap().pixels, vec![99]);
}

#[test]
fn reading_a_missing_file_names_it() {
    let path = Path::new("/nonexistent/neotime/missing.png");

    let problem = read_png_file(path).unwrap_err();

    assert!(
        problem.starts_with("cannot read `/nonexistent/neotime/missing.png`"),
        "{problem}"
    );
}

#[test]
fn reading_a_broken_file_names_it_before_the_problem() {
    let folder = temporary_folder("broken");
    let path = folder.join("broken.png");
    std::fs::write(&path, b"garbage").unwrap();

    let problem = read_png_file(&path).unwrap_err();

    std::fs::remove_dir_all(&folder).unwrap();
    let expected_start = format!("`{}` is not a PNG", path.display());
    assert!(problem.starts_with(&expected_start), "{problem}");
}

// ============================================================================
// Placing
// ============================================================================

#[test]
fn a_small_mask_is_centred_in_a_clear_square() {
    let picture = Picture {
        format: ImageFormat::Mask,
        width: 2,
        height: 1,
        pixels: vec![5, 6],
    };

    let centred = centre_mask(&picture, 4);

    assert_eq!((centred.width, centred.height), (4, 4));
    #[rustfmt::skip]
    let expected = vec![
        0, 0, 0, 0,
        0, 5, 6, 0,
        0, 0, 0, 0,
        0, 0, 0, 0,
    ];
    assert_eq!(centred.pixels, expected);
}

#[test]
fn a_mask_larger_than_the_square_keeps_its_top_left_corner() {
    let picture = Picture {
        format: ImageFormat::Mask,
        width: 3,
        height: 3,
        pixels: vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
    };

    let centred = centre_mask(&picture, 2);

    assert_eq!(centred.pixels, vec![1, 2, 4, 5]);
}

#[test]
fn asking_for_a_library_icon_past_the_end_is_refused() {
    let problem = library_icon_picture(60_000).unwrap_err();

    assert_eq!(problem, "the shared library has no icon 60000");
}

// ============================================================================
// The app's images
// ============================================================================

#[test]
fn a_png_icon_becomes_image_zero_and_marks_the_app_as_having_an_icon() {
    let folder = temporary_folder("icon");
    std::fs::write(folder.join("icon.png"), rgba_png(2, 2, &[9, 9, 9, 255].repeat(4))).unwrap();
    let source = app_with(r#"icon: "./icon.png","#, "");

    let bytes = compile_in(&source, &folder);

    std::fs::remove_dir_all(&folder).unwrap();
    let image = AppImage::parse(&bytes).unwrap();
    assert!(image.has_icon);
    assert_eq!(image.resource_count(), 1);
    assert_eq!(
        resource_pixels(&image, 0),
        (ImageFormat::Mask, 2, 2, vec![255; 4])
    );
}

#[test]
fn a_library_icon_is_copied_in_as_image_zero() {
    let source = app_with("icon: icons.timer,", "");

    let output = compile_source_in(&source, None);

    let bytes = output.image.expect("an image");
    let image = AppImage::parse(&bytes).unwrap();
    assert!(image.has_icon);
    let (format, width, height, pixels) = resource_pixels(&image, 0);
    assert_eq!((format, width, height), (ImageFormat::Mask, ICON_SIDE, ICON_SIDE));
    assert!(pixels.iter().any(|&coverage| coverage > 0));
}

#[test]
fn an_icon_that_is_neither_a_file_nor_a_library_icon_is_refused() {
    let source = app_with("icon: 42,", "");

    let message = first_error_in(&source, None);

    assert!(message.starts_with("`icon` is a PNG file"), "{message}");
}

#[test]
fn the_same_png_used_twice_is_stored_once() {
    let folder = temporary_folder("twice");
    std::fs::write(folder.join("dot.png"), rgba_png(1, 1, &[0, 0, 0, 255])).unwrap();
    let source = app_with(
        r#"icon: "./dot.png","#,
        r#"<Image src="./dot.png" /><Image src="dot.png" />"#,
    );

    let bytes = compile_in(&source, &folder);

    std::fs::remove_dir_all(&folder).unwrap();
    let image = AppImage::parse(&bytes).unwrap();
    assert_eq!(image.resource_count(), 1);
}

#[test]
fn different_pngs_get_their_own_image_numbers() {
    let folder = temporary_folder("two");
    std::fs::write(folder.join("red.png"), rgba_png(1, 1, &[255, 0, 0, 255])).unwrap();
    std::fs::write(
        folder.join("stripes.png"),
        rgba_png(2, 1, &[255, 0, 0, 255, 0, 255, 0, 255]),
    )
    .unwrap();
    let source = app_with("", r#"<Image src="./red.png" /><Image src="./stripes.png" />"#);

    let bytes = compile_in(&source, &folder);

    std::fs::remove_dir_all(&folder).unwrap();
    let image = AppImage::parse(&bytes).unwrap();
    assert!(!image.has_icon);
    assert_eq!(image.resource_count(), 2);
    assert_eq!(resource_pixels(&image, 0).0, ImageFormat::Mask);
    assert_eq!(
        resource_pixels(&image, 1),
        (
            ImageFormat::Color,
            2,
            1,
            vec![rgb565(255, 0, 0), rgb565(0, 255, 0)]
        )
    );
}

#[test]
fn an_image_in_source_text_without_a_folder_cannot_be_found() {
    let source = app_with("", r#"<Image src="./logo.png" />"#);

    let message = first_error_in(&source, None);

    assert!(message.starts_with("`./logo.png` cannot be found"), "{message}");
}

#[test]
fn a_missing_png_is_reported_by_path() {
    let folder = temporary_folder("missing");
    let source = app_with(r#"icon: "./absent.png","#, "");

    let message = first_error_in(&source, Some(&folder));

    std::fs::remove_dir_all(&folder).unwrap();
    assert!(message.starts_with("cannot read `"), "{message}");
    assert!(message.contains("absent.png"), "{message}");
}
