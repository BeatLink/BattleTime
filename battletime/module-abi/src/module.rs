//! A module's side: the heap it borrows from the firmware, its panic handler and the firmware calls.
//!
//! A module's entry calls `start` with the table it was given before anything else.

use core::{
    alloc::{GlobalAlloc, Layout},
    sync::atomic::{AtomicPtr, Ordering},
};

use crate::Host;

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

/// Microseconds since start, wrapping.
pub fn microseconds() -> u32 {
    (host().microseconds)()
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
