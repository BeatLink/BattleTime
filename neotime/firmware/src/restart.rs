//! Why the watch last restarted: the reset reasons read at start, and the panic or fault the previous run recorded.
//!
//! With the shell, a crash stays in the record until the wearer picks Restart on the crash screen or
//! the companion app says it saved a debug report, so a reset in between shows it again. The
//! recovery firmware has no crash screen and clears the record at start.
//!
//! The crash record sits in `.uninit`, which start-up leaves as it was, right after the firmware's
//! other static RAM. That is below the bootloader's deepest stack and above its own few statics, so
//! it survives the bootloader running between two starts; docs/memory-map.md has the numbers.

use core::{
    mem::MaybeUninit,
    ptr::{addr_of, addr_of_mut},
    sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering},
};

use embassy_sync::once_lock::OnceLock;
use neotime_crash_record::{
    find_return_addresses, last_restart_words, open_app_bytes, Crash, RecordedCrash, ResetReasons,
    RestartReason, OPEN_APP_BYTES, OUT_OF_MEMORY_RETURN_ADDRESSES, RECORD_SIZE,
};

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
    let recorded = RecordedCrash {
        crash: *crash,
        open_app: core::array::from_fn(|place| OPEN_APP[place].load(Ordering::Relaxed)),
    };
    unsafe { crash_record_address().write_volatile(recorded.encode()) };
}

// ============================================================================
// What the crash record names
// ============================================================================

/// The id of the app, face or launcher open now, kept for the crash record.
static OPEN_APP: [AtomicU8; OPEN_APP_BYTES] = [const { AtomicU8::new(0) }; OPEN_APP_BYTES];

pub fn set_open_app(id: &str) {
    for (kept, byte) in OPEN_APP.iter().zip(open_app_bytes(id)) {
        kept.store(byte, Ordering::Relaxed);
    }
}

/// The size of the last allocation the heap refused, or 0 once one succeeds after it.
static REFUSED_ALLOCATION_BYTES: AtomicU32 = AtomicU32::new(0);
/// The heap's free bytes when it refused that allocation.
static FREE_BYTES_AT_REFUSAL: AtomicU32 = AtomicU32::new(0);

/// How many allocations the heap has refused this run.
static ALLOCATION_FAILURES: AtomicU32 = AtomicU32::new(0);
/// The fewest free heap bytes after any allocation this run.
static LEAST_FREE_HEAP_BYTES: AtomicU32 = AtomicU32::new(u32::MAX);

/// Called by the heap after each allocation, so a panic straight after a refusal is recorded as running out of memory.
pub fn note_allocation(requested_bytes: usize, refused: bool, free_bytes: impl FnOnce() -> usize) {
    let free_bytes = free_bytes() as u32;
    LEAST_FREE_HEAP_BYTES.fetch_min(free_bytes, Ordering::Relaxed);
    if refused {
        ALLOCATION_FAILURES.fetch_add(1, Ordering::Relaxed);
        REFUSED_ALLOCATION_BYTES.store(requested_bytes as u32, Ordering::Relaxed);
        FREE_BYTES_AT_REFUSAL.store(free_bytes, Ordering::Relaxed);
    } else if REFUSED_ALLOCATION_BYTES.load(Ordering::Relaxed) != 0 {
        REFUSED_ALLOCATION_BYTES.store(0, Ordering::Relaxed);
    }
}

#[cfg_attr(not(with_shell), allow(dead_code))]
pub fn allocation_failures() -> u32 {
    ALLOCATION_FAILURES.load(Ordering::Relaxed)
}

/// The fewest free heap bytes this run, or `now_free` before the first allocation.
#[cfg_attr(not(with_shell), allow(dead_code))]
pub fn least_free_heap_bytes(now_free: usize) -> u32 {
    LEAST_FREE_HEAP_BYTES.load(Ordering::Relaxed).min(now_free as u32)
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
    let return_addresses = find_return_addresses(stack_words, code);
    let refused_bytes = REFUSED_ALLOCATION_BYTES.load(Ordering::Relaxed);
    let crash = if refused_bytes != 0 {
        let mut nearest = [0; OUT_OF_MEMORY_RETURN_ADDRESSES];
        nearest.copy_from_slice(&return_addresses[..OUT_OF_MEMORY_RETURN_ADDRESSES]);
        Crash::OutOfMemory {
            requested_bytes: refused_bytes,
            free_bytes: FREE_BYTES_AT_REFUSAL.load(Ordering::Relaxed),
            return_addresses: nearest,
        }
    } else {
        Crash::Panic { return_addresses }
    };
    record_crash(&crash);
}

