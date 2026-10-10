package com.waspos.companion.phonebridge

import android.app.Notification
import android.content.pm.PackageManager
import android.os.Bundle
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification
import java.util.concurrent.ConcurrentHashMap

// Receives every notification the user allows it to see and forwards the ones a watch should show.
class PhoneNotificationListener : NotificationListenerService() {
  private val labels = ConcurrentHashMap<String, String>()

  override fun onListenerConnected() {
    MediaTracker.start(this)
  }

  override fun onListenerDisconnected() {
    MediaTracker.stop()
  }

  override fun onDestroy() {
    MediaTracker.stop()
    super.onDestroy()
  }

  override fun onNotificationPosted(sbn: StatusBarNotification?) {
    if (sbn == null || !shouldForward(sbn)) {
      return
    }
    val extras = sbn.notification.extras ?: Bundle.EMPTY
    val title = text(extras, Notification.EXTRA_TITLE_BIG) ?: text(extras, Notification.EXTRA_TITLE) ?: ""
    val body = text(extras, Notification.EXTRA_BIG_TEXT)
      ?: lines(extras)
      ?: text(extras, Notification.EXTRA_TEXT)
      ?: ""
    if (title.isEmpty() && body.isEmpty()) {
      return
    }
    PhoneEventBus.emit(
      mapOf(
        "type" to "notification",
        "id" to idFor(sbn.key),
        "app" to appLabel(sbn.packageName),
        "title" to title,
        "body" to body,
      )
    )
  }

  override fun onNotificationRemoved(sbn: StatusBarNotification?) {
    if (sbn == null || !shouldForward(sbn)) {
      return
    }
    PhoneEventBus.emit(mapOf("type" to "notificationRemoved", "id" to idFor(sbn.key)))
  }

  // Leaves out ongoing and summary notifications, media and call notifications (sent as their own events), and this app's own.
  private fun shouldForward(sbn: StatusBarNotification): Boolean {
    val notification = sbn.notification ?: return false
    if (sbn.packageName == packageName || sbn.isOngoing) {
      return false
    }
    if ((notification.flags and Notification.FLAG_GROUP_SUMMARY) != 0) {
      return false
    }
    val category = notification.category
    if (category == Notification.CATEGORY_TRANSPORT || category == Notification.CATEGORY_CALL) {
      return false
    }
    return notification.extras?.containsKey(Notification.EXTRA_MEDIA_SESSION) != true
  }

  private fun appLabel(pkg: String): String {
    labels[pkg]?.let { return it }
    val label = try {
      val info = packageManager.getApplicationInfo(pkg, 0)
      packageManager.getApplicationLabel(info).toString()
    } catch (e: PackageManager.NameNotFoundException) {
      pkg
    }
    labels[pkg] = label
    return label
  }

  private fun text(extras: Bundle, key: String): String? =
    extras.getCharSequence(key)?.toString()?.trim()?.takeIf { it.isNotEmpty() }

  // Inbox-style notifications keep their content as separate lines.
  private fun lines(extras: Bundle): String? =
    extras.getCharSequenceArray(Notification.EXTRA_TEXT_LINES)
      ?.joinToString("\n") { it.toString() }
      ?.trim()
      ?.takeIf { it.isNotEmpty() }

  companion object {
    // The same key always gives the same id, including across restarts, so a removal matches its post.
    fun idFor(key: String): Int = key.hashCode() and 0x7fffffff
  }
}
