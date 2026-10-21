// Bluetooth to the watch through btleplug: scanning, the UART message channel and raw GATT.
//
// Every step has a timeout and a failed connect drops the link, because a half-open BlueZ link makes
// every later attempt wait for services that never resolve.

use std::collections::HashSet;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use btleplug::api::{
    Central, CentralEvent, CentralState, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral, PeripheralId};
use futures::StreamExt;
use serde_json::{json, Value};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::text::Utf8Joiner;

// ============================================================================
// Names the watch answers to
// ============================================================================

// Nordic UART Service, which carries BattleTime's Gadgetbridge-style message lines.
pub const NUS_SERVICE: Uuid = Uuid::from_u128(0x6e400001_b5a3_f393_e0a9_e50e24dcca9e);
pub const NUS_RX: Uuid = Uuid::from_u128(0x6e400002_b5a3_f393_e0a9_e50e24dcca9e);
pub const NUS_TX: Uuid = Uuid::from_u128(0x6e400003_b5a3_f393_e0a9_e50e24dcca9e);

// BattleTime advertises the legacy DFU service while its firmware runs; wasp-os only from its bootloader.
pub const DFU_SERVICE: Uuid = Uuid::from_u128(0x00001530_1212_efde_1523_785feabcd123);

// BattleTime's own service, which only its running firmware serves.
pub const BATTLETIME_SERVICE: Uuid = Uuid::from_u128(0x6e540001_0000_4000_8000_4e656f54696d);

// BattleTime has no room left in its advertisement for its own service, so it is recognised by name.
pub const BATTLETIME_NAME: &str = "BattleTime";

// BlueZ splits a write itself only up to the negotiated MTU, so stay at the size every link supports.
const CHUNK: usize = 20;

// ============================================================================
// Timing
// ============================================================================

// How long to wait for a watch that BlueZ has not seen yet.
const FIND_TIMEOUT: Duration = Duration::from_secs(15);

// How long any one step on the link may take.
const STEP_TIMEOUT: Duration = Duration::from_secs(20);

// How long service discovery may take before the link is dropped and tried once more.
const SERVICES_TIMEOUT: Duration = Duration::from_secs(12);

// How long a disconnect may take, since a wedged link makes BlueZ's disconnect hang.
const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(5);

// How often to look again for a watch that is not in BlueZ's list yet.
const FIND_POLL: Duration = Duration::from_millis(250);

// ============================================================================
// Events and errors
// ============================================================================

// Receives `device`, `line`, `state`, `mode`, `notify` and `error` events, as the window's bridge names them.
pub type EventSink = Arc<dyn Fn(&str, Value) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BleError(pub String);

impl std::fmt::Display for BleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for BleError {}

impl From<btleplug::Error> for BleError {
    fn from(error: btleplug::Error) -> Self {
        BleError(error.to_string())
    }
}

fn fail<T>(message: impl Into<String>) -> Result<T, BleError> {
    Err(BleError(message.into()))
}

async fn within<T, E: Into<BleError>>(
    limit: Duration,
    what: &str,
    step: impl Future<Output = Result<T, E>>,
) -> Result<T, BleError> {
    let result = match tokio::time::timeout(limit, step).await {
        Ok(result) => result.map_err(Into::into),
        Err(_) => fail(format!("Timed out {what}.")),
    };
    if let Err(error) = &result {
        log::warn!("{what} failed: {error}");
    }
    result
}

// ============================================================================
// What a scan reports
// ============================================================================

// True for a watch sitting in its bootloader: DFU advertised, no UART, and not BattleTime's own name.
pub fn is_bootloader(name: &str, advertised: &[Uuid]) -> bool {
    !advertised.contains(&NUS_SERVICE)
        && advertised.contains(&DFU_SERVICE)
        && !name.starts_with(BATTLETIME_NAME)
}

// The `device` event for one watch found while scanning.
pub fn device_event(address: &str, name: &str, rssi: Option<i16>, advertised: &[Uuid]) -> Value {
    json!({
        "id": address,
        "name": name,
        "rssi": rssi,
        "uuids": advertised.iter().map(Uuid::to_string).collect::<Vec<_>>(),
        "bootloader": is_bootloader(name, advertised),
    })
}

