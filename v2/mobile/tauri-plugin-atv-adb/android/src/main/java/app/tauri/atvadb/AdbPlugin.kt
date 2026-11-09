package app.tauri.atvadb

import android.app.Activity
import android.content.ClipData
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import androidx.activity.result.ActivityResult
import androidx.core.content.FileProvider
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Channel
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import org.json.JSONArray
import java.io.File
import java.io.FileInputStream
import java.io.IOException

@InvokeArg
internal class DiscoverArgs {
  var timeoutMs: Long = 3_000
}

@InvokeArg
internal class WatchNetworkArgs {
  lateinit var channel: Channel
}

@InvokeArg
internal class SaveCopyArgs {
  lateinit var path: String
  lateinit var name: String
  lateinit var mime: String
}

@InvokeArg
internal class ShareFileArgs {
  lateinit var path: String
  lateinit var mime: String
}

// mDNS discovery, the phone's network-change watch, and the one-file export bridge. Connect/pair/shell/
// screencap/reboot are driven directly from Rust by the app's WirelessAdb via the
// pure-Rust adb_client crate, so this plugin ships no ADB client code (and no GPL
// dependencies). Every command is called from the plugin's Rust side, never from JS; the app's
// export_file command confines export sources to its screenshots and downloads folders first.
@TauriPlugin
class AdbPlugin(private val activity: Activity) : Plugin(activity) {
  private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
  private val service by lazy { AdbService(activity.applicationContext) }
  // The file the open create-document picker will receive. Rust allows one export at a time.
  @Volatile private var pendingSave: SaveCopyArgs? = null

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

  @Command
  fun watchNetwork(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(WatchNetworkArgs::class.java)
      NetworkWatcher.start(activity.applicationContext) { change ->
        args.channel.send(JSObject().put("reason", change.reason))
      }
      invoke.resolve()
    } catch (error: Throwable) {
      invoke.reject(error.message ?: error.javaClass.simpleName)
    }
  }

  // Save a copy: Android's create-document picker (Storage Access Framework). The user picks
  // the destination; no storage permission is involved.
  @Command
  fun saveCopy(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(SaveCopyArgs::class.java)
      if (!File(args.path).isFile) {
        invoke.reject("That file is no longer in the app.")
        return
      }
      val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
        addCategory(Intent.CATEGORY_OPENABLE)
        type = args.mime
        putExtra(Intent.EXTRA_TITLE, args.name)
      }
      pendingSave = args
      startActivityForResult(invoke, intent, "saveCopyResult")
    } catch (error: Throwable) {
      pendingSave = null
      invoke.reject(error.message ?: error.javaClass.simpleName)
    }
  }

  @ActivityCallback
  fun saveCopyResult(invoke: Invoke, result: ActivityResult) {
    val args = pendingSave
    pendingSave = null
    val target = result.data?.data
    if (args == null) {
      invoke.reject("The save request was lost. Try again.")
      return
    }
    if (result.resultCode != Activity.RESULT_OK || target == null) {
      invoke.resolve(JSObject().put("saved", false))
      return
    }
    scope.launch {
      val resolver = activity.contentResolver
      try {
        val output = resolver.openOutputStream(target, "w")
          ?: throw IOException("the chosen location can't be written")
        output.use { out -> FileInputStream(File(args.path)).use { it.copyTo(out) } }
        invoke.resolve(JSObject().put("saved", true).put("name", displayName(target)))
      } catch (error: Throwable) {
        // Don't leave a partial document behind under the name the user chose.
        try {
          DocumentsContract.deleteDocument(resolver, target)
        } catch (ignored: Throwable) {
        }
        invoke.reject("Couldn't save the copy: ${error.message ?: error.javaClass.simpleName}")
      }
    }
  }

  private fun displayName(uri: Uri): String? = try {
    activity.contentResolver
      .query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
      ?.use { cursor -> if (cursor.moveToFirst()) cursor.getString(0) else null }
  } catch (ignored: Throwable) {
    null
  }

  // Share: a content URI from the app's FileProvider, which maps only cache/exports/, with a
  // read grant for the app the user picks. getUriForFile refuses any path outside that folder.
  @Command
  fun shareFile(invoke: Invoke) {
    val args = try {
      invoke.parseArgs(ShareFileArgs::class.java)
    } catch (error: Throwable) {
      invoke.reject(error.message ?: error.javaClass.simpleName)
      return
    }
    activity.runOnUiThread {
      try {
        val uri = FileProvider.getUriForFile(
          activity,
          "${activity.packageName}.fileprovider",
          File(args.path)
        )
        val send = Intent(Intent.ACTION_SEND).apply {
          type = args.mime
          putExtra(Intent.EXTRA_STREAM, uri)
          clipData = ClipData.newRawUri("", uri)
          addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        val chooser = Intent.createChooser(send, null)
          .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        activity.startActivity(chooser)
        invoke.resolve()
      } catch (error: Throwable) {
        invoke.reject("Couldn't open the share sheet: ${error.message ?: error.javaClass.simpleName}")
      }
    }
  }
}

private fun DiscoveredAdbDevice.toJson() = JSObject()
  .put("name", name)
  .put("host", host)
  .put("port", port)
  .put("service", service)
  .put("friendly_name", friendlyName)
