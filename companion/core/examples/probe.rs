// Checks the Bluetooth layer against a real watch: scans, connects, reads the battery and the time, and disconnects.
//
// cargo run -p battletime-companion-core --example probe [address]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use battletime_companion_core::ble::{WatchBle, BATTLETIME_NAME};
use serde_json::Value;

const BATTERY_SERVICE: &str = "0000180f-0000-1000-8000-00805f9b34fb";
const BATTERY_LEVEL: &str = "00002a19-0000-1000-8000-00805f9b34fb";
const TIME_SERVICE: &str = "00001805-0000-1000-8000-00805f9b34fb";
const CURRENT_TIME: &str = "00002a2b-0000-1000-8000-00805f9b34fb";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let found: Arc<Mutex<Vec<Value>>> = Arc::default();
    let recorded = found.clone();
    let ble = WatchBle::new(Arc::new(move |name, payload| {
        println!("event {name}: {payload}");
        if name == "device" {
            recorded.lock().unwrap().push(payload);
        }
    }));

    let address = match std::env::args().nth(1) {
        Some(address) => address,
        None => {
            ble.start_scan().await?;
            tokio::time::sleep(Duration::from_secs(8)).await;
            ble.stop_scan().await?;
            let watches = found.lock().unwrap();
            let watch = watches
                .iter()
                .find(|device| {
                    device["name"]
                        .as_str()
                        .is_some_and(|name| name.starts_with(BATTLETIME_NAME))
                })
                .ok_or("No BattleTime watch answered the scan.")?;
            watch["id"].as_str().unwrap().to_string()
        }
    };

    println!("connecting to {address}");
    ble.connect(&address).await?;
    println!(
        "battery {:?}",
        ble.gatt_read(BATTERY_SERVICE, BATTERY_LEVEL).await?
    );
    println!(
        "current time {:?}",
        ble.gatt_read(TIME_SERVICE, CURRENT_TIME).await?
    );
    ble.disconnect().await?;
    Ok(())
}
