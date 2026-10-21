package com.battletime.companion.phonebridge

import android.content.Context
import android.media.AudioAttributes
import android.media.Ringtone
import android.media.RingtoneManager
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Base64
import android.util.Log
import org.json.JSONObject
import java.io.File

// The phone as the Rust daemon's device: the daemon asks it to do things through `request`, and hears the phone's events from it.
//
// The daemon lives in the process, not in the window, so the window's plugin and the keep-alive service
// both attach here and whichever comes first starts it. tauri/src/android.rs is the other side.
object DaemonHost {
  private const val TAG = "PhoneBridge"

  // How long a location fix may take; the Rust side gives up a little later.
  private const val LOCATE_TIMEOUT_MILLISECONDS = 30_000L

  // The longest the phone rings for the watch, in case the stop never arrives.
  private const val FIND_PHONE_MILLISECONDS = 120_000L

  private val handler = Handler(Looper.getMainLooper())

  @Volatile
  private var context: Context? = null
  private var forwarding = false
  private var ringtone: Ringtone? = null
  private val stopRinging = Runnable { ring(false) }

  private val forward: PhoneEventListener = { event -> NativeDaemon.phoneEvent(JSONObject(event).toString()) }

  // ==========================================================================
  // Starting
  // ==========================================================================

  // Safe to call from the window and the service alike; the first call starts handing the phone's events to the daemon.
  @Synchronized
  fun attach(context: Context) {
    val app = context.applicationContext
    this.context = app
    if (forwarding) {
      return
    }
    forwarding = true
    NativeDaemon.attach()
    PhoneEventBus.addListener(forward)
    CallMonitor.start(app)
  }

  // The folder Tauri gives the app's data on Android, where the daemon keeps its settings.
  fun dataDir(context: Context): String = context.applicationContext.dataDir.absolutePath

  // Starts Bluetooth and the daemon on the main thread, and gives the watch the daemon keeps, or null.
  fun startDaemon(context: Context): String? {
    attach(context)
    NativeBluetooth.start()?.let { Log.w(TAG, "Bluetooth could not start: $it") }
    val kept = NativeDaemon.start(dataDir(context))
    PhoneEventBus.lastMedia?.let(forward)
    return kept
  }

  // ==========================================================================
  // What the daemon asks for
  // ==========================================================================

  // Runs one of the daemon's requests, given and answered as JSON; a failure throws with the reason the user sees.
  @JvmStatic
  fun request(action: String, argument: String): String {
    val context = context ?: throw PhoneActionException("The phone bridge has not started")
    val input = JSONObject(argument)
    val answer: Any? = when (action) {
      "doNotDisturb" -> PhoneActions.isDoNotDisturb(context)
      "media" -> PhoneActions.mediaCommand(context, input.getString("command"))
      "answerCall" -> PhoneActions.answerCall(context)
      "rejectCall" -> PhoneActions.rejectCall(context)
      "silenceCall" -> PhoneActions.silenceCall(context)
      "locate" -> PhoneLocation.findNow(context, LOCATE_TIMEOUT_MILLISECONDS)
      "readFile" -> JSONObject().put("base64", Base64.encodeToString(readFile(context, input.getString("uri")), Base64.NO_WRAP))
      "keepRunning" -> KeepAliveService.keepRunning(context, if (input.isNull("watchName")) null else input.getString("watchName"))
      "findPhone" -> {
        val ringing = input.getBoolean("ringing")
        handler.post { ring(ringing) }
        null
      }
      "intent" -> startIntent(context, input.getJSONObject("intent"))
      "sosScreen" -> SosActivity.show(context, input.getBoolean("on"), input.getBoolean("record"))
      "callNumber" -> PhoneActions.callNumber(context, input.getString("number"))
      "dialNumber" -> PhoneActions.dialNumber(context, input.getString("number"))
      "sendText" -> PhoneActions.sendText(context, input.getString("number"), input.getString("text"))
      else -> throw PhoneActionException("The phone does not know how to $action")
    }
    return when (answer) {
      is Boolean, is JSONObject -> answer.toString()
      else -> "null"
    }
  }

  private fun readFile(context: Context, uri: String): ByteArray {
    try {
      if (uri.startsWith("/")) {
        return File(uri).readBytes()
      }
      val stream = context.contentResolver.openInputStream(Uri.parse(uri))
        ?: throw PhoneActionException("The note's file could not be opened")
      return stream.use { it.readBytes() }
    } catch (e: SecurityException) {
      throw PhoneActionException("Android no longer lets the app read the note's file; choose it again")
    } catch (e: java.io.IOException) {
      throw PhoneActionException("The note's file could not be read: ${e.message}")
    }
  }

  // A remote button's intent, as core/src/daemon/remote.rs describes it.
  private fun startIntent(context: Context, intent: JSONObject) {
    val extras = mutableMapOf<String, String>()
    intent.optJSONObject("extras")?.let { given -> given.keys().forEach { key -> extras[key] = given.getString(key) } }
    val optional = { key: String -> if (intent.isNull(key)) null else intent.getString(key) }
    when (intent.optString("type")) {
      "deeplink" -> PhoneActions.openUri(context, intent.getString("uri"))
      "activity" -> PhoneActions.startActivity(
        context,
        intent.getString("packageName"),
        intent.getString("className"),
        optional("action"),
        extras,
      )
      "broadcast" -> PhoneActions.sendBroadcast(context, intent.getString("action"), optional("packageName"), extras)
      else -> throw PhoneActionException("The phone cannot start that kind of intent")
    }
  }

  // ==========================================================================
  // Finding the phone
  // ==========================================================================

  // Rings the default ringtone as an alarm, so it sounds even with the ringer silenced; always runs on the main thread.
  private fun ring(ringing: Boolean) {
    handler.removeCallbacks(stopRinging)
    ringtone?.stop()
    ringtone = null
    val context = context ?: return
    if (!ringing) {
      return
    }
    val uri = RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE)
      ?: RingtoneManager.getDefaultUri(RingtoneManager.TYPE_ALARM)
    val tone = RingtoneManager.getRingtone(context, uri) ?: return
    tone.audioAttributes = AudioAttributes.Builder()
      .setUsage(AudioAttributes.USAGE_ALARM)
      .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
      .build()
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
      tone.isLooping = true
    }
    tone.play()
    ringtone = tone
    handler.postDelayed(stopRinging, FIND_PHONE_MILLISECONDS)
  }
}
