//! The translator module: `battletime_aot::native_file::write_from_input` run on what the firmware gives it.
//!
//! The firmware calls it after the verifier passed an image, with the open app closed, giving it
//! the image's looping units as `native_file::translator_input` lays them out. The first call says
//! how long the native file is, so the firmware can lend room for it, and the second writes it
//! there; the firmware then keeps it at `/apps/<id>/native`.

#![no_std]
#![no_main]

extern crate alloc;

use battletime_aot::native_file::{self, WriteError};
use battletime_module_abi::{module, Bytes, Host};

// ============================================================================
// The calls
// ============================================================================

const MEASURE: u32 = 0;
const WRITE: u32 = 1;

const UNKNOWN_CALL: i32 = -1;
const HOST_TOO_OLD: i32 = -2;
const BAD_INPUT: i32 = -10;
const NO_ROOM: i32 = -12;

/// The native file's length, after writing it to `output` when it is given.
fn translate(input: &[u8], output: Option<&mut [u8]>) -> i32 {
    match native_file::write_from_input(input, output) {
        Ok(length) => length as i32,
        Err(WriteError::BadInput) => BAD_INPUT,
        Err(WriteError::NoRoom) => NO_ROOM,
    }
}

/// # Safety
///
/// The firmware calls it once placed, with its own table, the translator input and, for a write, room for the file.
#[no_mangle]
#[link_section = ".text.module_entry"]
pub unsafe extern "C" fn module_entry(
    host: &'static Host,
    call: u32,
    input: &Bytes,
    output: &mut Bytes,
) -> i32 {
    if !module::start(host) {
        return HOST_TOO_OLD;
    }
    let input = unsafe { module::input(input) };
    match call {
        MEASURE => translate(input, None),
        WRITE => translate(input, Some(unsafe { module::output(output) })),
        _ => UNKNOWN_CALL,
    }
}
