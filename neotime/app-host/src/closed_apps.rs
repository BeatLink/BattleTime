//! Apps that are closed but still have subscriptions to run, such as an alarm or a running timer.
//!
//! The embedder keeps one small entry for each such app: when its next subscription is due and the
//! heap a run needs, and nothing else of it in RAM. When an entry comes due, the embedder loads the
//! app without a screen (`AppHost::open_in_background`), `run_while_closed` fires what is due, and
//! the app is recorded again. A handler that calls `wake_screen` asks for its app to be opened and
//! the screen lit, on the screen its `go` named, as InfiniTime opens its alarm and timer. The alarm
//! app's next ring is `next_alarm`, the bell watch faces show.

use alloc::{string::String, vec::Vec};

use neotime_runtime::{memory_need::closed_run_bytes, EffectKind, Value, NONE};

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

    /// The closed apps other than the open one, which runs its own subscriptions.
    fn others<'table>(
        &'table self,
        open_id: &'table str,
    ) -> impl Iterator<Item = &'table ClosedApp> + 'table {
        self.apps.iter().filter(move |app| app.id != open_id)
    }

    /// When the next closed app comes due.
    pub fn next_due(&self, open_id: &str) -> Option<Value> {
        self.others(open_id).map(|app| app.next_due).min()
    }

    /// The closed app due soonest, if one is due by `now`.
    pub fn due(&self, now: Value, open_id: &str) -> Option<&str> {
        self.apps
            .iter()
            .filter(|app| app.id != open_id && app.next_due <= now)
            .min_by_key(|app| app.next_due)
            .map(|app| app.id.as_str())
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
        self.others(open_id)
            .find(|app| app.id == ALARM_APP_ID)
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
    /// brightness, going back, closing, opening another app and reading a file ahead are dropped. Anything else, such as a
    /// vibration, is left for the embedder to carry out.
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
            | EffectKind::PrefetchFile => {}
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
    let run = system.closed_run().take()?;
    run.wakes.then_some(WakeRequest { screen: run.screen })
}

#[cfg(test)]
#[path = "closed_apps_tests.rs"]
mod closed_apps_tests;
