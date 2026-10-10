package com.waspos.companion.phonebridge

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat

// A foreground service whose only job is to keep the process, and with it the JavaScript Bluetooth link, alive in the background.
class KeepAliveService : Service() {
  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    val watchName = intent?.getStringExtra(EXTRA_WATCH_NAME) ?: "your watch"
    ensureChannel()
    try {
      ServiceCompat.startForeground(this, NOTIFICATION_ID, buildNotification(watchName), foregroundType())
    } catch (e: Exception) {
      Log.w(TAG, "Could not enter the foreground", e)
      stopSelf()
    }
    // Without the app's JavaScript there is no link to keep, so a killed service stays stopped.
    return START_NOT_STICKY
  }

  private fun foregroundType(): Int =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE else 0

  private fun ensureChannel() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
      return
    }
    val manager = getSystemService(NotificationManager::class.java) ?: return
    val channel = NotificationChannel(
      CHANNEL_ID,
      getString(R.string.phone_bridge_channel_name),
      NotificationManager.IMPORTANCE_LOW,
    )
    channel.setShowBadge(false)
    manager.createNotificationChannel(channel)
  }

  private fun buildNotification(watchName: String) =
    NotificationCompat.Builder(this, CHANNEL_ID)
      .setSmallIcon(android.R.drawable.stat_sys_data_bluetooth)
      .setContentTitle(getString(R.string.phone_bridge_keepalive_title, watchName))
      .setContentText(getString(R.string.phone_bridge_keepalive_text))
      .setOngoing(true)
      .setCategory(NotificationCompat.CATEGORY_SERVICE)
      .setPriority(NotificationCompat.PRIORITY_LOW)
      .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
      .setContentIntent(openAppIntent())
      .build()

  private fun openAppIntent(): PendingIntent? {
    val launch = packageManager.getLaunchIntentForPackage(packageName) ?: return null
    return PendingIntent.getActivity(this, 0, launch, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
  }

  companion object {
    private const val TAG = "PhoneBridge"
    private const val CHANNEL_ID = "waspos-watch-link"
    private const val NOTIFICATION_ID = 0x5741
    private const val EXTRA_WATCH_NAME = "watchName"

    // Throws when Android refuses a foreground service, for example when the app is already in the background.
    fun start(context: Context, watchName: String) {
      val intent = Intent(context, KeepAliveService::class.java).putExtra(EXTRA_WATCH_NAME, watchName)
      ContextCompat.startForegroundService(context, intent)
    }

    fun stop(context: Context) {
      context.stopService(Intent(context, KeepAliveService::class.java))
    }
  }
}
