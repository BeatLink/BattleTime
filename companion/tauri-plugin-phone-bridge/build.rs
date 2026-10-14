// Builds the plugin's permissions and, for Android, gathers the Java and Kotlin its library compiles.

use std::path::{Path, PathBuf};
use std::process::Command;

// The Kotlin commands the window may call, in the snake_case the window uses.
const COMMANDS: &[&str] = &[
    "has_notification_access",
    "open_notification_access_settings",
    "request_call_permissions",
    "is_do_not_disturb",
    "answer_call",
    "reject_call",
    "silence_call",
    "media_command",
    "start_keep_alive",
    "stop_keep_alive",
    "locate",
    "open_uri",
    "start_activity",
    "send_broadcast",
    "register_listener",
    "remove_listener",
];

// The Expo module's glue, which the Tauri plugin replaces with its own.
const EXPO_ONLY_FILES: &[&str] = &["PhoneBridgeModule.kt", "WebRequest.kt"];

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        gather_android_sources();
    }
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}

// ============================================================================
// Android sources
// ============================================================================

// Copies the Expo module's services and btleplug's Java into android/.generated, which Gradle compiles.
fn gather_android_sources() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let generated = manifest_dir.join("android").join(".generated");
    let _ = std::fs::remove_dir_all(&generated);

    let expo_module = manifest_dir.join("../modules/phone-bridge/android/src/main");
    println!("cargo:rerun-if-changed={}", expo_module.display());
    copy_tree(
        &expo_module.join("java"),
        &generated.join("java"),
        EXPO_ONLY_FILES,
    );
    copy_tree(&expo_module.join("res"), &generated.join("res"), &[]);

    let btleplug_java = btleplug_dir().join("src/droidplug/java/src/main/java");
    copy_tree(&btleplug_java, &generated.join("java"), &[]);
}

// Finds btleplug's sources through cargo metadata, since its Java is not published anywhere else.
fn btleplug_dir() -> PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let manifest = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("Cargo.toml");
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--offline",
            "--manifest-path",
        ])
        .arg(&manifest)
        .output()
        .expect("cargo metadata could not run");
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("cargo metadata gave no JSON");
    let packages = metadata["packages"]
        .as_array()
        .expect("cargo metadata lists no packages");
    let btleplug = packages
        .iter()
        .find(|package| package["name"] == "btleplug")
        .expect("btleplug is not among the dependencies");
    let manifest_path = btleplug["manifest_path"].as_str().unwrap();
    Path::new(manifest_path).parent().unwrap().to_path_buf()
}

fn copy_tree(source: &Path, target: &Path, skipped_names: &[&str]) {
    for entry in std::fs::read_dir(source).unwrap_or_else(|error| panic!("{}: {error}", source.display())) {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if skipped_names.iter().any(|skipped| name == *skipped) {
            continue;
        }
        let from = entry.path();
        let to = target.join(&name);
        if from.is_dir() {
            copy_tree(&from, &to, skipped_names);
        } else {
            std::fs::create_dir_all(target).unwrap();
            std::fs::copy(&from, &to).unwrap();
        }
    }
}