/// Reads the previous run's crash; the recovery firmware also clears the record, having no crash screen.
fn read_previous_crash() -> Option<RecordedCrash> {
    let record = unsafe { crash_record_address().read_volatile() };
    if cfg!(not(with_shell)) {
        clear_record();
    }
    RecordedCrash::decode(&record)
}

fn clear_record() {
    unsafe { crash_record_address().write_volatile([0; RECORD_SIZE]) };
}

// ============================================================================
// Stack overflows, counted across restarts
// ============================================================================

/// The count and the count's bits flipped, so RAM after the power came on reads as no count.
#[link_section = ".uninit.STACK_OVERFLOWS"]
static mut STACK_OVERFLOWS: MaybeUninit<[u32; 2]> = MaybeUninit::uninit();

/// How many times the stack has overflowed into its guard since the power came on.
#[cfg_attr(not(with_shell), allow(dead_code))]
pub fn stack_overflows() -> u32 {
    let [count, check] = unsafe { addr_of!(STACK_OVERFLOWS).cast::<[u32; 2]>().read_volatile() };
    if check == !count {
        count
    } else {
        0
    }
}

/// Called from the stack guard's fault handler, so it takes no lock and no stack to speak of.
pub fn count_stack_overflow() {
    let count = stack_overflows().wrapping_add(1);
    unsafe {
        addr_of_mut!(STACK_OVERFLOWS)
            .cast::<[u32; 2]>()
            .write_volatile([count, !count])
    };
}

// ============================================================================
// The crash the wearer has not dealt with yet
// ============================================================================

/// The previous run's crash, set once at start.
static PREVIOUS_CRASH: OnceLock<RecordedCrash> = OnceLock::new();

/// Set while the previous run's crash waits for the wearer or the companion app.
static CRASH_PENDING: AtomicBool = AtomicBool::new(false);

/// The previous run's crash while it is still waiting for the wearer or the companion app.
#[cfg_attr(not(with_shell), allow(dead_code))]
pub fn pending_crash() -> Option<&'static RecordedCrash> {
    if CRASH_PENDING.load(Ordering::Relaxed) {
        PREVIOUS_CRASH.try_get()
    } else {
        None
    }
}

/// Forgets the crash, for Restart on the crash screen or a saved debug report; the diagnostics words keep it for this run.
pub fn dismiss_crash() {
    if CRASH_PENDING.swap(false, Ordering::Relaxed) {
        clear_record();
    }
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
    pub crash: Option<RecordedCrash>,
}

/// Room for every reset reason at once, so the text is never cut short.
const RESTART_REASON_BYTES: usize = 96;

/// Why the watch last restarted, in words, kept in a static so apps read it without copying it.
static RESTART_REASON: OnceLock<heapless::String<RESTART_REASON_BYTES>> = OnceLock::new();

/// Takes the reset reasons and the crash record once, and puts them after the diagnostics counters.
pub fn take_last_restart() -> LastRestart {
    let last_restart = LastRestart {
        reasons: take_reset_reasons(),
        crash: read_previous_crash(),
    };
    if let Some(recorded) = last_restart.crash {
        let _ = PREVIOUS_CRASH.init(recorded);
        CRASH_PENDING.store(cfg!(with_shell), Ordering::Relaxed);
    }
    diagnostics::set_last_restart(last_restart_words(
        last_restart.reasons,
        last_restart.crash.as_ref(),
    ));
    RESTART_REASON.get_or_init(|| {
        let mut text = heapless::String::new();
        let reason = RestartReason {
            reasons: last_restart.reasons,
            crash: last_restart.crash.as_ref().map(|recorded| &recorded.crash),
        };
        let _ = core::fmt::write(&mut text, format_args!("{reason}"));
        text
    });
    last_restart
}

/// Why the watch last restarted, as the `reset_reason` system value reads; empty until start-up has looked.
pub fn restart_reason() -> &'static str {
    RESTART_REASON.try_get().map_or("", |text| text.as_str())
}
