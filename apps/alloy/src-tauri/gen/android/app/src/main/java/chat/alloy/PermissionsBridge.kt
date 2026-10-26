package chat.alloy

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import android.webkit.JavascriptInterface
import androidx.core.content.ContextCompat

/**
 * Lets the Permissions screen read and set up what the app needs: window.AlloyPermissions.
 *
 * Names: `notifications`, `microphone`, `location`, `battery`.
 * `status(name)` returns "granted", "denied" (asked and refused, or blocked) or "prompt" (not asked yet).
 * `request(name)` shows the system prompt; the answer arrives as the `alloy-permission-changed` window event,
 * which the screen also receives when the user comes back from a system settings page.
 * `openSettings(name)` opens the matching system page.
 */
class PermissionsBridge(private val activity: MainActivity) {
  companion object {
    private const val PREFS = "alloy_permissions"

    /** The runtime permission behind a name, or null for `battery` (a settings page, not a prompt). */
    fun runtimePermission(name: String): String? = when (name) {
      "notifications" ->
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) Manifest.permission.POST_NOTIFICATIONS else null
      "microphone" -> Manifest.permission.RECORD_AUDIO
      "location" -> Manifest.permission.ACCESS_FINE_LOCATION
      else -> null
    }
  }

  @JavascriptInterface
  fun status(name: String): String {
    if (name == "battery") {
      val power = activity.getSystemService(PowerManager::class.java)
      return if (power != null && power.isIgnoringBatteryOptimizations(activity.packageName)) "granted" else "prompt"
    }
    val permission = runtimePermission(name)
      ?: return if (name == "notifications") "granted" else "unknown"
    if (ContextCompat.checkSelfPermission(activity, permission) == PackageManager.PERMISSION_GRANTED) return "granted"
    // Android reports "not granted" both before the first question and after a permanent refusal. Once the
    // system prompt has been shown and then no longer offers a rationale, it will not show again: that is "denied".
    val asked = activity.getSharedPreferences(PREFS, android.content.Context.MODE_PRIVATE).getBoolean(name, false)
    return if (asked && !activity.shouldShowRequestPermissionRationale(permission)) "denied" else "prompt"
  }

  @JavascriptInterface
  fun request(name: String) {
    activity.runOnUiThread {
      if (name == "battery") {
        openSettings(name)
        return@runOnUiThread
      }
      val permission = runtimePermission(name)
      if (permission == null) {
        activity.dispatchPermissionChanged(name)
        return@runOnUiThread
      }
      activity.getSharedPreferences(PREFS, android.content.Context.MODE_PRIVATE).edit().putBoolean(name, true).apply()
      activity.requestPermissionFor(name, permission)
    }
  }

  @JavascriptInterface
  fun openSettings(name: String) {
    activity.runOnUiThread {
      val intent = when (name) {
        "battery" -> Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)
        else -> Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", activity.packageName, null))
      }
      intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      try {
        activity.startActivity(intent)
      } catch (_: Exception) {
        activity.startActivity(
          Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", activity.packageName, null))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        )
      }
    }
  }
}
