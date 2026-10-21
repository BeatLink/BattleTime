// The companion's daemon, which keeps the watch linked and does the companion's background work with or without a window.
//
// It owns the Bluetooth link, the settings, the plugins and the sync. It reconnects to the remembered
// watch, sets its clock, reads its battery, forwards the device's notifications, calls and music, and
// answers the watch's requests. A host (the desktop app, or the Android service) gives it a `Device` and
// hears its events through a `HostSink`; docs/companion/daemon.md describes it.

pub mod clock;
pub mod device;
pub mod forward;
pub mod http_proxy;
pub mod keeper;
pub mod lease;
pub mod link;
pub mod notes;
pub mod remote;
pub mod replies;
pub mod settings;
#[cfg(unix)]
pub mod socket;
pub mod sos;
pub mod state;
pub mod weather;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, RwLock, Weak};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use serde_json::{json, Value};
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc, watch, Notify};

use crate::ble::{subscription_key, WatchBle};
use crate::gadgetbridge::{decode_watch_line, encode_for_watch, LineBuffer};
use crate::plugins::host::{HostOutput, PluginHost, WatchData};
use crate::plugins::http;
use crate::plugins::runtime::DataWrite;
use crate::plugins::store::PluginStore;
use crate::protocol::battletime::{
    read_battery_level, read_capabilities, set_watch_time, Capabilities, FILE_TRANSFER, FILE_TRANSFER_SERVICE,
};
use crate::protocol::link::WriteMode;
use crate::sync::files::{FileClient, FileTransport};
use crate::sync::store::{SyncSettings, SyncStore, LONGEST_INTERVAL, SHORTEST_INTERVAL};
use crate::sync::{SyncOutput, SyncReport, Syncer};
use clock::{ClockReading, ClockWatch, CLOCK_CHECK_INTERVAL};
use device::{CallCommand, Device, PhoneEvent};
use forward::{answer_state_request, Forwarder};
use keeper::{kept_watch, retry_delay, watch_to_try, KeeperHolds};
use lease::WindowLease;
use link::{Link, SharedLink};
use replies::{ReplyRouter, REPLY_TIMEOUT};
use settings::{CoreSettings, SettingsStore};
use state::{DaemonState, WatchRef};

// ============================================================================
// Timing
// ============================================================================

// How long a new link settles before the daemon reads what the watch serves.
const SETTLE_AFTER_CONNECTING: Duration = Duration::from_millis(500);

// How long after the clock is set the first status read waits.
const FIRST_STATUS_READ: Duration = Duration::from_millis(1500);

// The status interval when none is saved or the saved one is out of range.
const DEFAULT_STATUS_INTERVAL: Duration = Duration::from_secs(5 * 60);

// ============================================================================
// What a host provides and hears
// ============================================================================

// Receives every event for the window, named as tauri/src/bridge.js listens for them.
pub type HostSink = Arc<dyn Fn(&str, Value) + Send + Sync>;

// One event as every listener beside the window hears it: its name and its payload.
pub type HostEvent = (String, Value);

// How many events a slow listener may fall behind before it misses some.
const EVENT_BACKLOG: usize = 1024;

pub struct DaemonSetup {
    // Where the settings, plugins and synced records are kept.
    pub data_dir: PathBuf,
    pub device: Arc<dyn Device>,
    pub runtime: Handle,
}

// The link as the background jobs see it: which connection this is, and what the watch serves once read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Readiness {
    // Counts up on every connect and disconnect, so a job can tell its link has gone.
    pub epoch: u64,
    pub watch_id: Option<String>,
    // None until the watch has been asked, and while it is away.
    pub capabilities: Option<Capabilities>,
}

impl Readiness {
    pub fn uart(&self) -> bool {
        self.capabilities.is_some_and(|capabilities| capabilities.uart)
    }

    pub fn files(&self) -> bool {
        self.capabilities.is_some_and(|capabilities| capabilities.files)
    }
}

// ============================================================================
// The daemon
// ============================================================================

