//! Native modules: rarely used native code kept in `/system/modules` on the external flash and run from the heap.
//!
//! A call reads the module's file into the heap, places it there with
//! `battletime_module_abi::place`, and calls its entry through the RAM's code alias. The module
//! gives back its heap when the call returns. A crash inside it is recorded against its name, and
//! a module that crashed the watch twice is not run again until its file changes.

use alloc::{boxed::Box, vec::Vec};
use core::alloc::Layout;
use core::fmt::Write;
use core::future::Future;
use core::pin::Pin;

use battletime_flash_store::FileAccess;
use battletime_module_abi::{
    code_alias,
    crashes::{crash_tag, module_of_crash, CrashCounts, FILE_BYTES},
    place, Bytes, Entry, Host, ModuleError, ModuleHeader, ABI_VERSION, ALIGNMENT_BYTES, HEADER_BYTES,
};
use battletime_packages::commands::MOST_MODULE_NAME_BYTES;

use crate::restart;

pub const MODULES_FOLDER: &str = "/system/modules";
const CRASHES_FILE: &str = "/system/modules/crashes";

type ModulePath = heapless::String<{ MODULES_FOLDER.len() + 1 + MOST_MODULE_NAME_BYTES }>;

fn module_path(name: &str) -> ModulePath {
    let mut path = ModulePath::new();
    let _ = write!(path, "{MODULES_FOLDER}/{name}");
    path
}

// ============================================================================
// Why a module did not run
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleFailure {
    Missing,
    Unreadable,
    Refused(ModuleError),
    NoRoom,
    /// It crashed the watch too often to run again.
    Stopped,
}

impl ModuleFailure {
    pub fn describe(self) -> &'static str {
        match self {
            ModuleFailure::Missing => "the module is not on the watch",
            ModuleFailure::Unreadable => "the module file could not be read",
            ModuleFailure::Refused(error) => error.describe(),
            ModuleFailure::NoRoom => "the heap has no room for the module",
            ModuleFailure::Stopped => "the module crashed the watch twice, so it no longer runs",
        }
    }
}

// ============================================================================
// The firmware calls a module is given
// ============================================================================

unsafe extern "C" fn allocate(size: usize, align: usize) -> *mut u8 {
    match Layout::from_size_align(size, align) {
        Ok(layout) if size > 0 => unsafe { alloc::alloc::alloc(layout) },
        _ => core::ptr::null_mut(),
    }
}

unsafe extern "C" fn free(pointer: *mut u8, size: usize, align: usize) {
    if let Ok(layout) = Layout::from_size_align(size, align) {
        unsafe { alloc::alloc::dealloc(pointer, layout) };
    }
}

extern "C" fn module_panicked() -> ! {
    panic!("a module panicked");
}

extern "C" fn microseconds() -> u32 {
    embassy_time::Instant::now().as_micros() as u32
}

unsafe extern "C" fn random(out: *mut u8, length: usize) {
    battletime_bluetooth::read_random_bytes(unsafe { core::slice::from_raw_parts_mut(out, length) });
}

static HOST: Host = Host {
    length: core::mem::size_of::<Host>() as u32,
    abi_version: ABI_VERSION as u32,
    allocate,
    free,
    panic: module_panicked,
    microseconds,
    random,
};

// ============================================================================
// Running a module
// ============================================================================

/// A boxed future, polled out of line, so a rare call costs the task that awaits it no code or frame.
pub type OutOfLine<'call, Output> = Pin<Box<dyn Future<Output = Output> + 'call>>;

/// Runs call `call` of the module `name` on `input`, with room for its output in `output`, giving what it answers.
pub fn call<'call>(
    files: &'call mut impl FileAccess,
    name: &'call str,
    call: u32,
    input: &'call [u8],
    output: &'call mut [u8],
) -> OutOfLine<'call, Result<i32, ModuleFailure>> {
    Box::pin(run(files, name, call, input, output))
}

async fn run(
    files: &mut impl FileAccess,
    name: &str,
    call: u32,
    input: &[u8],
    output: &mut [u8],
) -> Result<i32, ModuleFailure> {
    let path = module_path(name);
    let file_length = files
        .file_length(&path)
        .await
        .map_err(|_| ModuleFailure::Unreadable)?
        .ok_or(ModuleFailure::Missing)?;
    let header = read_header(files, &path).await?;
    if header.name() != name {
        return Err(ModuleFailure::Refused(ModuleError::OtherName));
    }
    if read_crash_counts(files).await.is_stopped(name, header.crc) {
        return Err(ModuleFailure::Stopped);
    }

    let stack = match header.stack_bytes {
        0 => 0,
        asked => asked as usize + EXCEPTION_FRAME_BYTES,
    };
    let words = file_length
        .max(header.stack_at() + stack)
        .div_ceil(ALIGNMENT_BYTES);
    let mut ram: Vec<u64> = Vec::new();
    ram.try_reserve_exact(words).map_err(|_| ModuleFailure::NoRoom)?;
    ram.resize(words, 0);
    let bytes =
        unsafe { core::slice::from_raw_parts_mut(ram.as_mut_ptr().cast::<u8>(), words * ALIGNMENT_BYTES) };
    // A whole read from the file's start is checked against the store's checksum.
    let read = files.read_file(&path, 0, &mut bytes[..file_length]).await;
    if read != Ok(Some(file_length)) {
        return Err(ModuleFailure::Unreadable);
    }
    let ram_address = bytes.as_ptr() as u32;
    let header = place(bytes, file_length, ram_address).map_err(ModuleFailure::Refused)?;
    let stack_top = (stack > 0).then(|| ram_address + (header.stack_at() + stack) as u32);
    Ok(unsafe {
        enter(
            name,
            code_alias(ram_address) + header.entry,
            call,
            input,
            output,
            stack_top,
        )
    })
}

