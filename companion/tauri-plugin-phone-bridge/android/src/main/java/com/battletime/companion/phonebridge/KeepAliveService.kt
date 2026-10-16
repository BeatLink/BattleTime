package com.battletime.companion.phonebridge

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat

// The foreground service that hosts the Rust daemon, so the watch stays linked with no window open.
//
// The daemon starts and stops it through `keepRunning` while it keeps a watch. Boot, the watch coming
// into range and Android restarting it all start it with no watch named, and then it asks the daemon.
class KeepAliveService : Service() {
  private var watchName = DEFAULT_WATCH_NAME

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onCreate() {
    super.onCreate()
    running = this
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    val asked = intent?.getStringExtra(EXTRA_WATCH_NAME)
    asked?.let { watchName = it }
    if (!enterForeground()) {
      stopSelf()
      return START_NOT_STICKY
    }
    val kept = DaemonHost.startDaemon(this)
    if (asked == null && kept == null) {
      Log.i(TAG, "No watch to keep, so the service stops")
      stopSelf()
      return START_NOT_STICKY
    }
    kept?.let { show(it) }
    return START_STICKY
  }

  override fun onDestroy() {
    if (running === this) {
      running = null
    }
    super.onDestroy()
  }

  // Android 12 and later may refuse, and Android 14 refuses a connected-device service without nearby-devices access.
  private fun enterForeground(): Boolean {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE &&
      !PhoneActions.granted(this, Manifest.permission.BLUETOOTH_CONNECT)
    ) {
      Log.w(TAG, "The app may not connect to nearby devices, so it cannot stay in the background")
      return false
    }
    ensureChannel()
    return try {
      ServiceCompat.startForeground(this, NOTIFICATION_ID, buildNotification(), foregroundType())
      true
    } catch (e: Exception) {
      Log.w(TAG, "Could not enter the foreground", e)
      false
    }
  }

  private fun show(name: String) {
    watchName = name
    getSystemService(NotificationManager::class.java)?.notify(NOTIFICATION_ID, buildNotification())
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

  private fun buildNotification() =
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
    private const val CHANNEL_ID = "battletime-watch-link"
    private const val NOTIFICATION_ID = 0x5741
    private const val EXTRA_WATCH_NAME = "watchName"
    private const val DEFAULT_WATCH_NAME = "your watch"

    private val handler = Handler(Looper.getMainLooper())

    // The service while it runs, so a new watch name only changes its notification.
    @Volatile
    private var running: KeepAliveService? = null

    // The daemon asks for this whenever the watch it keeps changes; null lets the service go.
    fun keepRunning(context: Context, watchName: String?) {
      val app = context.applicationContext
      handler.post {
        val service = running
        when {
          watchName == null -> app.stopService(Intent(app, KeepAliveService::class.java))
          service != null -> service.show(watchName)
          else -> start(app, watchName)
        }
      }
    }

    // Starts the service when Android allows it: from the window, at boot, or when the companion watch appears.
    fun start(context: Context, watchName: String? = null) {
      val intent = Intent(context, KeepAliveService::class.java)
      watchName?.let { intent.putExtra(EXTRA_WATCH_NAME, it) }
      try {
        ContextCompat.startForegroundService(context, intent)
      } catch (e: Exception) {
        Log.w(TAG, "Android refused to start the keep-alive service", e)
      }
    }
  }
}
