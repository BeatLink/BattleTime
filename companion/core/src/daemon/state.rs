// What the daemon tells a window about the watch and its own work, sent whole whenever any of it changes.

use serde::Serialize;
use serde_json::{Map, Value};

use super::device::Media;
use crate::protocol::battletime::Capabilities;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WatchRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonState {
    // disconnected, connecting or connected.
    pub connection: String,
    // application or bootloader.
    pub mode: String,
    pub watch: Option<WatchRef>,
    pub capabilities: Capabilities,
    // The watch's last status reply: battery, millivolts, charging and steps.
    pub status: Option<Value>,
    // The Battery service's level, for a watch without the UART channel.
    pub battery_level: Option<u8>,
    // When the status or battery was last read, in milliseconds since 1970.
    pub last_synced: Option<i64>,
    pub now_playing: Option<Media>,
    pub find_phone: bool,
    pub last_error: Option<String>,
    // True while the daemon is trying to get the remembered watch back on its own.
    pub reconnecting: bool,
    // What each background job last reported, such as the note's and the remote layout's status.
    pub features: Map<String, Value>,
}

impl Default for DaemonState {
    fn default() -> Self {
        DaemonState {
            connection: "disconnected".into(),
            mode: "application".into(),
            watch: None,
            capabilities: Capabilities::default(),
            status: None,
            battery_level: None,
            last_synced: None,
            now_playing: None,
            find_phone: false,
            last_error: None,
            reconnecting: false,
            features: Map::new(),
        }
    }
}
