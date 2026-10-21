//! The P-256 module: the elliptic-curve Diffie-Hellman maths of Bluetooth's LE Secure Connections pairing.
//!
//! Pairing is rare, and the maths would cost the firmware tens of kilobytes, so it lives here and
//! runs only when asked. Call 0 is a self-test: it works out the DHKey of the Bluetooth Core
//! specification's P-256 sample data (Vol 3, Part H, 2.3.5.6.1) and answers how many microseconds
//! that took, which is about the cost of each step pairing needs.

#![no_std]
#![no_main]

use battletime_module_abi::{module, Host};
use p256::{
    ecdh::diffie_hellman, elliptic_curve::sec1::FromEncodedPoint, EncodedPoint, NonZeroScalar, PublicKey,
};

// ============================================================================
// The specification's sample data
// ============================================================================

const PRIVATE_A: [u8; 32] = hex32("3f49f6d4a3c55f3874c9b3e3d2103f504aff607beb40b7995899b8a6cd3c1abd");
const PUBLIC_B_X: [u8; 32] = hex32("1ea1f0f01faf1d9609592284f19e4c0047b58afd8615a69f559077b22faaa190");
const PUBLIC_B_Y: [u8; 32] = hex32("4c55f33e429dad377356703a9ab85160472d1130e28e36765f89aff915b1214a");
const DHKEY: [u8; 32] = hex32("ec0234a357c8ad05341010a60a397d9b99796b13b4f866f1868d34f373bfa698");

/// 32 bytes from 64 hex digits, most significant first, as the specification prints them.
const fn hex32(digits: &str) -> [u8; 32] {
    let digits = digits.as_bytes();
    let mut bytes = [0; 32];
    let mut place = 0;
    while place < 32 {
        bytes[place] = nibble(digits[2 * place]) << 4 | nibble(digits[2 * place + 1]);
        place += 1;
    }
    bytes
}

const fn nibble(digit: u8) -> u8 {
    match digit {
        b'0'..=b'9' => digit - b'0',
        _ => digit - b'a' + 10,
    }
}

// ============================================================================
// The calls
// ============================================================================

const SELF_TEST: u32 = 0;

const UNKNOWN_CALL: i32 = -1;
const HOST_TOO_OLD: i32 = -2;
const WRONG_ANSWER: i32 = -3;

/// The DHKey of the sample data, or none if a key would not read.
fn sample_dhkey() -> Option<[u8; 32]> {
    let private = Option::<NonZeroScalar>::from(NonZeroScalar::from_repr(PRIVATE_A.into()))?;
    let point = EncodedPoint::from_affine_coordinates(&PUBLIC_B_X.into(), &PUBLIC_B_Y.into(), false);
    let public = Option::<PublicKey>::from(PublicKey::from_encoded_point(&point))?;
    let shared = diffie_hellman(private, public.as_affine());
    Some((*shared.raw_secret_bytes()).into())
}

fn self_test() -> i32 {
    let started = module::microseconds();
    let dhkey = sample_dhkey();
    let took = module::microseconds().wrapping_sub(started);
    match dhkey == Some(DHKEY) {
        true => took.min(i32::MAX as u32) as i32,
        false => WRONG_ANSWER,
    }
}

/// # Safety
///
/// The firmware calls it once placed, with its own table; the input and output are unused.
#[no_mangle]
#[link_section = ".text.module_entry"]
pub unsafe extern "C" fn module_entry(
    host: &'static Host,
    call: u32,
    _input: *const u8,
    _input_length: usize,
    _output: *mut u8,
    _output_length: usize,
) -> i32 {
    if !module::start(host) {
        return HOST_TOO_OLD;
    }
    match call {
        SELF_TEST => self_test(),
        _ => UNKNOWN_CALL,
    }
}
