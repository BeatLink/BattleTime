// The daemon on Android: the phone bridge's Kotlin reaches it through these natives, and it reaches the phone through DaemonHost.kt.
//
// The keep-alive service starts the daemon here when no window is open, and the window's setup finds the
// same one in `DAEMON`; the plugin and the service both call `attach` first, so the daemon can reach
// DaemonHost from its own threads.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use battletime_companion_core::daemon::device::PhoneEvent;
use jni::errors::{Error, ThrowRuntimeExAndDefault};
use jni::objects::{JClass, JString, JValue};
use jni::refs::Global;
use jni::{jni_sig, jni_str, Env, EnvUnowned, JavaVM};
use serde_json::{json, Value};

use crate::phone_device::PhoneDevice;

// ============================================================================
// Reaching DaemonHost
// ============================================================================

// DaemonHost's class, found from a Kotlin call, since the daemon's own threads cannot see the app's classes.
static HOST_CLASS: OnceLock<Global<JClass<'static>>> = OnceLock::new();

// The daemon's latest wish to keep running, sent again once DaemonHost is reachable in case it came first.
static KEEP_RUNNING_WISH: Mutex<Option<Value>> = Mutex::new(None);

// The phone as the daemon sees it.
pub fn phone_device() -> PhoneDevice {
    PhoneDevice::new(Arc::new(ask_host))
}

fn ask_host(action: &str, argument: &Value) -> Result<Value, String> {
    if action == "keepRunning" {
        *KEEP_RUNNING_WISH.lock().unwrap() = Some(argument.clone());
    }
    let class = HOST_CLASS.get().ok_or("The phone bridge has not started yet.")?;
    let machine = JavaVM::singleton().map_err(|error| error.to_string())?;
    let answer = machine
        .attach_current_thread(|env| -> jni::errors::Result<String> {
            let action = env.new_string(action)?;
            let argument = env.new_string(argument.to_string())?;
            let answer = env
                .call_static_method(
                    class,
                    jni_str!("request"),
                    jni_sig!("(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;"),
                    &[JValue::from(&action), JValue::from(&argument)],
                )?
                .l()?;
            env.cast_local::<JString>(answer)?.try_to_string(env)
        })
        .map_err(failure_reason)?;
    serde_json::from_str(&answer).map_err(|error| error.to_string())
}

// The reason a Kotlin exception gave, which is written for the user.
fn failure_reason(error: Error) -> String {
    match error {
        Error::CaughtJavaException { msg, .. } => msg,
        other => other.to_string(),
    }
}

fn remember_host(env: &mut Env) -> jni::errors::Result<()> {
    if HOST_CLASS.get().is_some() {
        return Ok(());
    }
    let class = env.find_class(jni_str!("com/battletime/companion/phonebridge/DaemonHost"))?;
    let _ = HOST_CLASS.set(env.new_global_ref(&class)?);
    if let Some(wish) = KEEP_RUNNING_WISH.lock().unwrap().clone() {
        std::thread::spawn(move || ask_host("keepRunning", &wish));
    }
    Ok(())
}

// ============================================================================
// What the Kotlin calls
// ============================================================================

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_battletime_companion_phonebridge_NativeDaemon_attach<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
) {
    unowned_env
        .with_env(|env| -> jni::errors::Result<()> { remember_host(env) })
        .resolve::<ThrowRuntimeExAndDefault>()
}

// Starts the daemon unless the window has, and answers the watch it keeps the app running for, or null.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_battletime_companion_phonebridge_NativeDaemon_start<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    data_dir: JString<'caller>,
) -> JString<'caller> {
    unowned_env
        .with_env(|env| -> jni::errors::Result<JString<'caller>> {
            remember_host(env)?;
            let data_dir = PathBuf::from(data_dir.try_to_string(env)?);
            let daemon = crate::process_daemon(data_dir, Arc::new(phone_device()));
            match daemon.kept_running_for() {
                Some(name) => env.new_string(name),
                None => Ok(JString::null()),
            }
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_battletime_companion_phonebridge_NativeDaemon_phoneEvent<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    event: JString<'caller>,
) {
    unowned_env
        .with_env(|env| -> jni::errors::Result<()> {
            let event = event.try_to_string(env)?;
            let parsed = serde_json::from_str::<Value>(&event).unwrap_or(json!(null));
            if let (Some(daemon), Some(event)) = (crate::DAEMON.get(), PhoneEvent::from_value(&parsed)) {
                daemon.phone_event(event);
            }
            Ok(())
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}