// What the watch turned out to be running, from the services it serves once connected.
pub fn watch_mode(served: &[Uuid]) -> &'static str {
    if served.contains(&BATTLETIME_SERVICE) || served.contains(&NUS_SERVICE) {
        "application"
    } else {
        "bootloader"
    }
}

pub fn subscription_key(service: &str, characteristic: &str) -> String {
    format!("{service}/{characteristic}")
}

// The adapter to use: the current one while it is not off, else the first that is on, else the first not known to be off.
pub fn choose_adapter(states: &[Option<CentralState>], current: Option<usize>) -> Option<usize> {
    let off = |at: usize| matches!(states.get(at), Some(Some(CentralState::PoweredOff)));
    if let Some(current) = current.filter(|current| *current < states.len() && !off(*current)) {
        return Some(current);
    }
    states
        .iter()
        .position(|state| matches!(state, Some(CentralState::PoweredOn)))
        .or_else(|| (0..states.len()).find(|at| !off(*at)))
}

fn parse_uuid(text: &str) -> Result<Uuid, BleError> {
    Uuid::parse_str(text).or_else(|_| fail(format!("{text} is not a Bluetooth UUID.")))
}

// ============================================================================
// The link
// ============================================================================

struct Subscription {
    characteristic: Uuid,
    key: String,
}

// Rust code that hears a characteristic's notifications, such as the packages client.
pub type ValueListener = Arc<dyn Fn(&[u8]) + Send + Sync>;

#[derive(Default)]
struct Link {
    peripheral: Option<Peripheral>,
    peripheral_id: Option<PeripheralId>,
    characteristics: Vec<Characteristic>,
    rx: Option<Characteristic>,
    subscriptions: Vec<Subscription>,
    listeners: Vec<(Uuid, ValueListener)>,
    notification_task: Option<JoinHandle<()>>,
}

#[derive(Default)]
struct Scan {
    reporting: bool,
    seen: HashSet<String>,
}

struct Inner {
    sink: EventSink,
    // Every adapter BlueZ has, in the order of their names, and the one in use.
    adapters: tokio::sync::OnceCell<Vec<Adapter>>,
    chosen: Mutex<Option<usize>>,
    link: Mutex<Link>,
    scan: Mutex<Scan>,
    // Connecting, reconnecting and disconnecting take turns, so one never undoes another half way.
    operation: tokio::sync::Mutex<()>,
}

#[derive(Clone)]
pub struct WatchBle {
    inner: Arc<Inner>,
}

impl WatchBle {
    pub fn new(sink: EventSink) -> Self {
        WatchBle {
            inner: Arc::new(Inner {
                sink,
                adapters: tokio::sync::OnceCell::new(),
                chosen: Mutex::new(None),
                link: Mutex::new(Link::default()),
                scan: Mutex::new(Scan::default()),
                operation: tokio::sync::Mutex::new(()),
            }),
        }
    }

    fn emit(&self, name: &str, payload: Value) {
        match name {
            "state" | "mode" => log::info!("link {name}: {payload}"),
            "error" => log::warn!("link error: {payload}"),
            _ => {}
        }
        (self.inner.sink)(name, payload);
    }

    // ========================================================================
    // Adapter
    // ========================================================================

    // BlueZ lists adapters in no fixed order and one may be off, so the one in use is kept only while it stays on.
    async fn adapter(&self) -> Result<Adapter, BleError> {
        let adapters = self
            .inner
            .adapters
            .get_or_try_init(|| async {
                let manager = Manager::new().await?;
                let mut named = Vec::new();
                for adapter in manager.adapters().await? {
                    let name = adapter.adapter_info().await.unwrap_or_default();
                    self.follow_adapter_events(&adapter).await?;
                    named.push((name, adapter));
                }
                named.sort_by(|a, b| a.0.cmp(&b.0));
                Ok::<_, BleError>(named.into_iter().map(|(_, adapter)| adapter).collect::<Vec<_>>())
            })
            .await?;
        if adapters.is_empty() {
            log::warn!("no Bluetooth adapter found");
            return fail("This computer has no Bluetooth adapter.");
        }
        let mut states = Vec::new();
        for adapter in adapters {
            states.push(adapter.adapter_state().await.ok());
        }
        let current = *self.inner.chosen.lock().unwrap();
        let Some(chosen) = choose_adapter(&states, current) else {
            log::warn!("every Bluetooth adapter is off: {states:?}");
            return fail("The Bluetooth adapter is powered off.");
        };
        if current != Some(chosen) {
            log::info!("using Bluetooth adapter {chosen} of {}", adapters.len());
        }
        *self.inner.chosen.lock().unwrap() = Some(chosen);
        Ok(adapters[chosen].clone())
    }

