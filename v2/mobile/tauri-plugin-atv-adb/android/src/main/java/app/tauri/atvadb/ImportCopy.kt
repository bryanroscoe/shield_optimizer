package app.tauri.atvadb

import java.io.IOException
import java.io.InputStream
import java.io.OutputStream

/** A picked document grew past the size the app agreed to stage. Nothing past the cap is kept. */
internal class ImportLimitExceeded : IOException("over the import size limit")

/** A pick the app refuses before or while copying; the message is shown to the user as-is. */
internal class ImportRefused(message: String) : IOException(message)

// The pure part of the open-document import: a stream copy that stops at a byte cap, and the
// cap's wording. The ContentResolver side lives in AdbPlugin.
internal object ImportCopy {
  private const val GIB = 1024L * 1024 * 1024
  private const val MIB = 1024L * 1024

  /** Copy `input` to `output`, failing as soon as more than `limit` bytes have been read. */
  fun copyBounded(input: InputStream, output: OutputStream, limit: Long): Long {
    val buffer = ByteArray(64 * 1024)
    var total = 0L
    while (true) {
      val read = input.read(buffer)
      if (read < 0) return total
      total += read
      if (total > limit) throw ImportLimitExceeded()
      output.write(buffer, 0, read)
    }
  }

  fun describeLimit(bytes: Long): String =
    if (bytes >= GIB && bytes % GIB == 0L) "${bytes / GIB} GiB"
    else if (bytes >= MIB) "${bytes / MIB} MiB"
    else "$bytes bytes"
}
