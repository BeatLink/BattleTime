package com.battletime.companion.phonebridge

import android.bluetooth.le.ScanFilter
import android.companion.AssociationRequest
import android.companion.BluetoothLeDeviceFilter
import android.companion.CompanionDeviceManager
import android.companion.CompanionDeviceService
import android.content.Context
import android.os.Build
import android.util.Log
import androidx.annotation.RequiresApi

// The watch as Android's companion device, which lets the app start and run in the background for it.
object WatchCompanion {
  private const val TAG = "PhoneBridge"

  fun manager(context: Context): CompanionDeviceManager? =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) context.getSystemService(CompanionDeviceManager::class.java) else null

  @RequiresApi(Build.VERSION_CODES.O)
  fun isAssociated(manager: CompanionDeviceManager, address: String): Boolean {
    @Suppress("DEPRECATION")
    return manager.associations.any { it.equals(address, ignoreCase = true) }
  }

  // Asks for the one watch with this address; it must be advertising while Android looks for it.
  @RequiresApi(Build.VERSION_CODES.O)
  fun request(address: String): AssociationRequest {
    val scan = ScanFilter.Builder().setDeviceAddress(address.uppercase()).build()
    val filter = BluetoothLeDeviceFilter.Builder().setScanFilter(scan).build()
    return AssociationRequest.Builder().addDeviceFilter(filter).setSingleDevice(true).build()
  }

  // From Android 12, Android tells WatchPresenceService when the watch comes into range.
  @RequiresApi(Build.VERSION_CODES.O)
  fun observe(manager: CompanionDeviceManager, address: String) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) {
      return
    }
    try {
      @Suppress("DEPRECATION")
      manager.startObservingDevicePresence(address.uppercase())
    } catch (e: Exception) {
      Log.w(TAG, "Android will not say when the watch is near", e)
    }
  }
}

// Wakes the app when the companion watch comes into range, by starting the keep-alive service.
@RequiresApi(Build.VERSION_CODES.S)
class WatchPresenceService : CompanionDeviceService() {
  @Deprecated("Android 13 calls the AssociationInfo form, which reaches this one")
  override fun onDeviceAppeared(address: String) {
    Log.i(TAG, "Android saw the companion watch come into range")
    if (BootReceiver.remembersWatch(this)) {
      KeepAliveService.start(this)
    }
  }

  @Deprecated("Android 13 calls the AssociationInfo form, which reaches this one")
  override fun onDeviceDisappeared(address: String) {
    Log.i(TAG, "Android saw the companion watch go out of range")
  }

  private companion object {
    const val TAG = "PhoneBridge"
  }
}
