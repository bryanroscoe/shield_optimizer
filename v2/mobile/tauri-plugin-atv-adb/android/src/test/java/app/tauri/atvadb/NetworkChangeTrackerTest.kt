package app.tauri.atvadb

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class NetworkChangeTrackerTest {
  @Test fun theRegistrationReplayIsNotAChange() {
    val tracker = NetworkChangeTracker("100")
    assertNull(tracker.onAvailable("100"))
    assertNull(tracker.onAddresses("100", setOf("192.168.1.8")))
  }

  @Test fun aDifferentDefaultNetworkIsAChange() {
    val tracker = NetworkChangeTracker("100")
    assertEquals(NetworkChange.CHANGED, tracker.onAvailable("101"))
    assertNull(tracker.onAvailable("101"))
    assertNull("its first addresses are a baseline", tracker.onAddresses("101", setOf("10.0.0.8")))
  }

  @Test fun losingTheDefaultNetworkIsReportedOnce() {
    val tracker = NetworkChangeTracker("100")
    assertNull(tracker.onLost("99"))
    assertEquals(NetworkChange.LOST, tracker.onLost("100"))
    assertNull(tracker.onLost("100"))
    assertEquals(NetworkChange.CHANGED, tracker.onAvailable("100"))
  }

  @Test fun onlyARemovedAddressCounts() {
    val tracker = NetworkChangeTracker("100")
    tracker.onAddresses("100", setOf("192.168.1.8", "fe80::8"))
    assertNull(tracker.onAddresses("100", setOf("192.168.1.8", "fe80::8", "2001:db8::9")))
    assertEquals(NetworkChange.ADDRESSES, tracker.onAddresses("100", setOf("192.168.1.42", "fe80::8", "2001:db8::9")))
    assertNull(tracker.onAddresses("200", emptySet()))
  }

  @Test fun startingWithoutANetworkStillReportsTheFirstOne() {
    assertEquals(NetworkChange.CHANGED, NetworkChangeTracker(null).onAvailable("100"))
  }

  @Test fun reasonsMatchTheFrontendContract() {
    assertEquals(listOf("lost", "changed", "addresses"), NetworkChange.entries.map { it.reason })
  }
}
