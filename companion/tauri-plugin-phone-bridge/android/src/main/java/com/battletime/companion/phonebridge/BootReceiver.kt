package com.battletime.companion.phonebridge

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import org.json.JSONObject
import java.io.File

// Starts the keep-alive service after the phone boots or the app is updated, when the daemon remembers a watch to reconnect to.
class BootReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    if (intent.action != Intent.ACTION_BOOT_COMPLETED && intent.action != Intent.ACTION_MY_PACKAGE_REPLACED) {
      return
    }
    if (remembersWatch(context)) {
      KeepAliveService.start(context)
    }
  }

  companion object {
    private const val TAG = "PhoneBridge"

    // Reads the daemon's own settings file, core/src/daemon/settings.rs, for a watch it reconnects to.
    fun remembersWatch(context: Context): Boolean =
      try {
        val file = File(DaemonHost.dataDir(context), "companion/settings.json")
        val settings = JSONObject(file.readText())
        val watch = if (settings.isNull("lastWatchId")) "" else settings.optString("lastWatchId")
        watch.isNotEmpty() && settings.optBoolean("autoReconnect", true)
      } catch (e: Exception) {
        Log.i(TAG, "No remembered watch: ${e.message}")
        false
      }
  }
}
