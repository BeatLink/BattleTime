package com.waspos.companion.phonebridge

import android.util.Log
import java.util.concurrent.CopyOnWriteArraySet

typealias PhoneEventListener = (Map<String, Any?>) -> Unit

// Carries events from the services, which have their own lifecycles, to whichever module instance is listening.
object PhoneEventBus {
  private const val TAG = "PhoneBridge"

  private val listeners = CopyOnWriteArraySet<PhoneEventListener>()

  // The latest media state, replayed to a listener that joins after it was sent.
  @Volatile
  var lastMedia: Map<String, Any?>? = null
    private set

  fun addListener(listener: PhoneEventListener) {
    listeners.add(listener)
  }

  fun removeListener(listener: PhoneEventListener) {
    listeners.remove(listener)
  }

  fun emit(event: Map<String, Any?>) {
    if (event["type"] == "media") {
      lastMedia = event
    }
    for (listener in listeners) {
      try {
        listener(event)
      } catch (e: Exception) {
        Log.w(TAG, "A listener failed on ${event["type"]}", e)
      }
    }
  }
}
