package app.tauri.atvadb

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build
import android.util.Base64
import io.github.muntashirakon.adb.AdbStream
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream
import java.net.InetAddress
import java.net.NetworkInterface
import java.util.Collections

class AdbService(private val context: Context) {
  private val mutex = Mutex()
  private val manager: AtvAdbConnectionManager by lazy {
    AtvAdbConnectionManager.getInstance(context)
  }

  suspend fun discover(timeoutMs: Long = 3_000): List<DiscoveredAdbDevice> = withContext(Dispatchers.IO) {
    val nsd = context.getSystemService(Context.NSD_SERVICE) as NsdManager
    // Mutated from NSD resolve callbacks on a different thread, so keep it thread-safe.
    val out = Collections.synchronizedMap(linkedMapOf<String, DiscoveredAdbDevice>())
    val resolvedTypes = Collections.synchronizedSet(mutableSetOf<String>())
    // This phone advertises its own wireless-debugging service when USB/Wi-Fi debugging
    // is on; drop anything resolving to a local interface so we never list ourselves.
    val locals = localAddresses()
    val serviceTypes = listOf("_adb-tls-pairing._tcp.", "_adb-tls-connect._tcp.")
    val listeners = serviceTypes.map { serviceType ->
      object : NsdManager.DiscoveryListener {
        override fun onDiscoveryStarted(regType: String) = Unit
        override fun onDiscoveryStopped(serviceType: String) = Unit
        override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) = Unit
        override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) = Unit
        override fun onServiceLost(serviceInfo: NsdServiceInfo) = Unit
        override fun onServiceFound(serviceInfo: NsdServiceInfo) {
          if (serviceInfo.serviceType != serviceType) return
          nsd.resolveService(serviceInfo, object : NsdManager.ResolveListener {
            override fun onResolveFailed(serviceInfo: NsdServiceInfo, errorCode: Int) = Unit
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

  suspend fun pair(host: String, port: Int, code: String): ConnectResponse = mutex.withLock {
    withContext(Dispatchers.IO) {
      ensureAndroid11("Pairing requires Android 11 or newer on the phone.")
      val paired = manager.pair(host, port, code)
      if (!paired) error("Pairing failed. Check the pairing code and port shown on the TV.")
      ConnectResponse(serial = "$host:$port", host = host, port = port, message = "Paired with $host:$port.")
    }
  }

  suspend fun connect(host: String, port: Int): ConnectResponse = mutex.withLock {
    withContext(Dispatchers.IO) {
      val connected = manager.connect(host, port)
      if (!connected && !manager.isConnected) error("Could not connect to $host:$port.")
      ConnectResponse(serial = "$host:$port", host = host, port = port, message = "Connected to $host:$port.")
    }
  }

  suspend fun disconnect() = mutex.withLock {
    withContext(Dispatchers.IO) {
      runCatching { manager.close() }
    }
  }

  suspend fun shell(command: String): AdbCommandOutput = mutex.withLock {
    withContext(Dispatchers.IO) {
      val bytes = readStream(manager.openStream("shell:$command"))
      AdbCommandOutput(stdout = bytes.toString(Charsets.UTF_8), stderr = "", exitCode = 0)
    }
  }

  suspend fun screencap(): String = mutex.withLock {
    withContext(Dispatchers.IO) {
      // Use exec: not shell: so screencap runs without a pty — binary PNG output must
      // avoid the pty's LF->CRLF translation. This is what `adb exec-out` does under the hood.
      val bytes = readStream(manager.openStream("exec:screencap -p"))
      Base64.encodeToString(bytes, Base64.NO_WRAP)
    }
  }

  private fun readStream(stream: AdbStream): ByteArray {
    stream.openInputStream().use { input ->
      val output = ByteArrayOutputStream()
      val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
      while (true) {
        val read = input.read(buffer)
        if (read < 0) break
        output.write(buffer, 0, read)
      }
      return output.toByteArray()
    }
  }

  private fun ensureAndroid11(message: String) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) error(message)
  }
}

data class AdbCommandOutput(val stdout: String, val stderr: String, val exitCode: Int)
data class ConnectResponse(val serial: String, val host: String, val port: Int, val message: String)
data class DiscoveredAdbDevice(val name: String, val host: String, val port: Int, val service: String)

private val NsdServiceInfo.hostAddress: String?
  get() = host?.hostAddress ?: attributes["adb"]?.let { InetAddress.getByAddress(it).hostAddress }