/// Room below a module's own stack for the registers an interrupt saves on it before its handler moves to the main stack.
const EXCEPTION_FRAME_BYTES: usize = 128;

/// Calls the placed module's entry at `entry_address`, naming it for the crash record while it runs.
///
/// With `stack_top`, the module runs on its own stack: the process stack pointer is set there and
/// thread mode moves onto it for the call, while interrupts still run on the main stack.
#[inline(never)]
unsafe fn enter(
    name: &str,
    entry_address: u32,
    call: u32,
    input: &[u8],
    output: &mut [u8],
    stack_top: Option<u32>,
) -> i32 {
    // The code was written as data, so both finish before the first instruction is fetched.
    cortex_m::asm::dsb();
    cortex_m::asm::isb();
    let input = Bytes {
        pointer: input.as_ptr().cast_mut(),
        length: input.len(),
    };
    let mut output = Bytes {
        pointer: output.as_mut_ptr(),
        length: output.len(),
    };
    restart::set_running_module(Some(crash_tag(name)));
    let answer = match stack_top {
        None => {
            let entry: Entry = unsafe { core::mem::transmute::<usize, Entry>(entry_address as usize) };
            unsafe { entry(&HOST, call, &input, &mut output) }
        }
        Some(top) => unsafe { call_on_own_stack(entry_address, call, &input, &mut output, top) },
    };
    restart::set_running_module(None);
    answer
}

/// Calls the entry with the process stack pointer at `top` and thread mode on it, then goes back to the main stack.
#[inline(never)]
unsafe fn call_on_own_stack(
    entry_address: u32,
    call: u32,
    input: &Bytes,
    output: &mut Bytes,
    top: u32,
) -> i32 {
    let answer: i32;
    unsafe {
        core::arch::asm!(
            // CONTROL's bit 1 picks the process stack for thread mode; r4 and r5 survive the call.
            "mrs r4, CONTROL",
            "msr PSP, {top}",
            "orr r5, r4, #2",
            "msr CONTROL, r5",
            "isb",
            "blx {entry}",
            "msr CONTROL, r4",
            "isb",
            top = in(reg) top,
            entry = in(reg) entry_address,
            inout("r0") &HOST as *const Host => answer,
            inout("r1") call => _,
            inout("r2") input as *const Bytes => _,
            inout("r3") output as *mut Bytes => _,
            // What a call may change under the procedure call standard, the chip's single-precision registers included.
            out("r4") _,
            out("r5") _,
            out("r12") _,
            out("lr") _,
            out("s0") _, out("s1") _, out("s2") _, out("s3") _,
            out("s4") _, out("s5") _, out("s6") _, out("s7") _,
            out("s8") _, out("s9") _, out("s10") _, out("s11") _,
            out("s12") _, out("s13") _, out("s14") _, out("s15") _,
        );
    }
    answer
}

async fn read_header(files: &mut impl FileAccess, path: &str) -> Result<ModuleHeader, ModuleFailure> {
    let mut header = [0; HEADER_BYTES];
    let read = files.read_file(path, 0, &mut header).await;
    if read != Ok(Some(HEADER_BYTES)) {
        return Err(ModuleFailure::Refused(ModuleError::TooShort));
    }
    ModuleHeader::parse(&header).map_err(ModuleFailure::Refused)
}

// ============================================================================
// Counting crashes
// ============================================================================

async fn read_crash_counts(files: &mut impl FileAccess) -> CrashCounts {
    let mut bytes = [0; FILE_BYTES];
    match files.read_file(CRASHES_FILE, 0, &mut bytes).await {
        Ok(Some(length)) => CrashCounts::parse(&bytes[..length]),
        _ => CrashCounts::default(),
    }
}

/// Counts the crash this start found against the module that was running, once, at start.
pub fn count_last_crash(files: &mut impl FileAccess) -> OutOfLine<'_, ()> {
    Box::pin(count_crash(files))
}

async fn count_crash(files: &mut impl FileAccess) {
    let Some(recorded) = restart::pending_crash() else {
        return;
    };
    let Some(name) = module_of_crash(recorded.open_app_id()) else {
        return;
    };
    let record = recorded.encode();
    let record_crc = u32::from_le_bytes(record[record.len() - 4..].try_into().unwrap_or_default());
    let module_crc = match read_header(files, &module_path(name)).await {
        Ok(header) => header.crc,
        Err(_) => 0,
    };
    let mut counts = read_crash_counts(files).await;
    if counts.count(name, module_crc, record_crc) {
        let _ = files.write_file(CRASHES_FILE, &counts.encode()).await;
    }
}
