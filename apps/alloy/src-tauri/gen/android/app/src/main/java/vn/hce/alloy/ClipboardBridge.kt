package vn.hce.alloy

import android.content.ClipboardManager
import android.content.Context
import android.net.Uri
import android.os.Looper
import android.webkit.JavascriptInterface
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.UUID

/**
 * Bridges the system clipboard to JavaScript: window.AlloyClipboard.hasImage() and
 * window.AlloyClipboard.saveImage(). Used on Android when the user pastes an image
 * (e.g. screenshot from the screenshot preview, picture from Gallery, Gboard clipboard)
 * that the WebView receives as a content:// URI rather than a DataTransfer.files blob.
 */
class ClipboardBridge(private val context: Context) {
  companion object {
    private const val TAG = "ClipboardBridge"
    private val MAX_IMAGE_BYTES = 50L * 1024 * 1024
  }

  @JavascriptInterface
  fun hasImage(): Boolean {
    return try {
      val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
        ?: return false
      val clip = clipboard.primaryClip ?: return false
      val desc = clip.description

      for (i in 0 until clip.itemCount) {
        val item = clip.getItemAt(i) ?: continue
        if (isImageItem(item, desc)) return true
      }
      false
    } catch (_: Exception) {
      false
    }
  }

  @JavascriptInterface
  fun saveImage(): String {
    if (Looper.myLooper() == Looper.getMainLooper()) {
      return ""
    }

    return try {
      val clipboardDir = File(context.cacheDir, "clipboard")
      pruneOldDirectories(clipboardDir)

      val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
        ?: return ""
      val clip = clipboard.primaryClip ?: return ""
      val desc = clip.description

      var imageUri: Uri? = null
      var resolvedMime: String? = null

      for (i in 0 until clip.itemCount) {
        val item = clip.getItemAt(i) ?: continue
        val uri = item.uri ?: continue

        val directType = try {
          context.contentResolver.getType(uri)
        } catch (_: Exception) {
          null
        }
        if (directType?.startsWith("image/", ignoreCase = true) == true) {
          imageUri = uri
          resolvedMime = directType
          break
        }

        val streamTypes = try {
          context.contentResolver.getStreamTypes(uri, "image/*")
        } catch (_: Exception) {
          null
        }
        if (!streamTypes.isNullOrEmpty()) {
          imageUri = uri
          resolvedMime = streamTypes[0]
          break
        }

        if (desc != null && desc.hasMimeType("image/*")) {
          imageUri = uri
          for (m in 0 until desc.mimeTypeCount) {
            val t = desc.getMimeType(m)
            if (t != null && t.startsWith("image/", ignoreCase = true)) {
              resolvedMime = t
              break
            }
          }
          break
        }
      }

      val uri = imageUri ?: return ""
      val mime = resolvedMime ?: "image/png"

      val ext = when {
        mime.contains("jpeg", ignoreCase = true) || mime.contains("jpg", ignoreCase = true) -> "jpg"
        mime.contains("webp", ignoreCase = true) -> "webp"
        mime.contains("gif", ignoreCase = true) -> "gif"
        else -> "png"
      }

      val dateStr = SimpleDateFormat("yyyy-MM-dd HH.mm.ss", Locale.US).format(Date())
      val fileName = "Pasted image $dateStr.$ext"

      val targetDir = File(clipboardDir, UUID.randomUUID().toString())
      if (!targetDir.mkdirs() && !targetDir.isDirectory) {
        return ""
      }
      val targetFile = File(targetDir, fileName)

      val input = context.contentResolver.openInputStream(uri) ?: run {
        targetDir.delete()
        return ""
      }

      var success = false
      try {
        input.use { inStream ->
          targetFile.outputStream().use { outStream ->
            val buffer = ByteArray(8192)
            var total = 0L
            while (true) {
              val read = inStream.read(buffer)
              if (read == -1) break
              total += read
              if (total > MAX_IMAGE_BYTES) {
                return@use
              }
              outStream.write(buffer, 0, read)
            }
            success = total > 0 && total <= MAX_IMAGE_BYTES
          }
        }
      } catch (_: Exception) {
        success = false
      }

      if (success && targetFile.isFile && targetFile.length() > 0) {
        targetFile.absolutePath
      } else {
        targetFile.delete()
        targetDir.delete()
        ""
      }
    } catch (_: Exception) {
      ""
    }
  }

  private fun isImageItem(
    item: android.content.ClipData.Item?,
    desc: android.content.ClipDescription?
  ): Boolean {
    if (item == null) return false
    val uri = item.uri ?: return false

    val type = try {
      context.contentResolver.getType(uri)
    } catch (_: Exception) {
      null
    }
    if (type?.startsWith("image/", ignoreCase = true) == true) return true

    val streamTypes = try {
      context.contentResolver.getStreamTypes(uri, "image/*")
    } catch (_: Exception) {
      null
    }
    if (!streamTypes.isNullOrEmpty()) return true

    if (desc != null && desc.hasMimeType("image/*")) {
      return true
    }

    return false
  }

  private fun pruneOldDirectories(clipboardDir: File) {
    try {
      if (!clipboardDir.exists()) return
      val now = System.currentTimeMillis()
      val cutoff = now - 24L * 60 * 60 * 1000
      val dirs = clipboardDir.listFiles() ?: return
      for (dir in dirs) {
        if (dir.isDirectory && dir.lastModified() < cutoff) {
          dir.deleteRecursively()
        }
      }
    } catch (_: Exception) {
      // Best-effort pruning
    }
  }
}
