//! The shell's closed apps: alarms, timers and other subscriptions that go on while their app is closed.
//!
//! `battletime_app_host::closed_apps` keeps a few bytes for each closed app with something to run. The
//! shell fills it when the apps are read, by loading each app whose image has such a subscription,
//! and again whenever an app closes. When one comes due the shell loads it without a screen, runs
//! what is due and saves its state. When the heap lacks room beside the open app, the open app is
//! closed first and opened again afterwards. A run waits until no other task is in the store, so it
//! loads the state as last written, a write from the phone included. While the phone holds an app's
//! state to rewrite it, that app neither runs nor saves: a run that comes due waits for the hold to
//! end, and a save waits with the hold, dropped if the phone writes and written if it does not. A handler that calls
//! `wake_screen` opens its app over what is open, on the screen its `go` named, and lights the
//! screen, as InfiniTime opens its alarm.
//!
//! A message from the phone for an app goes to it at once when it is open. For a closed app it waits
//! in one slot, a newer message for any app replacing it, until the store is free; then the app is
//! loaded and its message handlers run as a due timer's would, with the same memory check and the
//! same hold. A message the shell cannot run then, for want of room or because the phone holds
//! the app's state, or for an app that is missing or hears no messages while closed, is dropped.

extern crate alloc;

use alloc::{boxed::Box, string::String, vec};

use battletime_app_host::closed_apps::{
    deliver_message_while_closed, hears_messages_while_closed, run_while_closed, RunsClosedApps, WakeRequest,
};
use battletime_app_host::{AppHost, Transition};
use battletime_companion::AppMessage;
use battletime_runtime::{memory_need::closed_run_bytes, AppImage, SystemValue, Value};
use embassy_time::Instant;

use super::{app_name_of, state_path, Shell, Showing};
use crate::{app_events, apps, apps::Lent, clock, heart_rate};

// ============================================================================
// Limits
// ============================================================================

/// Heap kept free beyond a closed app's need, for the store's calls and the allocator's gaps.
const ROOM_TO_SPARE_BYTES: usize = 2048;

impl RunsClosedApps for super::WatchSystem {
    fn closed_run(&mut self) -> &mut Option<battletime_app_host::closed_apps::ClosedRun> {
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
        let now = clock::utc_now();
        // One that came due while held runs next on the new state, counting its timers from before the hold.
        if self.system.closed_apps.is_due(id, now) {
            return;
        }
        if self.system.installed.place_of(id).is_none() || !self.files.runs_while_closed_now(id) {
            return;
        }
        let mut slot = None;
        if let Some(image) = self.load_closed_app(id, now, &mut slot) {
            if let Some(host) = &slot {
                self.system.closed_apps.record(id, host, now);
            }
            drop(slot);
            image.release();
        }
    }

    /// When the next closed app comes due, as the shell's clock counts, or the phone's hold runs out.
    #[inline(never)]
    pub(super) fn next_closed_app_due(&self) -> Instant {
        let now = Instant::now().as_ticks();
        let (due, hold_ends) = app_events::with_state_hold(|hold| {
            let held = hold.held(now).unwrap_or_default();
            let due = self
                .system
                .closed_apps
                .next_due(&[&self.system.open_app_id, held]);
            (due, hold.ends_at())
        });
        let due = due.map_or(Instant::MAX, clock::instant_at);
        due.min(hold_ends.map_or(Instant::MAX, Instant::from_ticks))
    }

    /// Writes an app's state, or keeps it with the hold while the phone is rewriting it.
    pub(super) fn write_state(&self, id: &str, bytes: &[u8]) {
        let now = Instant::now().as_ticks();
        if !app_events::with_state_hold(|hold| hold.keep_save(id, bytes, now)) {
            self.files.write_soon(&state_path(id), bytes);
        }
    }