    // One task follows the adapter for the app's whole life: it reports scan results and notices a dropped link.
    async fn follow_adapter_events(&self, adapter: &Adapter) -> Result<(), BleError> {
        let mut events = adapter.events().await?;
        let ble = self.clone();
        let adapter = adapter.clone();
        tokio::spawn(async move {
            while let Some(event) = events.next().await {
                match event {
                    CentralEvent::DeviceDiscovered(id)
                    | CentralEvent::DeviceUpdated(id)
                    | CentralEvent::ServicesAdvertisement { id, .. } => {
                        ble.report_device(&adapter, &id).await;
                    }
                    CentralEvent::DeviceDisconnected(id) => ble.on_disconnected(&id),
                    CentralEvent::StateUpdate(state) => log::info!("Bluetooth adapter is now {state:?}"),
                    _ => {}
                }
            }
        });
        Ok(())
    }

    // ========================================================================
    // Scanning
    // ========================================================================

    pub async fn start_scan(&self) -> Result<(), BleError> {
        log::info!("scanning for watches");
        let adapter = self.adapter().await?;
        {
            let mut scan = self.inner.scan.lock().unwrap();
            scan.reporting = true;
            scan.seen.clear();
        }
        adapter.start_scan(ScanFilter::default()).await?;
        // Watches BlueZ already knows raise no discovery event, so they are reported now.
        for peripheral in adapter.peripherals().await.unwrap_or_default() {
            self.report_device(&adapter, &peripheral.id()).await;
        }
        Ok(())
    }

    pub async fn stop_scan(&self) -> Result<(), BleError> {
        self.inner.scan.lock().unwrap().reporting = false;
        let chosen = *self.inner.chosen.lock().unwrap();
        if let (Some(adapters), Some(chosen)) = (self.inner.adapters.get(), chosen) {
            let _ = adapters[chosen].stop_scan().await;
        }
        Ok(())
    }

    // Reports a device once per scan, as soon as it has a name.
    async fn report_device(&self, adapter: &Adapter, id: &PeripheralId) {
        if !self.inner.scan.lock().unwrap().reporting {
            return;
        }
        let Ok(peripheral) = adapter.peripheral(id).await else {
            return;
        };
        let Ok(Some(properties)) = peripheral.properties().await else {
            return;
        };
        let Some(name) = properties.local_name else { return };
        let address = properties.address.to_string();
        if !self.inner.scan.lock().unwrap().seen.insert(address.clone()) {
            return;
        }
        self.emit(
            "device",
            device_event(&address, &name, properties.rssi, &properties.services),
        );
    }

    // ========================================================================
    // Connecting
    // ========================================================================

    pub async fn connect(&self, address: &str) -> Result<(), BleError> {
        let _turn = self.inner.operation.lock().await;
        self.connect_in_turn(address, true).await
    }

    // Drops the link and comes back to `address` without the UART, which is how a firmware update follows the watch into its bootloader.
    pub async fn reconnect(&self, address: &str) -> Result<(), BleError> {
        let _turn = self.inner.operation.lock().await;
        log::info!("reconnecting to {address} without the UART");
        self.drop_link().await;
        self.emit("state", json!("disconnected"));
        tokio::time::sleep(Duration::from_secs(1)).await;
        self.connect_in_turn(address, false).await
    }

