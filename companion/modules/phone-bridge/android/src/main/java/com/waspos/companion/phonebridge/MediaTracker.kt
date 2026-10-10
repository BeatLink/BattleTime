package com.waspos.companion.phonebridge

import android.content.ComponentName
import android.content.Context
import android.media.AudioManager
import android.media.MediaMetadata
import android.media.session.MediaController
import android.media.session.MediaSessionManager
import android.media.session.PlaybackState
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import android.view.KeyEvent
import kotlin.math.abs

// Follows the media session the user is most likely listening to and reports what it plays.
object MediaTracker {
  private const val TAG = "PhoneBridge"

  // A position this far from where playback should be counts as a seek and is reported.
  private const val SEEK_THRESHOLD_MS = 3000L

  private val handler = Handler(Looper.getMainLooper())
  private var manager: MediaSessionManager? = null
  private var listenerComponent: ComponentName? = null

  @Volatile
  private var controller: MediaController? = null

  private var lastKey: List<Any?>? = null
  private var lastPositionMs = -1L
  private var lastReportedAt = 0L
  private var lastPlaying = false

  private val sessionsListener = MediaSessionManager.OnActiveSessionsChangedListener { controllers ->
    handler.post { select(controllers.orEmpty()) }
  }

  private val callback = object : MediaController.Callback() {
    override fun onMetadataChanged(metadata: MediaMetadata?) {
      publish()
    }

    override fun onPlaybackStateChanged(state: PlaybackState?) {
      publish()
    }

    override fun onSessionDestroyed() {
      select(activeSessions())
    }
  }

  // Needs the notification listener to be enabled, because the system only shares media sessions with one.
  fun start(context: Context) {
    handler.post {
      if (manager != null) {
        return@post
      }
      val appContext = context.applicationContext
      val sessions = appContext.getSystemService(MediaSessionManager::class.java) ?: return@post
      val component = ComponentName(appContext, PhoneNotificationListener::class.java)
      try {
        sessions.addOnActiveSessionsChangedListener(sessionsListener, component, handler)
        manager = sessions
        listenerComponent = component
        select(sessions.getActiveSessions(component))
      } catch (e: SecurityException) {
        Log.w(TAG, "Media sessions are not readable without notification access", e)
      }
    }
  }

  fun stop() {
    handler.post {
      manager?.removeOnActiveSessionsChangedListener(sessionsListener)
      manager = null
      listenerComponent = null
      attach(null)
      publish()
    }
  }

  fun command(context: Context, command: String) {
    val audio = context.getSystemService(AudioManager::class.java)
    val current = controller
    when (command) {
      "volumeup", "volumedown" -> {
        val direction = if (command == "volumeup") AudioManager.ADJUST_RAISE else AudioManager.ADJUST_LOWER
        val remote = current?.playbackInfo?.playbackType == MediaController.PlaybackInfo.PLAYBACK_TYPE_REMOTE
        if (remote) {
          current?.adjustVolume(direction, 0)
        } else {
          audio?.adjustStreamVolume(AudioManager.STREAM_MUSIC, direction, AudioManager.FLAG_SHOW_UI)
        }
      }
      "play", "pause", "next", "previous" -> {
        val controls = current?.transportControls
        if (controls != null) {
          when (command) {
            "play" -> controls.play()
            "pause" -> controls.pause()
            "next" -> controls.skipToNext()
            else -> controls.skipToPrevious()
          }
        } else {
          // With no session to address, a media key reaches whichever player the system picks.
          val key = when (command) {
            "play" -> KeyEvent.KEYCODE_MEDIA_PLAY
            "pause" -> KeyEvent.KEYCODE_MEDIA_PAUSE
            "next" -> KeyEvent.KEYCODE_MEDIA_NEXT
            else -> KeyEvent.KEYCODE_MEDIA_PREVIOUS
          }
          audio?.dispatchMediaKeyEvent(KeyEvent(KeyEvent.ACTION_DOWN, key))
          audio?.dispatchMediaKeyEvent(KeyEvent(KeyEvent.ACTION_UP, key))
        }
      }
      else -> throw IllegalArgumentException("Unknown media command: $command")
    }
  }

  private fun activeSessions(): List<MediaController> {
    val sessions = manager ?: return emptyList()
    return try {
      sessions.getActiveSessions(listenerComponent).orEmpty()
    } catch (e: SecurityException) {
      emptyList()
    }
  }

  // Prefers a session that is playing; the system lists the rest by priority.
  private fun select(controllers: List<MediaController>) {
    val playing = controllers.firstOrNull { it.playbackState?.state == PlaybackState.STATE_PLAYING }
    val chosen = playing ?: controllers.firstOrNull()
    if (chosen?.sessionToken != controller?.sessionToken) {
      attach(chosen)
    }
    publish()
  }

  private fun attach(next: MediaController?) {
    controller?.unregisterCallback(callback)
    controller = next
    next?.registerCallback(callback, handler)
    lastKey = null
  }

  private fun publish() {
    val current = controller
    if (current == null) {
      if (lastKey != emptyList<Any?>()) {
        lastKey = emptyList<Any?>()
        PhoneEventBus.emit(mapOf("type" to "media", "playing" to false))
      }
      return
    }
    val state = current.playbackState
    val metadata = current.metadata
    val playing = state?.state == PlaybackState.STATE_PLAYING
    val artist = metadata?.getString(MediaMetadata.METADATA_KEY_ARTIST)
      ?: metadata?.getString(MediaMetadata.METADATA_KEY_ALBUM_ARTIST)
    val album = metadata?.getString(MediaMetadata.METADATA_KEY_ALBUM)
    val track = metadata?.getString(MediaMetadata.METADATA_KEY_TITLE)
      ?: metadata?.getString(MediaMetadata.METADATA_KEY_DISPLAY_TITLE)
    val durationMs = metadata?.getLong(MediaMetadata.METADATA_KEY_DURATION) ?: 0L
    val positionMs = positionOf(state, playing)

    val key = listOf(current.packageName, playing, artist, album, track, durationMs)
    val now = SystemClock.elapsedRealtime()
    if (key == lastKey && !seeked(positionMs, now)) {
      return
    }
    lastKey = key
    lastPositionMs = positionMs
    lastReportedAt = now
    lastPlaying = playing

    val event = mutableMapOf<String, Any?>("type" to "media", "playing" to playing)
    artist?.let { event["artist"] = it }
    album?.let { event["album"] = it }
    track?.let { event["track"] = it }
    if (durationMs > 0) {
      event["durationSec"] = (durationMs / 1000).toInt()
    }
    if (positionMs >= 0) {
      event["positionSec"] = (positionMs / 1000).toInt()
    }
    PhoneEventBus.emit(event)
  }

  // Where playback is now, carried forward from the session's last update while it plays.
  private fun positionOf(state: PlaybackState?, playing: Boolean): Long {
    if (state == null || state.position < 0) {
      return -1L
    }
    if (!playing || state.lastPositionUpdateTime <= 0) {
      return state.position
    }
    val elapsed = SystemClock.elapsedRealtime() - state.lastPositionUpdateTime
    return state.position + (elapsed * state.playbackSpeed).toLong()
  }

  private fun seeked(positionMs: Long, now: Long): Boolean {
    if (positionMs < 0 || lastPositionMs < 0) {
      return false
    }
    val expected = if (lastPlaying) lastPositionMs + (now - lastReportedAt) else lastPositionMs
    return abs(positionMs - expected) > SEEK_THRESHOLD_MS
  }
}
