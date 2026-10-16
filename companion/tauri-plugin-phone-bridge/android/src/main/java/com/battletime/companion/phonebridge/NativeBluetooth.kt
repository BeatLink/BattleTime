package com.battletime.companion.phonebridge

// The Rust side of Bluetooth, in the app's own library.
object NativeBluetooth {
  init {
    NativeDaemon.loadLibrary()
  }

  // Gives null once btleplug is ready, or the reason it could not start; it must run on the main thread.
  @JvmStatic
  external fun start(): String?
}
