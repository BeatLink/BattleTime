//! A module's side: the heap it borrows from the firmware, its panic handler and the firmware calls.
//!
//! A module's entry calls `start` with the table it was given before anything else.

use core::{
    alloc::{GlobalAlloc, Layout},
    sync::atomic::{AtomicPtr, Ordering},
};

use crate::{Bytes, Host};

static HOST: AtomicPtr<Host> = AtomicPtr::new(core::ptr::null_mut());

/// Keeps the firmware's table for the rest of the call; false when the firmware is too old to give what this crate calls.
pub fn start(host: &'static Host) -> bool {
    HOST.store(host as *const Host as *mut Host, Ordering::Relaxed);
    host.length as usize >= core::mem::size_of::<Host>()
}

/// The firmware's table that `start` kept.
pub fn host() -> &'static Host {
    unsafe { &*HOST.load(Ordering::Relaxed) }
}

/// The bytes the firmware lent as a call's input.
///
/// # Safety
///
/// `bytes` must be the input the firmware passed to the entry, for that call.
pub unsafe fn input(bytes: &Bytes) -> &[u8] {
    match bytes.pointer.is_null() {
        true => &[],
        false => unsafe { core::slice::from_raw_parts(bytes.pointer, bytes.length) },
    }
}

/// The room the firmware lent for a call's output.
///
/// # Safety
///
/// `bytes` must be the output the firmware passed to the entry, for that call.
pub unsafe fn output(bytes: &mut Bytes) -> &mut [u8] {
    match bytes.pointer.is_null() {
        true => &mut [],
        false => unsafe { core::slice::from_raw_parts_mut(bytes.pointer, bytes.length) },
    }
}

/// Microseconds since start, wrapping.
pub fn microseconds() -> u32 {
    (host().microseconds)()
}

/// Fills `out` with random numbers from the chip's generator.
pub fn random(out: &mut [u8]) {
    unsafe { (host().random)(out.as_mut_ptr(), out.len()) }
}

// ============================================================================
// The heap and panics
// ============================================================================

struct FirmwareHeap;

unsafe impl GlobalAlloc for FirmwareHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        (host().allocate)(layout.size(), layout.align())
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        (host().free)(pointer, layout.size(), layout.align())
    }
}

#[global_allocator]
static HEAP: FirmwareHeap = FirmwareHeap;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    (host().panic)()
}
