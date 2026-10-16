package com.battletime.companion.phonebridge

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.companion.CompanionDeviceManager
import android.content.Intent
import android.content.IntentSender
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.util.Log
import androidx.activity.result.ActivityResult
import androidx.activity.result.IntentSenderRequest
import androidx.appcompat.app.AppCompatActivity
import androidx.core.app.NotificationManagerCompat
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.json.JSONObject

// The event the window hears every phone event on.
private const val EVENT = "phone"

@InvokeArg
class MediaCommandArgs {
  lateinit var command: String
}

@InvokeArg
class AssociateWatchArgs {
  lateinit var address: String
}

@InvokeArg
class OpenUriArgs {
  lateinit var uri: String
}

@InvokeArg
class StartActivityArgs {
  lateinit var packageName: String
  lateinit var className: String
  var action: String? = null
  var extras: Map<String, String> = emptyMap()
}

@InvokeArg
class SendBroadcastArgs {
  lateinit var action: String
  var packageName: String? = null
  var extras: Map<String, String> = emptyMap()
}

// The phone bridge for the Tauri app: the notification, call and media services beside it, behind Tauri commands.
//
// The daemon does the phone's work itself through DaemonHost; these commands are what the window asks for.
@TauriPlugin(
  permissions = [
    Permission(strings = [Manifest.permission.BLUETOOTH_SCAN, Manifest.permission.BLUETOOTH_CONNECT], alias = "bluetooth"),
    // Android 12 ignores a request for fine location without coarse.
    Permission(strings = [Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION], alias = "location"),
    Permission(strings = [Manifest.permission.POST_NOTIFICATIONS], alias = "notifications"),
    Permission(
      strings = [
        Manifest.permission.READ_PHONE_STATE,
        Manifest.permission.READ_CALL_LOG,
        Manifest.permission.READ_CONTACTS,
        Manifest.permission.ANSWER_PHONE_CALLS,
      ],
      alias = "calls",
    ),
  ],
)
class PhoneBridgePlugin(private val activity: Activity) : Plugin(activity) {
  // The watch the companion device question is about, while it is open.
  private var associating: String? = null

  private val forward: PhoneEventListener = { event -> trigger(EVENT, toJs(event)) }

  init {
    // The constructor runs on the main thread, whose class loader sees btleplug's classes as its start needs.
    NativeBluetooth.start()?.let { Log.w("PhoneBridge", "Bluetooth could not start: $it") }
    DaemonHost.attach(activity)
    PhoneEventBus.addListener(forward)
  }

  override fun onDestroy(activity: AppCompatActivity) {
    PhoneEventBus.removeListener(forward)
  }

  // Follows calls once the window listens, and replays the media state it may have missed.
  @Command
  override fun registerListener(invoke: Invoke) {
    super.registerListener(invoke)
    CallMonitor.start(activity)
    PhoneEventBus.lastMedia?.let { trigger(EVENT, toJs(it)) }
  }

  // ==========================================================================
  // Permissions
  // ==========================================================================