pub struct Daemon {
    weak: Weak<Daemon>,
    runtime: Handle,
    link: Arc<dyn Link>,
    ble: Option<WatchBle>,
    device: Arc<dyn Device>,
    host: RwLock<HostSink>,
    events: broadcast::Sender<HostEvent>,
    settings: SettingsStore,
    state: Mutex<DaemonState>,
    router: ReplyRouter,
    lines: Mutex<LineBuffer>,
    forwarder: Mutex<Forwarder>,
    holds: Mutex<KeeperHolds>,
    keeper_wake: Notify,
    readiness: watch::Sender<Readiness>,
    messages: broadcast::Sender<Value>,
    phone_events: mpsc::UnboundedSender<PhoneEvent>,
    phone_queue: Mutex<Option<mpsc::UnboundedReceiver<PhoneEvent>>>,
    plugins: Arc<PluginHost>,
    syncer: Arc<Syncer>,
    lease: WindowLease,
    // The watch the device was last asked to stay running for.
    kept_running_for: Mutex<Option<String>>,
    features: OnceLock<Features>,
}

// The background jobs beside the daemon's own, started with it on the real link.
pub struct Features {
    pub weather: Arc<weather::Weather>,
    pub http_proxy: Arc<http_proxy::HttpProxy>,
    pub notes: Arc<notes::NoteKeeper>,
    pub remote: Arc<remote::Remote>,
    pub sos: Arc<sos::Sos>,
}

impl Daemon {
    // Starts the daemon on the watch's Bluetooth link.
    pub fn start(setup: DaemonSetup) -> Arc<Daemon> {
        let daemon = Arc::new_cyclic(|weak: &Weak<Daemon>| {
            let events = weak.clone();
            let ble = WatchBle::new(Arc::new(move |name, payload| {
                if let Some(daemon) = events.upgrade() {
                    daemon.on_link_event(name, payload);
                }
            }));
            Daemon::build(setup, weak.clone(), Arc::new(ble.clone()), Some(ble))
        });
        daemon.run();
        let features = Features {
            weather: weather::Weather::start(&daemon),
            http_proxy: http_proxy::HttpProxy::start(&daemon),
            notes: notes::NoteKeeper::start(&daemon),
            remote: remote::Remote::start(&daemon),
            sos: sos::Sos::start(&daemon),
        };
        let _ = daemon.features.set(features);
        daemon
    }

    // Starts the daemon on any link, whose events the caller hands to `on_link_event`.
    pub fn start_with_link(setup: DaemonSetup, link: Arc<dyn Link>) -> Arc<Daemon> {
        let daemon = Arc::new_cyclic(|weak: &Weak<Daemon>| Daemon::build(setup, weak.clone(), link, None));
        daemon.run();
        daemon
    }

    fn build(setup: DaemonSetup, weak: Weak<Daemon>, link: Arc<dyn Link>, ble: Option<WatchBle>) -> Daemon {
        let output = Arc::new(DaemonOutput { daemon: weak.clone() });
        let plugins = Arc::new(PluginHost::new(
            PluginStore::new(setup.data_dir.join("plugins")),
            output.clone(),
            Arc::new(http::fetch),
        ));
        let files = Arc::new(FileClient::new(Arc::new(DaemonFiles { link: link.clone() })));
        let syncer = Arc::new(Syncer::new(
            files,
            SyncStore::new(setup.data_dir.join("sync")),
            output.clone(),
        ));
        plugins.set_watch_data(output);
        let (phone_events, phone_queue) = mpsc::unbounded_channel();
        Daemon {
            weak,
            runtime: setup.runtime,
            link,
            ble,
            device: setup.device,
            host: RwLock::new(Arc::new(|_, _| {})),
            events: broadcast::channel(EVENT_BACKLOG).0,
            settings: SettingsStore::open(setup.data_dir.join("companion").join("settings.json")),
            state: Mutex::new(DaemonState::default()),
            router: ReplyRouter::default(),
            lines: Mutex::new(LineBuffer::default()),
            forwarder: Mutex::new(Forwarder::default()),
            holds: Mutex::new(KeeperHolds::default()),
            keeper_wake: Notify::new(),
            readiness: watch::channel(Readiness::default()).0,
            messages: broadcast::channel(64).0,
            phone_events,
            phone_queue: Mutex::new(Some(phone_queue)),
            plugins,
            syncer,
            lease: WindowLease::default(),
            kept_running_for: Mutex::new(None),
            features: OnceLock::new(),
        }
    }

    fn run(self: &Arc<Self>) {
        self.share_sync_interval();
        self.runtime.spawn(self.syncer.clone().run());
        let plugins = self.plugins.clone();
        std::thread::spawn(move || plugins.start_all());
        self.runtime.spawn(self.clone().keep_watch());
        if let Some(queue) = self.phone_queue.lock().unwrap().take() {
            self.runtime.spawn(self.clone().forward_phone_events(queue));
        }
    }

