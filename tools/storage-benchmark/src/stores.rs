//! The two key-value stores behind one small interface, so the benchmark runs the same steps on each.

use ekv::{Config, Database};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use sequential_storage::{
    cache::CacheImpl,
    map::{MapConfig, MapStorage},
};

use crate::{flash::SimulatedFlash, workload::Key};

// ============================================================================
// What the benchmark needs from a store
// ============================================================================

pub trait Store {
    /// Writes the values together: one transaction where the store has them, one write each where it does not.
    async fn write_all(&mut self, values: &[(Key, Vec<u8>)]);

    /// Reads one value into the buffer and returns its length, or `None` when the key is missing.
    async fn read(&mut self, key: Key, buffer: &mut [u8]) -> Option<usize>;
}

// ============================================================================
// ekv
// ============================================================================

pub struct EkvStore {
    database: Database<SimulatedFlash, NoopRawMutex>,
}

impl EkvStore {
    pub async fn format(flash: SimulatedFlash) -> EkvStore {
        let database = Database::new(flash, Config::default());
        database.format().await.expect("ekv format");
        EkvStore { database }
    }
}

impl Store for EkvStore {
    async fn write_all(&mut self, values: &[(Key, Vec<u8>)]) {
        // ekv takes a transaction's keys in ascending order, each once, so the last value for a key wins.
        let mut sorted: Vec<&(Key, Vec<u8>)> = values.iter().collect();
        sorted.reverse();
        sorted.sort_by_key(|(key, _)| *key);
        sorted.dedup_by_key(|(key, _)| *key);
        let mut transaction = self.database.write_transaction().await;
        for (key, value) in sorted {
            transaction.write(&key.to_be_bytes(), value).await.expect("ekv write");
        }
        transaction.commit().await.expect("ekv commit");
    }

    async fn read(&mut self, key: Key, buffer: &mut [u8]) -> Option<usize> {
        let transaction = self.database.read_transaction().await;
        match transaction.read(&key.to_be_bytes(), buffer).await {
            Ok(length) => Some(length),
            Err(ekv::ReadError::KeyNotFound) => None,
            Err(error) => panic!("ekv read: {error:?}"),
        }
    }
}

// ============================================================================
// sequential-storage
// ============================================================================

pub struct SequentialStore<C: CacheImpl<Key>> {
    map: MapStorage<Key, SimulatedFlash, C>,
    /// The buffer every operation borrows; it must hold the longest key and value together.
    data_buffer: Vec<u8>,
}

impl<C: CacheImpl<Key>> SequentialStore<C> {
    pub fn new(flash: SimulatedFlash, flash_bytes: u32, cache: C, data_buffer_bytes: usize) -> Self {
        SequentialStore {
            map: MapStorage::new(flash, MapConfig::new(0..flash_bytes), cache),
            data_buffer: vec![0; data_buffer_bytes],
        }
    }
}

impl<C: CacheImpl<Key>> Store for SequentialStore<C> {
    async fn write_all(&mut self, values: &[(Key, Vec<u8>)]) {
        for (key, value) in values {
            self.map
                .store_item(&mut self.data_buffer, key, &value.as_slice())
                .await
                .expect("sequential-storage store");
        }
    }

    async fn read(&mut self, key: Key, buffer: &mut [u8]) -> Option<usize> {
        let value: Option<&[u8]> =
            self.map.fetch_item(&mut self.data_buffer, &key).await.expect("sequential-storage fetch");
        value.map(|value| {
            buffer[..value.len()].copy_from_slice(value);
            value.len()
        })
    }
}
