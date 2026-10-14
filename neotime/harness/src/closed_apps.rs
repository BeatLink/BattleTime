//! The apps the test watch keeps closed, as the watch's shell does.
//!
//! Each installed app with subscriptions that run while it is closed, and each app the test leaves,
//! is kept as its image and saved state, with an entry in `ClosedApps`. When one comes due it is
//! loaded without a screen and run, and one that wakes the screen is opened in front.

use neotime_app_host::closed_apps::run_while_closed;
use neotime_app_host::AppHost;
use neotime_runtime::{AppImage, AppKind, SystemValue, SystemValues, WidgetTree, DEFAULT_THEME};

use super::{TestWatch, MOST_WAKEUPS_PER_ADVANCE};
use crate::sources::{compile_package, package_source, shared_library, OpenFailure, PackageKind};

// ============================================================================
// Moving between apps
// ============================================================================

impl TestWatch {
    /// Leaves the open app or face, which stays closed with its state, and opens the app `id`, as the launcher would.
    pub fn switch_to_app(&mut self, id: &str) -> Result<(), OpenFailure> {
        let image = match self.images.get(id) {
            Some(image) => *image,
            None => {
                let is_app = package_source(PackageKind::App, id).is_file();
                let kind = if is_app {
                    PackageKind::App
                } else {
                    PackageKind::Shell
                };
                Box::leak(compile_package(kind, id)?.into_boxed_slice())
            }
        };
        self.switch_to(image)
    }

    /// Leaves the open app, which stays closed with its state, and opens the face `id`, as the side button would.
    pub fn switch_to_face(&mut self, id: &str) -> Result<(), OpenFailure> {
        let image = match self.images.get(id) {
            Some(image) => *image,
            None => Box::leak(compile_package(PackageKind::Face, id)?.into_boxed_slice()),
        };
        self.switch_to(image)
    }

    /// The open package as reference pictures name it, such as `app-stopwatch` or `face-digital`.
    pub fn package_name(&self) -> String {
        let id = self.id();
        let kind = if self.host.image().kind == AppKind::Face {
            "face"
        } else if !package_source(PackageKind::App, id).is_file()
            && package_source(PackageKind::Shell, id).is_file()
        {
            "shell"
        } else {
            "app"
        };
        format!("{kind}-{id}")
    }

    /// The closed apps waiting for a subscription to come due, in the order they were left.
    pub fn closed_app_ids(&self) -> Vec<String> {
        self.closed_apps.ids().map(String::from).collect()
    }

    /// Closes the open app, keeping its state, and opens the image with the state it saved when it last closed.
    fn switch_to(&mut self, image: &'static [u8]) -> Result<(), OpenFailure> {
        self.leave_open_app();
        let parsed = AppImage::parse(image).map_err(|error| OpenFailure::CannotOpen(error.into()))?;
        let id = parsed.string(parsed.id_string).unwrap_or_default().to_string();
        self.images.insert(id.clone(), image);
        let saved = self.saved_states.get(&id).cloned().unwrap_or_default();
        let library = shared_library()?;
        let now = self.now();
        let mut host = AppHost::open_with_saved_state(image, DEFAULT_THEME, &saved, now, &mut self.system)
            .map_err(OpenFailure::CannotOpen)?;
        host.set_library(library, now, &mut self.system)
            .map_err(OpenFailure::CannotOpen)?;
        host.set_installed_app_icons(self.installed_icons);
        self.system.app_name = parsed.string(parsed.name_string).unwrap_or_default().to_string();
        self.widgets = WidgetTree::new(*host.image());
        self.host = host;
        self.open_image = image;
        // An open app runs its own subscriptions, and is noted again when it closes.
        self.closed_apps.forget(&id);
        self.report_next_alarm();
        self.report_notification_changes();
        Ok(())
    }