    // ========================================================================
    // For hosts
    // ========================================================================

    // Sends the daemon's events to a new window, in place of whichever had them.
    pub fn set_host(&self, sink: HostSink) {
        *self.host.write().unwrap() = sink;
    }

    // The background jobs, which only a daemon on the real link runs.
    pub fn features(&self) -> Option<&Features> {
        self.features.get()
    }

    pub fn ble(&self) -> Option<&WatchBle> {
        self.ble.as_ref()
    }

    pub fn device(&self) -> &Arc<dyn Device> {
        &self.device
    }

    pub fn plugins(&self) -> &Arc<PluginHost> {
        &self.plugins
    }

    pub fn syncer(&self) -> &Arc<Syncer> {
        &self.syncer
    }

    pub fn lease(&self) -> &WindowLease {
        &self.lease
    }

    pub fn runtime(&self) -> &Handle {
        &self.runtime
    }

    pub fn shared_link(&self) -> SharedLink {
        SharedLink(self.link.clone())
    }

    pub fn state(&self) -> Value {
        serde_json::to_value(&*self.state.lock().unwrap()).unwrap_or_default()
    }

    pub fn readiness(&self) -> watch::Receiver<Readiness> {
        self.readiness.subscribe()
    }

    // Every decoded message from the watch that no request of the daemon's took.
    pub fn watch_messages(&self) -> broadcast::Receiver<Value> {
        self.messages.subscribe()
    }

    // Every event the window hears, for other listeners such as the command line's socket.
    pub fn events(&self) -> broadcast::Receiver<HostEvent> {
        self.events.subscribe()
    }

    pub fn emit(&self, event: &str, payload: Value) {
        if self.events.receiver_count() > 0 {
            let _ = self.events.send((event.to_string(), payload.clone()));
        }
        let sink = self.host.read().unwrap().clone();
        sink(event, payload);
    }

    // Takes the file service from the daemon's sync for one operation, and gives back the number that ends it.
    pub async fn begin_files_turn(&self) -> u64 {
        let guard = self.syncer.files().lease().await;
        let number = self.lease.hold(guard);
        let daemon = self.weak.clone();
        self.runtime.spawn(async move {
            tokio::time::sleep(lease::LONGEST_LEASE).await;
            if let Some(daemon) = daemon.upgrade() {
                daemon.lease.release(Some(number));
            }
        });
        number
    }

    // Lets go of the watch and the plugins, when the app quits.
    pub async fn shut_down(&self) {
        self.plugins.stop_all();
        if let Some(ble) = &self.ble {
            ble.destroy().await;
        }
    }

    // ========================================================================
    // Settings
    // ========================================================================

    pub fn settings(&self) -> &SettingsStore {
        &self.settings
    }

    pub fn settings_json(&self) -> Value {
        self.settings.all()
    }

    pub fn update_settings(&self, patch: &Value) -> Result<Value, String> {
        let all = self.settings.update(patch)?;
        self.emit("daemon:settings", all.clone());
        self.share_sync_interval();
        self.keeper_wake.notify_one();
        Ok(all)
    }

    // Takes the settings a window kept before there was a daemon, once, and says whether it did.
    pub fn import_settings(&self, all: &Value) -> Result<bool, String> {
        if self.settings.has_saved() {
            return Ok(false);
        }
        self.update_settings(all)?;
        Ok(true)
    }

    fn share_sync_interval(&self) {
        let Some(seconds) = self.settings.view::<CoreSettings>().sync_interval_seconds else {
            return;
        };
        let wanted = SyncSettings {
            interval_seconds: valid_interval(seconds).as_secs(),
        };
        if self.syncer.settings() != wanted {
            let _ = self.syncer.set_settings(&wanted);
        }
    }

    // ========================================================================
    // Connecting
    // ========================================================================

    // Connects to the watch the user chose, and remembers it once the link is up.
    pub async fn connect(&self, id: &str, name: &str) -> Result<(), String> {
        {
            let mut holds = self.holds.lock().unwrap();
            holds.user_disconnected = false;
            holds.scanning = false;
        }
        self.change(|state| {
            state.watch = Some(WatchRef {
                id: id.into(),
                name: name.into(),
            })
        });
        self.link.connect(id.to_string()).await?;
        self.update_settings(&json!({ "lastWatchId": id, "lastWatchName": name }))?;
        Ok(())
    }

