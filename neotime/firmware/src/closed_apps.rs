//! The shell's closed apps: alarms, timers and other subscriptions that go on while their app is closed.
//!
//! `neotime_app_host::closed_apps` keeps a few bytes for each closed app with something to run. The
//! shell fills it when the apps are read, by loading each app whose image has such a subscription,
//! and again whenever an app closes. When one comes due the shell loads it without a screen, runs
//! what is due and saves its state. When the heap lacks room beside the open app, the open app is
//! closed first and opened again afterwards. A handler that calls `wake_screen` opens its app over
//! what is open, on the screen its `go` named, and lights the screen, as InfiniTime opens its alarm.

extern crate alloc;

use alloc::{string::String, vec};

use embassy_time::Instant;
use neotime_app_host::closed_apps::{run_while_closed, RunsClosedApps, WakeRequest};
use neotime_app_host::{AppHost, Transition};
use neotime_runtime::{SystemValue, Value};

use super::{app_name_of, state_path, Shell, Showing};
use crate::{apps, apps::Lent, clock, heart_rate};

// ============================================================================
// Limits
// ============================================================================

/// Heap kept free beyond a closed app's need, for the store's calls and the allocator's gaps.
const ROOM_TO_SPARE_BYTES: usize = 2048;

impl RunsClosedApps for super::WatchSystem {
    fn closed_run(&mut self) -> &mut Option<neotime_app_host::closed_apps::ClosedRun> {
        &mut self.closed_run
    }
}

// ============================================================================
// Keeping the table
// ============================================================================

impl Shell {
    /// Notes a closing app's next subscription, if it is a listed app with one.
    pub(super) fn note_closed_app(&mut self, host: &AppHost<'static>, now: Value) {
        if self.showing == Showing::Temporary {
            return;
        }
        let Some(id) = host.image().string(host.image().id_string) else {
            return;
        };
        if self.system.installed.place_of(id).is_some() {
            self.system.closed_apps.record(id, host, now);
        }
    }

    /// Loads each listed app that has subscriptions to run while closed, once, to note when each comes due.
    #[inline(never)]
    pub(super) fn find_closed_apps(&mut self) {
        self.system.closed_apps.clear();
        let now = clock::utc_now();
        for place in 0..self.system.installed.listed_count() {
            let Some(id) = self.system.installed.listed_id(place).map(String::from) else {
                continue;
            };
            if !self.files.runs_while_closed_now(&id) {
                continue;
            }
            let mut slot = None;
            if let Some(image) = self.load_closed_app(&id, now, &mut slot) {
                if let Some(host) = &slot {
                    self.system.closed_apps.record(&id, host, now);
                }
                drop(slot);
                image.release();
            }
        }
    }

    /// The phone rewrote an app's saved state: the open app opens again on it, unsaved, and a closed one's next run is noted again.
    #[inline(never)]
    pub(super) fn state_written(&mut self, id: &str) {
        if self.host.is_some() && self.system.open_app_id.as_str() == id && self.showing != Showing::Temporary
        {
            // Dropping the host skips its save, so the state that opens is the phone's.
            self.host = None;
            self.open(self.showing, Transition::None);
            return;
        }
        if self.system.installed.place_of(id).is_none() || !self.files.runs_while_closed_now(id) {
            return;
        }
        let now = clock::utc_now();
        let mut slot = None;
        if let Some(image) = self.load_closed_app(id, now, &mut slot) {
            if let Some(host) = &slot {
                self.system.closed_apps.record(id, host, now);
            }
            drop(slot);
            image.release();
        }
    }

    /// When the next closed app comes due, as the shell's clock counts.
    pub(super) fn next_closed_app_due(&self) -> Instant {
        self.system
            .closed_apps
            .next_due(&self.system.open_app_id)
            .map_or(Instant::MAX, clock::instant_at)
    }
}

// ============================================================================
// Running closed apps
// ============================================================================

impl Shell {
    /// Runs each closed app that has come due, and opens one that wakes the screen; true when any ran.
    #[inline(never)]
    pub(super) fn run_due_closed_apps(&mut self) -> bool {
        let now = clock::utc_now();
        let alarm_before = self.system.closed_apps.next_alarm(&self.system.open_app_id);
        let mut ran_any = false;
        let mut closed_for_room = false;
        let mut woke = false;
        while let Some(id) = self
            .system
            .closed_apps
            .due(now, &self.system.open_app_id)
            .map(String::from)
        {
            ran_any = true;
            if self.system.installed.place_of(&id).is_none() {
                self.system.closed_apps.forget(&id);
                continue;
            }
            closed_for_room |= self.make_room_for(&id, now);
            let Some(wake) = self.run_closed_app(&id, now) else {
                continue;
            };
            if self.open_woken_app(&id, wake, now) {
                woke = true;
                break;
            }
        }
        if closed_for_room && !woke {
            self.keeps_vibration_on_open = true;
            self.open(self.showing, Transition::None);
            self.keeps_vibration_on_open = false;
        }
        let alarm_after = self.system.closed_apps.next_alarm(&self.system.open_app_id);
        if alarm_after != alarm_before {
            if let Some(host) = &mut self.host {
                host.system_values_changed(SystemValue::NextAlarm.bit(), now, &mut self.system);
            }
        }
        ran_any
    }

