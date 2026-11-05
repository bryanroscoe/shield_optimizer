package app.tauri.atvadb

/** Why the phone's route to the TV may have changed. [reason] is the event payload the app sees. */
internal enum class NetworkChange(val reason: String) {
  /** The default network went away and nothing replaced it. */
  LOST("lost"),

  /** A different network became the default. */
  CHANGED("changed"),

  /** The default network stayed, but an address this phone had on it is gone. */
  ADDRESSES("addresses"),
}

/**
 * Reduces default-network callbacks to the events that can strand an open TV connection.
 * ConnectivityManager replays the current network on registration and repeats callbacks
 * for capability and link changes that leave the route intact; those are dropped. A newly
 * added address (an IPv6 privacy address, say) leaves existing sockets working, so only a
 * removed one counts. Networks and addresses are opaque keys, so this runs as a JVM test.
 */
internal class NetworkChangeTracker(initialNetwork: String?) {
  private var network: String? = initialNetwork
  private var addresses: Set<String>? = null

  @Synchronized fun onAvailable(network: String): NetworkChange? {
    if (network == this.network) return null
    this.network = network
    addresses = null
    return NetworkChange.CHANGED
  }

  @Synchronized fun onLost(network: String): NetworkChange? {
    if (network != this.network) return null
    this.network = null
    addresses = null
    return NetworkChange.LOST
  }

  @Synchronized fun onAddresses(network: String, current: Set<String>): NetworkChange? {
    if (network != this.network) return null
    val previous = addresses
    addresses = current
    return if (previous != null && !current.containsAll(previous)) NetworkChange.ADDRESSES else null
  }
}