  // Android 12 asks for nearby devices; older versions ask for location to scan.
  private fun bluetoothAliases(): Array<String> {
    val aliases = mutableListOf(if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) "bluetooth" else "location")
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      // The keep-alive notification shows only with this, so it is asked for alongside.
      aliases.add("notifications")
    }
    return aliases.toTypedArray()
  }

  private fun bluetoothGranted(): Boolean =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
      granted(Manifest.permission.BLUETOOTH_SCAN) && granted(Manifest.permission.BLUETOOTH_CONNECT)
    } else {
      granted(Manifest.permission.ACCESS_FINE_LOCATION)
    }

  @Command
  fun requestBluetoothPermissions(invoke: Invoke) {
    if (bluetoothGranted()) {
      invoke.resolve(JSObject().put("granted", true))
    } else {
      requestPermissionForAliases(bluetoothAliases(), invoke, "bluetoothPermissionsAnswered")
    }
  }

  @PermissionCallback
  fun bluetoothPermissionsAnswered(invoke: Invoke) {
    invoke.resolve(JSObject().put("granted", bluetoothGranted()))
  }

  @Command
  fun requestCallPermissions(invoke: Invoke) {
    requestPermissionForAlias("calls", invoke, "callPermissionsAnswered")
  }

  // Calls work with the phone state and answering; the call log and contacts only add the number and the name.
  @PermissionCallback
  fun callPermissionsAnswered(invoke: Invoke) {
    val essential = mutableListOf(Manifest.permission.READ_PHONE_STATE)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      essential.add(Manifest.permission.ANSWER_PHONE_CALLS)
    }
    CallMonitor.start(activity)
    invoke.resolveObject(essential.all { granted(it) })
  }

  // ==========================================================================
  // Notifications and Do Not Disturb
  // ==========================================================================

  @Command
  fun hasNotificationAccess(invoke: Invoke) {
    invoke.resolveObject(NotificationManagerCompat.getEnabledListenerPackages(activity).contains(activity.packageName))
  }

  @Command
  fun openNotificationAccessSettings(invoke: Invoke) {
    activity.startActivity(Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS))
    invoke.resolve()
  }

  @Command
  fun isDoNotDisturb(invoke: Invoke) {
    invoke.resolveObject(PhoneActions.isDoNotDisturb(activity))
  }

  // ==========================================================================
  // Running in the background
  // ==========================================================================

  @Command
  fun isIgnoringBatteryOptimizations(invoke: Invoke) {
    invoke.resolveObject(PhoneActions.isIgnoringBatteryOptimizations(activity))
  }

  // Answers whether the app may now run without battery limits, once the user has closed Android's question.
  @SuppressLint("BatteryLife")
  @Command
  fun requestIgnoreBatteryOptimizations(invoke: Invoke) {
    if (PhoneActions.isIgnoringBatteryOptimizations(activity)) {
      return invoke.resolveObject(true)
    }
    val intent = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:${activity.packageName}"))
    try {
      startActivityForResult(invoke, intent, "batteryAnswered")
    } catch (e: Exception) {
      invoke.reject("Android could not ask about battery limits: ${e.message}")
    }
  }

  @Suppress("UNUSED_PARAMETER")
  @ActivityCallback
  fun batteryAnswered(invoke: Invoke, result: ActivityResult) {
    invoke.resolveObject(PhoneActions.isIgnoringBatteryOptimizations(activity))
  }

  // Makes the watch this app's companion device, asking the user once, and answers whether it is one now.
  @Command
  fun associateWatch(invoke: Invoke) {
    val args = invoke.parseArgs(AssociateWatchArgs::class.java)
    val manager = WatchCompanion.manager(activity) ?: return invoke.resolveObject(false)
    if (WatchCompanion.isAssociated(manager, args.address)) {
      WatchCompanion.observe(manager, args.address)
      return invoke.resolveObject(true)
    }
    val request = try {
      WatchCompanion.request(args.address)
    } catch (e: IllegalArgumentException) {
      return invoke.reject("Android cannot look for that watch: ${e.message}")
    }
    associating = args.address
    manager.associate(
      request,
      object : CompanionDeviceManager.Callback() {
        @Deprecated("Android 13 calls onAssociationPending, which reaches this one")
        override fun onDeviceFound(chooserLauncher: IntentSender) {
          startIntentSenderForResult(invoke, IntentSenderRequest.Builder(chooserLauncher).build(), "associationAnswered")
        }

        override fun onFailure(error: CharSequence?) {
          invoke.reject(error?.toString() ?: "Android could not make the watch a companion device")
        }
      },
      null,
    )
  }

  @ActivityCallback
  fun associationAnswered(invoke: Invoke, result: ActivityResult) {
    val manager = WatchCompanion.manager(activity)
    val address = associating
    val associated = result.resultCode == Activity.RESULT_OK && manager != null && address != null
    if (associated) {
      WatchCompanion.observe(manager!!, address!!)
    }
    invoke.resolveObject(associated)
  }

  // ==========================================================================
  // Calls
  // ==========================================================================

  @Command
  fun answerCall(invoke: Invoke) = act(invoke) { PhoneActions.answerCall(activity) }

  @Command
  fun rejectCall(invoke: Invoke) = act(invoke) { PhoneActions.rejectCall(activity) }

  @Command
  fun silenceCall(invoke: Invoke) = act(invoke) { PhoneActions.silenceCall(activity) }

  // ==========================================================================
  // Media
  // ==========================================================================

  @Command
  fun mediaCommand(invoke: Invoke) {
    val args = invoke.parseArgs(MediaCommandArgs::class.java)
    act(invoke) { PhoneActions.mediaCommand(activity, args.command) }
  }

  // ==========================================================================
  // Location
  // ==========================================================================

  // The weather needs only the town, so an approximate location is enough.
  @Command
  fun locate(invoke: Invoke) {
    if (granted(Manifest.permission.ACCESS_COARSE_LOCATION)) {
      findLocation(invoke)
    } else {
      requestPermissionForAlias("location", invoke, "locationPermissionAnswered")
    }
  }

  @PermissionCallback
  fun locationPermissionAnswered(invoke: Invoke) {
    findLocation(invoke)
  }

  private fun findLocation(invoke: Invoke) {
    PhoneLocation.find(activity) { place, error ->
      if (place != null) invoke.resolve(JSObject(place.toString())) else invoke.reject(error ?: "This phone does not know where it is yet")
    }
  }

  // ==========================================================================
  // Remote button intents
  // ==========================================================================

  @Command
  fun openUri(invoke: Invoke) {
    val args = invoke.parseArgs(OpenUriArgs::class.java)
    act(invoke) { PhoneActions.openUri(activity, args.uri) }
  }

  @Command
  fun startActivity(invoke: Invoke) {
    val args = invoke.parseArgs(StartActivityArgs::class.java)
    act(invoke) { PhoneActions.startActivity(activity, args.packageName, args.className, args.action, args.extras) }
  }

  @Command
  fun sendBroadcast(invoke: Invoke) {
    val args = invoke.parseArgs(SendBroadcastArgs::class.java)
    act(invoke) { PhoneActions.sendBroadcast(activity, args.action, args.packageName, args.extras) }
  }

  // Resolves once the action is done, or rejects with the reason it could not be.
  private fun act(invoke: Invoke, action: () -> Unit) {
    try {
      action()
      invoke.resolve()
    } catch (e: PhoneActionException) {
      invoke.reject(e.message)
    }
  }

  private fun granted(permission: String): Boolean = PhoneActions.granted(activity, permission)
}

// Turns a bus event into the JSON the window reads, keeping nulls out.
internal fun toJs(event: Map<String, Any?>): JSObject {
  val result = JSObject()
  for ((key, value) in event) {
    if (value != null) {
      result.put(key, if (value is Map<*, *>) JSONObject(value) else value)
    }
  }
  return result
}
