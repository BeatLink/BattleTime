package com.battletime.companion.phonebridge

import android.app.Activity
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.TypedValue
import android.view.Gravity
import android.view.WindowManager
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.app.NotificationCompat
import java.lang.ref.WeakReference

// The phone's SOS screen: shown over the lock screen when the watch sends an SOS, it starts the recording and stops everything on a tap.
//
// Android lets only a visible screen start the camera and the microphone, so the recording starts here
// rather than from the daemon.
class SosActivity : Activity() {
  private val handler = Handler(Looper.getMainLooper())
  private lateinit var status: TextView
  private val refresh = object : Runnable {
    override fun run() {
      status.text = SosRecordingService.description
      handler.postDelayed(this, REFRESH_MILLISECONDS)
    }
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    current = WeakReference(this)
    showOverLockScreen()
    setContentView(layout())
    startRecordingIfAsked(intent)
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    setIntent(intent)
    startRecordingIfAsked(intent)
  }

  override fun onStart() {
    super.onStart()
    handler.post(refresh)
  }

  override fun onStop() {
    handler.removeCallbacks(refresh)
    super.onStop()
  }

  override fun onDestroy() {
    if (current?.get() === this) {
      current = null
    }
    super.onDestroy()
  }

  private fun startRecordingIfAsked(intent: Intent?) {
    if (intent?.getBooleanExtra(EXTRA_RECORD, false) == true) {
      SosRecordingService.start(this)
    }
  }

  // The wearer stopped it here, so the daemon is told, and it closes the screen in turn.
  private fun stop() {
    SosRecordingService.stop(this)
    PhoneEventBus.emit(mapOf("type" to "sosStopped"))
    finish()
  }

  private fun showOverLockScreen() {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O_MR1) {
      setShowWhenLocked(true)
      setTurnScreenOn(true)
    } else {
      @Suppress("DEPRECATION")
      window.addFlags(WindowManager.LayoutParams.FLAG_SHOW_WHEN_LOCKED or WindowManager.LayoutParams.FLAG_TURN_SCREEN_ON)
    }
    window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
  }

  // ==========================================================================
  // The screen
  // ==========================================================================

  private fun layout(): LinearLayout {
    val padding = pixels(24f)
    val page = LinearLayout(this)
    page.orientation = LinearLayout.VERTICAL
    page.gravity = Gravity.CENTER
    page.setPadding(padding, padding, padding, padding)
    page.setBackgroundColor(Color.rgb(0xb0, 0x10, 0x10))

    val title = TextView(this)
    title.text = getString(R.string.phone_bridge_sos_title)
    title.setTextColor(Color.WHITE)
    title.setTextSize(TypedValue.COMPLEX_UNIT_SP, 64f)
    title.gravity = Gravity.CENTER
    page.addView(title)

    val detail = TextView(this)
    detail.text = getString(R.string.phone_bridge_sos_detail)
    detail.setTextColor(Color.WHITE)
    detail.setTextSize(TypedValue.COMPLEX_UNIT_SP, 18f)
    detail.gravity = Gravity.CENTER
    page.addView(detail)

    status = TextView(this)
    status.setTextColor(Color.WHITE)
    status.setTextSize(TypedValue.COMPLEX_UNIT_SP, 16f)
    status.gravity = Gravity.CENTER
    status.setPadding(0, padding, 0, padding)
    page.addView(status)

    val stop = Button(this)
    stop.text = getString(R.string.phone_bridge_sos_stop)
    stop.setTextSize(TypedValue.COMPLEX_UNIT_SP, 22f)
    stop.setOnClickListener { stop() }
    page.addView(stop, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, pixels(72f)))
    return page
  }

  private fun pixels(dp: Float): Int =
    TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, dp, resources.displayMetrics).toInt()

  companion object {
    private const val EXTRA_RECORD = "record"
    private const val REFRESH_MILLISECONDS = 1000L
    private const val CHANNEL_ID = "battletime-sos"
    private const val NOTIFICATION_ID = 0x5053

    private val main = Handler(Looper.getMainLooper())

    @Volatile
    private var current: WeakReference<SosActivity>? = null

    // The daemon's sosScreen request: shows the screen and starts recording, or stops recording and closes it.
    fun show(context: Context, on: Boolean, record: Boolean) {
      val app = context.applicationContext
      if (!on) {
        SosRecordingService.stop(app)
        app.getSystemService(NotificationManager::class.java)?.cancel(NOTIFICATION_ID)
        main.post { current?.get()?.finish() }
        return
      }
      val intent = Intent(app, SosActivity::class.java)
        .putExtra(EXTRA_RECORD, record)
        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
      if (PhoneActions.mayStartActivities(app)) {
        app.startActivity(intent)
        return
      }
      notifyToOpen(app, intent)
      throw PhoneActionException(
        "Android kept the SOS screen from opening, so nothing is recorded; allow the app to display over other apps",
      )
    }

    // Without leave to open a screen, a notification offers it, and a tap opens it and starts the recording.
    private fun notifyToOpen(context: Context, intent: Intent) {
      val manager = context.getSystemService(NotificationManager::class.java) ?: return
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        val channel = NotificationChannel(
          CHANNEL_ID,
          context.getString(R.string.phone_bridge_sos_channel_name),
          NotificationManager.IMPORTANCE_HIGH,
        )
        manager.createNotificationChannel(channel)
      }
      val open = PendingIntent.getActivity(
        context,
        0,
        intent,
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
      )
      val notification = NotificationCompat.Builder(context, CHANNEL_ID)
        .setSmallIcon(android.R.drawable.stat_sys_warning)
        .setContentTitle(context.getString(R.string.phone_bridge_sos_title))
        .setContentText(context.getString(R.string.phone_bridge_sos_notification_text))
        .setPriority(NotificationCompat.PRIORITY_MAX)
        .setCategory(NotificationCompat.CATEGORY_ALARM)
        .setContentIntent(open)
        .setAutoCancel(true)
        .build()
      try {
        manager.notify(NOTIFICATION_ID, notification)
      } catch (e: SecurityException) {
        // Without leave to post notifications there is nothing more to offer.
      }
    }
  }
}