    // Disconnects, and keeps the watch away until the user connects again.
    pub async fn disconnect(&self) -> Result<(), String> {
        self.holds.lock().unwrap().user_disconnected = true;
        self.keeper_wake.notify_one();
        self.refresh_keeping();
        self.link.disconnect().await
    }

    // Holds the keeper back while a firmware update moves the link itself.
    pub fn hold(&self, held: bool) {
        self.holds.lock().unwrap().held = held;
        self.keeper_wake.notify_one();
    }

    // Holds the keeper back while the window scans, since connecting stops a scan.
    pub fn set_scanning(&self, scanning: bool) {
        self.holds.lock().unwrap().scanning = scanning;
        self.keeper_wake.notify_one();
    }

    fn watch_to_try(&self) -> Option<String> {
        let disconnected = self.state.lock().unwrap().connection == "disconnected";
        watch_to_try(&self.settings.view(), &self.holds.lock().unwrap(), disconnected)
    }

    // Runs for the daemon's life, bringing the remembered watch back whenever it is away.
    async fn keep_watch(self: Arc<Self>) {
        let mut failures = 0;
        loop {
            self.refresh_keeping();
            let Some(address) = self.watch_to_try() else {
                self.keeper_wake.notified().await;
                continue;
            };
            tokio::select! {
                _ = tokio::time::sleep(retry_delay(failures)) => {}
                _ = self.keeper_wake.notified() => continue,
            }
            if self.watch_to_try().as_deref() != Some(address.as_str()) {
                continue;
            }
            let name = self
                .settings
                .view::<CoreSettings>()
                .last_watch_name
                .unwrap_or_default();
            self.change(|state| {
                state.watch = Some(WatchRef {
                    id: address.clone(),
                    name,
                })
            });
            match self.link.connect(address).await {
                Ok(()) => failures = 0,
                Err(_) => failures += 1,
            }
        }
    }

    // The watch the device should stay running for, worked out now, for a host that starts before the daemon's own check.
    pub fn kept_running_for(&self) -> Option<String> {
        self.refresh_keeping();
        self.kept_running_for.lock().unwrap().clone()
    }

    // Says whether the daemon is reconnecting, and asks the device to keep running while it keeps a watch.
    fn refresh_keeping(&self) {
        let settings: CoreSettings = self.settings.view();
        let kept = kept_watch(&settings, &self.holds.lock().unwrap());
        let (connected, connected_name) = {
            let state = self.state.lock().unwrap();
            (
                state.connection == "connected",
                state.watch.as_ref().map(|watch| watch.name.clone()),
            )
        };
        let reconnecting = kept.is_some() && !connected;
        self.change(|state| state.reconnecting = reconnecting);

        let running_for = if connected {
            connected_name.or(settings.last_watch_name.clone())
        } else {
            kept.map(|_| settings.last_watch_name.clone().unwrap_or_default())
        };
        let mut last = self.kept_running_for.lock().unwrap();
        if *last != running_for {
            let name = running_for
                .as_deref()
                .map(|name| if name.is_empty() { "your watch" } else { name });
            self.device.keep_running(name);
            *last = running_for;
        }
    }

    // ========================================================================
    // The link's events
    // ========================================================================

    // Every event from the Bluetooth layer comes through here before the window hears it.
    pub fn on_link_event(&self, name: &str, payload: Value) {
        match (name, payload.as_str()) {
            ("state", Some("connecting")) => self.change(|state| state.connection = "connecting".into()),
            ("state", Some("connected")) => self.on_connected(),
            ("state", Some("disconnected")) => self.on_disconnected(),
            ("mode", Some(mode)) => self.change(|state| state.mode = mode.into()),
            ("line", Some(text)) => self.on_watch_text(text),
            ("notify", _) if payload["key"] == file_service_key() && !self.lease.held() => {
                let bytes: Vec<u8> = serde_json::from_value(payload["value"].clone()).unwrap_or_default();
                self.syncer.files().on_answer(bytes);
                return;
            }
            _ => {}
        }
        self.emit("ble:event", json!({ "name": name, "payload": payload }));
    }

