package com.waspos.companion.phonebridge

import android.Manifest
import android.annotation.SuppressLint
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import android.telecom.TelecomManager
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import expo.modules.interfaces.permissions.PermissionsResponseListener
import expo.modules.interfaces.permissions.PermissionsStatus
import expo.modules.kotlin.Promise
import expo.modules.kotlin.exception.CodedException
import expo.modules.kotlin.exception.Exceptions
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

private const val EVENT = "onPhoneEvent"

// The JavaScript face of the phone bridge: notification access, calls, media, Do Not Disturb and the keep-alive service.
class PhoneBridgeModule : Module() {
  private val forward: PhoneEventListener = { event -> sendEvent(EVENT, event) }

  private val context: Context
    get() = appContext.reactContext ?: throw Exceptions.ReactContextLost()

  override fun definition() = ModuleDefinition {
    Name("PhoneBridge")

    Events(EVENT)

    OnStartObserving(EVENT) {
      PhoneEventBus.addListener(forward)
      CallMonitor.start(context)
      PhoneEventBus.lastMedia?.let { sendEvent(EVENT, it) }
    }

    OnStopObserving(EVENT) {
      PhoneEventBus.removeListener(forward)
    }

    OnDestroy {
      PhoneEventBus.removeListener(forward)
    }

    AsyncFunction("hasNotificationAccess") {
      NotificationManagerCompat.getEnabledListenerPackages(context).contains(context.packageName)
    }

    AsyncFunction("openNotificationAccessSettings") {
      val intent = Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS)
      val activity = appContext.currentActivity
      if (activity != null) {
        activity.startActivity(intent)
      } else {
        context.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
      }
    }

    AsyncFunction("requestCallPermissions") { promise: Promise ->
      val permissions = appContext.permissions
      if (permissions == null) {
        promise.reject(CodedException("ERR_NO_PERMISSIONS", "The permissions manager is not available", null))
        return@AsyncFunction
      }
      permissions.askForPermissions(
        PermissionsResponseListener { result ->
          // Calls work with these two; the call log and contacts only add the number and the name.
          val essential = callPermissions().filter {
            it == Manifest.permission.READ_PHONE_STATE || it == Manifest.permission.ANSWER_PHONE_CALLS
          }
          val ok = essential.all { result[it]?.status == PermissionsStatus.GRANTED }
          CallMonitor.start(context)
          promise.resolve(ok)
        },
        *callPermissions().toTypedArray(),
      )
    }

    AsyncFunction("isDoNotDisturb") {
      val manager = context.getSystemService(NotificationManager::class.java)
      val filter = manager?.currentInterruptionFilter ?: NotificationManager.INTERRUPTION_FILTER_UNKNOWN
      filter != NotificationManager.INTERRUPTION_FILTER_ALL && filter != NotificationManager.INTERRUPTION_FILTER_UNKNOWN
    }

    AsyncFunction("answerCall") {
      answerCall()
    }

    AsyncFunction("rejectCall") {
      rejectCall()
    }

    AsyncFunction("mediaCommand") { command: String ->
      MediaTracker.command(context, command)
    }

    AsyncFunction("startKeepAlive") { watchName: String ->
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE &&
        !granted(Manifest.permission.BLUETOOTH_CONNECT)
      ) {
        // Android 14 refuses a connected-device service without it, and the refusal would crash the service.
        throw Exceptions.MissingPermissions(Manifest.permission.BLUETOOTH_CONNECT)
      }
      try {
        KeepAliveService.start(context, watchName)
      } catch (e: IllegalStateException) {
        throw CodedException("ERR_KEEP_ALIVE", "Android refused the foreground service: ${e.message}", e)
      }
    }

    AsyncFunction("stopKeepAlive") {
      KeepAliveService.stop(context)
    }
  }

  private fun callPermissions(): List<String> {
    val list = mutableListOf(
      Manifest.permission.READ_PHONE_STATE,
      Manifest.permission.READ_CALL_LOG,
      Manifest.permission.READ_CONTACTS,
    )
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      list.add(Manifest.permission.ANSWER_PHONE_CALLS)
    }
    return list
  }

  @SuppressLint("MissingPermission")
  private fun answerCall() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
      throw CodedException("ERR_UNSUPPORTED", "Answering calls needs Android 8 or later", null)
    }
    requirePermission(Manifest.permission.ANSWER_PHONE_CALLS)
    val telecom = context.getSystemService(TelecomManager::class.java)
      ?: throw CodedException("ERR_UNSUPPORTED", "This device has no telecom service", null)
    @Suppress("DEPRECATION")
    telecom.acceptRingingCall()
  }

  @SuppressLint("MissingPermission")
  private fun rejectCall() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P) {
      throw CodedException("ERR_UNSUPPORTED", "Rejecting calls needs Android 9 or later", null)
    }
    requirePermission(Manifest.permission.ANSWER_PHONE_CALLS)
    val telecom = context.getSystemService(TelecomManager::class.java)
      ?: throw CodedException("ERR_UNSUPPORTED", "This device has no telecom service", null)
    @Suppress("DEPRECATION")
    telecom.endCall()
  }

  private fun requirePermission(permission: String) {
    if (!granted(permission)) {
      throw Exceptions.MissingPermissions(permission)
    }
  }

  private fun granted(permission: String): Boolean =
    ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED
}
