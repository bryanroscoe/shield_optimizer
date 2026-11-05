// Derived in part from Kadb (Apache-2.0, https://github.com/flyfishxu/Kadb): the
// service-type normalization, the (name, type) endpoint key and the IPv4-first address
// choice follow kadb-mdns MdnsServiceTypeParser.kt, MdnsServiceKey.kt,
// NsdServiceInfoMapper.kt and MdnsRegistry.kt.
package app.tauri.atvadb

import java.net.Inet4Address
import java.net.Inet6Address
import java.net.InetAddress
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction

// Pure discovery decisions, kept free of android.* so they run as plain JVM unit tests.

/**
 * TLS pairing/connect are Android 11+ "Wireless debugging"; `_adb._tcp` is the legacy
 * "Network debugging" (:5555) that Shield and older TVs advertise, which needs no pairing
 * code, just an RSA-auth connect.
 */
internal val ADB_SERVICE_TYPES = listOf("_adb-tls-pairing._tcp.", "_adb-tls-connect._tcp.", "_adb._tcp.")
internal const val CAST_SERVICE_TYPE = "_googlecast._tcp."
internal val DISCOVERY_SERVICE_TYPES = ADB_SERVICE_TYPES + CAST_SERVICE_TYPE

/**
 * NsdManager reports one type as `_adb._tcp.`, `_adb._tcp`, `._adb._tcp` (a legacy resolve
 * quirk) or with `.local` depending on the Android release, so exact string comparison
 * silently drops services.
 */
internal fun normalizeServiceType(type: String?): String =
  type.orEmpty().trim().lowercase().trim('.').removeSuffix(".local").trim('.')

/** The supported DNS-SD service [type] names, or null for anything else. */
internal fun canonicalServiceType(type: String?): String? {
  val wanted = normalizeServiceType(type)
  return DISCOVERY_SERVICE_TYPES.firstOrNull { normalizeServiceType(it) == wanted }
}

/** Cast's `fn` TXT field is a display label, never a hardware identity. */
internal fun castFriendlyName(value: ByteArray?): String? {
  if (value == null || value.isEmpty() || value.size > 255) return null
  val decoded = runCatching {
    Charsets.UTF_8.newDecoder()
      .onMalformedInput(CodingErrorAction.REPORT)
      .onUnmappableCharacter(CodingErrorAction.REPORT)
      .decode(ByteBuffer.wrap(value)).toString()
  }.getOrNull() ?: return null
  if (decoded.any { it.isISOControl() }) return null
  return decoded.trim().takeIf { it.isNotEmpty() }
}

/** One advertised service, however many interfaces or address families it arrives on. */
internal data class ServiceKey(val name: String, val serviceType: String)

/** Null for a nameless record, which no DNS-SD responder sends for a real service. */
internal fun serviceKey(name: String?, serviceType: String): ServiceKey? {
  val trimmed = name?.trim().orEmpty()
  if (trimmed.isEmpty()) return null
  return ServiceKey(trimmed, serviceType)
}

/**
 * Whether a found or lost record belongs to the listener for [listenerType]. A type the
 * platform reports in a form we cannot parse is given the benefit of the doubt.
 */
internal fun belongsTo(reportedType: String?, listenerType: String): Boolean {
  val reported = canonicalServiceType(reportedType) ?: return true
  return reported == listenerType
}

/** Scope-free textual form, so an address read from an interface and one from mDNS compare equal. */
internal fun addressKey(address: InetAddress): String =
  InetAddress.getByAddress(address.address).hostAddress.orEmpty()

/**
 * The host string to connect to. The connect path parses `"$host:$port"` as a socket
 * address, which needs IPv6 in brackets and accepts only a numeric scope id.
 */
internal fun formatHost(address: InetAddress): String = when (address) {
  is Inet6Address -> {
    val bare = address.hostAddress.orEmpty().substringBefore('%')
    if (address.isLinkLocalAddress && address.scopeId > 0) "[$bare%${address.scopeId}]" else "[$bare]"
  }
  else -> address.hostAddress.orEmpty()
}

