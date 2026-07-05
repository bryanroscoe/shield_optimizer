package app.tauri.atvadb

import android.app.Activity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import org.json.JSONArray

@InvokeArg
internal class DiscoverArgs {
  var timeoutMs: Long = 3_000
}

// mDNS discovery only. Connect/pair/shell/screencap/reboot are driven directly
// from Rust by the app's WirelessAdb via the pure-Rust adb_client crate, so this
// plugin ships no ADB client code (and no GPL dependencies).
@TauriPlugin
class AdbPlugin(private val activity: Activity) : Plugin(activity) {
  private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
  private val service by lazy { AdbService(activity.applicationContext) }

  @Command
  fun discover(invoke: Invoke) = scope.launch {
    try {
      val args = invoke.parseArgs(DiscoverArgs::class.java)
      val devices = service.discover(args.timeoutMs)
      val response = JSONArray()
      devices.forEach { response.put(it.toJson()) }
      invoke.resolve(JSObject().put("devices", response))
    } catch (error: Throwable) {
      invoke.reject(error.message ?: error.javaClass.simpleName)
    }
  }
}

private fun DiscoveredAdbDevice.toJson() = JSObject()
  .put("name", name)
  .put("host", host)
  .put("port", port)
  .put("service", service)
