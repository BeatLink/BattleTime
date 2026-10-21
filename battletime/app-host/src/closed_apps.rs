//! Apps that are closed but still have subscriptions to run, such as an alarm or a running timer.
//!
//! The embedder keeps one small entry for each such app: when its next subscription is due and the
//! heap a run needs, and nothing else of it in RAM. When an entry comes due, the embedder loads the
//! app without a screen (`AppHost::open_in_background`), `run_while_closed` fires what is due, and
//! the app is recorded again. A message from the phone for a closed app runs its message handlers
//! the same way (`deliver_message_while_closed`). A handler that calls `wake_screen` asks for its app to be opened and
//! the screen lit, on the screen its `go` named, as InfiniTime opens its alarm and timer. The alarm
//! app's next ring is `next_alarm`, the bell watch faces show.

use alloc::{string::String, vec::Vec};

use battletime_runtime::{
    memory_need::closed_run_bytes, subscriptions::SubscriptionKind, AppImage, EffectKind, Value, NONE,
};

use crate::host::AppHost;
use crate::system::{AppEffect, System};

// ============================================================================
// The table of closed apps
// ============================================================================

/// The app whose next ring is the `next_alarm` faces show.
pub const ALARM_APP_ID: &str = "alarm";

/// One closed app with something to run: when, since when its timers were checked, and the heap a run needs.
struct ClosedApp {
    id: String,
    next_due: Value,
    checked_at: Value,
    run_bytes: u32,
}

/// Every closed app with a subscription still to come, a few bytes each.
#[derive(Default)]
pub struct ClosedApps {
    apps: Vec<ClosedApp>,
}

impl ClosedApps {
    pub const NONE: ClosedApps = ClosedApps { apps: Vec::new() };

    /// Notes when a closed app's next subscription is due, or forgets it when nothing of it runs while closed.
    pub fn record(&mut self, id: &str, host: &AppHost, now: Value) {
        self.forget(id);
        if host.is_open() {
            return;
        }
        let Some(next_due) = host.next_subscription_due(now) else {
            return;
        };
        self.apps.push(ClosedApp {
            id: String::from(id),
            next_due,
            checked_at: now,
            run_bytes: closed_run_bytes(host.image()),
        });
    }

    pub fn forget(&mut self, id: &str) {
        self.apps.retain(|app| app.id != id);
    }

    pub fn clear(&mut self) {
        self.apps.clear();
    }

    /// The closed apps' ids, in the order they were recorded.
    pub fn ids(&self) -> impl Iterator<Item = &str> + '_ {
        self.apps.iter().map(|app| app.id.as_str())
    }

    /// Whether the app may run, being neither the open one nor one whose state the phone holds.
    fn may_run(app: &ClosedApp, skipped: &[&str]) -> bool {
        !skipped.contains(&app.id.as_str())
    }

    /// When the next closed app not in `skipped` comes due.
    pub fn next_due(&self, skipped: &[&str]) -> Option<Value> {
        self.apps
            .iter()
            .filter(|app| Self::may_run(app, skipped))
            .map(|app| app.next_due)
            .min()
    }

    /// The closed app not in `skipped` due soonest, if one is due by `now`.
    pub fn due(&self, now: Value, skipped: &[&str]) -> Option<&str> {
        self.apps
            .iter()
            .filter(|app| Self::may_run(app, skipped) && app.next_due <= now)
            .min_by_key(|app| app.next_due)
            .map(|app| app.id.as_str())
    }

    /// Whether the app has come due by `now`, such as while the phone held its state.
    pub fn is_due(&self, id: &str, now: Value) -> bool {
        self.apps.iter().any(|app| app.id == id && app.next_due <= now)
    }

    /// The heap a run of the app needs, its image and the host's working memory.
    pub fn run_bytes(&self, id: &str) -> Option<u32> {
        self.apps.iter().find(|app| app.id == id).map(|app| app.run_bytes)
    }

    /// When the app was last recorded, which its timers count from when it next runs.
    pub fn checked_at(&self, id: &str) -> Option<Value> {
        self.apps
            .iter()
            .find(|app| app.id == id)
            .map(|app| app.checked_at)
    }

    /// When the closed alarm app rings next, or `NONE`, as the `next_alarm` system value reads.
    pub fn next_alarm(&self, open_id: &str) -> Value {
        self.apps
            .iter()
            .find(|app| app.id == ALARM_APP_ID && app.id != open_id)
            .map_or(NONE, |app| app.next_due)
    }
}

// ============================================================================
// Running a closed app
// ============================================================================

/// A closed app's handler asked to be seen: open the app, on this screen if it named one, and light the screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WakeRequest {
    pub screen: Option<String>,
}

/// What a closed app's run asked of the screen, which the embedder's `carry_out` hands to `hold_back` first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClosedRun {
    wakes: bool,
    screen: Option<String>,
}

