package com.neotime.companion.phonebridge

import android.annotation.SuppressLint
import android.util.Base64
import expo.modules.kotlin.exception.CodedException
import java.io.ByteArrayOutputStream
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest
import java.security.cert.CertificateException
import java.security.cert.X509Certificate
import javax.net.ssl.HttpsURLConnection
import javax.net.ssl.SSLContext
import javax.net.ssl.X509TrustManager

// The most of a reply kept, so a huge page cannot fill memory.
private const val MOST_REPLY_BYTES = 4 * 1024 * 1024

// Makes a web request the user set up (the notes URL, remote buttons), as src/services/web-request.ts asks.
//
// An https address's certificate is checked the usual way, against a pinned SHA-256 fingerprint for a
// self-signed certificate, or not at all; the handshake still proves the server holds the certificate's key.
object WebRequest {
  fun run(request: Map<String, Any?>): Map<String, Any?> {
    val url = URL(request["url"] as? String ?: throw failure("Give the address to request."))
    if (url.protocol != "http" && url.protocol != "https") {
      throw failure("$url is not an http or https URL.")
    }
    val connection = url.openConnection() as HttpURLConnection
    val timeout = (request["timeoutMs"] as? Number)?.toInt() ?: 15000
    connection.connectTimeout = timeout
    connection.readTimeout = timeout
    connection.requestMethod = request["method"] as? String ?: "GET"
    (request["headers"] as? Map<*, *>)?.forEach { (name, value) ->
      if (name is String && value is String) connection.setRequestProperty(name, value)
    }
    if (connection is HttpsURLConnection) {
      when (request["certificate"]) {
        "pinned" -> trust(connection, pinBytes(request["certificateSha256"] as? String))
        "any" -> trust(connection, null)
      }
    }
    try {
      (request["body"] as? String)?.let { body ->
        connection.doOutput = true
        connection.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
      }
      val status = connection.responseCode
      val stream = if (status >= 400) connection.errorStream else connection.inputStream
      val body = stream?.use { readLimited(it) } ?: ByteArray(0)
      return mapOf(
        "status" to status,
        "statusText" to (connection.responseMessage ?: ""),
        "bodyBase64" to Base64.encodeToString(body, Base64.NO_WRAP),
      )
    } catch (error: java.net.SocketTimeoutException) {
      throw failure("No answer within ${timeout / 1000} s.")
    } finally {
      connection.disconnect()
    }
  }

  private fun failure(message: String) = CodedException("ERR_WEB_REQUEST", message, null)

  private fun readLimited(stream: java.io.InputStream): ByteArray {
    val output = ByteArrayOutputStream()
    val chunk = ByteArray(16 * 1024)
    while (true) {
      val amount = stream.read(chunk)
      if (amount < 0) break
      if (output.size() + amount > MOST_REPLY_BYTES) {
        throw failure("The reply is over ${MOST_REPLY_BYTES / 1024 / 1024} MB.")
      }
      output.write(chunk, 0, amount)
    }
    return output.toByteArray()
  }

  // The fingerprint as 32 bytes, whether written with colons, spaces or neither.
  private fun pinBytes(text: String?): ByteArray {
    val digits = (text ?: "").filter { !it.isWhitespace() && it != ':' }
    if (digits.length != 64 || digits.any { Character.digit(it, 16) < 0 }) {
      throw failure("A SHA-256 fingerprint has 64 hex digits.")
    }
    return ByteArray(32) { index -> digits.substring(index * 2, index * 2 + 2).toInt(16).toByte() }
  }

  private fun hex(bytes: ByteArray) = bytes.joinToString("") { "%02x".format(it) }

  // Trusts only the certificate with the pinned fingerprint, or any certificate without a pin.
  @SuppressLint("CustomX509TrustManager", "TrustAllX509TrustManager", "BadHostnameVerifier")
  private fun trust(connection: HttpsURLConnection, pin: ByteArray?) {
    val manager = object : X509TrustManager {
      override fun checkClientTrusted(chain: Array<X509Certificate>, authType: String) {
        throw CertificateException("Client certificates are not checked here.")
      }

      override fun checkServerTrusted(chain: Array<X509Certificate>, authType: String) {
        if (pin == null) return
        val shown = MessageDigest.getInstance("SHA-256").digest(chain.first().encoded)
        if (!MessageDigest.isEqual(shown, pin)) {
          throw CertificateException(
            "${connection.url.host} showed a certificate with SHA-256 fingerprint ${hex(shown)}, not the pinned one."
          )
        }
      }

      override fun getAcceptedIssuers(): Array<X509Certificate> = emptyArray()
    }
    val tls = SSLContext.getInstance("TLS")
    tls.init(null, arrayOf(manager), null)
    connection.sslSocketFactory = tls.socketFactory
    // The pin or the user's choice to trust any certificate stands in for the name check.
    connection.hostnameVerifier = javax.net.ssl.HostnameVerifier { _, _ -> true }
  }
}