    pub async fn disconnect(&self) -> Result<(), BleError> {
        let _turn = self.inner.operation.lock().await;
        log::info!("disconnecting as asked");
        self.drop_link().await;
        self.emit("state", json!("disconnected"));
        Ok(())
    }

    async fn connect_in_turn(&self, address: &str, uart: bool) -> Result<(), BleError> {
        match self.try_connect(address, uart).await {
            Ok(()) => Ok(()),
            Err(error) => {
                log::warn!("connecting to {address} failed: {error}");
                self.drop_link().await;
                self.emit("state", json!("disconnected"));
                Err(error)
            }
        }
    }

    async fn try_connect(&self, address: &str, uart: bool) -> Result<(), BleError> {
        let adapter = self.adapter().await?;
        log::info!(
            "connecting to {address}{}",
            if uart { "" } else { " for its bootloader" }
        );
        self.emit("state", json!("connecting"));

        let peripheral = self.find(&adapter, address).await?;
        // Discovery competes with the connection, so it stops once the watch is found.
        self.stop_scan().await?;
        {
            let mut link = self.inner.link.lock().unwrap();
            link.peripheral = Some(peripheral.clone());
            link.peripheral_id = Some(peripheral.id());
        }

        // BlueZ may already hold the link from a previous run or another tool.
        if peripheral.is_connected().await.unwrap_or(false) {
            log::info!("the platform already holds a link to {address}");
        } else {
            within(STEP_TIMEOUT, "connecting to the watch", peripheral.connect()).await?;
        }
        self.resolve_services(&peripheral).await?;

        let characteristics: Vec<Characteristic> = peripheral.characteristics().into_iter().collect();
        let served: Vec<Uuid> = peripheral.services().iter().map(|service| service.uuid).collect();
        log::info!(
            "the watch serves {} services and {} characteristics: {}",
            served.len(),
            characteristics.len(),
            describe_characteristics(&characteristics)
        );
        let mut mode = "bootloader";
        let mut rx = None;
        if uart {
            mode = watch_mode(&served);
            let find = |uuid| {
                characteristics
                    .iter()
                    .find(|found| found.service_uuid == NUS_SERVICE && found.uuid == uuid)
                    .cloned()
            };
            if let (Some(found_rx), Some(found_tx)) = (find(NUS_RX), find(NUS_TX)) {
                within(
                    STEP_TIMEOUT,
                    "subscribing to the watch",
                    peripheral.subscribe(&found_tx),
                )
                .await?;
                rx = Some(found_rx);
            }
        }

        let notification_task = self.follow_notifications(&peripheral, uart).await?;
        {
            let mut link = self.inner.link.lock().unwrap();
            link.characteristics = characteristics;
            link.rx = rx;
            link.subscriptions.clear();
            link.listeners.clear();
            link.notification_task = Some(notification_task);
        }
        self.emit("mode", json!(mode));
        self.emit("state", json!("connected"));
        Ok(())
    }

