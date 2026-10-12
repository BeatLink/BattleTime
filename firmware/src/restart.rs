//! Why the watch last restarted: the reset reasons read at start, and the panic or fault the previous run recorded.
//!
//! The crash record sits in `.uninit`, which start-up leaves as it was, right after the firmware's
//! other static RAM. That is below the bootloader's deepest stack and above its own few statics, so
//! it survives the bootloader running between two starts; docs/memory-map.md has the numbers.

use core::{
    mem::MaybeUninit,
    ptr::{addr_of, addr_of_mut},
};

use neotime_crash_record::{find_return_addresses, last_restart_words, Crash, ResetReasons, RECORD_SIZE};

use crate::diagnostics;

// ============================================================================
// The record in RAM a reset keeps
// ============================================================================

#[link_section = ".uninit.CRASH_RECORD"]
static mut CRASH_RECORD: MaybeUninit<[u8; RECORD_SIZE]> = MaybeUninit::uninit();

fn crash_record_address() -> *mut [u8; RECORD_SIZE] {
    addr_of_mut!(CRASH_RECORD).cast()
}

/// Called from the panic and HardFault handlers, so it takes no lock and allocates nothing.
pub fn record_crash(crash: &Crash) {
    unsafe { crash_record_address().write_volatile(crash.encode()) };
}

/// The most stack words a panic looks through for return addresses.
const MOST_STACK_WORDS_SEARCHED: usize = 512;

/// Records the code addresses on the stack above `stack_pointer`; kept out of line, so its own frame sits below it.
#[inline(never)]
pub fn record_panic(stack_pointer: u32) {
    extern "C" {
        static __stext: u32;
        static __etext: u32;
        static _stack_start: u32;
    }
    let code = addr_of!(__stext) as u32..addr_of!(__etext) as u32;
    let stack_top = addr_of!(_stack_start) as u32;

    let stack_words = (stack_pointer..stack_top)
        .step_by(4)
        .take(MOST_STACK_WORDS_SEARCHED)
        .map(|address| unsafe { (address as *const u32).read_volatile() });
    record_crash(&Crash::Panic {
        return_addresses: find_return_addresses(stack_words, code),
    });
}

/// Reads the previous run's crash and clears the record, so the next ordinary reset shows none.
fn take_previous_crash() -> Option<Crash> {
    let record = unsafe { crash_record_address().read_volatile() };
    unsafe { crash_record_address().write_volatile([0; RECORD_SIZE]) };
    Crash::decode(&record)
}

// ============================================================================
// The reset reasons
// ============================================================================

/// Reads and clears `RESETREAS`, whose bits otherwise pile up from one reset to the next.
fn take_reset_reasons() -> ResetReasons {
    let register = nrf_pac::POWER.resetreas().read();
    nrf_pac::POWER.resetreas().write_value(register);
    ResetReasons::from_register(register.0)
}

// ============================================================================
// What the start found
// ============================================================================

pub struct LastRestart {
    pub reasons: ResetReasons,
    pub crash: Option<Crash>,
}

/// Takes the reset reasons and the crash record once, and puts them after the diagnostics counters.
pub fn take_last_restart() -> LastRestart {
    let last_restart = LastRestart {
        reasons: take_reset_reasons(),
        crash: take_previous_crash(),
    };
    diagnostics::set_last_restart(last_restart_words(
        last_restart.reasons,
        last_restart.crash.as_ref(),
    ));
    last_restart
}