impl ClosedRun {
    /// Takes an effect that needs a screen, which a closed app has none of; true when it was taken.
    ///
    /// `wake_screen` and `go` become the wake request, and turning the screen off, keeping it on, the
    /// brightness, going back, closing, opening another app or the face chooser, reading a file ahead and rolling the firmware back are
    /// dropped. Anything else, such as a vibration, is left for the embedder to carry out.
    pub fn hold_back(&mut self, effect: &AppEffect) -> bool {
        match effect.kind {
            EffectKind::WakeScreen => self.wakes = true,
            EffectKind::Go => self.screen = effect.text(0).map(String::from),
            EffectKind::ScreenOff
            | EffectKind::KeepScreenOn
            | EffectKind::SetBrightness
            | EffectKind::Back
            | EffectKind::Close
            | EffectKind::OpenApp
            | EffectKind::ChooseWatchFace
            | EffectKind::PrefetchFile
            | EffectKind::RollBackFirmware => {}
            _ => return false,
        }
        true
    }
}

/// An embedder that can run closed apps: its `carry_out` gives each effect to the `ClosedRun` held here first.
///
/// The embedder's own system is used rather than a wrapper around it, so the host's code is not built twice.
pub trait RunsClosedApps: System {
    fn closed_run(&mut self) -> &mut Option<ClosedRun>;
}

/// Fires the closed app's subscriptions due since `checked_at`, and says whether a handler asked to wake the screen.
pub fn run_while_closed<Embedder: RunsClosedApps>(
    host: &mut AppHost,
    checked_at: Value,
    now: Value,
    system: &mut Embedder,
) -> Option<WakeRequest> {
    *system.closed_run() = Some(ClosedRun::default());
    host.check_timers_from(checked_at);
    host.tick(now, system);
    take_wake_request(system)
}

/// Hands a closed app a message from the phone, as `deliver_message` does, and says whether a handler asked to wake the screen.
///
/// Only its message subscriptions without `while_open` run, since the app is closed.
pub fn deliver_message_while_closed<Embedder: RunsClosedApps>(
    host: &mut AppHost,
    numbers: &[(&str, Value)],
    text: Option<(&str, &str)>,
    now: Value,
    system: &mut Embedder,
) -> Option<WakeRequest> {
    *system.closed_run() = Some(ClosedRun::default());
    host.deliver_message(numbers, text, now, system);
    take_wake_request(system)
}

/// Ends a closed run, with the wake request its handlers made, if any.
fn take_wake_request<Embedder: RunsClosedApps>(system: &mut Embedder) -> Option<WakeRequest> {
    let run = system.closed_run().take()?;
    run.wakes.then_some(WakeRequest { screen: run.screen })
}

/// Whether a closed app's image hears messages from the phone, so a message for it is worth loading it for.
pub fn hears_messages_while_closed(image: &AppImage) -> bool {
    (0..image.subscription_count())
        .filter_map(|index| image.subscription(index))
        .any(|subscription| subscription.kind == SubscriptionKind::Message && !subscription.only_while_open)
}

// ============================================================================
// Holding an app's state for the phone
// ============================================================================

/// The app whose saved state the phone is rewriting, which neither runs while closed nor saves until the phone is done.
///
/// Times are the embedder's own clock ticks, which only need to count up.
/// A save the open app makes meanwhile waits here: the phone's write replaces it, and a hold that ends
/// without one, by `hold: false`, the time running out or the link dropping, hands it back to write.
#[derive(Default)]
pub struct StateHold {
    app: String,
    /// When the hold runs out, or 0 once it has ended.
    ends_at: u64,
    waiting_save: Option<Vec<u8>>,
}

impl StateHold {
    pub const NONE: StateHold = StateHold {
        app: String::new(),
        ends_at: 0,
        waiting_save: None,
    };

    /// Holds the app's state until `ends_at`, ending a hold on any other app.
    pub fn hold(&mut self, app: &str, ends_at: u64) {
        // The shell writes a save handed back before the phone can hold another app, so none is dropped here.
        if self.app != app {
            self.app = String::from(app);
            self.waiting_save = None;
        }
        self.ends_at = ends_at;
    }

    /// Ends the hold on `app`: a save that waited is dropped when the phone wrote, or else handed back to write.
    pub fn release(&mut self, app: &str, written: bool) {
        if self.app == app {
            self.ends_at = 0;
            if written {
                self.waiting_save = None;
            }
        }
    }

    /// Ends any hold without a write from the phone, such as when the link drops.
    pub fn release_all(&mut self) {
        self.ends_at = 0;
    }

    /// The held app at `now`, if its hold has neither ended nor run out.
    pub fn held(&self, now: u64) -> Option<&str> {
        (now < self.ends_at).then_some(self.app.as_str())
    }

    /// When the hold runs out, if it has not ended.
    pub fn ends_at(&self) -> Option<u64> {
        (self.ends_at != 0).then_some(self.ends_at)
    }

    /// Keeps a save of `app` while it is held, and true when it did; otherwise the caller writes it.
    pub fn keep_save(&mut self, app: &str, save: &[u8], now: u64) -> bool {
        let kept = self.held(now) == Some(app);
        if kept {
            self.waiting_save = Some(Vec::from(save));
        }
        kept
    }

    /// Once the hold has ended or run out by `now`, the save it handed back, for its app, which the caller writes.
    pub fn take_released_save(&mut self, now: u64) -> Option<(&str, Vec<u8>)> {
        if self.held(now).is_some() {
            return None;
        }
        self.ends_at = 0;
        let save = self.waiting_save.take()?;
        Some((self.app.as_str(), save))
    }
}

#[cfg(test)]
#[path = "closed_apps_tests.rs"]
mod closed_apps_tests;
