package app.tauri.atvadb

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import java.net.InetAddress
import java.net.NetworkInterface
import java.util.Collections

private const val TAG = "AtvAdb"

/**
 * mDNS discovery of Android-TV debugging services on the LAN, via the framework
 * [NsdManager]. This is the only Android-native code in the transport — connect,
 * shell, screencap and reboot are all driven from Rust by the app's WirelessAdb
 * via the pure-Rust `adb_client` crate.
 */
class AdbService(private val context: Context) {

  suspend fun discover(timeoutMs: Long = 3_000): List<DiscoveredAdbDevice> = withContext(Dispatchers.IO) {
    val nsd = context.getSystemService(Context.NSD_SERVICE) as NsdManager
    // Mutated from NSD resolve callbacks on a different thread, so keep it thread-safe.
    val out = Collections.synchronizedMap(linkedMapOf<String, DiscoveredAdbDevice>())
    val resolvedTypes = Collections.synchronizedSet(mutableSetOf<String>())
    // This phone advertises its own wireless-debugging service when USB/Wi-Fi debugging
    // is on; drop anything resolving to a local interface so we never list ourselves.
    val locals = localAddresses()
    // TLS pairing/connect are Android-11 "Wireless debugging"; _adb._tcp is the
    // legacy "Network debugging" (:5555) that Shield and older TVs advertise —
    // it needs no pairing code, just an RSA-auth connect.
    val serviceTypes = listOf("_adb-tls-pairing._tcp.", "_adb-tls-connect._tcp.", "_adb._tcp.")
    val listeners = serviceTypes.map { serviceType ->
      object : NsdManager.DiscoveryListener {
        override fun onDiscoveryStarted(regType: String) = Unit
        override fun onDiscoveryStopped(serviceType: String) = Unit
        override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) {
          Log.w(TAG, "discovery start failed for $serviceType (errorCode=$errorCode)")
        }
        override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) {
          Log.w(TAG, "discovery stop failed for $serviceType (errorCode=$errorCode)")
        }
        override fun onServiceLost(serviceInfo: NsdServiceInfo) = Unit
        override fun onServiceFound(serviceInfo: NsdServiceInfo) {
          if (serviceInfo.serviceType != serviceType) return
          nsd.resolveService(serviceInfo, object : NsdManager.ResolveListener {
            override fun onResolveFailed(serviceInfo: NsdServiceInfo, errorCode: Int) {
              Log.w(TAG, "resolve failed for ${serviceInfo.serviceName} (errorCode=$errorCode)")
            }
            override fun onServiceResolved(resolved: NsdServiceInfo) {
              val host = resolved.hostAddress ?: return
              if (host in locals) return
              val key = "$host:${resolved.port}:${resolved.serviceType}"
              out[key] = DiscoveredAdbDevice(
                name = resolved.serviceName ?: "Wireless debugging",
                host = host,
                port = resolved.port,
                service = resolved.serviceType,
              )
              resolvedTypes.add(serviceType)
            }
          })
        }
      }.also { nsd.discoverServices(serviceType, NsdManager.PROTOCOL_DNS_SD, it) }
    }
    // Exit as soon as we're confident we've seen the TV: either both service types
    // resolved (a TV in pairing mode advertises both) or at least one device has
    // resolved and a short grace window has passed to let a second one arrive. This
    // keeps a common connect-only scan under ~1.5s instead of always burning timeoutMs.
    val pollMs = 100L
    val graceMs = 1_500L
    var waited = 0L
    while (waited < timeoutMs) {
      delay(pollMs)
      waited += pollMs
      val bothTypes = resolvedTypes.containsAll(serviceTypes)
      val foundAndSettled = resolvedTypes.isNotEmpty() && waited >= graceMs
      if (bothTypes || foundAndSettled) break
    }
    listeners.forEach { runCatching { nsd.stopServiceDiscovery(it) } }
    out.values.toList()
  }

  /** Non-loopback local interface addresses, used to filter this phone out of discovery. */
  private fun localAddresses(): Set<String> =
    runCatching {
      NetworkInterface.getNetworkInterfaces().asSequence()
        .flatMap { it.inetAddresses.asSequence() }
        .filterNot { it.isLoopbackAddress }
        .mapNotNull { it.hostAddress?.substringBefore('%') }
        .toSet()
    }.getOrDefault(emptySet())
}

data class DiscoveredAdbDevice(val name: String, val host: String, val port: Int, val service: String)

private val NsdServiceInfo.hostAddress: String?
  get() = host?.hostAddress ?: attributes["adb"]?.let { InetAddress.getByAddress(it).hostAddress }