    /// Closes the open app, saves its state and notes when it next has something to run.
    fn leave_open_app(&mut self) {
        let now = self.now();
        let id = self.id().to_string();
        self.host.close(now, &mut self.system);
        self.images.insert(id.clone(), self.open_image);
        save_state(&mut self.saved_states, &id, &mut self.host);
        self.closed_apps.record(&id, &self.host, now);
    }
}

/// Keeps what the app's state changed to, as the shell writes `/apps/<id>/state`.
fn save_state(saved_states: &mut std::collections::HashMap<String, Vec<u8>>, id: &str, host: &mut AppHost) {
    let mut buffer = vec![0; neotime_app_host::LARGEST_SAVED_STATE_BYTES];
    if let Ok(Some(length)) = host.save_changed_state(&mut buffer) {
        buffer.truncate(length);
        saved_states.insert(id.to_string(), buffer);
    }
}

// ============================================================================
// Running closed apps
// ============================================================================

impl TestWatch {
    /// Keeps an installed app's image, and notes it as a closed app if anything of it runs while closed.
    pub(super) fn keep_installed_app(&mut self, id: &str, image: &'static [u8]) {
        self.images.insert(id.to_string(), image);
        let runs_while_closed = AppImage::parse(image).is_ok_and(|parsed| {
            (0..parsed.subscription_count())
                .filter_map(|index| parsed.subscription(index))
                .any(|subscription| subscription.runs_while_closed())
        });
        if !runs_while_closed || id == self.id() {
            return;
        }
        if let Some(host) = self.load_closed(id) {
            self.closed_apps.record(id, &host, self.now());
        }
        self.report_next_alarm();
    }

    /// Runs each closed app that has come due, and opens one that wakes the screen.
    pub(super) fn run_due_closed_apps(&mut self) {
        let now = self.now();
        let mut runs = 0;
        while let Some(id) = self.closed_apps.due(now, self.id()).map(String::from) {
            runs += 1;
            assert!(runs <= MOST_WAKEUPS_PER_ADVANCE, "{id}: came due again at once");
            let (Some(image), Some(mut host)) = (self.images.get(&id).copied(), self.load_closed(&id)) else {
                self.closed_apps.forget(&id);
                continue;
            };
            let (beats_per_minute, confidence) = self.latest_heart_rate;
            host.remember_heart_rate(beats_per_minute, confidence);
            let checked_at = self.closed_apps.checked_at(&id).unwrap_or(now);
            let wake = run_while_closed(&mut host, checked_at, now, &mut self.system);
            save_state(&mut self.saved_states, &id, &mut host);
            self.closed_apps.record(&id, &host, now);
            drop(host);
            self.report_next_alarm();
            self.report_notification_changes();
            let Some(wake) = wake else {
                continue;
            };
            self.switch_to(image)
                .unwrap_or_else(|failure| panic!("{id} woke but would not open: {failure}"));
            if let Some(screen) = wake.screen {
                self.host.start_on_screen(&screen, now, &mut self.system);
            }
            if !self.is_screen_on {
                self.screen_on();
            }
        }
    }

    /// Loads a closed app without a screen, from its image and the state it last saved.
    fn load_closed(&mut self, id: &str) -> Option<AppHost<'static>> {
        let image = *self.images.get(id)?;
        let saved = self.saved_states.get(id).cloned().unwrap_or_default();
        let now = self.now();
        let mut host =
            AppHost::open_in_background(image, DEFAULT_THEME, &saved, now, &mut self.system).ok()?;
        host.set_library(shared_library().ok()?, now, &mut self.system)
            .ok()?;
        Some(host)
    }

    /// Sets `next_alarm` from the closed alarm app, and tells the open app when it changed.
    fn report_next_alarm(&mut self) {
        let next_alarm = self.closed_apps.next_alarm(self.id());
        if self.system.read(SystemValue::NextAlarm) == next_alarm {
            return;
        }
        self.system.set(SystemValue::NextAlarm, next_alarm);
        let now = self.now();
        self.host
            .system_values_changed(SystemValue::NextAlarm.bit(), now, &mut self.system);
    }
}
