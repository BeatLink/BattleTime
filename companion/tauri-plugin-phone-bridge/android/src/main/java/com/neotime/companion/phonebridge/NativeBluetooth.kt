package com.neotime.companion.phonebridge

// The Rust side of Bluetooth, in the app's own library, which Tauri has loaded before any plugin.
object NativeBluetooth {
  // Gives null once btleplug is ready, or the reason it could not start.
  @JvmStatic
  external fun start(): String?
}
