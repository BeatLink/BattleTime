//! The P-256 module: the elliptic-curve Diffie-Hellman maths of Bluetooth's LE Secure Connections pairing.
//!
//! Pairing is rare, and the maths would cost the firmware tens of kilobytes, so it lives here and
//! runs only when asked. Keys and points are 32-byte big-endian numbers, as the specification's
//! pairing messages carry them after their byte order is turned round.
//!
//! - Call 0 is a self-test: it works out the DHKey of the Bluetooth Core specification's P-256
//!   sample data (Vol 3, Part H, 2.3.5.6.1) and answers how many microseconds that took.
//! - Call 1 makes a key pair from the chip's random numbers: the private key, then the public key's
//!   x and y, 96 bytes of output; it answers 96.
//! - Call 2 works out the DHKey from 96 bytes of input, our private key then the phone's public
//!   key's x and y, into 32 bytes of output; it answers 32, or -4 for a point not on the curve.
//! - Call 3 makes two key pairs and checks that each side's DHKey agrees, answering the microseconds.

#![no_std]
#![no_main]

use battletime_module_abi::{module, Bytes, Host};
use p256::{
    ecdh::diffie_hellman,
    elliptic_curve::sec1::{FromEncodedPoint, ToEncodedPoint},
    EncodedPoint, NonZeroScalar, PublicKey,
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

/// A stack of its own: the curve's arithmetic takes about 5 KB, deeper than the firmware's stack leaves room for.
#[used]
#[no_mangle]
pub static MODULE_STACK_BYTES: u32 = 5632;

const SELF_TEST: u32 = 0;
const KEY_PAIR: u32 = 1;
const DHKEY_OF: u32 = 2;
const PAIRING_SELF_TEST: u32 = 3;

const UNKNOWN_CALL: i32 = -1;
const HOST_TOO_OLD: i32 = -2;
const WRONG_ANSWER: i32 = -3;
const NOT_ON_THE_CURVE: i32 = -4;
const TOO_SHORT: i32 = -5;

const KEY_BYTES: usize = 32;
const KEY_PAIR_BYTES: usize = 3 * KEY_BYTES;
/// Tries at drawing a private key below the curve's order, which each one misses with odds of about 2^-32.
const MOST_DRAWS: usize = 8;

/// The DHKey of the sample data, or none if a key would not read.
fn sample_dhkey() -> Option<[u8; 32]> {
    let private = Option::<NonZeroScalar>::from(NonZeroScalar::from_repr(PRIVATE_A.into()))?;
    let point = EncodedPoint::from_affine_coordinates(&PUBLIC_B_X.into(), &PUBLIC_B_Y.into(), false);
    let public = Option::<PublicKey>::from(PublicKey::from_encoded_point(&point))?;
    let shared = diffie_hellman(private, public.as_affine());
    Some((*shared.raw_secret_bytes()).into())
}

/// A private key from the chip's random numbers.
fn random_private_key() -> Option<NonZeroScalar> {
    for _ in 0..MOST_DRAWS {
        let mut bytes = [0u8; KEY_BYTES];
        module::random(&mut bytes);
        if let Some(key) = Option::<NonZeroScalar>::from(NonZeroScalar::from_repr(bytes.into())) {
            return Some(key);
        }
    }
    None
}

/// A key pair as call 1 gives it: the private key, then the public key's x and y.
fn key_pair() -> Option<[u8; KEY_PAIR_BYTES]> {
    let private = random_private_key()?;
    let public = PublicKey::from_secret_scalar(&private).to_encoded_point(false);
    let mut out = [0u8; KEY_PAIR_BYTES];
    out[..KEY_BYTES].copy_from_slice(&private.to_bytes());
    out[KEY_BYTES..2 * KEY_BYTES].copy_from_slice(public.x()?);
    out[2 * KEY_BYTES..].copy_from_slice(public.y()?);
    Some(out)
}

fn key_at(keys: &[u8; KEY_PAIR_BYTES], place: usize) -> [u8; KEY_BYTES] {
    let mut key = [0u8; KEY_BYTES];
    key.copy_from_slice(&keys[place * KEY_BYTES..(place + 1) * KEY_BYTES]);
    key
}

/// The DHKey of our private key and the phone's public key, laid out as call 2 takes them.
fn dhkey_of(keys: &[u8; KEY_PAIR_BYTES]) -> Result<[u8; KEY_BYTES], i32> {
    let private = Option::<NonZeroScalar>::from(NonZeroScalar::from_repr(key_at(keys, 0).into()))
        .ok_or(NOT_ON_THE_CURVE)?;
    let point =
        EncodedPoint::from_affine_coordinates(&key_at(keys, 1).into(), &key_at(keys, 2).into(), false);
    let public = Option::<PublicKey>::from(PublicKey::from_encoded_point(&point)).ok_or(NOT_ON_THE_CURVE)?;
    Ok((*diffie_hellman(private, public.as_affine()).raw_secret_bytes()).into())
}

/// Makes two key pairs and checks that both sides reach the same DHKey.
fn pairing_self_test() -> i32 {
    let started = module::microseconds();
    let (Some(ours), Some(theirs)) = (key_pair(), key_pair()) else {
        return WRONG_ANSWER;
    };
    let mut ours_with_theirs = ours;
    ours_with_theirs[KEY_BYTES..].copy_from_slice(&theirs[KEY_BYTES..]);
    let mut theirs_with_ours = theirs;
    theirs_with_ours[KEY_BYTES..].copy_from_slice(&ours[KEY_BYTES..]);
    let agree = matches!(
        (dhkey_of(&ours_with_theirs), dhkey_of(&theirs_with_ours)),
        (Ok(ours), Ok(theirs)) if ours == theirs
    );
    let took = module::microseconds().wrapping_sub(started);
    match agree {
        true => took.min(i32::MAX as u32) as i32,
        false => WRONG_ANSWER,
    }
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
/// The firmware calls it once placed, with its own table, the call's input and room for its output.
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
    let output = unsafe { module::output(output) };
    match call {
        SELF_TEST => self_test(),
        KEY_PAIR => match (output.get_mut(..KEY_PAIR_BYTES), key_pair()) {
            (Some(out), Some(pair)) => {
                out.copy_from_slice(&pair);
                KEY_PAIR_BYTES as i32
            }
            (None, _) => TOO_SHORT,
            (_, None) => WRONG_ANSWER,
        },
        DHKEY_OF => {
            let (Some(keys), Some(out)) = (input.get(..KEY_PAIR_BYTES), output.get_mut(..KEY_BYTES)) else {
                return TOO_SHORT;
            };
            let mut laid_out = [0u8; KEY_PAIR_BYTES];
            laid_out.copy_from_slice(keys);
            match dhkey_of(&laid_out) {
                Ok(dhkey) => {
                    out.copy_from_slice(&dhkey);
                    KEY_BYTES as i32
                }
                Err(answer) => answer,
            }
        }
        PAIRING_SELF_TEST => pairing_self_test(),
        _ => UNKNOWN_CALL,
    }
}
