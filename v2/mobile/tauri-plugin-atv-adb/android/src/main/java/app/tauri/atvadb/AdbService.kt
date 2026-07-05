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
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
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
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

private const val TAG = "AtvAdb"

// AdbStream.read() blocks in mReadQueue.wait() with no time bound of its own; it only
// returns EOF when libadb's background reader routes a peer A_CLSE to the stream. The
// connection manager's setTimeout(30s) governs only the connect handshake, not stream
// reads. Cap a single service read so a peer that never sends CLSE (empirically: the
// legacy Nvidia Shield's adbd, over libadb's demux) can't hang the diagnostic forever.
// This is the OUTER (hard) bound: it only fails a read that delivered ZERO bytes. A read
// that delivered data then went idle completes fast via the idle-EOF path below.
private const val STREAM_READ_TIMEOUT_MS = 8_000L

// Soft/idle EOF. The legacy Shield's adbd delivers a command's full output over `exec:`
// but frequently never sends the peer A_CLSE, so AdbStream.read() never reaches EOF and
// would burn the full STREAM_READ_TIMEOUT_MS. Instead: once bytes have arrived, if the
// stream goes quiet (no new bytes, no CLSE) for IDLE_EOF_MS, treat the output as COMPLETE
// and close the stream to unblock the parked read — returning what we have as success.
// This turns meminfo/hardware_properties from 8s failures into ~700ms successes. A stream
// that never delivers ANY bytes still fails at the hard cap (a real transport error).
private const val IDLE_EOF_MS = 700L

// How often the idle supervisor samples the last-byte timestamp.
private const val IDLE_POLL_MS = 150L

