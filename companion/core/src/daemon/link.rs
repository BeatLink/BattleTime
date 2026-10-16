// The daemon's view of the watch link: connecting, the UART lines and raw GATT, behind one object-safe trait.
//
// The Bluetooth layer stands behind it in the app and a fake watch in the tests.

use std::sync::Arc;

use futures::future::BoxFuture;
use uuid::Uuid;

use crate::ble::{ValueListener, WatchBle};
use crate::protocol::link::{GattLink, WriteMode};

// ============================================================================
// The trait
// ============================================================================

pub trait Link: Send + Sync + 'static {
    fn connect(&self, address: String) -> BoxFuture<'static, Result<(), String>>;
    fn disconnect(&self) -> BoxFuture<'static, Result<(), String>>;
    fn write_text(&self, text: String) -> BoxFuture<'static, Result<(), String>>;
    fn has_characteristic(&self, service: Uuid, characteristic: Uuid) -> bool;
    fn read(&self, service: Uuid, characteristic: Uuid) -> BoxFuture<'static, Result<Vec<u8>, String>>;
    fn write(
        &self,
        service: Uuid,
        characteristic: Uuid,
        data: Vec<u8>,
        mode: WriteMode,
    ) -> BoxFuture<'static, Result<(), String>>;
    fn listen(
        &self,
        service: Uuid,
        characteristic: Uuid,
        listener: ValueListener,
    ) -> BoxFuture<'static, Result<(), String>>;
    // Subscribes so the characteristic's notifications arrive as `notify` link events.
    fn subscribe(&self, service: Uuid, characteristic: Uuid) -> BoxFuture<'static, Result<(), String>>;
}

// A shared link that the protocols in `crate::protocol` can use as their GATT link.
#[derive(Clone)]
pub struct SharedLink(pub Arc<dyn Link>);

impl GattLink for SharedLink {
    fn has_characteristic(&self, service: Uuid, characteristic: Uuid) -> bool {
        self.0.has_characteristic(service, characteristic)
    }

    async fn read(&self, service: Uuid, characteristic: Uuid) -> Result<Vec<u8>, String> {
        self.0.read(service, characteristic).await
    }

    async fn write(
        &self,
        service: Uuid,
        characteristic: Uuid,
        data: &[u8],
        mode: WriteMode,
    ) -> Result<(), String> {
        self.0.write(service, characteristic, data.to_vec(), mode).await
    }

    async fn listen(
        &self,
        service: Uuid,
        characteristic: Uuid,
        listener: ValueListener,
    ) -> Result<(), String> {
        self.0.listen(service, characteristic, listener).await
    }
}

// ============================================================================
// The Bluetooth layer as the link
// ============================================================================

impl Link for WatchBle {
    fn connect(&self, address: String) -> BoxFuture<'static, Result<(), String>> {
        let ble = self.clone();
        Box::pin(async move { WatchBle::connect(&ble, &address).await.map_err(|error| error.0) })
    }

    fn disconnect(&self) -> BoxFuture<'static, Result<(), String>> {
        let ble = self.clone();
        Box::pin(async move { WatchBle::disconnect(&ble).await.map_err(|error| error.0) })
    }

    fn write_text(&self, text: String) -> BoxFuture<'static, Result<(), String>> {
        let ble = self.clone();
        Box::pin(async move { WatchBle::write(&ble, &text).await.map_err(|error| error.0) })
    }

    fn has_characteristic(&self, service: Uuid, characteristic: Uuid) -> bool {
        GattLink::has_characteristic(self, service, characteristic)
    }

    fn read(&self, service: Uuid, characteristic: Uuid) -> BoxFuture<'static, Result<Vec<u8>, String>> {
        let ble = self.clone();
        Box::pin(async move { GattLink::read(&ble, service, characteristic).await })
    }

    fn write(
        &self,
        service: Uuid,
        characteristic: Uuid,
        data: Vec<u8>,
        mode: WriteMode,
    ) -> BoxFuture<'static, Result<(), String>> {
        let ble = self.clone();
        Box::pin(async move { GattLink::write(&ble, service, characteristic, &data, mode).await })
    }

    fn listen(
        &self,
        service: Uuid,
        characteristic: Uuid,
        listener: ValueListener,
    ) -> BoxFuture<'static, Result<(), String>> {
        let ble = self.clone();
        Box::pin(async move { GattLink::listen(&ble, service, characteristic, listener).await })
    }

    fn subscribe(&self, service: Uuid, characteristic: Uuid) -> BoxFuture<'static, Result<(), String>> {
        let ble = self.clone();
        Box::pin(async move {
            ble.gatt_subscribe(&service.to_string(), &characteristic.to_string())
                .await
                .map_err(|error| error.0)
        })
    }
}