    // BlueZ forgets an unpaired device soon after it last saw one, so a watch may need discovering again first.
    async fn find(&self, adapter: &Adapter, address: &str) -> Result<Peripheral, BleError> {
        let deadline = tokio::time::Instant::now() + FIND_TIMEOUT;
        let mut scanning = false;
        loop {
            for peripheral in adapter.peripherals().await.unwrap_or_default() {
                let found = peripheral.properties().await.ok().flatten();
                if found
                    .is_some_and(|properties| properties.address.to_string().eq_ignore_ascii_case(address))
                {
                    return Ok(peripheral);
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return fail(format!(
                    "No watch at {address} is in range. Wake it, or move it closer."
                ));
            }
            if !scanning {
                adapter.start_scan(ScanFilter::default()).await?;
                scanning = true;
            }
            tokio::time::sleep(FIND_POLL).await;
        }
    }

    // Some links come up without ever resolving their services, often in the bootloader; one fresh link clears it.
    async fn resolve_services(&self, peripheral: &Peripheral) -> Result<(), BleError> {
        if within(
            SERVICES_TIMEOUT,
            "discovering services",
            peripheral.discover_services(),
        )
        .await
        .is_ok()
        {
            return Ok(());
        }
        log::warn!("services never resolved, so the link is dropped and made again");
        let _ = tokio::time::timeout(DISCONNECT_TIMEOUT, peripheral.disconnect()).await;
        tokio::time::sleep(Duration::from_millis(1500)).await;
        within(STEP_TIMEOUT, "connecting to the watch", peripheral.connect()).await?;
        if within(
            SERVICES_TIMEOUT,
            "discovering services",
            peripheral.discover_services(),
        )
        .await
        .is_ok()
        {
            return Ok(());
        }
        fail("Connected, but the watch never offered its services. Move it closer, or restart it.")
    }

    // Passes UART bytes on as text, and every other subscribed characteristic as raw bytes.
    async fn follow_notifications(
        &self,
        peripheral: &Peripheral,
        uart: bool,
    ) -> Result<JoinHandle<()>, BleError> {
        let mut notifications = peripheral.notifications().await?;
        let ble = self.clone();
        Ok(tokio::spawn(async move {
            let mut joiner = Utf8Joiner::default();
            while let Some(notification) = notifications.next().await {
                if uart && notification.uuid == NUS_TX {
                    let text = joiner.push(&notification.value);
                    if !text.is_empty() {
                        ble.emit("line", json!(text));
                    }
                    continue;
                }
                let (keys, listeners): (Vec<String>, Vec<ValueListener>) = {
                    let link = ble.inner.link.lock().unwrap();
                    (
                        link.subscriptions
                            .iter()
                            .filter(|subscription| subscription.characteristic == notification.uuid)
                            .map(|subscription| subscription.key.clone())
                            .collect(),
                        link.listeners
                            .iter()
                            .filter(|(characteristic, _)| *characteristic == notification.uuid)
                            .map(|(_, listener)| listener.clone())
                            .collect(),
                    )
                };
                for listener in listeners {
                    listener(&notification.value);
                }
                for key in keys {
                    ble.emit("notify", json!({ "key": key, "value": notification.value }));
                }
            }
        }))
    }

    fn on_disconnected(&self, id: &PeripheralId) {
        let ours = {
            let mut link = self.inner.link.lock().unwrap();
            let ours = link.peripheral_id.as_ref() == Some(id);
            if ours {
                forget(&mut link);
            }
            ours
        };
        if ours {
            log::info!("the watch's link dropped");
            self.emit("state", json!("disconnected"));
        } else {
            log::debug!("a link that is not the watch's dropped: {id:?}");
        }
    }

    // Lets go of whatever BlueZ still holds, without hanging on a link that is already wedged.
    async fn drop_link(&self) {
        let peripheral = {
            let mut link = self.inner.link.lock().unwrap();
            let peripheral = link.peripheral.take();
            forget(&mut link);
            peripheral
        };
        if let Some(peripheral) = peripheral {
            log::info!("dropping the link");
            if tokio::time::timeout(DISCONNECT_TIMEOUT, peripheral.disconnect())
                .await
                .is_err()
            {
                log::warn!("the platform did not finish disconnecting in time");
            }
        }
    }

    // ========================================================================
    // The UART message channel
    // ========================================================================

    pub async fn write(&self, text: &str) -> Result<(), BleError> {
        self.write_bytes(text.as_bytes()).await
    }

    pub async fn write_bytes(&self, data: &[u8]) -> Result<(), BleError> {
        let (peripheral, rx) = {
            let link = self.inner.link.lock().unwrap();
            match (&link.peripheral, &link.rx) {
                (Some(peripheral), Some(rx)) => (peripheral.clone(), rx.clone()),
                (Some(_), None) => return fail("This watch has no UART message channel."),
                _ => return fail("Not connected"),
            }
        };
        for chunk in data.chunks(CHUNK) {
            within(
                STEP_TIMEOUT,
                "writing to the watch",
                peripheral.write(&rx, chunk, WriteType::WithoutResponse),
            )
            .await?;
        }
        Ok(())
    }

    // ========================================================================
    // Raw GATT, which a firmware update needs and the UART does not
    // ========================================================================

    fn characteristic(
        &self,
        service: &str,
        characteristic: &str,
    ) -> Result<(Peripheral, Characteristic), BleError> {
        let service = parse_uuid(service)?;
        let characteristic = parse_uuid(characteristic)?;
        let link = self.inner.link.lock().unwrap();
        let Some(peripheral) = &link.peripheral else {
            return fail("Not connected");
        };
        let found = link
            .characteristics
            .iter()
            .find(|found| found.service_uuid == service && found.uuid == characteristic)
            .cloned()
            .ok_or_else(|| {
                log::debug!("asked for {characteristic} in {service}, which the watch does not serve");
                BleError(format!(
                    "The watch has no characteristic {characteristic} in service {service}."
                ))
            })?;
        Ok((peripheral.clone(), found))
    }

    pub fn gatt_has(&self, service: &str, characteristic: &str) -> bool {
        self.characteristic(service, characteristic).is_ok()
    }

    pub async fn gatt_read(&self, service: &str, characteristic: &str) -> Result<Vec<u8>, BleError> {
        let (peripheral, found) = self.characteristic(service, characteristic)?;
        log::debug!("reading {} ({:?})", found.uuid, found.properties);
        let read = within(STEP_TIMEOUT, "reading from the watch", peripheral.read(&found)).await?;
        log::debug!("read {} bytes from {}", read.len(), found.uuid);
        Ok(read)
    }

    // `mode` is "request" for a write the watch acknowledges, anything else for one it does not.
    pub async fn gatt_write(
        &self,
        service: &str,
        characteristic: &str,
        data: &[u8],
        mode: &str,
    ) -> Result<(), BleError> {
        let (peripheral, found) = self.characteristic(service, characteristic)?;
        let write_type = if mode == "request" {
            WriteType::WithResponse
        } else {
            WriteType::WithoutResponse
        };
        log::debug!("writing {} bytes to {} ({write_type:?})", data.len(), found.uuid);
        within(
            STEP_TIMEOUT,
            "writing to the watch",
            peripheral.write(&found, data, write_type),
        )
        .await
    }

    // Subscribes and hands every notification to `listener` as well, until the link drops.
    pub async fn listen(
        &self,
        service: &str,
        characteristic: &str,
        listener: ValueListener,
    ) -> Result<(), BleError> {
        let uuid = parse_uuid(characteristic)?;
        self.gatt_subscribe(service, characteristic).await?;
        self.inner.link.lock().unwrap().listeners.push((uuid, listener));
        Ok(())
    }

    pub async fn gatt_subscribe(&self, service: &str, characteristic: &str) -> Result<(), BleError> {
        let key = subscription_key(service, characteristic);
        let (peripheral, found) = self.characteristic(service, characteristic)?;
        if self
            .inner
            .link
            .lock()
            .unwrap()
            .subscriptions
            .iter()
            .any(|subscription| subscription.key == key)
        {
            return Ok(());
        }
        log::debug!("subscribing to {}", found.uuid);
        within(
            STEP_TIMEOUT,
            "subscribing to the watch",
            peripheral.subscribe(&found),
        )
        .await?;
        self.inner.link.lock().unwrap().subscriptions.push(Subscription {
            characteristic: found.uuid,
            key,
        });
        Ok(())
    }

    pub async fn gatt_unsubscribe(&self, service: &str, characteristic: &str) -> Result<(), BleError> {
        let key = subscription_key(service, characteristic);
        let removed = {
            let mut link = self.inner.link.lock().unwrap();
            let before = link.subscriptions.len();
            link.subscriptions.retain(|subscription| subscription.key != key);
            before != link.subscriptions.len()
        };
        if removed {
            if let Ok((peripheral, found)) = self.characteristic(service, characteristic) {
                let _ = tokio::time::timeout(STEP_TIMEOUT, peripheral.unsubscribe(&found)).await;
            }
        }
        Ok(())
    }

    // ========================================================================
    // Shutting down
    // ========================================================================

    pub async fn destroy(&self) {
        let _ = self.stop_scan().await;
        let _ = self.disconnect().await;
    }
}

// Each characteristic as its id and what it allows, for the log.
fn describe_characteristics(characteristics: &[Characteristic]) -> String {
    characteristics
        .iter()
        .map(|found| format!("{} {:?}", found.uuid, found.properties))
        .collect::<Vec<_>>()
        .join(", ")
}

fn forget(link: &mut Link) {
    if let Some(task) = link.notification_task.take() {
        task.abort();
    }
    *link = Link::default();
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_watch_advertising_only_dfu_is_in_its_bootloader() {
        assert!(is_bootloader("PineDFU", &[DFU_SERVICE]));
    }

    #[test]
    fn battletime_advertises_dfu_while_running_and_is_not_a_bootloader() {
        assert!(!is_bootloader("BattleTime", &[DFU_SERVICE]));
    }

    #[test]
    fn a_watch_with_the_uart_is_not_a_bootloader() {
        assert!(!is_bootloader("wasp-os", &[NUS_SERVICE, DFU_SERVICE]));
        assert!(!is_bootloader("Headphones", &[]));
    }

    #[test]
    fn the_device_event_carries_what_the_scan_screen_reads() {
        let event = device_event("AA:BB:CC:DD:EE:01", "PineDFU", Some(-60), &[DFU_SERVICE]);
        assert_eq!(event["id"], "AA:BB:CC:DD:EE:01");
        assert_eq!(event["name"], "PineDFU");
        assert_eq!(event["rssi"], -60);
        assert_eq!(event["uuids"][0], "00001530-1212-efde-1523-785feabcd123");
        assert_eq!(event["bootloader"], true);
    }

    #[test]
    fn an_unknown_signal_strength_is_null() {
        assert!(device_event("AA", "BattleTime", None, &[])["rssi"].is_null());
    }

    #[test]
    fn running_firmware_serves_battletime_or_the_uart() {
        assert_eq!(watch_mode(&[BATTLETIME_SERVICE]), "application");
        assert_eq!(watch_mode(&[NUS_SERVICE]), "application");
        assert_eq!(watch_mode(&[DFU_SERVICE]), "bootloader");
    }

    #[test]
    fn an_adapter_that_is_off_is_passed_over() {
        use CentralState::{PoweredOff, PoweredOn};
        assert_eq!(
            choose_adapter(&[Some(PoweredOff), Some(PoweredOn)], None),
            Some(1)
        );
        assert_eq!(
            choose_adapter(&[Some(PoweredOn), Some(PoweredOn)], Some(1)),
            Some(1)
        );
        assert_eq!(
            choose_adapter(&[Some(PoweredOn), Some(PoweredOff)], Some(1)),
            Some(0)
        );
        assert_eq!(choose_adapter(&[None, Some(PoweredOff)], None), Some(0));
        assert_eq!(choose_adapter(&[Some(PoweredOff)], Some(0)), None);
        assert_eq!(choose_adapter(&[], None), None);
    }

    #[test]
    fn subscription_keys_match_the_windows_format() {
        assert_eq!(subscription_key("a", "b"), "a/b");
    }

    #[test]
    fn a_bad_uuid_is_reported_by_its_text() {
        assert_eq!(
            parse_uuid("nope"),
            Err(BleError("nope is not a Bluetooth UUID.".into()))
        );
        assert_eq!(parse_uuid(&NUS_SERVICE.to_string()), Ok(NUS_SERVICE));
    }

    #[tokio::test]
    async fn gatt_calls_without_a_link_say_not_connected() {
        let ble = WatchBle::new(Arc::new(|_, _| {}));
        let service = NUS_SERVICE.to_string();
        let characteristic = NUS_RX.to_string();
        assert!(!ble.gatt_has(&service, &characteristic));
        assert_eq!(
            ble.gatt_read(&service, &characteristic).await,
            fail("Not connected")
        );
        assert_eq!(ble.write("hi").await, fail("Not connected"));
        assert_eq!(ble.gatt_unsubscribe(&service, &characteristic).await, Ok(()));
    }

    #[tokio::test]
    async fn a_step_that_overruns_its_limit_names_what_timed_out() {
        let never = std::future::pending::<Result<(), BleError>>();
        assert_eq!(
            within(Duration::from_millis(1), "testing", never).await,
            fail("Timed out testing.")
        );
    }
}
