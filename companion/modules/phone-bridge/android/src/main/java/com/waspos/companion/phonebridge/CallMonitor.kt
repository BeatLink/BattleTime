package com.waspos.companion.phonebridge

import android.Manifest
import android.annotation.SuppressLint
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.ContactsContract
import android.telephony.PhoneStateListener
import android.telephony.TelephonyCallback
import android.telephony.TelephonyManager
import android.util.Log
import androidx.annotation.RequiresApi
import androidx.core.content.ContextCompat
import java.util.concurrent.Executors

// Turns the phone's call state into incoming, outgoing, start and end events.
object CallMonitor {
  private const val TAG = "PhoneBridge"

  // How long a ringing call waits for its number, which can arrive separately and a little later.
  private const val NUMBER_WAIT_MS = 600L

  private val handler = Handler(Looper.getMainLooper())

  // Contact lookups run here so they never hold up the main thread, and events stay in order.
  private val worker = Executors.newSingleThreadExecutor()

  private var appContext: Context? = null
  private var telephonyCallback: Any? = null
  private var legacyListener: PhoneStateListener? = null
  private var numberReceiver: BroadcastReceiver? = null

  private var lastState = TelephonyManager.CALL_STATE_IDLE
  private var callNumber: String? = null
  private var pendingIncoming: Runnable? = null

  // Safe to call repeatedly; it registers whatever the granted permissions now allow.
  fun start(context: Context) {
    val app = context.applicationContext
    handler.post { register(app) }
  }

  // Each registration below is guarded by its own permission check, which lint cannot follow.
  @SuppressLint("MissingPermission")
  private fun register(context: Context) {
    appContext = context
    val telephony = context.getSystemService(TelephonyManager::class.java) ?: return
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
      if (telephonyCallback == null && granted(context, Manifest.permission.READ_PHONE_STATE)) {
        try {
          val callback = ModernCallback()
          telephony.registerTelephonyCallback(context.mainExecutor, callback)
          telephonyCallback = callback
        } catch (e: SecurityException) {
          Log.w(TAG, "Call state is not readable", e)
        }
      }
      if (numberReceiver == null && granted(context, Manifest.permission.READ_CALL_LOG)) {
        val receiver = NumberReceiver()
        ContextCompat.registerReceiver(
          context,
          receiver,
          IntentFilter(TelephonyManager.ACTION_PHONE_STATE_CHANGED),
          ContextCompat.RECEIVER_EXPORTED,
        )
        numberReceiver = receiver
      }
    } else if (legacyListener == null) {
      try {
        val listener = LegacyListener()
        @Suppress("DEPRECATION")
        telephony.listen(listener, PhoneStateListener.LISTEN_CALL_STATE)
        legacyListener = listener
      } catch (e: SecurityException) {
        Log.w(TAG, "Call state is not readable", e)
      }
    }
  }

  // Always runs on the main thread.
  private fun onState(state: Int, number: String?) {
    if (!number.isNullOrEmpty()) {
      callNumber = number
    }
    when (state) {
      TelephonyManager.CALL_STATE_RINGING -> if (lastState != TelephonyManager.CALL_STATE_RINGING) {
        if (callNumber != null) {
          emit("incoming")
        } else {
          val pending = Runnable {
            pendingIncoming = null
            emit("incoming")
          }
          pendingIncoming = pending
          handler.postDelayed(pending, NUMBER_WAIT_MS)
        }
      }
      TelephonyManager.CALL_STATE_OFFHOOK -> {
        flushIncoming()
        when (lastState) {
          TelephonyManager.CALL_STATE_RINGING -> emit("start")
          TelephonyManager.CALL_STATE_IDLE -> emit("outgoing")
        }
      }
      TelephonyManager.CALL_STATE_IDLE -> {
        flushIncoming()
        if (lastState != TelephonyManager.CALL_STATE_IDLE) {
          emit("end")
        }
        callNumber = null
      }
    }
    lastState = state
  }

  // The number for a ringing call, from the system broadcast; it can arrive before or after the state change.
  private fun onNumber(number: String) {
    callNumber = number
    val pending = pendingIncoming ?: return
    handler.removeCallbacks(pending)
    pendingIncoming = null
    emit("incoming")
  }

  // Sends a still-waiting incoming event now, so it is never lost or reordered.
  private fun flushIncoming() {
    val pending = pendingIncoming ?: return
    handler.removeCallbacks(pending)
    pendingIncoming = null
    emit("incoming")
  }

  private fun emit(state: String) {
    val number = callNumber
    val context = appContext
    worker.execute {
      val event = mutableMapOf<String, Any?>("type" to "call", "state" to state)
      if (number != null) {
        event["number"] = number
        if (context != null) {
          lookupName(context, number)?.let { event["name"] = it }
        }
      }
      PhoneEventBus.emit(event)
    }
  }

  private fun lookupName(context: Context, number: String): String? {
    if (!granted(context, Manifest.permission.READ_CONTACTS)) {
      return null
    }
    val uri = Uri.withAppendedPath(ContactsContract.PhoneLookup.CONTENT_FILTER_URI, Uri.encode(number))
    return try {
      context.contentResolver.query(uri, arrayOf(ContactsContract.PhoneLookup.DISPLAY_NAME), null, null, null)
        ?.use { cursor -> if (cursor.moveToFirst()) cursor.getString(0) else null }
    } catch (e: Exception) {
      Log.w(TAG, "Contact lookup failed", e)
      null
    }
  }

  private fun granted(context: Context, permission: String): Boolean =
    ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED

  @RequiresApi(Build.VERSION_CODES.S)
  private class ModernCallback : TelephonyCallback(), TelephonyCallback.CallStateListener {
    override fun onCallStateChanged(state: Int) {
      onState(state, null)
    }
  }

  @Suppress("DEPRECATION")
  private class LegacyListener : PhoneStateListener() {
    @Deprecated("Replaced by TelephonyCallback from Android 12")
    override fun onCallStateChanged(state: Int, phoneNumber: String?) {
      onState(state, phoneNumber)
    }
  }

  private class NumberReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
      if (intent.getStringExtra(TelephonyManager.EXTRA_STATE) != TelephonyManager.EXTRA_STATE_RINGING) {
        return
      }
      @Suppress("DEPRECATION")
      val number = intent.getStringExtra(TelephonyManager.EXTRA_INCOMING_NUMBER)
      if (!number.isNullOrEmpty()) {
        onNumber(number)
      }
    }
  }
}
