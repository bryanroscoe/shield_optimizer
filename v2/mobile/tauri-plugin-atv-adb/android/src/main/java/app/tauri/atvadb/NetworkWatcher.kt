package app.tauri.atvadb

import android.content.Context
import android.net.ConnectivityManager
import android.net.LinkProperties
import android.net.Network
import android.util.Log

/**
 * Reports default-network changes so the app can probe the TV connection within seconds
 * instead of waiting for its heartbeat. The TV socket is not bound to a network, so it rides
 * whatever was the default network when it connected; that is the network to watch.
 *
 * Process-wide on purpose: the plugin can be loaded again for a new activity, and a second
 * registration would double every event and leak the first callback.
 */
internal object NetworkWatcher {
  private var manager: ConnectivityManager? = null
  private var callback: ConnectivityManager.NetworkCallback? = null

  /** Replaces any earlier watch, so the newest [onChange] is the only receiver. */
  @Synchronized fun start(context: Context, onChange: (NetworkChange) -> Unit) {
    stopLocked()
    val cm = context.applicationContext.getSystemService(ConnectivityManager::class.java)
      ?: throw IllegalStateException("ConnectivityManager is unavailable")
    val tracker = NetworkChangeTracker(cm.activeNetwork?.toString())
    val report = { change: NetworkChange? ->
      // These run on ConnectivityManager's thread, where an exception would crash the app.
      if (change != null) {
        runCatching { onChange(change) }.onFailure { Log.w(TAG, "network change delivery failed", it) }
      }
    }
    val cb = object : ConnectivityManager.NetworkCallback() {
      override fun onAvailable(network: Network) = report(tracker.onAvailable(network.toString()))
      override fun onLost(network: Network) = report(tracker.onLost(network.toString()))
      override fun onLinkPropertiesChanged(network: Network, linkProperties: LinkProperties) {
        val addresses = linkProperties.linkAddresses.mapNotNull { link -> link.address?.let(::addressKey) }.toSet()
        report(tracker.onAddresses(network.toString(), addresses))
      }
    }
    cm.registerDefaultNetworkCallback(cb)
    manager = cm
    callback = cb
  }

  private fun stopLocked() {
    val cb = callback ?: return
    runCatching { manager?.unregisterNetworkCallback(cb) }
      .onFailure { Log.w(TAG, "network callback unregister failed", it) }
    callback = null
    manager = null
  }
}
