//! A guard at the bottom of the stack, so a stack that grows too deep faults instead of overwriting the statics below it.
//!
//! The memory protection unit makes the lowest `GUARD_BYTES` of the stack unreadable and unwritable.
//! The first store into them raises a MemManage fault, whose handler moves the stack pointer back to
//! the top of the stack, records the fault in the crash record, and halts until the watchdog resets
//! the watch. A frame bigger than the guard could step over it, so the guard catches the usual
//! overflow, a few hundred bytes too deep, rather than every one.

use core::ptr::addr_of;

use neotime_crash_record::Crash;

use crate::{restart, stage, storage};

// ============================================================================
// The guard
// ============================================================================

/// The guard's size: a power of two from 32 up, and the most bytes a frame can skip past unnoticed.
pub const GUARD_BYTES: u32 = 256;

const MPU_CONTROL: *mut u32 = 0xE000_ED94 as *mut u32;
const MPU_REGION_NUMBER: *mut u32 = 0xE000_ED98 as *mut u32;
const MPU_REGION_BASE: *mut u32 = 0xE000_ED9C as *mut u32;
const MPU_REGION_ATTRIBUTES: *mut u32 = 0xE000_EDA0 as *mut u32;
const SYSTEM_HANDLER_CONTROL: *mut u32 = 0xE000_ED24 as *mut u32;
const CONFIGURABLE_FAULT_STATUS: *const u32 = 0xE000_ED28 as *const u32;
const MEMORY_FAULT_ADDRESS: *const u32 = 0xE000_ED34 as *const u32;

const MPU_ENABLE: u32 = 1;
/// Everything outside the regions keeps the default memory map.
const MPU_DEFAULT_MAP_FOR_PRIVILEGED_CODE: u32 = 1 << 2;
const REGION_ENABLE: u32 = 1;
const NEVER_EXECUTE: u32 = 1 << 28;
const MEMORY_FAULT_ENABLE: u32 = 1 << 16;

/// The guard's lowest address: the first `GUARD_BYTES` boundary at or above the end of the statics.
pub fn guard_start() -> u32 {
    extern "C" {
        static _stack_end: u32;
    }
    let stack_end = addr_of!(_stack_end) as u32;
    stack_end.next_multiple_of(GUARD_BYTES)
}

/// Turns the guard on; called first thing in `main`, before anything deep runs.
pub fn start() {
    let size_field = (GUARD_BYTES.trailing_zeros() - 1) << 1;
    unsafe {
        MPU_REGION_NUMBER.write_volatile(0);
        MPU_REGION_BASE.write_volatile(guard_start());
        // No access at all, and no instructions fetched from it.
        MPU_REGION_ATTRIBUTES.write_volatile(NEVER_EXECUTE | size_field | REGION_ENABLE);
        MPU_CONTROL.write_volatile(MPU_ENABLE | MPU_DEFAULT_MAP_FOR_PRIVILEGED_CODE);
        SYSTEM_HANDLER_CONTROL.write_volatile(SYSTEM_HANDLER_CONTROL.read_volatile() | MEMORY_FAULT_ENABLE);
    }
    cortex_m::asm::dsb();
    cortex_m::asm::isb();
}

// ============================================================================
// The fault
// ============================================================================

// The handler takes no stack until it has moved the stack pointer back to the top, since the fault
// may have come from a push into the guard.
core::arch::global_asm!(
    ".section .text.MemoryManagement, \"ax\"",
    ".global MemoryManagement",
    ".type MemoryManagement, %function",
    ".thumb_func",
    "MemoryManagement:",
    "    mrs r0, msp",
    "    ldr r1, =_stack_start",
    "    msr msp, r1",
    "    b {record}",
    record = sym record_memory_fault,
);

/// Records the fault, then halts so the watchdog resets the watch.
///
/// `faulting_stack_pointer` is where the fault left the stack; the instruction's address is in the
/// frame stacked there, unless that frame itself landed in the guard.
unsafe extern "C" fn record_memory_fault(faulting_stack_pointer: u32) -> ! {
    let fault_status = unsafe { CONFIGURABLE_FAULT_STATUS.read_volatile() };
    let fault_address = unsafe { MEMORY_FAULT_ADDRESS.read_volatile() };
    let frame_is_readable = faulting_stack_pointer >= guard_start() + GUARD_BYTES;
    let stacked_word = |index: u32| {
        if frame_is_readable {
            unsafe { ((faulting_stack_pointer + 4 * index) as *const u32).read_volatile() }
        } else {
            0
        }
    };
    let program_counter = stacked_word(6);
    storage::record(storage::Breadcrumbs {
        stage: stage::FAULTED,
        detail: program_counter,
        fault_status,
    });
    restart::record_crash(&Crash::HardFault {
        program_counter,
        link_register: stacked_word(5),
        fault_status,
        fault_address,
    });
    loop {
        cortex_m::asm::wfe();
    }
}
