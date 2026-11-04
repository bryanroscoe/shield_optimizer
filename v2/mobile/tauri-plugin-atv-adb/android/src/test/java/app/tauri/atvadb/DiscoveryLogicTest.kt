package app.tauri.atvadb

import java.net.Inet6Address
import java.net.InetAddress
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class DiscoveryLogicTest {
  private val connect = "_adb-tls-connect._tcp."
  private fun ip(literal: String): InetAddress = InetAddress.getByName(literal)

  @Test fun metadataCannotTakeTheNativeAdbSlotWhenCancellationIsUnavailable() {
    val named = unresolvedMetadata("Study TV", REMOTE_SERVICE, false)!!
    assertEquals("Study TV", named.friendlyName)
    assertEquals("", named.host)
    assertEquals(0, named.port)
    assertNull(unresolvedMetadata("Cast-id", CAST_SERVICE, false))
    assertNull(unresolvedMetadata("adb-A", connect, false))
    assertNull(unresolvedMetadata("Study TV", REMOTE_SERVICE, true))
  }

  @Test fun friendlyNamesComeFromTheRightProtocol() {
    val attrs = mapOf("fn" to "Study TV".toByteArray(), "ca" to "5".toByteArray())
    assertEquals("Study TV", advertisedFriendlyName(CAST_SERVICE, "Cast-id", attrs))
    assertEquals("Study TV", advertisedFriendlyName(REMOTE_SERVICE, "Study TV", emptyMap()))
    assertNull(advertisedFriendlyName(connect, "adb-serial", attrs))
    assertNull(advertisedFriendlyName(CAST_SERVICE, "Cast-id", attrs + ("ca" to "4".toByteArray())))
    assertNull(advertisedFriendlyName(CAST_SERVICE, "Cast-id", emptyMap()))
    assertNull(advertisedFriendlyName(CAST_SERVICE, "Cast-id", attrs + ("ca" to "-1".toByteArray())))
    assertNull(advertisedFriendlyName(CAST_SERVICE, "Cast-id", attrs + ("ca" to "4294967296".toByteArray())))
    assertNull(advertisedFriendlyName(CAST_SERVICE, "Cast-id", attrs + ("fn" to byteArrayOf(0xc3.toByte(), 0x28))))
    assertNull(displayName("bad\nname"))
    assertNull(displayName("bad\u202ename"))
    assertNull(displayName("x".repeat(129)))
    assertEquals("Café TV", displayName(" Café TV "))
    assertEquals(CAST_SERVICE, canonicalServiceType("_GOOGLECAST._tcp.local."))
    assertEquals(REMOTE_SERVICE, canonicalServiceType("_androidtvremote2._tcp"))
  }

  @Test fun serviceTypesNormalizeAcrossPlatformSpellings() {
    for (spelling in listOf("_adb._tcp.", "_adb._tcp", "._adb._tcp", "_ADB._tcp.local.", " _adb._tcp.local ")) {
      assertEquals(spelling, "_adb._tcp.", canonicalServiceType(spelling))
    }
    assertEquals(connect, canonicalServiceType("_adb-tls-connect._tcp"))
    assertNull(canonicalServiceType("_http._tcp."))
    assertNull(canonicalServiceType(null))
  }

  @Test fun recordsOfAnotherTypeAreNotAccepted() {
    assertTrue(belongsTo("_adb-tls-connect._tcp", connect))
    assertFalse(belongsTo("_adb-tls-pairing._tcp.", connect))
    assertTrue(belongsTo("garbled", connect))
  }

  @Test fun ipv4IsPreferredWhateverTheOrder() {
    val host = selectHost(listOf(ip("2001:db8::5"), ip("fe80::1"), ip("192.168.1.20")), emptySet())
    assertEquals("192.168.1.20", host)
  }

  @Test fun anIpv6OnlyServiceIsKeptAndBracketed() {
    assertEquals("[2001:db8:0:0:0:0:0:5]", selectHost(listOf(ip("fe80::1"), ip("2001:db8::5")), emptySet()))
  }

  @Test fun linkLocalIpv6CarriesANumericScope() {
    val scoped = Inet6Address.getByAddress(null, ip("fe80::1c2d").address, 3)
    assertEquals("[fe80:0:0:0:0:0:0:1c2d%3]", selectHost(listOf(scoped), emptySet()))
    assertEquals("[fe80:0:0:0:0:0:0:1]", selectHost(listOf(ip("fe80::1")), emptySet()))
  }

  @Test fun thisPhonesOwnAdvertIsDroppedWhicheverAddressMatches() {
    val locals = setOf(addressKey(ip("192.168.1.8")), addressKey(ip("2001:db8::8")))
    assertNull(selectHost(listOf(ip("2001:db8::8"), ip("10.0.0.99")), locals))
    assertNull(selectHost(listOf(ip("192.168.1.8")), locals))
    assertEquals("192.168.1.20", selectHost(listOf(ip("192.168.1.20")), locals))
  }

  @Test fun localMatchingIgnoresTheScope() {
    val scoped = Inet6Address.getByAddress(null, ip("fe80::8").address, 7)
    assertNull(selectHost(listOf(scoped), setOf(addressKey(ip("fe80::8")))))
  }

  @Test fun unusableAddressesYieldNothing() {
    assertNull(selectHost(emptyList(), emptySet()))
    assertNull(selectHost(listOf(ip("127.0.0.1"), ip("::1"), ip("0.0.0.0"), ip("224.0.0.251")), emptySet()))
  }

  @Test fun deviceNeedsAValidPortAndGetsAFallbackName() {
    assertNull(discoveredDevice("adb-x", connect, 0, listOf(ip("192.168.1.20")), emptySet()))
    assertNull(discoveredDevice("adb-x", connect, 70_000, listOf(ip("192.168.1.20")), emptySet()))
    val device = discoveredDevice("  ", connect, 37_001, listOf(ip("192.168.1.20")), emptySet())
    assertEquals(DiscoveredAdbDevice("Wireless debugging", "192.168.1.20", 37_001, connect), device)
  }

  @Test fun theSameServiceOnTwoInterfacesIsOneRow() {
    val services = DiscoveredServices()
    val key = serviceKey("adb-SERIAL-abc123", connect)!!
    services.upsert(key, DiscoveredAdbDevice("adb-SERIAL-abc123", "[2001:db8:0:0:0:0:0:5]", 37_001, connect))
    services.upsert(key, DiscoveredAdbDevice("adb-SERIAL-abc123", "192.168.1.20", 37_001, connect))
    services.upsert(serviceKey("adb-SERIAL-abc123", "_adb-tls-pairing._tcp.")!!, DiscoveredAdbDevice("adb-SERIAL-abc123", "192.168.1.20", 41_000, "_adb-tls-pairing._tcp."))
    assertEquals(listOf("192.168.1.20:37001", "192.168.1.20:41000"), services.snapshot().map { "${it.host}:${it.port}" })
    services.upsert(key, null)
    assertEquals(1, services.count())
    assertNull(serviceKey(" ", connect))
  }

  @Test fun scanSettlesOnlyWithAResultNothingPendingAndQuiet() {
    assertFalse("nothing found", ScanSettle.shouldStop(2_500, 0, 0, 2_000))
    assertFalse("metadata alone must not shorten the ADB discovery window", ScanSettle.shouldStop(2_000, 0, 0, 1_000))
    assertFalse("too early", ScanSettle.shouldStop(1_000, 1, 0, 1_000))
    assertFalse("a resolve is queued", ScanSettle.shouldStop(2_000, 1, 1, 1_000))
    assertFalse("a record just arrived", ScanSettle.shouldStop(2_000, 1, 0, 100))
    assertTrue(ScanSettle.shouldStop(1_500, 1, 0, 500))
  }

  @Test fun startFailuresAreReportedOnlyWhenNothingWasFound() {
    val message = startFailureMessage(0, mapOf("_adb._tcp." to 0))
    assertNotNull(message)
    assertTrue(message!!, message.contains("_adb._tcp error 0"))
    assertNull(startFailureMessage(2, mapOf("_adb._tcp." to 0)))
    assertNull(startFailureMessage(0, emptyMap()))
  }
}

