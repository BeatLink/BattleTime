package com.battletime.companion.phonebridge

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.app.NotificationManager
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.telecom.TelecomManager
import androidx.core.content.ContextCompat

// A phone action that could not be done, with the reason the user sees.
class PhoneActionException(message: String) : Exception(message)

// What the phone does for the window's commands and the daemon alike, each from any context.
object PhoneActions {
  // ==========================================================================
  // Do Not Disturb and battery limits
  // ==========================================================================

  fun isDoNotDisturb(context: Context): Boolean {
    val manager = context.getSystemService(NotificationManager::class.java)
    val filter = manager?.currentInterruptionFilter ?: NotificationManager.INTERRUPTION_FILTER_UNKNOWN
    return filter != NotificationManager.INTERRUPTION_FILTER_ALL && filter != NotificationManager.INTERRUPTION_FILTER_UNKNOWN
  }

  fun isIgnoringBatteryOptimizations(context: Context): Boolean {
    val power = context.getSystemService(PowerManager::class.java) ?: return false
    return power.isIgnoringBatteryOptimizations(context.packageName)
  }

  // ==========================================================================
  // Calls and media
  // ==========================================================================

  @SuppressLint("MissingPermission")
  fun answerCall(context: Context) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
      throw PhoneActionException("Answering calls needs Android 8 or later")
    }
    @Suppress("DEPRECATION")
    telecom(context).acceptRingingCall()
  }

  @SuppressLint("MissingPermission")
  fun rejectCall(context: Context) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P) {
      throw PhoneActionException("Rejecting calls needs Android 9 or later")
    }
    @Suppress("DEPRECATION")
    telecom(context).endCall()
  }

  fun silenceCall(context: Context) {
    CallMonitor.silenceRinger(context)
  }

  fun mediaCommand(context: Context, command: String) {
    try {
      MediaTracker.command(context, command)
    } catch (e: IllegalArgumentException) {
      throw PhoneActionException(e.message ?: "Unknown media command")
    }
  }

  private fun telecom(context: Context): TelecomManager {
    if (!granted(context, Manifest.permission.ANSWER_PHONE_CALLS)) {
      throw PhoneActionException("Allow the app to answer calls first")
    }
    return context.getSystemService(TelecomManager::class.java)
      ?: throw PhoneActionException("This device has no telecom service")
  }

  // ==========================================================================
  // Remote button intents
  // ==========================================================================

  fun openUri(context: Context, uri: String) {
    launch(context, Intent(Intent.ACTION_VIEW, Uri.parse(uri)))
  }

  fun startActivity(context: Context, packageName: String, className: String, action: String?, extras: Map<String, String>) {
    val intent = Intent().setClassName(packageName, className)
    action?.let { intent.action = it }
    extras.forEach { (key, value) -> intent.putExtra(key, value) }
    launch(context, intent)
  }

  fun sendBroadcast(context: Context, action: String, packageName: String?, extras: Map<String, String>) {
    val intent = Intent(action)
    packageName?.let { intent.setPackage(it) }
    extras.forEach { (key, value) -> intent.putExtra(key, value) }
    context.sendBroadcast(intent)
  }

  // An activity started from a service needs a task of its own.
  private fun launch(context: Context, intent: Intent) {
    if (context !is Activity) {
      intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    }
    try {
      context.startActivity(intent)
    } catch (e: ActivityNotFoundException) {
      throw PhoneActionException("No app on this phone handles that: ${e.message}")
    } catch (e: SecurityException) {
      throw PhoneActionException("Android refused to start it: ${e.message}")
    }
  }

  fun granted(context: Context, permission: String): Boolean =
    ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED
}
