package app.tauri.atvadb

import android.app.Activity
import android.webkit.WebView
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import io.github.muntashirakon.adb.PRNGFixes
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import org.conscrypt.Conscrypt
import org.json.JSONArray
import java.security.Security

@InvokeArg
internal class DiscoverArgs {
  var timeoutMs: Long = 3_000
}

@InvokeArg
internal class PairArgs {
  lateinit var host: String
  var port: Int = 0
  lateinit var code: String
}

@InvokeArg
internal class ConnectArgs {
  lateinit var host: String
  var port: Int = 0
}

@InvokeArg
internal class SerialArgs {
  lateinit var serial: String
}

@InvokeArg
internal class ShellArgs {
  lateinit var serial: String
  lateinit var command: String
}

@TauriPlugin
class AdbPlugin(private val activity: Activity) : Plugin(activity) {
  private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
  private val service by lazy { AdbService(activity.applicationContext) }

  // Insert the Conscrypt security provider + libadb's PRNG fixes before any
  // pairing/connect happens. Done here (not in the app's Application class) so
  // the transport stays self-contained and survives app project regeneration.
  override fun load(webView: WebView) {
    PRNGFixes.apply()
    if (Security.getProvider(Conscrypt.newProvider().name) == null) {
      Security.insertProviderAt(Conscrypt.newProvider(), 1)
    }
  }

  @Command
  fun discover(invoke: Invoke) = launch(invoke) {
    val args = invoke.parseArgs(DiscoverArgs::class.java)
    val devices = service.discover(args.timeoutMs)
    val response = JSONArray()
    devices.forEach { response.put(it.toJson()) }
    invoke.resolve(JSObject().put("devices", response))
  }

  @Command
  fun pair(invoke: Invoke) = launch(invoke) {
    val args = invoke.parseArgs(PairArgs::class.java)
    invoke.resolve(service.pair(args.host, args.port, args.code).toJson())
  }

  @Command
  fun connect(invoke: Invoke) = launch(invoke) {
    val args = invoke.parseArgs(ConnectArgs::class.java)
    invoke.resolve(service.connect(args.host, args.port).toJson())
  }

  @Command
  fun disconnect(invoke: Invoke) = launch(invoke) {
    invoke.parseArgs(SerialArgs::class.java)
    service.disconnect()
    invoke.resolve(JSObject())
  }

  @Command
  fun shell(invoke: Invoke) = launch(invoke) {
    val args = invoke.parseArgs(ShellArgs::class.java)
    invoke.resolve(service.shell(args.command).toJson())
  }

  @Command
  fun screencap(invoke: Invoke) = launch(invoke) {
    invoke.parseArgs(SerialArgs::class.java)
    invoke.resolve(JSObject().put("png_base64", service.screencap()))
  }

  private fun launch(invoke: Invoke, block: suspend () -> Unit) {
    scope.launch {
      try {
        block()
      } catch (error: Throwable) {
        invoke.reject(error.message ?: error.javaClass.simpleName)
      }
    }
  }
}

private fun ConnectResponse.toJson() = JSObject()
  .put("serial", serial)
  .put("host", host)
  .put("port", port)
  .put("message", message)

private fun AdbCommandOutput.toJson() = JSObject()
  .put("stdout", stdout)
  .put("stderr", stderr)
  .put("exit_code", exitCode)

private fun DiscoveredAdbDevice.toJson() = JSObject()
  .put("name", name)
  .put("host", host)
  .put("port", port)
  .put("service", service)
