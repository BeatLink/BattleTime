// The phone bridge as a Tauri plugin: the Kotlin in android/ answers the window's phone calls on Android.
//
// The window calls the Kotlin commands directly as `plugin:phone-bridge|<command>`, and hears phone
// events through the plugin's `phone` event. On other platforms the plugin registers nothing.

#[cfg(target_os = "android")]
mod bluetooth;

use serde::Deserialize;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

// The Kotlin side's package and class.
#[cfg(target_os = "android")]
const PLUGIN_PACKAGE: &str = "com.neotime.companion.phonebridge";
#[cfg(target_os = "android")]
const PLUGIN_CLASS: &str = "PhoneBridgePlugin";

// What the Kotlin side answers when asked for Bluetooth permissions.
#[derive(Debug, Deserialize, PartialEq)]
pub struct PermissionAnswer {
    pub granted: bool,
}

// The app's handle on the Kotlin plugin, for the parts the Rust side asks for itself.
pub struct PhoneBridge<R: Runtime> {
    #[cfg(target_os = "android")]
    handle: tauri::plugin::PluginHandle<R>,
    #[cfg(not(target_os = "android"))]
    _runtime: std::marker::PhantomData<fn() -> R>,
}

impl<R: Runtime> PhoneBridge<R> {
    // Asks for the permissions scanning and connecting need, and says whether they were granted.
    #[cfg(target_os = "android")]
    pub async fn request_bluetooth_permissions(&self) -> Result<bool, String> {
        self.handle
            .run_mobile_plugin_async::<PermissionAnswer>("requestBluetoothPermissions", ())
            .await
            .map(|answer| answer.granted)
            .map_err(|error| error.to_string())
    }

    // Anywhere but Android, Bluetooth needs no permission from the app.
    #[cfg(not(target_os = "android"))]
    pub async fn request_bluetooth_permissions(&self) -> Result<bool, String> {
        Ok(true)
    }
}

// Lets any part of the app reach the plugin through its handle.
pub trait PhoneBridgeExt<R: Runtime> {
    fn phone_bridge(&self) -> &PhoneBridge<R>;
}

impl<R: Runtime, T: Manager<R>> PhoneBridgeExt<R> for T {
    fn phone_bridge(&self) -> &PhoneBridge<R> {
        self.state::<PhoneBridge<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("phone-bridge")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            let bridge = PhoneBridge {
                handle: api.register_android_plugin(PLUGIN_PACKAGE, PLUGIN_CLASS)?,
            };
            #[cfg(not(target_os = "android"))]
            let bridge = {
                let _ = api;
                PhoneBridge::<R> {
                    _runtime: std::marker::PhantomData,
                }
            };
            app.manage(bridge);
            Ok(())
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_permission_answer_reads_the_kotlin_reply() {
        let answer: PermissionAnswer = serde_json::from_str(r#"{"granted":true}"#).unwrap();
        assert_eq!(answer, PermissionAnswer { granted: true });
    }

    #[test]
    fn bluetooth_needs_no_permission_off_android() {
        let app = tauri::test::mock_builder()
            .plugin(init())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let granted = tauri::async_runtime::block_on(app.phone_bridge().request_bluetooth_permissions());
        assert_eq!(granted, Ok(true));
    }
}
