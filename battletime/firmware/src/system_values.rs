//! System values that changed since the shell last looked, such as the battery or the step count.

use core::sync::atomic::{AtomicU32, Ordering};

use battletime_runtime::SystemValue;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

static PENDING: AtomicU32 = AtomicU32::new(0);
static WAKE: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Tells the shell these values changed, so it redraws what shows them.
pub fn changed(values: &[SystemValue]) {
    let bits = values.iter().fold(0, |bits, value| bits | value.bit());
    PENDING.fetch_or(bits, Ordering::Relaxed);
    WAKE.signal(());
}

/// Waits for a change, and gives every value that changed since the last call as `SystemValue::bit`s.
pub async fn next_changes() -> u32 {
    WAKE.wait().await;
    PENDING.swap(0, Ordering::Relaxed)
}
