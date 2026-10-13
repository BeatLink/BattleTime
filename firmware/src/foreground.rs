//! What the wearer is watching, as the shell and the touch panel report it, for background work to wait on.

use core::sync::atomic::{AtomicBool, Ordering};

use embassy_time::{Duration, Instant, Timer};
use neotime_foreground::{Foreground, CHECK_INTERVAL_IN_MILLISECONDS};

static IS_DRAWING: AtomicBool = AtomicBool::new(false);
static IS_FINGER_DOWN: AtomicBool = AtomicBool::new(false);

pub fn set_drawing(is_drawing: bool) {
    IS_DRAWING.store(is_drawing, Ordering::Relaxed);
}

pub fn set_finger_down(is_finger_down: bool) {
    IS_FINGER_DOWN.store(is_finger_down, Ordering::Relaxed);
}

fn current() -> Foreground {
    Foreground {
        is_drawing: IS_DRAWING.load(Ordering::Relaxed),
        is_finger_down: IS_FINGER_DOWN.load(Ordering::Relaxed),
    }
}

/// Waits until the screen is still and no finger is down, or until the longest hold has passed.
pub async fn wait_for_a_pause() {
    let started = Instant::now();
    while !current().may_start(started.elapsed().as_millis()) {
        Timer::after(Duration::from_millis(CHECK_INTERVAL_IN_MILLISECONDS)).await;
    }
}