    fn on_connected(&self) {
        let (mode, watch_id) = {
            let mut state = self.state.lock().unwrap();
            state.connection = "connected".into();
            (
                state.mode.clone(),
                state.watch.as_ref().map(|watch| watch.id.clone()),
            )
        };
        self.publish();
        let epoch = self.next_epoch(watch_id);
        self.plugins.on_connection(true);
        self.syncer.on_connection(true);
        self.refresh_keeping();
        if mode == "application" {
            if let Some(daemon) = self.weak.upgrade() {
                self.runtime.spawn(daemon.run_session(epoch));
            }
        }
    }

    fn on_disconnected(&self) {
        self.change(|state| {
            state.connection = "disconnected".into();
            state.mode = "application".into();
            state.capabilities = Capabilities::default();
            state.status = None;
            state.battery_level = None;
        });
        self.next_epoch(None);
        self.router.reset();
        self.lines.lock().unwrap().reset();
        self.plugins.on_connection(false);
        self.syncer.on_connection(false);
        self.keeper_wake.notify_one();
        self.refresh_keeping();
    }

    fn next_epoch(&self, watch_id: Option<String>) -> u64 {
        let mut epoch = 0;
        self.readiness.send_modify(|readiness| {
            readiness.epoch += 1;
            readiness.watch_id = watch_id;
            readiness.capabilities = None;
            epoch = readiness.epoch;
        });
        epoch
    }

    fn still(&self, epoch: u64) -> bool {
        self.readiness.borrow().epoch == epoch
    }

    // ========================================================================
    // A connected watch
    // ========================================================================

    // Finds out what the watch serves, sets its clock, then keeps its status and clock up to date until it goes.
    async fn run_session(self: Arc<Self>, epoch: u64) {
        tokio::time::sleep(SETTLE_AFTER_CONNECTING).await;
        if !self.still(epoch) {
            return;
        }
        let link = self.shared_link();
        let capabilities = read_capabilities(&link);
        self.change(|state| state.capabilities = capabilities);
        self.readiness.send_modify(|readiness| {
            if readiness.epoch == epoch {
                readiness.capabilities = Some(capabilities);
            }
        });
        if capabilities.time {
            self.set_clock().await;
        }
        self.runtime.spawn(self.clone().follow_clock(epoch, capabilities));
        if capabilities.uart || capabilities.battery {
            self.runtime.spawn(self.clone().read_status_regularly(epoch));
        }
    }

    async fn set_clock(&self) {
        if let Err(error) = set_watch_time(&self.shared_link(), &chrono::Local::now()).await {
            self.change(|state| state.last_error = Some(error));
        }
    }

    async fn follow_clock(self: Arc<Self>, epoch: u64, capabilities: Capabilities) {
        let started = Instant::now();
        let mut clock = ClockWatch::new(ClockReading::now(started));
        loop {
            tokio::time::sleep(CLOCK_CHECK_INTERVAL).await;
            if !self.still(epoch) {
                return;
            }
            if clock.check(ClockReading::now(started)) && capabilities.time {
                self.set_clock().await;
            }
        }
    }

    async fn read_status_regularly(self: Arc<Self>, epoch: u64) {
        tokio::time::sleep(FIRST_STATUS_READ).await;
        while self.still(epoch) {
            let _ = self.refresh_status().await;
            let seconds = self.settings.view::<CoreSettings>().sync_interval_seconds;
            tokio::time::sleep(seconds.map_or(DEFAULT_STATUS_INTERVAL, valid_interval)).await;
        }
    }

    // Reads the battery and steps over the UART, or the Battery service's level on a watch without it.
    pub async fn refresh_status(&self) -> Result<(), String> {
        let capabilities = self
            .readiness
            .borrow()
            .capabilities
            .ok_or("Connect to a watch first.")?;
        if capabilities.uart {
            let reply = self.request(&json!({ "t": "status" }), "status").await?;
            self.change(|state| {
                state.status = Some(reply);
                state.last_synced = Some(chrono::Utc::now().timestamp_millis());
            });
        } else if capabilities.battery {
            let level = read_battery_level(&self.shared_link()).await?;
            self.change(|state| {
                state.battery_level = Some(level);
                state.last_synced = Some(chrono::Utc::now().timestamp_millis());
            });
        }
        Ok(())
    }

    // ========================================================================
    // Talking to the watch
    // ========================================================================

    // Sends one message as a GB line; the window's console hears it too.
    pub async fn send(&self, message: &Value) -> Result<(), String> {
        let line = encode_for_watch(message);
        self.emit("daemon:sent", json!(line.trim_end()));
        self.link.write_text(line).await
    }