class AdbService(private val context: Context) {
  // One global lock serializes every stream op (connect/pair/disconnect/shell/screencap).
  // We deliberately KEEP it as a full mutex rather than demoting it to a lifecycle-only
  // lock that would allow concurrent shells. Rationale: the "no-CLSE" behavior this file
  // works around is itself a demux-reliability doubt on the legacy Shield, and running
  // concurrent `exec:` streams over libadb's single multiplexed socket could make that
  // worse (interleaved A_WRTE/A_CLSE routing is exactly the fragile path). The idle-EOF
  // fix below makes each command return in ~700ms instead of 8s, so serialization is now
  // cheap: health_report's ~6 commands finish in ~4s, and the `wireless_status` liveness
  // probe (`echo ok`, which emits bytes immediately then idles) and the device-name
  // getprop no longer starve behind a stack of 8s reads. Revisit concurrency only if
  // concurrent streams are proven to demux correctly on these devices.
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
      // stderr, so AdbOutput.success() was always true on mobile. We append an
      // exit-code marker — `; echo __EXIT__$?` — then recover the real code from
      // the trailing marker and strip it from stdout.
      //
      // The marker MUST run through a shell. On this legacy Shield, `exec:<cmd>`
      // ran the command directly (not via a shell), so the `; echo` compound was
      // never evaluated — hence the log's "exit marker not found" even for
      // commands that produced full output. Wrapping in `sh -c '<cmd>; echo …'`
      // guarantees both the command and the marker echo run under a shell. If the
      // marker is missing (e.g. the idle-EOF path closed the stream before the
      // echo line arrived) or unparseable we quietly fall back to exitCode 0.
      val bytes = readServiceOutput(shellExecDestination(command))
      val raw = bytes.toString(Charsets.UTF_8)
      Log.i(TAG, "shell done (${bytes.size}B): $command")
      parseExitMarker(raw)
    }
  }

  /**
   * Wrap [command] so it runs under a shell and emits a trailing `__EXIT__<code>` marker.
   *
   * `exec:` does not reliably run a compound command through a shell on all adbd builds
   * (the legacy Shield runs the argument directly), so we wrap explicitly in `sh -c`. The
   * script is single-quoted for adbd; embedded single quotes are escaped `'\''`.
   */
  private fun shellExecDestination(command: String): String {
    val script = "$command; echo __EXIT__\$?"
    val escaped = script.replace("'", "'\\''")
    return "exec:sh -c '$escaped'"
  }

  /** Split the trailing `__EXIT__<n>` marker off [raw], returning real stdout + exit code. */
  private fun parseExitMarker(raw: String): AdbCommandOutput {
    val marker = "__EXIT__"
    val idx = raw.lastIndexOf(marker)
    if (idx < 0) {
      // Expected on the idle-EOF path when the stream closed before the echo line
      // arrived. Default to 0 quietly — no warning spam per successful command.
      Log.i(TAG, "shell: no exit marker (idle-EOF or non-shell exec:); defaulting exitCode=0")
      return AdbCommandOutput(stdout = raw, stderr = "", exitCode = 0)
    }
    val code = raw.substring(idx + marker.length).trim().toIntOrNull()
    // Everything before the marker is the command's own output; the echo added a
    // single trailing newline that we drop to restore the real stdout.
    var stdout = raw.substring(0, idx)
    if (stdout.endsWith("\n")) stdout = stdout.dropLast(1)
    if (stdout.endsWith("\r")) stdout = stdout.dropLast(1)
    if (code == null) {
      Log.i(TAG, "shell: unparseable exit marker; defaulting exitCode=0")
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
   * Opens [destination], reads it to EOF (or idle-EOF), and returns the bytes.
   *
   * AdbStream.read() parks in mReadQueue.wait() until libadb's background reader routes a
   * peer A_CLSE to the stream, and does not reliably honor coroutine cancellation via a
   * thread interrupt (an earlier runInterruptible attempt never fired at 87s on the legacy
   * Shield). So we do NOT rely on interruption. The blocking read runs in its own [async]
   * job that accumulates into [output] and records [lastByteAt]; a supervising [launch]
   * timer watches for idleness. Three ways this completes:
   *
   *  1. Clean EOF — the peer sent A_CLSE, read() returned -1. Normal, fastest path.
   *  2. Idle-EOF (the legacy-Shield case) — bytes arrived, then no new bytes for
   *     [IDLE_EOF_MS] and no CLSE. The idle watcher closes the stream (which unblocks the
   *     parked read); we return the accumulated bytes as SUCCESS. Logged "idle-complete".
   *  3. Hard timeout at [STREAM_READ_TIMEOUT_MS] — the outer bound. If ANY bytes arrived we
   *     still return them as success (large continuous output that never idled); only a read
   *     that delivered ZERO bytes is treated as a transport error (IOException).
   *
   * [closedByUs] tells [readStream] that a close it observes was our own idle/hard-cap
   * close, so it should treat the accumulated bytes as the complete output rather than
   * re-throwing the "Stream closed." IOException.
   */
  private suspend fun readServiceOutput(destination: String): ByteArray {
    val stream = manager.openStream(destination)
    val output = ByteArrayOutputStream()
    val bytesRead = AtomicLong(0)
    val lastByteAt = AtomicLong(0) // 0 = no bytes seen yet
    val closedByUs = AtomicBoolean(false)
    return supervisorScope {
      val readJob = async(Dispatchers.IO) { readStream(stream, output, bytesRead, lastByteAt, closedByUs) }
      // Idle supervisor: once data has flowed, if the stream goes quiet with no CLSE for
      // IDLE_EOF_MS, close it to unblock the parked read and complete successfully.
      val idleWatcher = launch(Dispatchers.IO) {
        while (isActive) {
          delay(IDLE_POLL_MS)
          val last = lastByteAt.get()
          if (last != 0L && System.currentTimeMillis() - last > IDLE_EOF_MS) {
            Log.i(TAG, "readStream: idle-complete (${bytesRead.get()}B, no CLSE after ${IDLE_EOF_MS}ms idle); $destination")
            closedByUs.set(true)
            withContext(NonCancellable) { runCatching { stream.close() } }
            break
          }
        }
      }
      try {
        withTimeout(STREAM_READ_TIMEOUT_MS) { readJob.await() }
        idleWatcher.cancel()
        output.toByteArray()
      } catch (e: TimeoutCancellationException) {
        idleWatcher.cancel()
        val seen = bytesRead.get()
        // Close synchronously (uncancellable) BEFORE throwing: closing sends CLSE
        // and unblocks the parked read. A `launch` here would be cancelled by the
        // throw before it runs, leaking the parked read thread for the process life.
        closedByUs.set(true)
        withContext(NonCancellable) { runCatching { stream.close() } }
        readJob.cancel()
        if (seen > 0L) {
          // Bytes flowed continuously up to the hard cap without ever idling (large
          // dump). Return what we captured as a successful — if truncated — read;
          // the diagnostic parsers tolerate partial output.
          Log.w(TAG, "readStream: hard cap ${STREAM_READ_TIMEOUT_MS}ms reached, $seen bytes still flowing (no CLSE); returning as complete; $destination")
          output.toByteArray()
        } else {
          Log.w(TAG, "readStream: timed out after ${STREAM_READ_TIMEOUT_MS}ms, 0 bytes; $destination")
          throw IOException("Timed out after ${STREAM_READ_TIMEOUT_MS}ms reading $destination")
        }
      }
    }
  }

  private fun readStream(
    stream: AdbStream,
    output: ByteArrayOutputStream,
    bytesRead: AtomicLong,
    lastByteAt: AtomicLong,
    closedByUs: AtomicBoolean,
  ) {
    stream.openInputStream().use { input ->
      val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
      var loggedFirst = false
      while (true) {
        val read = try {
          input.read(buffer)
        } catch (e: IOException) {
          // The idle watcher / hard-cap path closed the stream to unblock this parked
          // read; the bytes already accumulated are the complete output.
          if (closedByUs.get()) break else throw e
        }
        if (!loggedFirst) {
          loggedFirst = true
          if (read < 0) Log.i(TAG, "readStream: first read returned -1 (immediate EOF, 0B)")
          else Log.i(TAG, "readStream: first read returned ${read}B")
        }
        if (read < 0) break
        output.write(buffer, 0, read)
        bytesRead.addAndGet(read.toLong())
        lastByteAt.set(System.currentTimeMillis())
      }
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
