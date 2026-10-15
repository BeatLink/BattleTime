package com.battletime.companion.phonebridge

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.app.NotificationManager
import android.content.ActivityNotFoundException
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.telecom.TelecomManager
import android.util.Log
import androidx.appcompat.app.AppCompatActivity
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.json.JSONObject

private const val TAG = "PhoneBridge"

// The event the window hears every phone event on.
private const val EVENT = "phone"

@InvokeArg
class MediaCommandArgs {
  lateinit var command: String
}

@InvokeArg
class KeepAliveArgs {
  lateinit var watchName: String
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
  private val forward: PhoneEventListener = { event -> trigger(EVENT, toJs(event)) }

  init {
    // The constructor runs on the main thread, whose class loader sees btleplug's classes as its start needs.
    NativeBluetooth.start()?.let { Log.w(TAG, "Bluetooth could not start: $it") }
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
    val manager = activity.getSystemService(NotificationManager::class.java)
    val filter = manager?.currentInterruptionFilter ?: NotificationManager.INTERRUPTION_FILTER_UNKNOWN
    invoke.resolveObject(
      filter != NotificationManager.INTERRUPTION_FILTER_ALL && filter != NotificationManager.INTERRUPTION_FILTER_UNKNOWN,
    )
  }

  // ==========================================================================
  // Calls
  // ==========================================================================

  @SuppressLint("MissingPermission")
  @Command
  fun answerCall(invoke: Invoke) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
      return invoke.reject("Answering calls needs Android 8 or later")
    }
    val telecom = telecomOrReject(invoke) ?: return
    @Suppress("DEPRECATION")
    telecom.acceptRingingCall()
    invoke.resolve()
  }

  @SuppressLint("MissingPermission")
  @Command
  fun rejectCall(invoke: Invoke) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P) {
      return invoke.reject("Rejecting calls needs Android 9 or later")
    }
    val telecom = telecomOrReject(invoke) ?: return
    @Suppress("DEPRECATION")
    telecom.endCall()
    invoke.resolve()
  }

  @Command
  fun silenceCall(invoke: Invoke) {
    CallMonitor.silenceRinger(activity)
    invoke.resolve()
  }

  private fun telecomOrReject(invoke: Invoke): TelecomManager? {
    if (!granted(Manifest.permission.ANSWER_PHONE_CALLS)) {
      invoke.reject("Allow the app to answer calls first")
      return null
    }
    val telecom = activity.getSystemService(TelecomManager::class.java)
    if (telecom == null) {
      invoke.reject("This device has no telecom service")
    }
    return telecom
  }

  // ==========================================================================
  // Media and the keep-alive service
  // ==========================================================================

  @Command
  fun mediaCommand(invoke: Invoke) {
    val args = invoke.parseArgs(MediaCommandArgs::class.java)
    MediaTracker.command(activity, args.command)
    invoke.resolve()
  }

  @Command
  fun startKeepAlive(invoke: Invoke) {
    val args = invoke.parseArgs(KeepAliveArgs::class.java)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE && !granted(Manifest.permission.BLUETOOTH_CONNECT)) {
      // Android 14 refuses a connected-device service without it, and the refusal would crash the service.
      return invoke.reject("Allow the app to connect to nearby devices first")
    }
    try {
      KeepAliveService.start(activity, args.watchName)
      invoke.resolve()
    } catch (e: IllegalStateException) {
      invoke.reject("Android refused the foreground service: ${e.message}")
    }
  }

  @Command
  fun stopKeepAlive(invoke: Invoke) {
    KeepAliveService.stop(activity)
    invoke.resolve()
  }

  // ==========================================================================
  // Location
  // ==========================================================================

  // The weather needs only the town, so an approximate location is enough.
  @Command
  fun locate(invoke: Invoke) {
    if (granted(Manifest.permission.ACCESS_COARSE_LOCATION)) {
      PhoneLocation.find(activity, invoke)
    } else {
      requestPermissionForAlias("location", invoke, "locationPermissionAnswered")
    }
  }

  @PermissionCallback
  fun locationPermissionAnswered(invoke: Invoke) {
    if (granted(Manifest.permission.ACCESS_COARSE_LOCATION)) {
      PhoneLocation.find(activity, invoke)
    } else {
      invoke.reject("Allow the app to use this phone's location first")
    }
  }

  // ==========================================================================
  // Remote button intents
  // ==========================================================================

  @Command
  fun openUri(invoke: Invoke) {
    val args = invoke.parseArgs(OpenUriArgs::class.java)
    launch(invoke, Intent(Intent.ACTION_VIEW, Uri.parse(args.uri)))
  }

  @Command
  fun startActivity(invoke: Invoke) {
    val args = invoke.parseArgs(StartActivityArgs::class.java)
    val intent = Intent().setClassName(args.packageName, args.className)
    args.action?.let { intent.action = it }
    args.extras.forEach { (key, value) -> intent.putExtra(key, value) }
    launch(invoke, intent)
  }

  @Command
  fun sendBroadcast(invoke: Invoke) {
    val args = invoke.parseArgs(SendBroadcastArgs::class.java)
    val intent = Intent(args.action)
    args.packageName?.let { intent.setPackage(it) }
    args.extras.forEach { (key, value) -> intent.putExtra(key, value) }
    activity.sendBroadcast(intent)
    invoke.resolve()
  }

  private fun launch(invoke: Invoke, intent: Intent) {
    try {
      activity.startActivity(intent)
      invoke.resolve()
    } catch (e: ActivityNotFoundException) {
      invoke.reject("No app on this phone handles that: ${e.message}")
    } catch (e: SecurityException) {
      invoke.reject("Android refused to start it: ${e.message}")
    }
  }

  private fun granted(permission: String): Boolean =
    ContextCompat.checkSelfPermission(activity, permission) == PackageManager.PERMISSION_GRANTED
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
