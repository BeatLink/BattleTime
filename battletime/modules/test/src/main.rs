//! The test module: call 0 works through its heap, data and function addresses, 1 panics and 2 faults.
//!
//! The firmware runs it with `{"c":"module","name":"test","call":n}` on the packages
//! characteristic, which proves a module is placed and called, and that its crashes are recorded
//! against it and stop it after two.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;

use battletime_module_abi::{module, Bytes, Host};

// ============================================================================
// The calls
// ============================================================================

const WORKS: u32 = 0;
const PANICS: u32 = 1;
const FAULTS: u32 = 2;

/// Call 0's answer: the sum of the squares of 1 to 10.
const SUM_OF_SQUARES: i32 = 385;

const UNKNOWN_CALL: i32 = -1;
const HOST_TOO_OLD: i32 = -2;
const WRONG_ANSWER: i32 = -3;

fn square(number: i32) -> i32 {
    number * number
}

fn negate(number: i32) -> i32 {
    -number
}

/// Function addresses in data, which the loader moves by the code alias.
static STEPS: [fn(i32) -> i32; 2] = [square, negate];

/// Data the loader places by the RAM's own address.
static NUMBERS: [i32; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

/// Reads the table through `black_box`, so the calls really go through the placed addresses.
fn sum_of_squares() -> i32 {
    let steps = core::hint::black_box(&STEPS);
    let squares: Vec<i32> = NUMBERS.iter().map(|&number| steps[0](number)).collect();
    steps[1](steps[1](squares.iter().sum()))
}

/// # Safety
///
/// The firmware calls it once placed, with its own table; the input and output are unused.
#[no_mangle]
#[link_section = ".text.module_entry"]
pub unsafe extern "C" fn module_entry(
    host: &'static Host,
    call: u32,
    _input: &Bytes,
    _output: &mut Bytes,
) -> i32 {
    if !module::start(host) {
        return HOST_TOO_OLD;
    }
    match call {
        WORKS if sum_of_squares() == SUM_OF_SQUARES => SUM_OF_SQUARES,
        WORKS => WRONG_ANSWER,
        PANICS => panic!("the test module panics when asked"),
        FAULTS => {
            core::arch::asm!("udf #0");
            UNKNOWN_CALL
        }
        _ => UNKNOWN_CALL,
    }
}
