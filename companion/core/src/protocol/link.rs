// Raw GATT as the protocols need it, so they never touch a Bluetooth library and tests can stand in a fake watch.

use std::future::Future;

use uuid::Uuid;

use crate::ble::{ValueListener, WatchBle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    // The watch acknowledges the write.
    Request,
    // The write is sent and not acknowledged.
    Command,
}

pub trait GattLink: Send + Sync {
    fn has_characteristic(&self, service: Uuid, characteristic: Uuid) -> bool;
    fn read(
        &self,
        service: Uuid,
        characteristic: Uuid,
    ) -> impl Future<Output = Result<Vec<u8>, String>> + Send;
    fn write(
        &self,
        service: Uuid,
        characteristic: Uuid,
        data: &[u8],
        mode: WriteMode,
    ) -> impl Future<Output = Result<(), String>> + Send;
    // Hands every notification of the characteristic to `listener` until the link drops.
    fn listen(
        &self,
        service: Uuid,
        characteristic: Uuid,
        listener: ValueListener,
    ) -> impl Future<Output = Result<(), String>> + Send;
}

impl GattLink for WatchBle {
    fn has_characteristic(&self, service: Uuid, characteristic: Uuid) -> bool {
        self.gatt_has(&service.to_string(), &characteristic.to_string())
    }

    async fn read(&self, service: Uuid, characteristic: Uuid) -> Result<Vec<u8>, String> {
        self.gatt_read(&service.to_string(), &characteristic.to_string())
            .await
            .map_err(|error| error.0)
    }

    async fn write(
        &self,
        service: Uuid,
        characteristic: Uuid,
        data: &[u8],
        mode: WriteMode,
    ) -> Result<(), String> {
        let mode = if mode == WriteMode::Request {
            "request"
        } else {
            "command"
        };
        self.gatt_write(&service.to_string(), &characteristic.to_string(), data, mode)
            .await
            .map_err(|error| error.0)
    }

    async fn listen(
        &self,
        service: Uuid,
        characteristic: Uuid,
        listener: ValueListener,
    ) -> Result<(), String> {
        WatchBle::listen(self, &service.to_string(), &characteristic.to_string(), listener)
            .await
            .map_err(|error| error.0)
    }
}

// ============================================================================
// A fake watch for tests
// ============================================================================

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    // Serves fixed values, records writes, and answers a write with whatever `answer` returns, as notifications.
    #[derive(Default)]
    pub struct FakeWatch {
        pub values: Mutex<HashMap<Uuid, Vec<u8>>>,
        pub writes: Mutex<Vec<(Uuid, Vec<u8>, WriteMode)>>,
        pub listeners: Mutex<Vec<(Uuid, ValueListener)>>,
        #[allow(clippy::type_complexity)]
        pub answer: Mutex<Option<Box<dyn FnMut(Uuid, &[u8]) -> Vec<Vec<u8>> + Send>>>,
    }

    impl FakeWatch {
        pub fn serving(values: &[(Uuid, Vec<u8>)]) -> FakeWatch {
            let watch = FakeWatch::default();
            watch.values.lock().unwrap().extend(values.iter().cloned());
            watch
        }

        pub fn notify(&self, characteristic: Uuid, value: &[u8]) {
            let listeners: Vec<ValueListener> = self
                .listeners
                .lock()
                .unwrap()
                .iter()
                .filter(|(uuid, _)| *uuid == characteristic)
                .map(|(_, listener)| listener.clone())
                .collect();
            for listener in listeners {
                listener(value);
            }
        }
    }

    impl GattLink for FakeWatch {
        fn has_characteristic(&self, _service: Uuid, characteristic: Uuid) -> bool {
            self.values.lock().unwrap().contains_key(&characteristic)
        }

        async fn read(&self, _service: Uuid, characteristic: Uuid) -> Result<Vec<u8>, String> {
            self.values
                .lock()
                .unwrap()
                .get(&characteristic)
                .cloned()
                .ok_or_else(|| "not served".to_string())
        }

        async fn write(
            &self,
            _service: Uuid,
            characteristic: Uuid,
            data: &[u8],
            mode: WriteMode,
        ) -> Result<(), String> {
            self.writes
                .lock()
                .unwrap()
                .push((characteristic, data.to_vec(), mode));
            let replies = match self.answer.lock().unwrap().as_mut() {
                Some(answer) => answer(characteristic, data),
                None => Vec::new(),
            };
            for reply in replies {
                self.notify(characteristic, &reply);
            }
            Ok(())
        }

        async fn listen(
            &self,
            _service: Uuid,
            characteristic: Uuid,
            listener: ValueListener,
        ) -> Result<(), String> {
            self.listeners.lock().unwrap().push((characteristic, listener));
            Ok(())
        }
    }
}
