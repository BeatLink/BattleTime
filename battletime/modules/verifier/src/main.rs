//! The verifier module: `battletime_runtime::verify_with` run on the image the firmware gives it.
//!
//! The firmware carries no verifier of its own. Its shell calls this module once per image, with
//! the open app closed, after an install or before an image's first open, and records the image
//! as verified when it answers 0.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;

use battletime_module_abi::{module, Host};
use battletime_runtime::{verify_with, AppImage, CheckedFunctions};

// ============================================================================
// The calls
// ============================================================================

const VERIFY: u32 = 0;

const VERIFIED: i32 = 0;
const UNKNOWN_CALL: i32 = -1;
const HOST_TOO_OLD: i32 = -2;
const NOT_AN_IMAGE: i32 = -10;
const CODE_FAILS: i32 = -11;

fn verify(image_bytes: &[u8]) -> i32 {
    let Ok(image) = AppImage::parse(image_bytes) else {
        return NOT_AN_IMAGE;
    };
    match verify_with(&image, &mut Box::new(CheckedFunctions::NONE)) {
        Ok(()) => VERIFIED,
        Err(_) => CODE_FAILS,
    }
}

/// # Safety
///
/// The firmware calls it once placed, with its own table and `input` holding `input_length` bytes.
#[no_mangle]
#[link_section = ".text.module_entry"]
pub unsafe extern "C" fn module_entry(
    host: &'static Host,
    call: u32,
    input: *const u8,
    input_length: usize,
    _output: *mut u8,
    _output_length: usize,
) -> i32 {
    if !module::start(host) {
        return HOST_TOO_OLD;
    }
    match call {
        VERIFY => verify(unsafe { core::slice::from_raw_parts(input, input_length) }),
        _ => UNKNOWN_CALL,
    }
}