/**
 * Picks the address to list for a resolved service: IPv4 first, then routable IPv6, then
 * link-local IPv6, so an IPv6-only TV is still listed. Null when the advert is this phone's
 * own (any of its addresses is local: phones advertise wireless debugging too) or it has
 * no usable address.
 */
internal fun selectHost(addresses: List<InetAddress>, localKeys: Set<String>): String? {
  if (addresses.any { addressKey(it) in localKeys }) return null
  val usable = addresses.filterNot { it.isLoopbackAddress || it.isAnyLocalAddress || it.isMulticastAddress }
  val chosen = usable.firstOrNull { it is Inet4Address }
    ?: usable.firstOrNull { it is Inet6Address && !it.isLinkLocalAddress }
    ?: usable.firstOrNull()
    ?: return null
  return formatHost(chosen)
}

internal fun discoveredDevice(
  name: String?,
  serviceType: String,
  port: Int,
  addresses: List<InetAddress>,
  localKeys: Set<String>,
): DiscoveredAdbDevice? {
  if (port !in 1..65535) return null
  val host = selectHost(addresses, localKeys) ?: return null
  val label = name?.trim().takeUnless { it.isNullOrEmpty() } ?: "Wireless debugging"
  return DiscoveredAdbDevice(name = label, host = host, port = port, service = serviceType)
}

/** Resolved services keyed by [ServiceKey]; written from NSD callback threads. */
internal class DiscoveredServices {
  private data class Resolved(val device: DiscoveredAdbDevice, val addresses: Set<String>)
  private val entries = linkedMapOf<ServiceKey, Resolved>()
  private var closed = false

  /** A null [device] (now unusable, or this phone's own) removes any earlier entry. */
  @Synchronized fun upsert(
    key: ServiceKey,
    device: DiscoveredAdbDevice?,
    addresses: Set<String> = setOfNotNull(device?.host),
  ) {
    if (closed) return
    if (device == null) entries.remove(key) else entries[key] = Resolved(device, addresses)
  }

  @Synchronized fun remove(key: ServiceKey) {
    if (closed) return
    entries.remove(key)
  }

  @Synchronized fun close() { closed = true }

  @Synchronized fun count(): Int = entries.values.count { it.device.service in ADB_SERVICE_TYPES }

  @Synchronized fun snapshot(): List<DiscoveredAdbDevice> {
    val names = entries.values.filter { it.device.service == CAST_SERVICE_TYPE }
    return entries.values.filter { it.device.service in ADB_SERVICE_TYPES }.map { adb ->
      // This association only labels the current endpoint. It never changes the
      // ADB instance/serial used to recognize a previously verified saved TV.
      val advertisedNames = names.filter { cast ->
        cast.addresses.any { it in adb.addresses }
      }.mapNotNull { it.device.friendlyName }.toSet()
      adb.device.copy(friendlyName = advertisedNames.singleOrNull())
    }
  }
}

/**
 * When a one-shot scan may stop before its timeout. Which service types have resolved says
 * nothing about how many TVs are on the network (one TV usually advertises one or two
 * types, and a second TV's records can trail the first's), so the scan waits until it has
 * a result, a minimum window has passed, no resolve is still queued or in flight, and
 * nothing new has been found, resolved or lost for a short quiet period.
 */
internal object ScanSettle {
  const val MIN_SCAN_MS = 1_500L
  const val QUIET_MS = 500L

  fun shouldStop(elapsedMs: Long, resolved: Int, pending: Int, quietForMs: Long): Boolean =
    resolved > 0 && pending == 0 && elapsedMs >= MIN_SCAN_MS && quietForMs >= QUIET_MS
}

/**
 * Discovery that could not start for a service type must not read as "no TVs found".
 * Returns the error to report, or null when the results can be returned as they are.
 */
internal fun startFailureMessage(foundCount: Int, failures: Map<String, Int>): String? {
  val adbFailures = failures.filterKeys { it in ADB_SERVICE_TYPES }
  if (adbFailures.isEmpty() || foundCount > 0) return null
  val detail = adbFailures.entries.joinToString(", ") { (type, code) -> "${normalizeServiceType(type)} error $code" }
  return "This phone's network discovery could not start ($detail). Check that Wi-Fi is on and " +
    "try again, or enter the TV's IP address."
}

