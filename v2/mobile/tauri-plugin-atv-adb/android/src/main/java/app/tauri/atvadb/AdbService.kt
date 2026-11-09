// Derived in part from Kadb (Apache-2.0, https://github.com/flyfishxu/Kadb): the API 34+
// ServiceInfoCallback path, with one callback per service and every callback unregistered
// on lost and on stop, follows kadb-mdns KadbMdnsAndroid.kt and AndroidServiceInfoCallback.kt.
package app.tauri.atvadb

import android.annotation.SuppressLint
import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.os.SystemClock
import android.os.ext.SdkExtensions
import android.util.Log
import androidx.annotation.RequiresApi
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.net.InetAddress
import java.net.NetworkInterface
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong

internal const val TAG = "AtvAdb"

private const val POLL_MS = 100L
private const val RESOLVE_TIMEOUT_MS = 1_000L

/**
 * mDNS discovery of Android-TV debugging services on the LAN, via the framework
 * [NsdManager]. This is the only Android-native code in the transport — connect,
 * shell, screencap and reboot are all driven from Rust by the app's WirelessAdb
 * via the pure-Rust `adb_client` crate.
 */
class AdbService(private val context: Context) {

  /**
   * A scan requested while another is running joins it rather than starting a second
   * one, which would compete for the single NSD resolve slot older phones allow.
   */
  suspend fun discover(timeoutMs: Long = 3_000): List<DiscoveredAdbDevice> {
    val scan = synchronized(scanLock) {
      inFlight?.takeIf { it.isActive }
        ?: scanScope.async { Scan(context.applicationContext, timeoutMs).run() }.also { inFlight = it }
    }
    return scan.await()
  }

  private companion object {
    val scanLock = Any()
    var inFlight: Deferred<List<DiscoveredAdbDevice>>? = null
    val scanScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
  }
}

data class DiscoveredAdbDevice(
  val name: String,
  val host: String,
  val port: Int,
  val service: String,
  val friendlyName: String? = null,
)

private interface Resolver {
  fun found(key: ServiceKey, info: NsdServiceInfo)
  fun lost(key: ServiceKey)
  fun pending(): Int
  fun close()
}

/** One browse of debugging services and optional Cast names, fully torn down on exit. */
private class Scan(private val context: Context, private val timeoutMs: Long) {
  private val nsd = context.getSystemService(Context.NSD_SERVICE) as NsdManager
  private val localKeys = localAddressKeys()
  private val results = DiscoveredServices()
  private val startFailures = ConcurrentHashMap<String, Int>()
  private val lastActivity = AtomicLong(SystemClock.elapsedRealtime())

