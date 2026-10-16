package com.battletime.companion.phonebridge

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.location.Geocoder
import android.location.Location
import android.location.LocationManager
import android.os.Build
import org.json.JSONObject
import java.util.Locale
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

// Where the phone is, as {latitude, longitude, name}, or the reason it is not known.
typealias LocationAnswer = (JSONObject?, String?) -> Unit

// Finds where the phone is for the weather: a fresh fix where Android offers one, or the newest known.
object PhoneLocation {
  private val worker = Executors.newSingleThreadExecutor()

  // The location check below guards every call, which lint cannot follow.
  @SuppressLint("MissingPermission")
  fun find(context: Context, answer: LocationAnswer) {
    if (!PhoneActions.granted(context, Manifest.permission.ACCESS_COARSE_LOCATION)) {
      return answer(null, "Allow the app to use this phone's location first")
    }
    val manager = context.getSystemService(LocationManager::class.java)
      ?: return answer(null, "This phone has no location service")
    val provider = listOf(LocationManager.NETWORK_PROVIDER, LocationManager.GPS_PROVIDER)
      .firstOrNull { manager.isProviderEnabled(it) }
      ?: return answer(null, "Turn on this phone's location first")
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
      manager.getCurrentLocation(provider, null, worker) { location ->
        reply(context, location ?: newestKnown(manager), answer)
      }
    } else {
      worker.execute { reply(context, newestKnown(manager), answer) }
    }
  }

  // Waits for `find` on the calling thread, which must not be the main one.
  fun findNow(context: Context, timeoutMilliseconds: Long): JSONObject {
    val done = CountDownLatch(1)
    var found: JSONObject? = null
    var failure: String? = null
    find(context) { place, error ->
      found = place
      failure = error
      done.countDown()
    }
    if (!done.await(timeoutMilliseconds, TimeUnit.MILLISECONDS)) {
      throw PhoneActionException("This phone took too long to find where it is")
    }
    return found ?: throw PhoneActionException(failure ?: "This phone does not know where it is yet")
  }

  @SuppressLint("MissingPermission")
  private fun newestKnown(manager: LocationManager): Location? =
    manager.getProviders(true).mapNotNull { manager.getLastKnownLocation(it) }.maxByOrNull { it.time }

  private fun reply(context: Context, location: Location?, answer: LocationAnswer) {
    if (location == null) {
      return answer(null, "This phone does not know where it is yet")
    }
    answer(
      JSONObject()
        .put("latitude", location.latitude)
        .put("longitude", location.longitude)
        .put("name", placeName(context, location)),
      null,
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
