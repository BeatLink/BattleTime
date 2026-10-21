// The SOS job: when the watch's sos app asks, the phone shows its SOS screen, records, texts the emergency contacts where it is, and calls for help.
//
// The watch sends {"t":"app","ch":"sos","d":{"on":1}} to start and {"on":0} to stop. Android lets no app
// call an emergency number itself, so that choice only opens the dialer with the number typed in. The
// contacts are called in turn, and since the phone cannot tell whether an outgoing call was answered, a
// call that ends sooner than `ANSWERED_AFTER` counts as unanswered and the next contact is called. It
// reports to the window under the feature "sosStatus".

use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{broadcast, watch};
use tokio::time::Instant;

use super::device::{CallState, DeviceKind};
use super::Daemon;

// ============================================================================
// Timing and limits
// ============================================================================

pub const SOS_CHANNEL: &str = "sos";

// A call to a contact that lasts this long counts as answered, and ends the calling.
pub const ANSWERED_AFTER: Duration = Duration::from_secs(30);

// How long the phone may take to start dialling before the call counts as not placed.
pub const DIAL_TIMEOUT: Duration = Duration::from_secs(20);

// How many steps the window's log keeps, newest first.
pub const SOS_LOG_LIMIT: usize = 30;

// ============================================================================
// The settings it reads
// ============================================================================

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SosContact {
    pub name: String,
    pub number: String,
}

impl SosContact {
    fn label(&self) -> String {
        let name = self.name.trim();
        if name.is_empty() {
            self.number.trim().to_string()
        } else {
            name.to_string()
        }
    }
}

// Whom the SOS calls: the contacts in turn, or the emergency number through the dialer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SosCall {
    #[default]
    Contacts,
    Emergency,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SosSettings {
    pub sos_contacts: Vec<SosContact>,
    pub sos_call: SosCall,
    pub sos_emergency_number: String,
    pub sos_text_location: bool,
    pub sos_record: bool,
}

impl Default for SosSettings {
    fn default() -> Self {
        SosSettings {
            sos_contacts: Vec::new(),
            sos_call: SosCall::Contacts,
            sos_emergency_number: "911".into(),
            sos_text_location: true,
            sos_record: true,
        }
    }
}

impl SosSettings {
    // The contacts that have a number, in the order they were set up.
    pub fn contacts(&self) -> Vec<SosContact> {
        self.sos_contacts
            .iter()
            .filter(|contact| !contact.number.trim().is_empty())
            .cloned()
            .collect()
    }
}