    /// Writes the save a hold kept once the hold ended without the phone writing.
    #[inline(never)]
    pub(super) fn write_released_saves(&self) {
        let now = Instant::now().as_ticks();
        // The write happens outside the lock, which holds off interrupts.
        let released = app_events::with_state_hold(|hold| {
            let (id, bytes) = hold.take_released_save(now)?;
            Some((app_events::StateWritten::try_from(id).ok()?, bytes))
        });
        if let Some((id, bytes)) = released {
            self.files.write_soon(&state_path(&id), &bytes);
        }
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
        let ticks_now = Instant::now().as_ticks();
        while let Some(id) = app_events::with_state_hold(|hold| {
            let held = hold.held(ticks_now).unwrap_or_default();
            let due = self
                .system
                .closed_apps
                .due(now, &[&self.system.open_app_id, held]);
            due.map(String::from)
        }) {
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
        self.finish_closed_runs(closed_for_room && !woke, alarm_before, now);
        ran_any
    }

    /// Opens again what was closed for room, and tells the open app when the closed alarm's next ring moved.
    fn finish_closed_runs(&mut self, reopen: bool, alarm_before: Value, now: Value) {
        if reopen {
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
    }

    /// Closes the open app when the heap lacks room to run `id` beside it; true when it did.
    fn make_room_for(&mut self, id: &str, now: Value) -> bool {
        let need = self.system.closed_apps.run_bytes(id).unwrap_or(0) as usize + ROOM_TO_SPARE_BYTES;
        self.make_room(need, now)
    }

    /// Closes the open app when the heap has less than `need` bytes free; true when it did.
    fn make_room(&mut self, need: usize, now: Value) -> bool {
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
        let image = apps::read_image(self.files, battletime_packages::APPS_DIRECTORY, id)?;
        self.open_closed_image(id, image, now, slot)
    }

    /// Opens an image already read without a screen into `slot`, as `load_closed_app` does.
    fn open_closed_image(
        &mut self,
        id: &str,
        image: Lent<[u8]>,
        now: Value,
        slot: &mut Option<AppHost<'static>>,
    ) -> Option<Lent<[u8]>> {
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
        let size = battletime_runtime::memory_need::saved_state_bytes(host.image());
        if size == 0 {
            return;
        }
        let mut buffer = vec![0; size];
        if let Ok(Some(length)) = host.save_changed_state(&mut buffer) {
            self.write_state(id, &buffer[..length]);
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

// ============================================================================
// Messages from the phone
// ============================================================================

/// The heap a closed run of the image needs, or 0 when it hears no messages while closed.
// Out of line, so the parsed image is off the stack while the app runs.
#[inline(never)]
fn message_run_bytes(image: &[u8]) -> usize {
    match AppImage::parse(image) {
        Ok(parsed) if hears_messages_while_closed(&parsed) => closed_run_bytes(&parsed) as usize,
        _ => 0,
    }
}

/// The id of the app a host runs.
fn app_id_of<'host>(host: &'host AppHost<'static>) -> Option<&'host str> {
    host.image().string(host.image().id_string)
}

impl Shell {
    /// Hands a message from the phone to the open app it names, or keeps it to run that app while closed.
    pub(super) fn app_message_arrived(&mut self, message: Box<AppMessage>, now: Value) {
        let topic = message.topic.as_str();
        match self.host.as_mut().filter(|host| app_id_of(host) == Some(topic)) {
            Some(host) => {
                host.deliver_message(
                    &message.number_fields(),
                    message.text_field(),
                    now,
                    &mut self.system,
                );
            }
            None => self.waiting_app_message = Some(message),
        }
    }

    /// Runs the waiting message's closed app, closing the open app first if the heap needs it; true when anything ran.
    #[inline(never)]
    pub(super) fn run_waiting_app_message(&mut self) -> bool {
        let Some(message) = self.waiting_app_message.take() else {
            return false;
        };
        let now = clock::utc_now();
        let id = message.topic.as_str();
        // The app may have opened since the message came.
        if self.host.as_ref().is_some_and(|host| app_id_of(host) == Some(id)) {
            self.app_message_arrived(message, now);
            return true;
        }
        let ticks_now = Instant::now().as_ticks();
        let is_held = app_events::with_state_hold(|hold| hold.held(ticks_now) == Some(id));
        if is_held || self.system.installed.place_of(id).is_none() {
            return false;
        }
        let Some(image) = apps::read_image(self.files, battletime_packages::APPS_DIRECTORY, id) else {
            return false;
        };
        let run_bytes = message_run_bytes(image.get());
        if run_bytes == 0 {
            image.release();
            return false;
        }
        // The image is on the heap already, so only the rest of the run needs room.
        let need = run_bytes.saturating_sub(image.get().len()) + ROOM_TO_SPARE_BYTES;
        let alarm_before = self.system.closed_apps.next_alarm(&self.system.open_app_id);
        let closed_for_room = self.make_room(need, now);
        let wake = match crate::free_heap_bytes() >= need {
            true => self.run_message_while_closed(id, image, &message, now),
            false => {
                image.release();
                None
            }
        };
        if let Some(wake) = wake {
            if self.open_woken_app(id, wake, now) {
                self.finish_closed_runs(false, alarm_before, now);
                return true;
            }
        }
        self.finish_closed_runs(closed_for_room, alarm_before, now);
        true
    }

    /// Loads a closed app, runs its message handlers, saves its state and notes it again.
    #[inline(never)]
    fn run_message_while_closed(
        &mut self,
        id: &str,
        image: Lent<[u8]>,
        message: &AppMessage,
        now: Value,
    ) -> Option<WakeRequest> {
        let mut slot = None;
        let image = self.open_closed_image(id, image, now, &mut slot)?;
        let wake = match &mut slot {
            Some(host) => {
                // A notification the app makes shows its own name as the source.
                let open_name = core::mem::replace(&mut self.system.open_app_name, app_name_of(image.get()));
                let numbers = message.number_fields();
                let wake =
                    deliver_message_while_closed(host, &numbers, message.text_field(), now, &mut self.system);
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
}
