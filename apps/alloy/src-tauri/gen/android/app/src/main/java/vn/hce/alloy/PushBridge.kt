package vn.hce.alloy

import android.content.Context
import android.content.SharedPreferences
import android.content.pm.PackageManager
import android.os.Build
import android.util.Log
import android.webkit.JavascriptInterface
import androidx.core.content.ContextCompat
import java.util.concurrent.Executors
import org.json.JSONArray
import org.json.JSONObject
import org.unifiedpush.android.connector.UnifiedPush

/**
 * JS interface `window.AlloyPush` - enables Alloy's Svelte UI to control push notifications.
 * State is persisted in SharedPreferences and all state changes are broadcast to the WebView
 * via CustomEvent 'alloy-push-state'.
 */
class PushBridge(private val context: Context) {
  private val prefs: SharedPreferences = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
  private val executor = Executors.newSingleThreadExecutor()

  companion object {
    private const val TAG = "PushBridge"
    private const val PREFS_NAME = "alloy_push"
    private const val PREF_ENABLED = "enabled"
    private const val PREF_ENDPOINT = "endpoint"
    private const val PREF_GATEWAY = "gateway"
    private const val PREF_ERROR = "error"
    private const val PREF_PREVIEW = "preview"

    /** Whether notifications may show the message text; mirrors the "Show message preview" setting. */
    fun showPreview(context: Context): Boolean =
      context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE).getBoolean(PREF_PREVIEW, true)

    fun getStoredEndpoint(context: Context): String? {
      val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
      return prefs.getString(PREF_ENDPOINT, null)
    }

    fun getStoredGateway(context: Context): String? {
      val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
      return prefs.getString(PREF_GATEWAY, null)
    }

    fun onEndpointChanged(context: Context, endpoint: String, gateway: String, success: Boolean, error: String?) {
      val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
      with(prefs.edit()) {
        putString(PREF_ENDPOINT, endpoint)
        putString(PREF_GATEWAY, gateway)
        if (success) {
          remove(PREF_ERROR)
        } else {
          putString(PREF_ERROR, error)
        }
        apply()
      }
      broadcastStateChange(context)
    }

    fun onUnregistered(context: Context) {
      val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
      with(prefs.edit()) {
        remove(PREF_ENDPOINT)
        remove(PREF_GATEWAY)
        remove(PREF_ERROR)
        apply()
      }
      broadcastStateChange(context)
    }

    fun onError(context: Context, message: String) {
      val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
      with(prefs.edit()) {
        putString(PREF_ERROR, message)
        apply()
      }
      broadcastStateChange(context)
    }

    fun broadcastStateChange(context: Context) {
      if (context is MainActivity) {
        context.runOnUiThread {
          context.dispatchPushStateEvent()
        }
      }
    }

    /** The label of an installed app (for example "ntfy"), or the package name when it cannot be read. */
    private fun appLabel(context: Context, packageName: String): String =
      try {
        context.packageManager.getApplicationInfo(packageName, 0).loadLabel(context.packageManager).toString()
      } catch (e: PackageManager.NameNotFoundException) {
        packageName
      }

