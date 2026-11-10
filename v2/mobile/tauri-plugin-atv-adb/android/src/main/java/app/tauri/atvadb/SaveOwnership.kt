package app.tauri.atvadb

/** Owns the picker AND provider write. A late callback must never target a newer save. */
internal class SaveOwnership<Owner : Any, Args : Any> {
  private var owner: Owner? = null
  private var args: Args? = null
  private var copying = false

  @Synchronized
  fun begin(request: Owner, value: Args): Boolean {
    if (owner != null) return false
    owner = request
    args = value
    copying = false
    return true
  }

  /** Claim the callback once, without releasing the operation during provider IO. */
  @Synchronized
  fun claim(request: Owner): Args? {
    if (owner !== request || copying) return null
    copying = true
    return args
  }

  @Synchronized
  fun finish(request: Owner) {
    if (owner !== request) return
    owner = null
    args = null
    copying = false
  }
}

internal fun saveFailureMessage(detail: String, partialRemoved: Boolean): String =
  "Couldn't save the copy: $detail" + if (partialRemoved) "" else
    ". A partial document may remain. Check the chosen folder before trying again."