class SerialResolveQueueTest {
  private val started = mutableListOf<Pair<String, Long>>()
  private val abandoned = mutableListOf<String>()
  private val timers = mutableListOf<Pair<Long, () -> Unit>>()
  private val queue = SerialResolveQueue<String>(
    start = { key, token -> started += key to token },
    schedule = { delayMs, action -> timers += delayMs to action },
    abandon = { key, _ -> abandoned += key },
    resolveTimeoutMs = 1_000,
    maxAttempts = 3,
    retryDelayMs = 200,
  )

  private fun lastToken() = started.last().second
  private fun fire(delayMs: Long) {
    val due = timers.filter { it.first == delayMs }
    timers.removeAll(due)
    due.forEach { it.second() }
  }

  @Test fun metadataCannotStarveQueuedOrLateAdb() {
    var now = 0L
    val starts = mutableListOf<Pair<String, Long>>()
    val scheduled = mutableListOf<() -> Unit>()
    val q = SerialResolveQueue<String>(
      start = { key, token -> starts += key to token },
      schedule = { _, action -> scheduled += action },
      prioritize = { it.startsWith("adb") },
      mayStart = { it.startsWith("adb") || now < 1_000 },
    )
    q.enqueue("slow-cast")
    q.enqueue("cast-two")
    q.enqueue("cast-three")
    q.enqueue("adb-connect")
    now = 1_000
    scheduled.first().invoke()
    assertEquals(listOf("slow-cast", "adb-connect"), starts.map { it.first })
    q.resolved(starts.last().second)
    assertEquals(0, q.pending())
    now = 1_800
    q.enqueue("adb-late")
    assertEquals("adb-late", starts.last().first)
    q.close()
  }

