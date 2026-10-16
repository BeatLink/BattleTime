package com.battletime.companion.phonebridge

// The Rust daemon in the app's own library (tauri/src/android.rs), which the service and the window share.
object NativeDaemon {
  // The app's Rust library, which the service loads itself when it starts without the window.
  private const val LIBRARY = "battletime_companion_lib"

  init {
    loadLibrary()
  }

  fun loadLibrary() {
    System.loadLibrary(LIBRARY)
  }

  // Lets the Rust side reach DaemonHost, which it calls for everything the phone does.
  @JvmStatic
  external fun attach()

  // Starts the daemon unless the window already has, and gives the watch it keeps the app running for, or null.
  @JvmStatic
  external fun start(dataDir: String): String?

  // Hands the daemon one phone event, as the JSON the PhoneEventBus carries.
  @JvmStatic
  external fun phoneEvent(event: String)
}
