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

  @Test fun serviceTypesNormalizeAcrossPlatformSpellings() {
    for (spelling in listOf("_adb._tcp.", "_adb._tcp", "._adb._tcp", "_ADB._tcp.local.", " _adb._tcp.local ")) {
      assertEquals(spelling, "_adb._tcp.", canonicalServiceType(spelling))
    }
    assertEquals(connect, canonicalServiceType("_adb-tls-connect._tcp"))
    assertEquals(CAST_SERVICE_TYPE, canonicalServiceType("_GOOGLECAST._tcp.local."))
    assertNull(canonicalServiceType("_http._tcp."))
    assertNull(canonicalServiceType(null))
  }

  @Test fun recordsOfAnotherTypeAreNotAccepted() {
    assertTrue(belongsTo("_adb-tls-connect._tcp", connect))
    assertFalse(belongsTo("_adb-tls-pairing._tcp.", connect))
    assertTrue(belongsTo("garbled", connect))
    assertFalse(belongsTo(CAST_SERVICE_TYPE, connect))
    assertFalse(belongsTo(connect, CAST_SERVICE_TYPE))
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
    assertNull(startFailureMessage(0, mapOf(CAST_SERVICE_TYPE to 0)))
    assertFalse(allAdbDiscoveryFailed(setOf(CAST_SERVICE_TYPE)))
    assertFalse(allAdbDiscoveryFailed(ADB_SERVICE_TYPES.take(2).toSet() + CAST_SERVICE_TYPE))
    assertTrue(allAdbDiscoveryFailed(ADB_SERVICE_TYPES.toSet()))
  }
}

class FriendlyDiscoveryNamesTest {
  private val legacy = "_adb._tcp."
  private val adbKey = ServiceKey("adb-tv-a", legacy)
  private val castKey = ServiceKey("cast-receiver-a", CAST_SERVICE_TYPE)
  private val adb = DiscoveredAdbDevice("adb-tv-a", "192.0.2.10", 5555, legacy)
  private fun cast(name: String?, host: String = adb.host) =
    DiscoveredAdbDevice("opaque-cast-id", host, 8009, CAST_SERVICE_TYPE, name)

  @Test fun castTxtNamesDecodeStrictlyWithoutChangingTheirUnicode() {
    assertEquals("Studio TV", castFriendlyName(" Studio TV ".toByteArray()))
    assertEquals("テレビ 📺", castFriendlyName("テレビ 📺".toByteArray(Charsets.UTF_8)))
    for (bad in listOf(null, byteArrayOf(), "  ".toByteArray(), "\u0000".toByteArray(), "TV\nName".toByteArray(), byteArrayOf(0xc3.toByte(), 0x28), ByteArray(256) { 65 })) {
      assertNull(castFriendlyName(bad))
    }
  }

  @Test fun castOnlyResultsNeverCreateAnAdbEndpointOrSettleTheScan() {
    val results = DiscoveredServices()
    results.upsert(castKey, cast("Studio TV"))
    assertEquals(0, results.count())
    assertTrue(results.snapshot().isEmpty())
    assertFalse(ScanSettle.shouldStop(2_000, results.count(), 0, 1_000))
  }

  @Test fun aNameArrivingBeforeOrAfterAdbEnrichesOnlyThatCurrentEndpoint() {
    for (castFirst in listOf(true, false)) {
      val results = DiscoveredServices()
      if (castFirst) results.upsert(castKey, cast("Studio TV"))
      results.upsert(adbKey, adb)
      if (!castFirst) results.upsert(castKey, cast("Studio TV"))
      val result = results.snapshot().single()
      assertEquals("Studio TV", result.friendlyName)
      assertEquals(adb.name, result.name)
      assertEquals(adb.port, result.port)
      assertEquals(adb.service, result.service)
      assertEquals(1, results.count())
    }
  }

  @Test fun namesCanMatchAnAddressFamilyOtherThanThePreferredConnectionAddress() {
    val results = DiscoveredServices()
    results.upsert(adbKey, adb, setOf(adb.host, "[2001:db8::10]"))
    results.upsert(castKey, cast("Studio TV", "[2001:db8::10]"))
    assertEquals("Studio TV", results.snapshot().single().friendlyName)
    assertEquals(adb.host, results.snapshot().single().host)
  }

  @Test fun namesAtOtherEndpointsAreNeverBorrowed() {
    val results = DiscoveredServices()
    results.upsert(adbKey, adb)
    results.upsert(castKey, cast("Another TV", "192.0.2.20"))
    assertNull(results.snapshot().single().friendlyName)
  }

  @Test fun conflictingNamesAreUnnamedAndLostMetadataIsRemoved() {
    val results = DiscoveredServices()
    val other = ServiceKey("cast-other", CAST_SERVICE_TYPE)
    results.upsert(adbKey, adb)
    results.upsert(castKey, cast("Studio TV"))
    results.upsert(other, cast("Kitchen TV"))
    assertNull(results.snapshot().single().friendlyName)
    results.remove(other)
    assertEquals("Studio TV", results.snapshot().single().friendlyName)
    results.remove(castKey)
    assertNull(results.snapshot().single().friendlyName)
    results.upsert(castKey, cast("New name"))
    results.upsert(castKey, null)
    assertNull(results.snapshot().single().friendlyName)
  }

  @Test fun closingFreezesTheResultAndTheNextScanHasNoCachedNames() {
    val results = DiscoveredServices()
    results.upsert(adbKey, adb)
    results.upsert(castKey, cast("Studio TV"))
    results.close()
    results.upsert(castKey, cast("Late callback"))
    results.remove(adbKey)
    assertEquals("Studio TV", results.snapshot().single().friendlyName)
    val nextScan = DiscoveredServices()
    nextScan.upsert(adbKey, adb)
    assertNull(nextScan.snapshot().single().friendlyName)
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

  @Test fun losingAnActiveServiceCancelsItAndRejectsItsLateCallbackAfterRediscovery() {
    queue.enqueue("a")
    val old = lastToken()
    queue.remove("a")
    assertEquals(listOf("a"), abandoned)
    queue.enqueue("a")
    val current = lastToken()
    assertTrue(old != current)
    queue.resolved(old)
    queue.failed(old, alreadyActive = true)
    assertEquals(1, queue.pending())
    queue.resolved(current)
    assertEquals(0, queue.pending())
  }

  @Test fun optionalCastResolvesNeverJumpAheadOfQueuedAdbEndpoints() {
    val started = mutableListOf<String>()
    val tokens = mutableListOf<Long>()
    val priorities = SerialResolveQueue<String>(
      start = { key, token -> started += key; tokens += token },
      schedule = { _, _ -> },
      priority = { if (it.startsWith("cast")) 1 else 0 },
    )
    priorities.enqueue("adb-a")
    priorities.enqueue("cast-a")
    priorities.enqueue("cast-b")
    priorities.enqueue("adb-b")
    priorities.resolved(tokens.last())
    assertEquals(listOf("adb-a", "adb-b"), started)
    priorities.resolved(tokens.last())
    assertEquals(listOf("adb-a", "adb-b", "cast-a"), started)
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
