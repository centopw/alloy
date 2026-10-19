package chat.alloy

import android.content.ContentValues
import android.content.Context
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import android.webkit.JavascriptInterface
import java.io.File

/**
 * Copies a cached media file into the public Downloads folder. The web UI calls it as
 * `AlloyAndroid.saveToDownloads(path, name, mime)`. Only regular files inside the app's own
 * `media_cache` directory are accepted, so page script cannot read anything else through it.
 */
class DownloadBridge(private val context: Context) {
  /** Returns an empty string on success, otherwise a message for the user. */
  @JavascriptInterface
  fun saveToDownloads(path: String, name: String, mime: String): String {
    return try {
      val source = File(path).canonicalFile
      val root = context.dataDir.canonicalFile
      val allowed = source.isFile && source.path.startsWith(root.path + File.separator) &&
        source.path.contains(File.separator + "media_cache" + File.separator)
      if (!allowed) return "That file cannot be saved."
      val safeName = cleanName(name)
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) saveWithMediaStore(source, safeName, mime)
      else saveLegacy(source, safeName)
      ""
    } catch (e: Exception) {
      e.message ?: "Saving failed."
    }
  }

  private fun saveWithMediaStore(source: File, name: String, mime: String) {
    val resolver = context.contentResolver
    val values = ContentValues().apply {
      put(MediaStore.Downloads.DISPLAY_NAME, name)
      put(MediaStore.Downloads.MIME_TYPE, mime.ifBlank { "application/octet-stream" })
      put(MediaStore.Downloads.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS)
      put(MediaStore.Downloads.IS_PENDING, 1)
    }
    val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
      ?: throw IllegalStateException("Could not create the file in Downloads.")
    try {
      resolver.openOutputStream(uri)?.use { out -> source.inputStream().use { it.copyTo(out) } }
        ?: throw IllegalStateException("Could not write to Downloads.")
      values.clear()
      values.put(MediaStore.Downloads.IS_PENDING, 0)
      resolver.update(uri, values, null, null)
    } catch (e: Exception) {
      resolver.delete(uri, null, null)
      throw e
    }
  }

  @Suppress("DEPRECATION")
  private fun saveLegacy(source: File, name: String) {
    val dir = Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS)
    dir.mkdirs()
    source.copyTo(uniqueFile(dir, name), overwrite = false)
  }

  private fun uniqueFile(dir: File, name: String): File {
    var candidate = File(dir, name)
    var n = 1
    val dot = name.lastIndexOf('.')
    val stem = if (dot > 0) name.substring(0, dot) else name
    val ext = if (dot > 0) name.substring(dot) else ""
    while (candidate.exists()) candidate = File(dir, "$stem ($n)$ext").also { n++ }
    return candidate
  }

  /** Keeps the final path component and drops characters file systems reject. */
  private fun cleanName(raw: String): String {
    val last = raw.substringAfterLast('/').substringAfterLast('\\')
    val cleaned = last.filter { it >= ' ' && it !in "<>:\"|?*" }.trim().take(120)
    return cleaned.ifBlank { "download" }
  }
}
