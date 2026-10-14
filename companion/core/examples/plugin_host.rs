// Installs plugins into a folder and runs them against a real watch, without the desktop app.
//
// cargo run -p neotime-companion-core --example plugin_host -- install <file.ntplugin> <folder>
// cargo run -p neotime-companion-core --example plugin_host -- run <folder> <watch address>

use std::sync::{Arc, OnceLock};

use neotime_companion_core::ble::WatchBle;
use neotime_companion_core::plugins::host::{Fetcher, HostOutput, PluginHost};
use neotime_companion_core::plugins::http;
use neotime_companion_core::plugins::store::PluginStore;
use serde_json::Value;

struct Terminal {
    ble: Arc<OnceLock<WatchBle>>,
    // Plugins write from their own threads, which have no Tokio runtime of their own.
    runtime: tokio::runtime::Handle,
}

impl HostOutput for Terminal {
    fn write_to_watch(&self, line: String) {
        println!("to the watch: {}", line.trim_end());
        if let Some(ble) = self.ble.get().cloned() {
            self.runtime.spawn(async move {
                if let Err(error) = ble.write(&line).await {
                    println!("write failed: {error}");
                }
            });
        }
    }
    fn send_to_ui(&self, plugin: &str, message: Value) {
        println!("{plugin} to its page: {message}");
    }
    fn log(&self, plugin: &str, line: &str) {
        println!("{plugin}: {line}");
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let ble_cell = Arc::new(OnceLock::new());
    let fetcher: Fetcher = Arc::new(http::fetch);
    let terminal = Arc::new(Terminal {
        ble: ble_cell.clone(),
        runtime: tokio::runtime::Handle::current(),
    });
    match arguments
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["install", file, folder] => {
            let host = PluginHost::new(PluginStore::new(folder), terminal, fetcher);
            let status = host.install(&std::fs::read(file)?)?;
            println!(
                "installed {} {}, running: {}",
                status.installed.manifest.id, status.installed.manifest.version, status.running
            );
            host.stop_all();
        }
        ["run", folder, address] => {
            let host = Arc::new(PluginHost::new(PluginStore::new(folder), terminal, fetcher));
            host.start_all();
            let followed = host.clone();
            let ble = WatchBle::new(Arc::new(move |name, payload| match (name, payload.as_str()) {
                ("line", Some(text)) => followed.on_watch_text(text),
                ("state", Some(state)) => {
                    println!("watch {state}");
                    followed.on_connection(state == "connected");
                }
                _ => {}
            }));
            let _ = ble_cell.set(ble.clone());
            ble.connect(address).await?;
            println!("running {} plugin(s); press Ctrl+C to stop", host.list().len());
            tokio::signal::ctrl_c().await?;
            ble.disconnect().await?;
            host.stop_all();
        }
        _ => {
            eprintln!("usage: plugin_host install <file.ntplugin> <folder> | run <folder> <watch address>");
            std::process::exit(2);
        }
    }
    Ok(())
}
