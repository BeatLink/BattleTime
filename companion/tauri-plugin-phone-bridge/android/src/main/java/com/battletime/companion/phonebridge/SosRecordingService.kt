package com.battletime.companion.phonebridge

import android.Manifest
import android.annotation.SuppressLint
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.hardware.camera2.CameraCaptureSession
import android.hardware.camera2.CameraCharacteristics
import android.hardware.camera2.CameraDevice
import android.hardware.camera2.CameraManager
import android.media.MediaRecorder
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.os.Handler
import android.os.HandlerThread
import android.os.IBinder
import android.os.ParcelFileDescriptor
import android.provider.MediaStore
import android.util.Log
import android.util.Size
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

// Records the SOS: video from both cameras at once where the phone can, else the back camera, else only sound.
//
// The microphone goes with the back camera's video, or with the sound-only file. Each file lands in
// Movies/BattleTime SOS, or Music/BattleTime SOS for sound only, and is kept when the recording stops.
// During a phone call Android gives the microphone to the call, so the sound is silent until it ends.
class SosRecordingService : Service() {
  private lateinit var thread: HandlerThread
  private lateinit var worker: Handler
  private val recordings = mutableListOf<Recording>()

  // One file being written: its recorder, the camera feeding it, if any, and where it goes.
  private class Recording(
    val recorder: MediaRecorder,
    val output: Output,
    var camera: CameraDevice? = null,
    var session: CameraCaptureSession? = null,
  )

  private class Output(val file: ParcelFileDescriptor, val uri: Uri?)

  override fun onBind(intent: Intent?): IBinder? = null

  private fun describe(text: String) {
    description = text
  }

  override fun onCreate() {
    super.onCreate()
    thread = HandlerThread("sos-recording")
    thread.start()
    worker = Handler(thread.looper)
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (!enterForeground()) {
      stopSelf()
      return START_NOT_STICKY
    }
    if (recordings.isEmpty()) {
      worker.post { begin() }
    }
    return START_NOT_STICKY
  }

  override fun onDestroy() {
    worker.post {
      recordings.forEach { finish(it) }
      recordings.clear()
      describe(getString(R.string.phone_bridge_sos_saved))
    }
    thread.quitSafely()
    super.onDestroy()
  }

  private fun enterForeground(): Boolean {
    ensureChannel()
    val notification = NotificationCompat.Builder(this, CHANNEL_ID)
      .setSmallIcon(android.R.drawable.presence_video_online)
      .setContentTitle(getString(R.string.phone_bridge_sos_title))
      .setContentText(getString(R.string.phone_bridge_sos_recording))
      .setOngoing(true)
      .setCategory(NotificationCompat.CATEGORY_SERVICE)
      .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
      .build()
    return try {
      ServiceCompat.startForeground(this, NOTIFICATION_ID, notification, foregroundType())
      true
    } catch (e: Exception) {
      Log.w(TAG, "Could not record in the foreground", e)
      describe(getString(R.string.phone_bridge_sos_not_recording))
      false
    }
  }