// ============================================================================
// What the window shows
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SosLogEntry {
    pub at: i64,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SosStatus {
    pub active: bool,
    pub started_at: Option<i64>,
    pub log: Vec<SosLogEntry>,
}

fn milliseconds_now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

// ============================================================================
// Reading the watch
// ============================================================================

// Whether an sos line asks to start (Some(true)) or stop (Some(false)); None for any other line.
pub fn sos_request(message: &Value) -> Option<bool> {
    if message["t"] != "app" || message["ch"] != SOS_CHANNEL {
        return None;
    }
    message["d"]["on"].as_i64().map(|on| on != 0)
}

// The text each contact is sent, with a map link when the phone knows where it is.
pub fn sos_text(location: Option<(f64, f64)>) -> String {
    let headline = "SOS: I need help. This was sent automatically from my watch.";
    match location {
        Some((latitude, longitude)) => {
            format!("{headline} I am here: https://maps.google.com/?q={latitude:.5},{longitude:.5}")
        }
        None => format!("{headline} My phone could not find where I am."),
    }
}

// ============================================================================
// The job
// ============================================================================

pub struct Sos {
    daemon: Weak<Daemon>,
    status: Mutex<SosStatus>,
    // Counts up on every start and stop, so the steps of an SOS that was stopped give up.
    run: watch::Sender<u64>,
    calls: broadcast::Sender<CallState>,
}

impl Sos {
    // Starts hearing the watch's sos lines on the daemon's runtime.
    pub fn start(daemon: &Arc<Daemon>) -> Arc<Sos> {
        let sos = Arc::new(Sos {
            daemon: Arc::downgrade(daemon),
            status: Mutex::new(SosStatus::default()),
            run: watch::channel(0).0,
            calls: broadcast::channel(16).0,
        });
        sos.publish(daemon);
        daemon
            .runtime()
            .spawn(sos.clone().hear_watch(daemon.watch_messages()));
        sos
    }

    // The phone's call state, which the daemon hands over as the phone reports it.
    pub fn on_call(&self, state: CallState) {
        let _ = self.calls.send(state);
    }

    // ========================================================================
    // Starting and stopping
    // ========================================================================

    pub fn begin(self: &Arc<Self>) {
        let Some(daemon) = self.daemon.upgrade() else {
            return;
        };
        if self.status.lock().unwrap().active {
            self.add_log(&daemon, true, "The watch asked again; the SOS is already on.");
            return;
        }
        if daemon.device().kind() != DeviceKind::Phone {
            self.add_log(&daemon, false, "SOS needs the companion on an Android phone.");
            return;
        }
        let run = self.next_run();
        {
            let mut status = self.status.lock().unwrap();
            status.active = true;
            status.started_at = Some(milliseconds_now());
            status.log.clear();
        }
        self.add_log(&daemon, true, "The watch sent an SOS.");

        let settings: SosSettings = daemon.settings().view();
        daemon
            .runtime()
            .spawn(self.clone().show_screen(settings.sos_record));
        if settings.sos_text_location {
            daemon
                .runtime()
                .spawn(self.clone().send_texts(run, settings.contacts()));
        }
        match settings.sos_call {
            SosCall::Emergency => {
                daemon.runtime().spawn(
                    self.clone()
                        .dial(settings.sos_emergency_number.trim().to_string()),
                );
            }
            SosCall::Contacts => {
                daemon
                    .runtime()
                    .spawn(self.clone().call_in_turn(run, settings.contacts()));
            }
        }
    }

    pub fn end(self: &Arc<Self>) {
        let Some(daemon) = self.daemon.upgrade() else {
            return;
        };
        if !self.status.lock().unwrap().active {
            return;
        }
        self.next_run();
        self.status.lock().unwrap().active = false;
        self.add_log(&daemon, true, "The SOS was stopped.");
        let closing = daemon.device().sos_screen(false, false);
        let sos = self.clone();
        daemon.runtime().spawn(async move {
            if let Err(error) = closing.await {
                sos.report(false, format!("The phone could not stop recording: {error}"));
            }
        });
    }

    fn next_run(&self) -> u64 {
        self.run.send_modify(|run| *run += 1);
        *self.run.borrow()
    }

    fn is_current(&self, run: u64) -> bool {
        *self.run.borrow() == run
    }

    async fn hear_watch(self: Arc<Self>, mut messages: broadcast::Receiver<Value>) {
        loop {
            let message = match messages.recv().await {
                Ok(message) => message,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return,
            };
            match sos_request(&message) {
                Some(true) => self.begin(),
                Some(false) => self.end(),
                None => {}
            }
        }
    }

    // ========================================================================
    // The steps
    // ========================================================================

    async fn show_screen(self: Arc<Self>, record: bool) {
        let Some(daemon) = self.daemon.upgrade() else {
            return;
        };
        let showing = daemon.device().sos_screen(true, record);
        drop(daemon);
        match showing.await {
            Ok(()) if record => self.report(true, "The phone shows the SOS screen and records."),
            Ok(()) => self.report(true, "The phone shows the SOS screen."),
            Err(error) => self.report(false, format!("The phone could not show the SOS screen: {error}")),
        }
    }

    async fn send_texts(self: Arc<Self>, run: u64, contacts: Vec<SosContact>) {
        let Some(daemon) = self.daemon.upgrade() else {
            return;
        };
        if contacts.is_empty() {
            return;
        }
        let locating = daemon.device().locate();
        drop(daemon);
        let location = match locating.await {
            Ok(location) => Some((location.latitude, location.longitude)),
            Err(error) => {
                self.report(false, format!("The phone could not find where it is: {error}"));
                None
            }
        };
        let text = sos_text(location);
        for contact in contacts {
            if !self.is_current(run) {
                return;
            }
            let Some(daemon) = self.daemon.upgrade() else {
                return;
            };
            let sending = daemon.device().send_text(contact.number.trim(), &text);
            drop(daemon);
            match sending.await {
                Ok(()) => self.report(true, format!("Texted {}.", contact.label())),
                Err(error) => self.report(false, format!("Could not text {}: {error}", contact.label())),
            }
        }
    }

    async fn dial(self: Arc<Self>, number: String) {
        let Some(daemon) = self.daemon.upgrade() else {
            return;
        };
        if number.is_empty() {
            self.report(false, "No emergency number is set.");
            return;
        }
        let dialing = daemon.device().dial_number(&number);
        drop(daemon);
        match dialing.await {
            Ok(()) => self.report(
                true,
                format!("Opened the dialer with {number}; tap Call on the phone."),
            ),
            Err(error) => self.report(false, format!("Could not open the dialer: {error}")),
        }
    }

    // Calls each contact until a call lasts long enough to have been answered, once through the list.
    async fn call_in_turn(self: Arc<Self>, run: u64, contacts: Vec<SosContact>) {
        if contacts.is_empty() {
            self.report(false, "No emergency contacts are set, so nobody was called.");
            return;
        }
        let mut stopped = self.run.subscribe();
        for contact in contacts {
            let mut calls = self.calls.subscribe();
            if !self.is_current(run) {
                return;
            }
            let Some(daemon) = self.daemon.upgrade() else {
                return;
            };
            let calling = daemon.device().call_number(contact.number.trim());
            drop(daemon);
            if let Err(error) = calling.await {
                self.report(false, format!("Could not call {}: {error}", contact.label()));
                continue;
            }
            self.report(true, format!("Calling {}.", contact.label()));

            let dialled = tokio::select! {
                state = wait_for(&mut calls, CallState::Outgoing) => state,
                _ = tokio::time::sleep(DIAL_TIMEOUT) => false,
                _ = stopped.changed() => return,
            };
            if !dialled {
                self.report(false, format!("The call to {} never started.", contact.label()));
                continue;
            }
            let began = Instant::now();
            tokio::select! {
                _ = wait_for(&mut calls, CallState::End) => {},
                _ = stopped.changed() => return,
            }
            if began.elapsed() >= ANSWERED_AFTER {
                self.report(
                    true,
                    format!("The call to {} ended after it was answered.", contact.label()),
                );
                return;
            }
            self.report(false, format!("{} did not answer.", contact.label()));
        }
        if self.is_current(run) {
            self.report(false, "Nobody answered.");
        }
    }

    // ========================================================================
    // The window's status
    // ========================================================================

    fn report(&self, ok: bool, message: impl Into<String>) {
        if let Some(daemon) = self.daemon.upgrade() {
            self.add_log(&daemon, ok, message);
        }
    }

    fn add_log(&self, daemon: &Daemon, ok: bool, message: impl Into<String>) {
        {
            let mut status = self.status.lock().unwrap();
            status.log.insert(
                0,
                SosLogEntry {
                    at: milliseconds_now(),
                    ok,
                    message: message.into(),
                },
            );
            status.log.truncate(SOS_LOG_LIMIT);
        }
        self.publish(daemon);
    }

    fn publish(&self, daemon: &Daemon) {
        let status = self.status.lock().unwrap().clone();
        daemon.set_feature(
            "sosStatus",
            serde_json::to_value(status).unwrap_or_else(|_| json!({})),
        );
    }
}

// Waits until the phone reports `wanted`, and says whether it did before the reports ran out.
async fn wait_for(calls: &mut broadcast::Receiver<CallState>, wanted: CallState) -> bool {
    loop {
        match calls.recv().await {
            Ok(state) if state == wanted => return true,
            Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => return false,
        }
    }
}

#[cfg(test)]
mod tests;
