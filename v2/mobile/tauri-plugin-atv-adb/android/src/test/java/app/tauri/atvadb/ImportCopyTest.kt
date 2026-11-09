package app.tauri.atvadb

import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class ImportCopyTest {
  @Test fun copiesEverythingUpToTheCap() {
    val bytes = ByteArray(200_000) { (it % 251).toByte() }
    val out = ByteArrayOutputStream()
    assertEquals(200_000L, ImportCopy.copyBounded(ByteArrayInputStream(bytes), out, 200_000))
    assertArrayEquals(bytes, out.toByteArray())
  }

  @Test fun stopsAsSoonAsTheCapIsPassed() {
    val out = ByteArrayOutputStream()
    try {
      ImportCopy.copyBounded(ByteArrayInputStream(ByteArray(200_001)), out, 200_000)
      fail("an over-limit copy must fail")
    } catch (expected: ImportLimitExceeded) {
    }
    assertTrue("nothing past the cap is written", out.size() <= 200_000)
  }

  @Test fun anEmptyDocumentCopiesNothing() {
    assertEquals(0L, ImportCopy.copyBounded(ByteArrayInputStream(ByteArray(0)), ByteArrayOutputStream(), 10))
  }

  @Test fun limitsReadAsWholeUnits() {
    assertEquals("2 GiB", ImportCopy.describeLimit(2L * 1024 * 1024 * 1024))
    assertEquals("512 MiB", ImportCopy.describeLimit(512L * 1024 * 1024))
    assertEquals("900 bytes", ImportCopy.describeLimit(900))
  }
}