    // Sends a message and waits for the watch's answer of `reply_type`.
    pub async fn request(&self, message: &Value, reply_type: &str) -> Result<Value, String> {
        let pending = self.router.expect(reply_type);
        if let Err(error) = self.send(message).await {
            self.router.cancel(&pending);
            return Err(error);
        }
        self.router.wait(pending, REPLY_TIMEOUT).await
    }

    fn on_watch_text(&self, text: &str) {
        self.plugins.on_watch_text(text);
        self.syncer.on_watch_text(text);
        let lines = self.lines.lock().unwrap().push(text);
        for message in lines.iter().filter_map(|line| decode_watch_line(line)) {
            self.on_watch_message(message);
        }
    }

    fn on_watch_message(&self, message: Value) {
        if self.router.push(&message) {
            return;
        }
        let _ = self.messages.send(message.clone());
        let settings: CoreSettings = self.settings.view();
        match message["t"].as_str().unwrap_or_default() {
            "findPhone" => {
                let ringing = message["n"] == true || message["n"] == "true";
                self.device.find_phone(ringing);
                self.change(|state| state.find_phone = ringing);
            }
            "music" if settings.forward_music => {
                if let Some(command) = message["n"].as_str() {
                    self.spawn_quietly(self.device.media_command(command));
                }
            }
            "request" => {
                let media = self.state.lock().unwrap().now_playing.clone();
                for reply in answer_state_request(&message, media.as_ref(), settings.forward_music) {
                    self.spawn_send(reply);
                }
            }
            "call" => {
                if let Some(answer) = message["n"].as_str() {
                    self.forwarder.lock().unwrap().calls.note_watch_answer(answer);
                    if let Some(command) = CallCommand::from_watch(answer) {
                        self.spawn_quietly(self.device.call_command(command));
                    }
                }
            }
            "error" => {
                let text = message["msg"].as_str().unwrap_or_default().to_string();
                self.change(|state| state.last_error = Some(text));
            }
            _ => {}
        }
    }

    // Shows that the watch is looking for this device, as the watch's own find phone does.
    pub fn start_finding_phone(&self) {
        self.device.find_phone(true);
        self.change(|state| state.find_phone = true);
    }

    // The window dismissed the find-phone alert.
    pub fn stop_finding_phone(&self) {
        self.device.find_phone(false);
        self.change(|state| state.find_phone = false);
    }

    fn spawn_send(&self, message: Value) {
        if let Some(daemon) = self.weak.upgrade() {
            self.runtime.spawn(async move {
                let _ = daemon.send(&message).await;
            });
        }
    }

    fn spawn_quietly(&self, work: BoxFuture<'static, Result<(), String>>) {
        self.runtime.spawn(async move {
            let _ = work.await;
        });
    }

    // ========================================================================
    // The device's events
    // ========================================================================

    pub fn phone_event(&self, event: PhoneEvent) {
        let _ = self.phone_events.send(event);
    }

    // Forwards the device's events in the order they came, since a removal must follow its notification.
    async fn forward_phone_events(self: Arc<Self>, mut queue: mpsc::UnboundedReceiver<PhoneEvent>) {
        while let Some(event) = queue.recv().await {
            if let PhoneEvent::Media(media) = &event {
                let media = media.clone();
                self.change(|state| state.now_playing = Some(media));
            }
            match (&event, self.features()) {
                (PhoneEvent::Call { state, .. }, Some(features)) => features.sos.on_call(*state),
                (PhoneEvent::SosStopped, Some(features)) => features.sos.end(),
                _ => {}
            }
            let settings: CoreSettings = self.settings.view();
            let quiet = Forwarder::needs_quiet(&event, &settings) && self.device.is_do_not_disturb().await;
            let ready = self.readiness.borrow().uart();
            let messages = self
                .forwarder
                .lock()
                .unwrap()
                .messages(&event, &settings, ready, quiet);
            for message in messages {
                let _ = self.send(&message).await;
            }
        }
    }

    // ========================================================================
    // The window's state
    // ========================================================================

    // Records what a background job reports, such as {"kind":"sent"} under "noteStatus".
    pub fn set_feature(&self, key: &str, value: Value) {
        self.change(|state| {
            state.features.insert(key.to_string(), value);
        });
    }

    // Shows a background job's failure to the window as the last error.
    pub fn report_error(&self, text: impl Into<String>) {
        let text = text.into();
        self.change(|state| state.last_error = Some(text));
    }