    fun buildStateJson(context: Context): String {
      val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
      val enabled = prefs.getBoolean(PREF_ENABLED, false)
      val endpoint = prefs.getString(PREF_ENDPOINT, null)
      val gateway = prefs.getString(PREF_GATEWAY, null)
      val error = prefs.getString(PREF_ERROR, null)
      val registered = endpoint != null && error == null

      val distributors = UnifiedPush.getDistributors(context)
      val currentDistributor = UnifiedPush.getAckDistributor(context)

      val permission = when {
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU -> "granted"
        ContextCompat.checkSelfPermission(context, android.Manifest.permission.POST_NOTIFICATIONS) ==
          PackageManager.PERMISSION_GRANTED -> "granted"
        else -> "denied"
      }

      val distJsonArray = JSONArray()
      for (d in distributors) {
        distJsonArray.put(appLabel(context, d))
      }

      val obj = JSONObject().apply {
        put("supported", true)
        put("enabled", enabled)
        put("registered", registered)
        put("distributor", currentDistributor?.let { appLabel(context, it) } ?: JSONObject.NULL)
        put("distributors", distJsonArray)
        put("endpoint", endpoint ?: JSONObject.NULL)
        put("gateway", gateway ?: JSONObject.NULL)
        put("permission", permission)
        put("error", error ?: JSONObject.NULL)
      }
      return obj.toString()
    }
  }

  /**
   * Return the current push state as JSON string.
   */
  @JavascriptInterface
  fun state(): String {
    return buildStateJson(context)
  }

  /**
   * Called by the web UI whenever "Show message preview" changes (and at start). With it off, background
   * notifications say only that there is a new message, like the in-app ones do.
   */
  @JavascriptInterface
  fun setPreview(enabled: Boolean) {
    context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE).edit().putBoolean(PREF_PREVIEW, enabled).apply()
  }

  /**
   * Enable push notifications.
   * Requests POST_NOTIFICATIONS permission (API 33+), resolves a distributor, and calls UnifiedPush.register.
   */
  @JavascriptInterface
  fun enable() {
    executor.execute {
      try {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
          val hasPermission = ContextCompat.checkSelfPermission(
            context,
            android.Manifest.permission.POST_NOTIFICATIONS,
          ) == PackageManager.PERMISSION_GRANTED

          if (!hasPermission) {
            with(prefs.edit()) {
              putBoolean(PREF_ENABLED, true)
              apply()
            }
            if (context is MainActivity) {
              context.runOnUiThread {
                context.requestNotificationPermission { granted ->
                  if (granted) {
                    continueEnable()
                  } else {
                    onError(context, "POST_NOTIFICATIONS permission denied")
                  }
                }
              }
            }
            broadcastStateChange(context)
            return@execute
          }
        }

        continueEnable()
      } catch (e: Exception) {
        Log.e(TAG, "Error enabling push", e)
        onError(context, "Error enabling push: ${e.message}")
      }
    }
  }

  private fun continueEnable() {
    try {
      with(prefs.edit()) {
        putBoolean(PREF_ENABLED, true)
        remove(PREF_ERROR)
        apply()
      }

      if (context is MainActivity) {
        context.runOnUiThread {
          context.tryUseCurrentOrDefaultDistributor()
        }
      } else {
        Log.w(TAG, "Cannot request distributor picker from non-MainActivity context")
      }
      broadcastStateChange(context)
    } catch (e: Exception) {
      Log.e(TAG, "Error in continueEnable", e)
      onError(context, "Error enabling push: ${e.message}")
    }
  }

  fun onPermissionResult(context: Context, granted: Boolean) {
    if (granted && prefs.getBoolean(PREF_ENABLED, false)) {
      executor.execute {
        continueEnable()
      }
    } else if (!granted) {
      onError(context, "POST_NOTIFICATIONS permission denied")
    }
  }

  /**
   * Disable push notifications.
   * Unregisters from the distributor and removes the pusher from the homeserver.
   */
  @JavascriptInterface
  fun disable() {
    executor.execute {
      try {
        val endpoint = getStoredEndpoint(context)
        if (endpoint != null) {
          val gateway = getStoredGateway(context) ?: "https://matrix.gateway.unifiedpush.org/_matrix/push/v1/notify"
          val deviceName = "${Build.MANUFACTURER} ${Build.MODEL}".take(100)
          val config = JSONObject().apply {
            put("app_id", "vn.hce.alloy.android")
            put("pushkey", endpoint)
            put("gateway_url", gateway)
            put("app_display_name", "Alloy")
            put("device_display_name", deviceName)
            put("lang", "en")
          }.toString()
          PushNative.removePusher(context.dataDir.absolutePath, config)
        }

        UnifiedPush.unregister(context)
        with(prefs.edit()) {
          putBoolean(PREF_ENABLED, false)
          remove(PREF_ENDPOINT)
          remove(PREF_GATEWAY)
          remove(PREF_ERROR)
          apply()
        }
        broadcastStateChange(context)
      } catch (e: Exception) {
        Log.e(TAG, "Error disabling push", e)
        onError(context, "Error disabling push: ${e.message}")
      }
    }
  }

  /**
   * Open the system distributor picker.
   * Called when the user wants to select a different distributor.
   */
  @JavascriptInterface
  fun pick() {
    executor.execute {
      try {
        if (context is MainActivity) {
          context.runOnUiThread {
            context.tryPickDistributor()
          }
        } else {
          Log.w(TAG, "Cannot pick distributor from non-MainActivity context")
        }
      } catch (e: Exception) {
        Log.e(TAG, "Error picking distributor", e)
        onError(context, "Error picking distributor: ${e.message}")
      }
    }
  }

  /**
   * Return and clear any pending room ID from notification intent.
   * Called by the UI after page load to check if the app was launched by tapping a notification.
   */
  @JavascriptInterface
  fun pendingRoom(): String {
    val room = if (context is MainActivity) {
      context.takePendingRoom()
    } else {
      null
    }
    return room ?: ""
  }

  /**
   * Alias for pendingRoom() per peer coordination contract.
   */
  @JavascriptInterface
  fun takePendingRoom(): String {
    return pendingRoom()
  }
}