  suspend fun run(): List<DiscoveredAdbDevice> = coroutineScope {
    val timers = CoroutineScope(coroutineContext + SupervisorJob(coroutineContext[Job]))
    val schedule: (Long, () -> Unit) -> Unit = { delayMs, action ->
      timers.launch {
        delay(delayMs)
        action()
      }
    }
    val resolver: Resolver =
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) CallbackResolver(schedule)
      else QueuedResolver(schedule)
    val multicast = acquireMulticastLock()
    val listeners = mutableListOf<NsdManager.DiscoveryListener>()
    try {
      for (type in discoveryServiceTypes(Build.VERSION.SDK_INT)) {
        val listener = discoveryListener(type, resolver)
        try {
          nsd.discoverServices(type, NsdManager.PROTOCOL_DNS_SD, listener)
          listeners += listener
        } catch (error: RuntimeException) {
          Log.w(TAG, "discovery start threw for $type", error)
          startFailures[type] = NsdManager.FAILURE_INTERNAL_ERROR
        }
      }
      awaitSettled(resolver)
    } finally {
      results.close()
      // A listener whose start failed is already unregistered and throws here; that's fine.
      listeners.forEach { runCatching { nsd.stopServiceDiscovery(it) } }
      resolver.close()
      timers.cancel()
      multicast?.let { lock -> runCatching { lock.release() } }
    }
    val devices = results.snapshot()
    startFailureMessage(devices.size, startFailures.toMap())?.let { throw IllegalStateException(it) }
    if (startFailures.isNotEmpty()) {
      Log.w(TAG, "discovery did not start for ${startFailures.keys}; returning the other types' results")
    }
    devices
  }

  private suspend fun awaitSettled(resolver: Resolver) {
    val started = SystemClock.elapsedRealtime()
    while (true) {
      val now = SystemClock.elapsedRealtime()
      val elapsed = now - started
      if (elapsed >= timeoutMs || allAdbDiscoveryFailed(startFailures.keys)) return
      if (ScanSettle.shouldStop(elapsed, results.count(), resolver.pending(), now - lastActivity.get())) return
      delay(POLL_MS)
    }
  }

  private fun touch() = lastActivity.set(SystemClock.elapsedRealtime())

  private fun discoveryListener(type: String, resolver: Resolver) = object : NsdManager.DiscoveryListener {
    override fun onDiscoveryStarted(serviceType: String) = Unit
    override fun onDiscoveryStopped(serviceType: String) = Unit
    override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) {
      Log.w(TAG, "discovery start failed for $type (errorCode=$errorCode)")
      startFailures[type] = errorCode
    }
    override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) {
      Log.w(TAG, "discovery stop failed for $type (errorCode=$errorCode)")
    }
    override fun onServiceFound(serviceInfo: NsdServiceInfo) {
      if (!belongsTo(serviceInfo.serviceType, type)) return
      val key = serviceKey(serviceInfo.serviceName, type) ?: return
      touch()
      resolver.found(key, serviceInfo)
    }
    // A pairing service is withdrawn the moment its dialog closes on the TV; listing it
    // would send the user to a dead port.
    override fun onServiceLost(serviceInfo: NsdServiceInfo) {
      if (!belongsTo(serviceInfo.serviceType, type)) return
      val key = serviceKey(serviceInfo.serviceName, type) ?: return
      touch()
      resolver.lost(key)
    }
  }

  private fun recordResolved(key: ServiceKey, info: NsdServiceInfo) {
    val addresses = info.addressList()
    val device = discoveredDevice(info.serviceName ?: key.name, key.serviceType, info.port, addresses, localKeys)
      ?.copy(friendlyName = if (key.serviceType == CAST_SERVICE_TYPE) castFriendlyName(info.attributes["fn"]) else null)
    results.upsert(key, device, addresses.map(::formatHost).toSet())
  }

  /** Below API 34: [SerialResolveQueue] feeds NsdManager one resolve at a time. */
  private inner class QueuedResolver(schedule: (Long, () -> Unit) -> Unit) : Resolver {
    private val infos = ConcurrentHashMap<ServiceKey, NsdServiceInfo>()
    private val resolvedKeys = ConcurrentHashMap.newKeySet<ServiceKey>()
    private val listeners = ConcurrentHashMap<Long, NsdManager.ResolveListener>()
    private var closed = false
    private val queue = SerialResolveQueue<ServiceKey>(
      start = ::startResolve,
      schedule = schedule,
      abandon = { _, token -> abandonResolve(token) },
      resolveTimeoutMs = RESOLVE_TIMEOUT_MS,
    )

    @Synchronized override fun found(key: ServiceKey, info: NsdServiceInfo) {
      // The same advert seen again (another interface or address family) is resolved once.
      if (closed || key.serviceType !in ADB_SERVICE_TYPES || key in resolvedKeys) return
      infos.putIfAbsent(key, info)
      queue.enqueue(key)
    }

    @Synchronized override fun lost(key: ServiceKey) {
      queue.remove(key)
      resolvedKeys.remove(key)
      infos.remove(key)
      results.remove(key)
    }

    override fun pending(): Int = queue.pending()

    @Synchronized override fun close() {
      closed = true
      queue.close()
    }

    @Synchronized private fun startResolve(key: ServiceKey, token: Long) {
      if (closed) return
      val info = infos[key] ?: return queue.failed(token, alreadyActive = false)
      val listener = object : NsdManager.ResolveListener {
        override fun onServiceResolved(resolved: NsdServiceInfo) {
          synchronized(this@QueuedResolver) {
            if (listeners.remove(token) !== this) return
            touch()
            if (infos[key] === info) {
              resolvedKeys += key
              recordResolved(key, resolved)
            }
          }
          queue.resolved(token)
        }
        override fun onResolveFailed(failed: NsdServiceInfo, errorCode: Int) {
          if (listeners.remove(token) !== this) return
          Log.w(TAG, "resolve failed for ${key.name} (errorCode=$errorCode)")
          queue.failed(token, alreadyActive = errorCode == NsdManager.FAILURE_ALREADY_ACTIVE)
        }
      }
      listeners[token] = listener
      try {
        @Suppress("DEPRECATION")
        nsd.resolveService(info, listener)
      } catch (error: RuntimeException) {
        listeners.remove(token)
        Log.w(TAG, "resolve threw for ${key.name}", error)
        queue.failed(token, alreadyActive = false)
      }
    }

    // Timed out or the scan ended. Where the platform can cancel a resolve, free its slot.
    @SuppressLint("NewApi")
    private fun abandonResolve(token: Long) {
      val listener = listeners.remove(token) ?: return
      if (tiramisuExtension() >= 7) runCatching { nsd.stopServiceResolution(listener) }
    }
  }

  /** API 34+: one ServiceInfoCallback per service, each unregistered on lost and on close. */
  @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
  private inner class CallbackResolver(private val schedule: (Long, () -> Unit) -> Unit) : Resolver {
    private val callbacks = HashMap<ServiceKey, NsdManager.ServiceInfoCallback>()
    private val awaiting = HashSet<ServiceKey>()
    private var closed = false

    override fun found(key: ServiceKey, info: NsdServiceInfo) {
      val callback = synchronized(this) {
        if (closed || key in callbacks) return
        val callback = serviceInfoCallback(key)
        // Registered under the lock so close() can never miss a callback and leak it.
        try {
          nsd.registerServiceInfoCallback(info, context.mainExecutor, callback)
        } catch (error: RuntimeException) {
          Log.w(TAG, "service info callback threw for ${key.name}", error)
          return
        }
        callbacks[key] = callback
        awaiting += key
        callback
      }
      schedule(RESOLVE_TIMEOUT_MS) {
        synchronized(this) { if (callbacks[key] === callback) awaiting.remove(key) }
      }
    }

    @Synchronized override fun lost(key: ServiceKey) {
      unregister(key)
      results.remove(key)
    }

    @Synchronized override fun pending(): Int = awaiting.size

    override fun close() {
      val remaining = synchronized(this) {
        closed = true
        awaiting.clear()
        callbacks.values.toList().also { callbacks.clear() }
      }
      remaining.forEach { runCatching { nsd.unregisterServiceInfoCallback(it) } }
    }

    private fun unregister(key: ServiceKey) {
      val callback = synchronized(this) {
        awaiting.remove(key)
        callbacks.remove(key)
      } ?: return
      runCatching { nsd.unregisterServiceInfoCallback(callback) }
    }

    private fun serviceInfoCallback(key: ServiceKey) = object : NsdManager.ServiceInfoCallback {
      override fun onServiceInfoCallbackRegistrationFailed(errorCode: Int) {
        Log.w(TAG, "service info callback failed for ${key.name} (errorCode=$errorCode)")
        synchronized(this@CallbackResolver) {
          if (callbacks[key] !== this) return
          awaiting.remove(key)
          callbacks.remove(key)
        }
      }
      override fun onServiceUpdated(serviceInfo: NsdServiceInfo) {
        synchronized(this@CallbackResolver) {
          if (closed || callbacks[key] !== this) return
          awaiting.remove(key)
          touch()
          recordResolved(key, serviceInfo)
        }
      }
      override fun onServiceLost() {
        synchronized(this@CallbackResolver) {
          if (callbacks[key] !== this) return
          touch()
          results.remove(key)
          unregister(key)
        }
      }
      override fun onServiceInfoCallbackUnregistered() {
        synchronized(this@CallbackResolver) {
          if (callbacks[key] === this) callbacks.remove(key)
        }
      }
    }
  }

  private fun acquireMulticastLock(): WifiManager.MulticastLock? {
    if (tiramisuExtension() >= 7) return null
    val wifi = context.getSystemService(Context.WIFI_SERVICE) as? WifiManager ?: return null
    return runCatching {
      wifi.createMulticastLock("atv-optimizer-mdns").apply {
        setReferenceCounted(false)
        acquire()
      }
    }.onFailure { Log.w(TAG, "multicast lock unavailable; discovery may miss TVs", it) }.getOrNull()
  }
}

/**
 * The "T extensions" level that decides two NsdManager behaviors: below 7 an app must hold
 * a Wi-Fi multicast lock itself to receive mDNS replies, even in the foreground (Android 12
 * and below, and Android 13 without that update), and it cannot cancel a resolve.
 */
private fun tiramisuExtension(): Int = when {
  Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU -> 0
  else -> SdkExtensions.getExtensionVersion(Build.VERSION_CODES.TIRAMISU)
}

private fun NsdServiceInfo.addressList(): List<InetAddress> {
  val reported: List<InetAddress> =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) hostAddresses
    else listOfNotNull(@Suppress("DEPRECATION") host)
  if (reported.isNotEmpty()) return reported
  val raw = attributes["adb"] ?: return emptyList()
  return listOfNotNull(runCatching { InetAddress.getByAddress(raw) }.getOrNull())
}

/** Every address on this phone's interfaces, so its own wireless-debugging advert is dropped. */
private fun localAddressKeys(): Set<String> =
  runCatching {
    NetworkInterface.getNetworkInterfaces()?.asSequence().orEmpty()
      .flatMap { it.inetAddresses.asSequence() }
      .filterNot { it.isLoopbackAddress }
      .map(::addressKey)
      .toSet()
  }.getOrDefault(emptySet())