    fn change(&self, edit: impl FnOnce(&mut DaemonState)) {
        let changed = {
            let mut state = self.state.lock().unwrap();
            let before = state.clone();
            edit(&mut state);
            *state != before
        };
        if changed {
            self.publish();
        }
    }

    fn publish(&self) {
        let state = self.state();
        self.emit("daemon:state", state);
    }
}

// A saved interval outside what the sync accepts falls back to the default, as the window's syncIntervalMs does.
fn valid_interval(seconds: f64) -> Duration {
    let whole = seconds.fract() == 0.0;
    let fits = (SHORTEST_INTERVAL.as_secs_f64()..=LONGEST_INTERVAL.as_secs_f64()).contains(&seconds);
    if whole && fits {
        Duration::from_secs(seconds as u64)
    } else {
        DEFAULT_STATUS_INTERVAL
    }
}

fn file_service_key() -> String {
    subscription_key(&FILE_TRANSFER_SERVICE.to_string(), &FILE_TRANSFER.to_string())
}

// ============================================================================
// The plugins' and the sync's way out
// ============================================================================

struct DaemonOutput {
    daemon: Weak<Daemon>,
}

impl DaemonOutput {
    fn write_line(&self, line: String) {
        let Some(daemon) = self.daemon.upgrade() else {
            return;
        };
        let runtime = daemon.runtime.clone();
        runtime.spawn(async move {
            if let Err(error) = daemon.link.write_text(line).await {
                daemon.emit(
                    "plugins:log",
                    json!({ "id": "", "line": format!("error: {error}") }),
                );
            }
        });
    }
}

impl HostOutput for DaemonOutput {
    fn write_to_watch(&self, line: String) {
        self.write_line(line);
    }

    fn send_to_ui(&self, plugin: &str, message: Value) {
        if let Some(daemon) = self.daemon.upgrade() {
            daemon.emit("plugins:ui", json!({ "id": plugin, "message": message }));
        }
    }

    fn log(&self, plugin: &str, line: &str) {
        eprintln!("plugin {plugin}: {line}");
        if let Some(daemon) = self.daemon.upgrade() {
            daemon.emit("plugins:log", json!({ "id": plugin, "line": line }));
        }
    }
}

impl SyncOutput for DaemonOutput {
    fn write_to_watch(&self, line: String) {
        self.write_line(line);
    }

    fn synced(&self, report: &SyncReport) {
        if let Some(daemon) = self.daemon.upgrade() {
            daemon.plugins.on_synced(&report.changed);
            daemon.emit("sync:report", serde_json::to_value(report).unwrap_or_default());
        }
    }
}

impl WatchData for DaemonOutput {
    fn read(&self, source: &str) -> Option<Value> {
        self.daemon.upgrade()?.syncer.store().get(source)
    }

    fn list(&self, prefix: &str) -> Vec<String> {
        self.daemon
            .upgrade()
            .map_or_else(Vec::new, |daemon| daemon.syncer.store().sources(prefix))
    }

    // Plugins call this from a thread of their own, so it may wait for the watch.
    fn write(&self, plugin: &str, write: &DataWrite) -> Result<Value, String> {
        let daemon = self.daemon.upgrade().ok_or("The companion is shutting down.")?;
        let syncer = daemon.syncer.clone();
        match write {
            DataWrite::AppState(changes) => daemon.runtime.block_on(syncer.write_app_state(plugin, changes)),
            DataWrite::Settings(changes) => daemon.runtime.block_on(syncer.write_settings(changes.clone())),
        }
    }

    fn sync_now(&self) {
        if let Some(daemon) = self.daemon.upgrade() {
            daemon.syncer.sync_now();
        }
    }
}

// The file service over the watch link; its answers come back as `notify` events.
struct DaemonFiles {
    link: Arc<dyn Link>,
}

impl FileTransport for DaemonFiles {
    fn send(&self, command: Vec<u8>) -> BoxFuture<'static, Result<(), String>> {
        let link = self.link.clone();
        Box::pin(async move {
            // The window unsubscribes when it closes its own client, so subscribe again each time.
            link.subscribe(FILE_TRANSFER_SERVICE, FILE_TRANSFER).await?;
            link.write(FILE_TRANSFER_SERVICE, FILE_TRANSFER, command, WriteMode::Request)
                .await
        })
    }
}

#[cfg(test)]
mod tests;
