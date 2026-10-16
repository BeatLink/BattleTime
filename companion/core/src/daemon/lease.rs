// The window's turns with the watch's file service, which it shares with the daemon's own sync.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::OwnedMutexGuard;

// The longest the window may hold the file service, in case it never lets go.
pub const LONGEST_LEASE: Duration = Duration::from_secs(120);

// While the window holds a turn, the file service's answers go to it rather than the daemon.
#[derive(Default)]
pub struct WindowLease {
    guard: Mutex<Option<(u64, OwnedMutexGuard<()>)>>,
    held: AtomicBool,
    next: AtomicU64,
}

impl WindowLease {
    pub fn held(&self) -> bool {
        self.held.load(Ordering::SeqCst)
    }

    // Takes the turn the file client handed over, and gives back the number that ends it.
    pub fn hold(&self, guard: OwnedMutexGuard<()>) -> u64 {
        let number = self.next.fetch_add(1, Ordering::SeqCst);
        *self.guard.lock().unwrap() = Some((number, guard));
        self.held.store(true, Ordering::SeqCst);
        number
    }

    // Ends the turn `number`, or whichever is held when it is None.
    pub fn release(&self, number: Option<u64>) {
        let mut guard = self.guard.lock().unwrap();
        if number.is_none() || guard.as_ref().map(|(held, _)| *held) == number {
            *guard = None;
            self.held.store(false, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn only_the_turn_held_now_can_be_ended_by_number() {
        let lease = WindowLease::default();
        let service = Arc::new(tokio::sync::Mutex::new(()));

        let number = lease.hold(service.clone().lock_owned().await);
        lease.release(Some(number + 7));
        assert!(lease.held());
        lease.release(Some(number));
        assert!(!lease.held());

        lease.hold(service.lock_owned().await);
        lease.release(None);
        assert!(!lease.held());
    }
}
