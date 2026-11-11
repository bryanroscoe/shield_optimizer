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
import java.io.FileOutputStream
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

@InvokeArg
internal class PickDocumentsArgs {
  lateinit var dir: String
  var maxFiles: Int = 1
  var maxFileBytes: Long = 0
  var maxTotalBytes: Long = 0
}

// mDNS discovery, the phone's network-change watch, the one-file export bridge and the
// open-document import. Connect/pair/shell/
// screencap/reboot are driven directly from Rust by the app's WirelessAdb via the
// pure-Rust adb_client crate, so this plugin ships no ADB client code (and no GPL
// dependencies). Every command is called from the plugin's Rust side, never from JS; the app's
// export_file command confines export sources to its screenshots and downloads folders first, and
// its import commands hand pickDocuments a fresh staging folder of their own.
@TauriPlugin
class AdbPlugin(private val activity: Activity) : Plugin(activity) {
  private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
  private val service by lazy { AdbService(activity.applicationContext) }
  // Retain ownership through the picker and provider IO. Rust timeouts do not cancel either.
  private val pendingSave = SaveOwnership<Invoke, SaveCopyArgs>()
  // The same for the open-document picker.
  @Volatile private var pendingPick: Pair<Invoke, PickDocumentsArgs>? = null

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
      if (!pendingSave.begin(invoke, args)) {
        invoke.reject("Another save is still pending.")
        return
      }
      startActivityForResult(invoke, intent, "saveCopyResult")
    } catch (error: Throwable) {
      pendingSave.finish(invoke)
      invoke.reject(error.message ?: error.javaClass.simpleName)
    }
  }

  @ActivityCallback
  fun saveCopyResult(invoke: Invoke, result: ActivityResult) {
    // Ignore stale/duplicate callbacks. Rejecting the same invoke twice could prematurely
    // release the Rust worker while the first callback is still writing the document.
    val args = pendingSave.claim(invoke) ?: return
    val target = result.data?.data
    if (result.resultCode != Activity.RESULT_OK || target == null) {
      pendingSave.finish(invoke)
      invoke.resolve(JSObject().put("saved", false))
      return
    }
    scope.launch {
      val resolver = activity.contentResolver
      val response = try {
        val output = resolver.openOutputStream(target, "w")
          ?: throw IOException("the chosen location can't be written")
        output.use { out -> FileInputStream(File(args.path)).use { it.copyTo(out) } }
        JSObject().put("saved", true).put("name", displayName(target))
      } catch (error: Throwable) {
        // Providers may refuse deletion or throw. Never promise that cleanup succeeded.
        val removed = try {
          DocumentsContract.deleteDocument(resolver, target)
        } catch (ignored: Throwable) {
          false
        }
        pendingSave.finish(invoke)
        invoke.reject(saveFailureMessage(error.message ?: error.javaClass.simpleName, removed))
        return@launch
      }
      pendingSave.finish(invoke)
      invoke.resolve(response)
    }
  }

  private fun displayName(uri: Uri): String? = try {
    activity.contentResolver
      .query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
      ?.use { cursor -> if (cursor.moveToFirst()) cursor.getString(0) else null }
  } catch (ignored: Throwable) {
    null
  }

  // Import: Android's open-document picker (Storage Access Framework). Each picked document is
  // copied into the staging folder Rust created, under the size caps it sent; Rust then names,
  // validates and later deletes the copies. The picker's read grant is temporary: no persistable
  // permission is taken, and no URI is kept once its copy is done.
  @Command
  fun pickDocuments(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(PickDocumentsArgs::class.java)
      if (!File(args.dir).isDirectory || args.maxFiles < 1) {
        invoke.reject("The import folder isn't ready. Try again.")
        return
      }
      val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
        addCategory(Intent.CATEGORY_OPENABLE)
        type = "*/*"
        putExtra(Intent.EXTRA_ALLOW_MULTIPLE, args.maxFiles > 1)
      }
      pendingPick?.first?.reject("Replaced by a newer pick.")
      pendingPick = Pair(invoke, args)
      startActivityForResult(invoke, intent, "pickDocumentsResult")
    } catch (error: Throwable) {
      pendingPick = null
      invoke.reject(error.message ?: error.javaClass.simpleName)
    }
  }

  @ActivityCallback
  fun pickDocumentsResult(invoke: Invoke, result: ActivityResult) {
    val pending = pendingPick
    if (pending == null || pending.first !== invoke) {
      invoke.reject("The pick request was lost. Try again.")
      return
    }
    pendingPick = null
    val args = pending.second
    val data = result.data
    val uris = if (result.resultCode == Activity.RESULT_OK && data != null) pickedUris(data) else emptyList()
    if (uris.isEmpty()) {
      invoke.resolve(JSObject().put("picked", false))
      return
    }
    if (uris.size > args.maxFiles) {
      invoke.reject("Pick at most ${args.maxFiles} files at a time. Nothing was imported.")
      return
    }
    scope.launch {
      val dir = File(args.dir)
      val written = mutableListOf<File>()
      try {
        val files = JSONArray()
        var total = 0L
        uris.forEachIndexed { index, uri ->
          val (name, declared) = documentFacts(uri)
          val label = name ?: "A picked file"
          if (declared != null && declared > args.maxFileBytes) {
            throw ImportRefused("$label is over the ${ImportCopy.describeLimit(args.maxFileBytes)} limit for one file. Nothing was imported.")
          }
          if (declared != null && total + declared > args.maxTotalBytes) {
            throw ImportRefused("These files are over the ${ImportCopy.describeLimit(args.maxTotalBytes)} limit together. Nothing was imported.")
          }
          if (!dir.isDirectory) throw ImportRefused("The import was given up. Try again.")
          if (declared != null && declared > dir.usableSpace) {
            throw ImportRefused("There isn't enough free space on this phone for $label. Nothing was imported.")
          }
          val target = File(dir, "$index.part")
          written.add(target)
          val input = activity.contentResolver.openInputStream(uri)
            ?: throw IOException("$label can't be opened")
          val limit = minOf(args.maxFileBytes, args.maxTotalBytes - total)
          val copied = try {
            input.use { source ->
              FileOutputStream(target).use { out -> ImportCopy.copyBounded(source, out, limit) }
            }
          } catch (ignored: ImportLimitExceeded) {
            throw ImportRefused("$label is larger than the import limit. Nothing was imported.")
          }
          total += copied
          files.put(
            JSObject().put("name", name ?: "").put("size", copied).put("file", target.absolutePath)
          )
        }
        invoke.resolve(JSObject().put("picked", true).put("files", files))
      } catch (error: Throwable) {
        written.forEach { it.delete() }
        invoke.reject(
          if (error is ImportRefused) error.message ?: "Nothing was imported."
          else "Couldn't copy the picked file: ${error.message ?: error.javaClass.simpleName}"
        )
      }
    }
  }

  private fun pickedUris(data: Intent): List<Uri> {
    val clip = data.clipData
    if (clip != null && clip.itemCount > 0) {
      return (0 until clip.itemCount).mapNotNull { clip.getItemAt(it)?.uri }
    }
    return listOfNotNull(data.data)
  }

  // The provider's display name and size, when it reports them. A missing size is checked
  // while copying instead.
  private fun documentFacts(uri: Uri): Pair<String?, Long?> = try {
    activity.contentResolver
      .query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)
      ?.use { cursor ->
        if (!cursor.moveToFirst()) {
          Pair(null, null)
        } else {
          val name = if (cursor.isNull(0)) null else cursor.getString(0)
          val size = if (cursor.isNull(1)) null else cursor.getLong(1)
          Pair(name, size)
        }
      } ?: Pair(null, null)
  } catch (ignored: Throwable) {
    Pair(null, null)
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
