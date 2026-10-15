// Starts btleplug's Android side, which the Kotlin plugin calls as it is made.
//
// btleplug looks up its Java classes once, from a thread that sees the app's class loader; Tauri
// makes the plugin on the main thread, which does.

use jni::objects::{JClass, JString};
use jni::EnvUnowned;

// Answers null once btleplug is ready, or the reason it could not start.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_battletime_companion_phonebridge_NativeBluetooth_start<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
) -> JString<'caller> {
    unowned_env
        .with_env(|env| -> jni::errors::Result<JString<'caller>> {
            match btleplug::platform::init(env) {
                Ok(()) => Ok(JString::null()),
                Err(error) => env.new_string(error.to_string()),
            }
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}