internal fun allAdbDiscoveryFailed(failures: Set<String>): Boolean = ADB_SERVICE_TYPES.all { it in failures }

/**
 * One NSD resolve at a time. Below API 34 NsdManager rejects a second concurrent resolve
 * from the same app with FAILURE_ALREADY_ACTIVE rather than queueing it, so firing one per
 * found service loses every TV after the first.
 *
 * [start] issues the resolve for a key and must report back through [resolved] or [failed]
 * with the token it was given; a resolve that never reports is abandoned after
 * [resolveTimeoutMs] and [abandon] is offered the key to cancel it where the platform can.
 * Callbacks carrying a stale token are ignored.
 */
internal class SerialResolveQueue<K : Any>(
  private val start: (key: K, token: Long) -> Unit,
  private val schedule: (delayMs: Long, action: () -> Unit) -> Unit,
  private val abandon: (key: K, token: Long) -> Unit = { _, _ -> },
  private val resolveTimeoutMs: Long = 1_000,
  private val maxAttempts: Int = 4,
  private val retryDelayMs: Long = 200,
  private val priority: (K) -> Int = { 0 },
) {
  private class Active<K>(val key: K, val attempt: Int, val token: Long)

  private val queue = ArrayDeque<K>()
  private var active: Active<K>? = null
  private var nextToken = 0L
  private var closed = false

  fun enqueue(key: K) {
    synchronized(this) {
      if (closed || key in queue || active?.key == key) return
      queue.addLast(key)
    }
    pump()
  }

  /** A lost service must not publish through a resolve that finishes later. */
  fun remove(key: K) {
    val inFlight = synchronized(this) {
      queue.remove(key)
      active?.takeIf { it.key == key }?.also { active = null }
    }
    inFlight?.let { abandon(it.key, it.token) }
    pump()
  }

  /** Queued plus in-flight resolves; a retry waiting out its backoff counts as in flight. */
  @Synchronized fun pending(): Int = queue.size + if (active != null) 1 else 0

  fun resolved(token: Long) = finish(token)

  /** [alreadyActive] is FAILURE_ALREADY_ACTIVE: the slot was busy, so try the same key again. */
  fun failed(token: Long, alreadyActive: Boolean) {
    val retry = synchronized(this) {
      val current = active?.takeIf { it.token == token } ?: return
      if (closed || !alreadyActive || current.attempt >= maxAttempts) {
        active = null
        null
      } else {
        current
      }
    }
    if (retry == null) {
      pump()
      return
    }
    schedule(retryDelayMs) { restart(retry) }
  }

  /** Stops the queue for good. A resolve still in flight is offered to [abandon]. */
  fun close() {
    val inFlight = synchronized(this) {
      closed = true
      queue.clear()
      active.also { active = null }
    }
    inFlight?.let { abandon(it.key, it.token) }
  }

  private fun restart(previous: Active<K>) {
    val next = synchronized(this) {
      if (closed || active?.token != previous.token) return
      Active(previous.key, previous.attempt + 1, ++nextToken).also { active = it }
    }
    launch(next)
  }

  private fun finish(token: Long) {
    synchronized(this) {
      if (active?.token != token) return
      active = null
    }
    pump()
  }

  private fun pump() {
    val next = synchronized(this) {
      if (closed || active != null) return
      val key = queue.minByOrNull(priority) ?: return
      queue.remove(key)
      Active(key, 1, ++nextToken).also { active = it }
    }
    launch(next)
  }

  private fun launch(entry: Active<K>) {
    schedule(resolveTimeoutMs) { timeout(entry.token) }
    start(entry.key, entry.token)
  }

  private fun timeout(token: Long) {
    val expired = synchronized(this) {
      val current = active?.takeIf { it.token == token } ?: return
      active = null
      current
    }
    abandon(expired.key, expired.token)
    pump()
  }
}