    /// Closes the open app when the heap lacks room to run `id` beside it; true when it did.
    fn make_room_for(&mut self, id: &str, now: Value) -> bool {
        let need = self.system.closed_apps.run_bytes(id).unwrap_or(0) as usize + ROOM_TO_SPARE_BYTES;
        if self.host.is_none() || crate::free_heap_bytes() >= need {
            return false;
        }
        self.close_open_app(now);
        if let Some(image) = self.image.take() {
            image.release();
        }
        if let Some(icons) = self.launcher_icons.take() {
            icons.release();
        }
        // The app just closed has its own entry now, which must not be skipped as if it were open.
        self.system.open_app_id.clear();
        true
    }

    /// Loads one closed app, runs what is due, saves its state and notes it again.
    #[inline(never)]
    fn run_closed_app(&mut self, id: &str, now: Value) -> Option<WakeRequest> {
        let mut slot = None;
        let Some(image) = self.load_closed_app(id, now, &mut slot) else {
            self.system.closed_apps.forget(id);
            return None;
        };
        let wake = match &mut slot {
            Some(host) => {
                let (beats_per_minute, confidence) = heart_rate::latest_reading();
                host.remember_heart_rate(beats_per_minute, confidence);
                let checked_at = self.system.closed_apps.checked_at(id).unwrap_or(now);
                // A notification the app makes shows its own name as the source.
                let open_name = core::mem::replace(&mut self.system.open_app_name, app_name_of(image.get()));
                let wake = run_while_closed(host, checked_at, now, &mut self.system);
                self.system.open_app_name = open_name;
                self.save_closed_state(id, host);
                self.system.closed_apps.record(id, host, now);
                wake
            }
            None => None,
        };
        drop(slot);
        image.release();
        wake
    }

    /// Loads an app without a screen into `slot`, with its saved state and the shared library; the image it borrows is returned.
    #[inline(never)]
    fn load_closed_app(
        &mut self,
        id: &str,
        now: Value,
        slot: &mut Option<AppHost<'static>>,
    ) -> Option<Lent<[u8]>> {
        let image = apps::read_image(self.files, neotime_packages::APPS_DIRECTORY, id)?;
        let saved = self.files.read_now(&state_path(id)).unwrap_or_default();
        let opened =
            AppHost::open_in_background_into(slot, image.get(), self.theme, &saved, now, &mut self.system);
        if opened.is_err() {
            *slot = None;
        }
        drop(saved);
        let loaded = match (slot.as_mut(), self.library) {
            (Some(host), Some(library)) => host.set_library(library, now, &mut self.system).is_ok(),
            (Some(_), None) => true,
            (None, _) => false,
        };
        if !loaded {
            *slot = None;
            image.release();
            return None;
        }
        Some(image)
    }

    /// Writes a closed app's persistent state when its run changed any.
    fn save_closed_state(&self, id: &str, host: &mut AppHost<'static>) {
        let size = neotime_runtime::memory_need::saved_state_bytes(host.image());
        if size == 0 {
            return;
        }
        let mut buffer = vec![0; size];
        if let Ok(Some(length)) = host.save_changed_state(&mut buffer) {
            self.files.write_now(&state_path(id), &buffer[..length]);
        }
    }

    /// Opens an app that woke the screen over what is open, keeping its vibration; false while a firmware page is up.
    fn open_woken_app(&mut self, id: &str, wake: WakeRequest, now: Value) -> bool {
        // The crash and memory pages stay up until the wearer answers them.
        if self.firmware_page.is_some() {
            return false;
        }
        let Some(place) = self.system.installed.place_of(id) else {
            return false;
        };
        let transition = if self.lit {
            Transition::Up
        } else {
            Transition::None
        };
        self.keeps_vibration_on_open = true;
        self.go_forward(Showing::App(place), transition, false);
        self.keeps_vibration_on_open = false;
        if let (Some(screen), Some(host)) = (wake.screen, &mut self.host) {
            host.start_on_screen(&screen, now, &mut self.system);
        }
        self.system.screen_request = Some(true);
        true
    }
}
