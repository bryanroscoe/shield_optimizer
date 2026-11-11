package app.tauri.atvadb

import org.junit.Assert.*
import org.junit.Test

class SaveOwnershipTest {
  @Test fun pickerAndSlowCopyBothBlockRetry() {
    val slot = SaveOwnership<Any, String>()
    val first = Any()
    val retry = Any()
    assertTrue(slot.begin(first, "first"))
    assertFalse(slot.begin(retry, "retry"))
    assertEquals("first", slot.claim(first))
    assertFalse(slot.begin(retry, "retry"))
    assertNull(slot.claim(first)) // duplicate result cannot resolve an active write
    slot.finish(first)
    assertTrue(slot.begin(retry, "retry"))
  }

  @Test fun lateCallbackCannotClaimOrClearNewSave() {
    val slot = SaveOwnership<Any, String>()
    val old = Any()
    val current = Any()
    assertTrue(slot.begin(old, "old"))
    slot.finish(old) // terminal cancellation, success or failed-write cleanup
    assertTrue(slot.begin(current, "new"))
    assertNull(slot.claim(old))
    slot.finish(old)
    assertFalse(slot.begin(Any(), "overlap"))
    assertEquals("new", slot.claim(current))
  }

  @Test fun failedOrRefusedDeletionWarnsAboutPartialDocument() {
    assertTrue(saveFailureMessage("write failed", false).contains("partial document may remain"))
    assertFalse(saveFailureMessage("write failed", true).contains("partial document may remain"))
  }
}
