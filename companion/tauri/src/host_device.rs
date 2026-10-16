// The computer as the daemon's device, over D-Bus; a phone's is phone_device.rs.

pub use desktop::DesktopDevice;

mod desktop {
    use std::sync::{Arc, Mutex, Weak};

    use battletime_companion_core::daemon::device::{
        CallCommand, Device, DeviceAnswer, DeviceKind, Location,
    };
    use battletime_companion_core::daemon::Daemon;
    use futures::future::BoxFuture;
    use serde_json::Value;
    use tauri::{AppHandle, Emitter};

    use crate::desktop::DesktopIntegration;
    use crate::desktop_events::DesktopEventTracker;
    use crate::picked_file;
    use crate::remote;

    // The computer playing the phone's part: its notifications, media players, Do Not Disturb and location.
    pub struct DesktopDevice {
        pub integration: DesktopIntegration,
        tracker: Arc<Mutex<DesktopEventTracker>>,
        app: AppHandle,
    }

    impl DesktopDevice {
        // Raw D-Bus facts go to the window as before, and as phone events to the daemon once it exists.
        pub fn new(app: &AppHandle, daemon: Arc<std::sync::OnceLock<Weak<Daemon>>>) -> DesktopDevice {
            let tracker = Arc::new(Mutex::new(DesktopEventTracker::default()));
            let (window, followed) = (app.clone(), tracker.clone());
            let integration = DesktopIntegration::new(Arc::new(move |raw: Value| {
                let _ = window.emit("desktop:event", raw.clone());
                let event = followed.lock().unwrap().handle(&raw);
                let daemon = daemon.get().and_then(Weak::upgrade);
                if let (Some(event), Some(daemon)) = (event, daemon) {
                    daemon.phone_event(event);
                }
            }));
            DesktopDevice {
                integration,
                tracker,
                app: app.clone(),
            }
        }
    }

    impl Device for DesktopDevice {
        fn kind(&self) -> DeviceKind {
            DeviceKind::Computer
        }

        fn is_do_not_disturb(&self) -> BoxFuture<'static, bool> {
            let integration = self.integration.clone();
            Box::pin(async move { integration.is_do_not_disturb().await })
        }

        fn media_command(&self, command: &str) -> DeviceAnswer<()> {
            let integration = self.integration.clone();
            let player = self.tracker.lock().unwrap().current_player.clone();
            let command = command.to_string();
            Box::pin(async move {
                let player = player.ok_or("No media player is running.")?;
                integration.media_command(&player, &command).await
            })
        }

        fn call_command(&self, _command: CallCommand) -> DeviceAnswer<()> {
            Box::pin(async { Err("A computer has no calls to answer.".to_string()) })
        }

        fn locate(&self) -> DeviceAnswer<Location> {
            let integration = self.integration.clone();
            Box::pin(async move {
                let found = integration.locate().await?;
                let name = found["description"]
                    .as_str()
                    .filter(|name| !name.is_empty())
                    .unwrap_or("Here");
                Ok(Location {
                    latitude: found["latitude"]
                        .as_f64()
                        .ok_or("The location service gave no latitude.")?,
                    longitude: found["longitude"]
                        .as_f64()
                        .ok_or("The location service gave no longitude.")?,
                    name: name.to_string(),
                })
            })
        }

        fn read_file(&self, uri: &str) -> DeviceAnswer<Vec<u8>> {
            let read = picked_file::read_file(&self.app, uri);
            Box::pin(async move { read })
        }

        // A shell command is the one action only the computer runs; see remote.rs.
        fn remote_action(&self, action: &Value) -> DeviceAnswer<Value> {
            let command = action["command"].as_str().map(str::to_string);
            Box::pin(async move {
                let command = command.ok_or("This computer runs only shell commands for the remote.")?;
                let result = remote::run_shell_command(&command, remote::COMMAND_TIMEOUT).await?;
                serde_json::to_value(result).map_err(|error| error.to_string())
            })
        }
    }
}
