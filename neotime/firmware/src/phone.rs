//! Whether a phone is connected over Bluetooth, as apps read it through `is_phone_connected`.

use core::sync::atomic::{AtomicBool, Ordering};

use neotime_runtime::SystemValue;

use crate::system_values;

static IS_CONNECTED: AtomicBool = AtomicBool::new(false);

pub fn is_connected() -> bool {
    IS_CONNECTED.load(Ordering::Relaxed)
}

/// Records a phone connecting or going away, and wakes the shell so apps hear of it.
pub fn set_connected(is_connected: bool) {
    if IS_CONNECTED.swap(is_connected, Ordering::Relaxed) != is_connected {
        system_values::changed(&[SystemValue::IsPhoneConnected]);
    }
}