  // Android 14 refuses a camera or microphone type without the matching permission, so only granted ones are asked for.
  private fun foregroundType(): Int {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
      return 0
    }
    var type = 0
    if (PhoneActions.granted(this, Manifest.permission.CAMERA)) {
      type = type or ServiceInfo.FOREGROUND_SERVICE_TYPE_CAMERA
    }
    if (PhoneActions.granted(this, Manifest.permission.RECORD_AUDIO)) {
      type = type or ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
    }
    return type
  }

  private fun ensureChannel() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
      return
    }
    val manager = getSystemService(NotificationManager::class.java) ?: return
    val channel = NotificationChannel(
      CHANNEL_ID,
      getString(R.string.phone_bridge_sos_channel_name),
      NotificationManager.IMPORTANCE_HIGH,
    )
    manager.createNotificationChannel(channel)
  }

  // ==========================================================================
  // Choosing what to record
  // ==========================================================================

  // Runs on the worker thread.
  private fun begin() {
    val hasCamera = PhoneActions.granted(this, Manifest.permission.CAMERA)
    val hasMicrophone = PhoneActions.granted(this, Manifest.permission.RECORD_AUDIO)
    val manager = getSystemService(CameraManager::class.java)
    val back = manager?.let { cameraFacing(it, CameraCharacteristics.LENS_FACING_BACK) }
    val front = manager?.let { cameraFacing(it, CameraCharacteristics.LENS_FACING_FRONT) }
    val pair = if (manager != null && back != null && front != null) concurrentPair(manager) else null
    when {
      hasCamera && manager != null && pair != null -> {
        describe(getString(R.string.phone_bridge_sos_both_cameras))
        record(manager, pair.first, "back", hasMicrophone) { recordBackOnly(manager, back, hasMicrophone) }
        record(manager, pair.second, "front", false) {}
      }
      hasCamera && manager != null && back != null -> recordBackOnly(manager, back, hasMicrophone)
      hasMicrophone -> recordSoundOnly()
      else -> describe(getString(R.string.phone_bridge_sos_not_recording))
    }
  }

  private fun recordBackOnly(manager: CameraManager, back: String?, withSound: Boolean) {
    recordings.toList().forEach { finish(it) }
    recordings.clear()
    if (back == null) {
      if (withSound) recordSoundOnly() else describe(getString(R.string.phone_bridge_sos_not_recording))
      return
    }
    describe(getString(R.string.phone_bridge_sos_back_camera))
    record(manager, back, "back", withSound) {
      if (withSound) recordSoundOnly() else describe(getString(R.string.phone_bridge_sos_not_recording))
    }
  }

  private fun cameraFacing(manager: CameraManager, facing: Int): String? =
    try {
      manager.cameraIdList.firstOrNull { manager.getCameraCharacteristics(it).get(CameraCharacteristics.LENS_FACING) == facing }
    } catch (e: Exception) {
      null
    }

  // A back and a front camera Android says can stream at the same time, back first, or null.
  private fun concurrentPair(manager: CameraManager): Pair<String, String>? {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
      return null
    }
    val sets = try {
      manager.concurrentCameraIds
    } catch (e: Exception) {
      return null
    }
    for (set in sets) {
      val facing = { want: Int -> set.firstOrNull { manager.getCameraCharacteristics(it).get(CameraCharacteristics.LENS_FACING) == want } }
      val back = facing(CameraCharacteristics.LENS_FACING_BACK)
      val front = facing(CameraCharacteristics.LENS_FACING_FRONT)
      if (back != null && front != null) {
        return Pair(back, front)
      }
    }
    return null
  }

  // ==========================================================================
  // Recording
  // ==========================================================================

  // Opens one camera and records it to a new file; `failed` runs on the worker thread if it cannot.
  @SuppressLint("MissingPermission")
  private fun record(manager: CameraManager, id: String, name: String, withSound: Boolean, failed: () -> Unit) {
    val characteristics = manager.getCameraCharacteristics(id)
    val size = videoSize(characteristics)
    val output = newOutput("video/mp4", "sos-$name", ".mp4") ?: return failed()
    val recorder = newRecorder()
    try {
      if (withSound) recorder.setAudioSource(MediaRecorder.AudioSource.MIC)
      recorder.setVideoSource(MediaRecorder.VideoSource.SURFACE)
      recorder.setOutputFormat(MediaRecorder.OutputFormat.MPEG_4)
      recorder.setOutputFile(output.file.fileDescriptor)
      recorder.setVideoEncoder(MediaRecorder.VideoEncoder.H264)
      recorder.setVideoEncodingBitRate(VIDEO_BITS_PER_SECOND)
      recorder.setVideoFrameRate(FRAMES_PER_SECOND)
      recorder.setVideoSize(size.width, size.height)
      recorder.setOrientationHint(characteristics.get(CameraCharacteristics.SENSOR_ORIENTATION) ?: 0)
      if (withSound) recorder.setAudioEncoder(MediaRecorder.AudioEncoder.AAC)
      recorder.prepare()
    } catch (e: Exception) {
      Log.w(TAG, "The $name camera's recorder would not start", e)
      recorder.release()
      discard(output)
      return failed()
    }
    val recording = Recording(recorder, output)
    recordings.add(recording)
    val giveUp = {
      recordings.remove(recording)
      finish(recording)
      discard(output)
      failed()
    }
    try {
      manager.openCamera(
        id,
        object : CameraDevice.StateCallback() {
          override fun onOpened(camera: CameraDevice) {
            recording.camera = camera
            startSession(recording, giveUp)
          }

          override fun onDisconnected(camera: CameraDevice) {
            camera.close()
          }

          override fun onError(camera: CameraDevice, error: Int) {
            Log.w(TAG, "The $name camera failed with $error")
            camera.close()
            recording.camera = null
            giveUp()
          }
        },
        worker,
      )
    } catch (e: Exception) {
      Log.w(TAG, "The $name camera would not open", e)
      giveUp()
    }
  }

  private fun startSession(recording: Recording, giveUp: () -> Unit) {
    val camera = recording.camera ?: return giveUp()
    val surface = recording.recorder.surface
    val ready = object : CameraCaptureSession.StateCallback() {
      override fun onConfigured(session: CameraCaptureSession) {
        recording.session = session
        try {
          val request = camera.createCaptureRequest(CameraDevice.TEMPLATE_RECORD)
          request.addTarget(surface)
          session.setRepeatingRequest(request.build(), null, worker)
          recording.recorder.start()
        } catch (e: Exception) {
          Log.w(TAG, "The camera would not record", e)
          giveUp()
        }
      }

      override fun onConfigureFailed(session: CameraCaptureSession) {
        giveUp()
      }
    }
    try {
      @Suppress("DEPRECATION")
      camera.createCaptureSession(listOf(surface), ready, worker)
    } catch (e: Exception) {
      Log.w(TAG, "The camera would not take the recorder", e)
      giveUp()
    }
  }

  private fun recordSoundOnly() {
    val output = newOutput("audio/mp4", "sos-sound", ".m4a")
    if (output == null) {
      describe(getString(R.string.phone_bridge_sos_not_recording))
      return
    }
    val recorder = newRecorder()
    try {
      recorder.setAudioSource(MediaRecorder.AudioSource.MIC)
      recorder.setOutputFormat(MediaRecorder.OutputFormat.MPEG_4)
      recorder.setOutputFile(output.file.fileDescriptor)
      recorder.setAudioEncoder(MediaRecorder.AudioEncoder.AAC)
      recorder.prepare()
      recorder.start()
      recordings.add(Recording(recorder, output))
      describe(getString(R.string.phone_bridge_sos_sound_only))
    } catch (e: Exception) {
      Log.w(TAG, "The microphone would not record", e)
      recorder.release()
      discard(output)
      describe(getString(R.string.phone_bridge_sos_not_recording))
    }
  }

  // Stops one recording and keeps its file; a recorder stopped before any frame throws, and that file is dropped.
  private fun finish(recording: Recording) {
    var kept = true
    try {
      recording.session?.close()
    } catch (e: Exception) {
      Log.w(TAG, "The camera session would not close", e)
    }
    try {
      recording.recorder.stop()
    } catch (e: RuntimeException) {
      kept = false
    }
    recording.recorder.release()
    recording.camera?.close()
    if (kept) keep(recording.output) else discard(recording.output)
  }

  private fun newRecorder(): MediaRecorder =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
      MediaRecorder(this)
    } else {
      @Suppress("DEPRECATION")
      MediaRecorder()
    }

  // The largest size up to 720p that the camera records, since two at once must fit together.
  private fun videoSize(characteristics: CameraCharacteristics): Size {
    val sizes = characteristics.get(CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP)
      ?.getOutputSizes(MediaRecorder::class.java)
      .orEmpty()
    return sizes
      .filter { it.width <= LARGEST_WIDTH && it.height <= LARGEST_HEIGHT }
      .maxByOrNull { it.width * it.height }
      ?: Size(LARGEST_WIDTH, LARGEST_HEIGHT)
  }

  // ==========================================================================
  // Files
  // ==========================================================================

  private fun newOutput(mime: String, prefix: String, extension: String): Output? {
    val name = "$prefix-${SimpleDateFormat("yyyyMMdd-HHmmss", Locale.US).format(Date())}$extension"
    val isVideo = mime.startsWith("video")
    return try {
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
        val values = ContentValues().apply {
          put(MediaStore.MediaColumns.DISPLAY_NAME, name)
          put(MediaStore.MediaColumns.MIME_TYPE, mime)
          put(
            MediaStore.MediaColumns.RELATIVE_PATH,
            "${if (isVideo) Environment.DIRECTORY_MOVIES else Environment.DIRECTORY_MUSIC}/$FOLDER",
          )
          put(MediaStore.MediaColumns.IS_PENDING, 1)
        }
        val collection = if (isVideo) {
          MediaStore.Video.Media.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)
        } else {
          MediaStore.Audio.Media.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)
        }
        val uri = contentResolver.insert(collection, values) ?: return null
        val file = contentResolver.openFileDescriptor(uri, "w") ?: return null
        Output(file, uri)
      } else {
        val folder = File(getExternalFilesDir(if (isVideo) Environment.DIRECTORY_MOVIES else Environment.DIRECTORY_MUSIC), FOLDER)
        folder.mkdirs()
        val file = File(folder, name)
        Output(ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_CREATE or ParcelFileDescriptor.MODE_READ_WRITE), null)
      }
    } catch (e: Exception) {
      Log.w(TAG, "Could not make a file to record into", e)
      null
    }
  }

  private fun keep(output: Output) {
    output.file.close()
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q && output.uri != null) {
      val values = ContentValues().apply { put(MediaStore.MediaColumns.IS_PENDING, 0) }
      contentResolver.update(output.uri, values, null, null)
    }
  }

  private fun discard(output: Output) {
    try {
      output.file.close()
      output.uri?.let { contentResolver.delete(it, null, null) }
    } catch (e: Exception) {
      Log.w(TAG, "Could not remove an empty recording", e)
    }
  }

  companion object {
    private const val TAG = "PhoneBridge"
    private const val CHANNEL_ID = "battletime-sos"
    private const val NOTIFICATION_ID = 0x5052
    private const val FOLDER = "BattleTime SOS"
    private const val LARGEST_WIDTH = 1280
    private const val LARGEST_HEIGHT = 720
    private const val FRAMES_PER_SECOND = 30
    private const val VIDEO_BITS_PER_SECOND = 4_000_000

    // What is being recorded, which the SOS screen shows.
    @Volatile
    var description: String = ""
      private set

    // Starts recording; only a visible screen may call this, as Android refuses the camera otherwise.
    fun start(context: Context) {
      try {
        ContextCompat.startForegroundService(context, Intent(context, SosRecordingService::class.java))
      } catch (e: Exception) {
        Log.w(TAG, "Android refused to start recording", e)
      }
    }

    fun stop(context: Context) {
      context.stopService(Intent(context, SosRecordingService::class.java))
    }
  }
}