  @Test fun resolvesOneAtATimeInOrder() {
    queue.enqueue("a")
    queue.enqueue("b")
    queue.enqueue("a")
    assertEquals(listOf("a"), started.map { it.first })
    assertEquals(2, queue.pending())
    queue.resolved(lastToken())
    assertEquals(listOf("a", "b"), started.map { it.first })
    queue.resolved(lastToken())
    assertEquals(0, queue.pending())
  }

  @Test fun alreadyActiveIsRetriedThenGivenUp() {
    queue.enqueue("a")
    queue.enqueue("b")
    repeat(2) {
      queue.failed(lastToken(), alreadyActive = true)
      assertEquals("waits out the backoff", "a", started.last().first)
      fire(200)
    }
    assertEquals(listOf("a", "a", "a"), started.map { it.first })
    queue.failed(lastToken(), alreadyActive = true)
    assertEquals(listOf("a", "a", "a", "b"), started.map { it.first })
  }

  @Test fun otherFailuresMoveOnAtOnce() {
    queue.enqueue("a")
    queue.enqueue("b")
    queue.failed(lastToken(), alreadyActive = false)
    assertEquals(listOf("a", "b"), started.map { it.first })
  }

  @Test fun aSilentResolveTimesOutAndLateCallbacksAreIgnored() {
    queue.enqueue("a")
    queue.enqueue("b")
    val stale = lastToken()
    fire(1_000)
    assertEquals(listOf("a"), abandoned)
    assertEquals(listOf("a", "b"), started.map { it.first })
    queue.resolved(stale)
    queue.failed(stale, alreadyActive = true)
    assertEquals(1, queue.pending())
    assertEquals(listOf("a", "b"), started.map { it.first })
  }

  @Test fun lostServicesLeaveTheQueue() {
    queue.enqueue("a")
    queue.enqueue("b")
    queue.remove("b")
    queue.resolved(lastToken())
    assertEquals(listOf("a"), started.map { it.first })
  }

  @Test fun closeAbandonsTheResolveInFlightAndStartsNothing() {
    queue.enqueue("a")
    queue.enqueue("b")
    val token = lastToken()
    queue.close()
    assertEquals(listOf("a"), abandoned)
    queue.resolved(token)
    queue.enqueue("c")
    fire(1_000)
    assertEquals(listOf("a"), started.map { it.first })
    assertEquals(0, queue.pending())
  }
}
