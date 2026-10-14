package com.neotime.companion.phonebridge

import android.annotation.SuppressLint
import android.content.Context
import android.location.Geocoder
import android.location.Location
import android.location.LocationManager
import android.os.Build
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import java.util.Locale
import java.util.concurrent.Executors

// Finds where the phone is for the weather: a fresh fix where Android offers one, or the newest known.
object PhoneLocation {
  private val worker = Executors.newSingleThreadExecutor()

  // The caller has checked the location permission.
  @SuppressLint("MissingPermission")
  fun find(context: Context, invoke: Invoke) {
    val manager = context.getSystemService(LocationManager::class.java)
    if (manager == null) {
      return invoke.reject("This phone has no location service")
    }
    val provider = listOf(LocationManager.NETWORK_PROVIDER, LocationManager.GPS_PROVIDER)
      .firstOrNull { manager.isProviderEnabled(it) }
    if (provider == null) {
      return invoke.reject("Turn on this phone's location first")
    }
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
      manager.getCurrentLocation(provider, null, worker) { location ->
        answer(context, invoke, location ?: newestKnown(manager))
      }
    } else {
      worker.execute { answer(context, invoke, newestKnown(manager)) }
    }
  }

  @SuppressLint("MissingPermission")
  private fun newestKnown(manager: LocationManager): Location? =
    manager.getProviders(true).mapNotNull { manager.getLastKnownLocation(it) }.maxByOrNull { it.time }

  private fun answer(context: Context, invoke: Invoke, location: Location?) {
    if (location == null) {
      return invoke.reject("This phone does not know where it is yet")
    }
    invoke.resolve(
      JSObject()
        .put("latitude", location.latitude)
        .put("longitude", location.longitude)
        .put("name", placeName(context, location)),
    )
  }

  // The town's name where the phone can look it up, which needs a network.
  private fun placeName(context: Context, location: Location): String =
    try {
      @Suppress("DEPRECATION")
      Geocoder(context, Locale.getDefault()).getFromLocation(location.latitude, location.longitude, 1)
        ?.firstOrNull()?.locality ?: ""
    } catch (e: Exception) {
      ""
    }
}
