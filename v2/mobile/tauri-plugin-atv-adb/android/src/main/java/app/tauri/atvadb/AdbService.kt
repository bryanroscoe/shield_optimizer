package app.tauri.atvadb

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build
import android.util.Base64
import android.util.Log
import io.github.muntashirakon.adb.AdbStream
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.supervisorScope
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.net.InetAddress
import java.net.NetworkInterface
import java.util.Collections
import java.util.concurrent.atomic.AtomicLong

private const val TAG = "AtvAdb"

// AdbStream.read() blocks in mReadQueue.wait() with no time bound of its own; it only
// returns EOF when libadb's background reader routes a peer A_CLSE to the stream. The
// connection manager's setTimeout(30s) governs only the connect handshake, not stream
// reads. Cap a single service read so a peer that never sends CLSE (empirically: the
// legacy Nvidia Shield's adbd, over libadb's demux) can't hang the diagnostic forever.
// Kept short so a stuck read surfaces in the UI fast instead of appearing frozen.
private const val STREAM_READ_TIMEOUT_MS = 8_000L

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
      try {
        Log.i(TAG, "connect $host:$port")
        val connected = manager.connect(host, port)
        Log.i(TAG, "connect $host:$port -> connected=$connected isConnected=${manager.isConnected}")
        if (!connected && !manager.isConnected) error("Could not connect to $host:$port.")
        ConnectResponse(serial = "$host:$port", host = host, port = port, message = "Connected to $host:$port.")
      } catch (e: Throwable) {
        Log.e(TAG, "connect $host:$port failed", e)
        throw e
      }
    }
  }

  suspend fun disconnect() = mutex.withLock {
    withContext(Dispatchers.IO) {
      runCatching { manager.close() }
    }
  }

  suspend fun shell(command: String): AdbCommandOutput = mutex.withLock {
    withContext(Dispatchers.IO) {
      Log.i(TAG, "shell start: $command")
      // Run one-shot commands over the `exec:` service, not `shell:`. The interactive
      // `shell:` service runs the command under a pty and — on some adbd builds, notably
      // the legacy Nvidia Shield — does not reliably send a CLSE when a one-shot command
      // with sizable output (e.g. `dumpsys`) exits. AdbStream.read() only reaches EOF on
      // that peer CLSE and otherwise blocks forever, which hangs the whole diagnostic.
      // `exec:` runs the command without a pty on a raw pipe and closes cleanly on exit —
      // the same service `adb exec-out` uses.
      //
      // stderr caveat: `exec:` returns stdout only (no stderr merge), so `stderr` below is
      // always empty. The core's AdbOutput.shell_reported_failure() scans combined
      // stdout+stderr for pm/settings error markers; those tools write their errors to
      // stdout, so the heuristic still fires. Revisit if a command whose failures surface
      // only on stderr is ever routed through here.
      // `exec:` carries no exit status (the stream always closes 0) and no
      // stderr, so AdbOutput.success() was always true on mobile. Append an
      // exit-code marker — `; echo "__EXIT__$?"` — then recover the real code
      // from the trailing marker and strip it from stdout. If the marker is
      // missing or unparseable we fall back to exitCode 0 and warn (never crash).
      val marked = "$command; echo \"__EXIT__\$?\""
      val bytes = readServiceOutput("exec:$marked")
      val raw = bytes.toString(Charsets.UTF_8)
      Log.i(TAG, "shell done (${bytes.size}B): $command")
      parseExitMarker(raw)
    }
  }

  /** Split the trailing `__EXIT__<n>` marker off [raw], returning real stdout + exit code. */
  private fun parseExitMarker(raw: String): AdbCommandOutput {
    val marker = "__EXIT__"
    val idx = raw.lastIndexOf(marker)
    if (idx < 0) {
      Log.w(TAG, "shell: exit marker not found; defaulting exitCode=0")
      return AdbCommandOutput(stdout = raw, stderr = "", exitCode = 0)
    }
    val code = raw.substring(idx + marker.length).trim().toIntOrNull()
    // Everything before the marker is the command's own output; the echo added a
    // single trailing newline that we drop to restore the real stdout.
    var stdout = raw.substring(0, idx)
    if (stdout.endsWith("\n")) stdout = stdout.dropLast(1)
    if (stdout.endsWith("\r")) stdout = stdout.dropLast(1)
    if (code == null) {
      Log.w(TAG, "shell: unparseable exit marker; defaulting exitCode=0")
      return AdbCommandOutput(stdout = stdout, stderr = "", exitCode = 0)
    }
    return AdbCommandOutput(stdout = stdout, stderr = "", exitCode = code)
  }

  suspend fun screencap(): String = mutex.withLock {
    withContext(Dispatchers.IO) {
      // Use exec: not shell: so screencap runs without a pty — binary PNG output must
      // avoid the pty's LF->CRLF translation. This is what `adb exec-out` does under the hood.
      val bytes = readServiceOutput("exec:screencap -p")
      Base64.encodeToString(bytes, Base64.NO_WRAP)
    }
  }

  /**
   * Opens [destination], reads it to EOF, and returns the bytes, bounded by
   * [STREAM_READ_TIMEOUT_MS].
   *
   * AdbStream.read() parks in mReadQueue.wait() until libadb's background reader routes a
   * peer A_CLSE to the stream, and does not reliably honor coroutine cancellation via a
   * thread interrupt (an earlier runInterruptible attempt never fired at 87s on the legacy
   * Shield). So we do NOT rely on interruption: the read runs in its own [async] job and we
   * time out the [await]. On timeout we call stream.close() first — that sends an A_CLSE and
   * sets mIsClosed, which unblocks the parked read (it throws "Stream closed.") so the job
   * actually ends and adbd's side of the stream is torn down — then throw. A supervisorScope
   * keeps the read job's post-close IOException from racing/propagating over our timeout.
   */
  private suspend fun readServiceOutput(destination: String): ByteArray {
    val stream = manager.openStream(destination)
    val bytesRead = AtomicLong(0)
    return supervisorScope {
      val readJob = async(Dispatchers.IO) { readStream(stream, bytesRead) }
      try {
        withTimeout(STREAM_READ_TIMEOUT_MS) { readJob.await() }
      } catch (e: TimeoutCancellationException) {
        val seen = bytesRead.get()
        val detail = if (seen == 0L) "no bytes before timeout" else "$seen bytes arrived but stream never closed (no EOF)"
        Log.w(TAG, "readStream: timed out after ${STREAM_READ_TIMEOUT_MS}ms — $detail; closing $destination")
        // Close synchronously (uncancellable) BEFORE throwing: closing sends CLSE
        // and unblocks the parked read. A `launch` here would be cancelled by the
        // throw before it runs, leaking the parked read thread for the process life.
        withContext(NonCancellable) { runCatching { stream.close() } }
        readJob.cancel()
        throw IOException("Timed out after ${STREAM_READ_TIMEOUT_MS}ms reading $destination")
      }
    }
  }

  private fun readStream(stream: AdbStream, bytesRead: AtomicLong): ByteArray {
    stream.openInputStream().use { input ->
      val output = ByteArrayOutputStream()
      val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
      var loggedFirst = false
      while (true) {
        val read = input.read(buffer)
        if (!loggedFirst) {
          loggedFirst = true
          if (read < 0) Log.i(TAG, "readStream: first read returned -1 (immediate EOF, 0B)")
          else Log.i(TAG, "readStream: first read returned ${read}B")
        }
        if (read < 0) break
        output.write(buffer, 0, read)
        bytesRead.addAndGet(read.toLong())
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
