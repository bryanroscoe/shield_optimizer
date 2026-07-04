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
import java.util.Collections

class AdbService(private val context: Context) {
  private val mutex = Mutex()
  private val manager: AtvAdbConnectionManager by lazy {
    AtvAdbConnectionManager.getInstance(context)
  }

  suspend fun discover(timeoutMs: Long = 3_000): List<DiscoveredAdbDevice> = withContext(Dispatchers.IO) {
    val nsd = context.getSystemService(Context.NSD_SERVICE) as NsdManager
    val out = linkedMapOf<String, DiscoveredAdbDevice>()
    // Track which service types have resolved at least one entry. Mutated from NSD
    // resolve callbacks on a different thread, so keep it thread-safe.
    val resolvedTypes = Collections.synchronizedSet(mutableSetOf<String>())
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
    // Poll until both service types have resolved (a TV in pairing mode advertises both),
    // capped at timeoutMs. A TV NOT in pairing mode only advertises _adb-tls-connect, so
    // requiring one of EACH type keeps us waiting the full timeout for possible pairing
    // services rather than exiting early on connect-only results.
    val pollMs = 100L
    var waited = 0L
    while (waited < timeoutMs) {
      delay(pollMs)
      waited += pollMs
      if (resolvedTypes.containsAll(serviceTypes)) break
    }
    listeners.forEach { runCatching { nsd.stopServiceDiscovery(it) } }
    out.values.toList()
  }

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
