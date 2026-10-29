package vn.hce.alloy

import android.util.Log

/**
 * JNI declarations for Rust background push handling in `alloy_app_lib`.
 *
 * Implements the contract defined in `local://push-contract.md`:
 * - `external fun fetchNotification(dataDir: String, payloadJson: String): String`
 * - `external fun setPusher(dataDir: String, configJson: String): String`
 * - `external fun removePusher(dataDir: String, configJson: String): String`
 */
object PushNative {
  private const val TAG = "PushNative"
  private var loaded = false

  @Synchronized
  fun init() {
    if (!loaded) {
      try {
        System.loadLibrary("alloy_app_lib")
        loaded = true
        Log.i(TAG, "Loaded alloy_app_lib successfully")
      } catch (e: UnsatisfiedLinkError) {
        Log.e(TAG, "Failed to load alloy_app_lib", e)
      } catch (e: Exception) {
        Log.e(TAG, "Unexpected error loading alloy_app_lib", e)
      }
    }
  }

  init {
    init()
  }

  /**
   * Decrypts and processes a push payload.
   *
   * @param dataDir The app data directory, `context.dataDir.absolutePath` (where Tauri keeps accounts.json and secrets.json)
   * @param payloadJson The raw JSON payload received from UnifiedPush
   * @return JSON response with status "event", "generic", "skip", or "error"
   */
  @JvmStatic
  external fun fetchNotification(dataDir: String, payloadJson: String): String

  /**
   * Registers an HTTP pusher with the Matrix homeserver.
   *
   * @param dataDir The path to the app's files directory
   * @param configJson JSON with app_id, pushkey, gateway_url, app_display_name, device_display_name, lang
   * @return JSON response: {"ok":true} or {"ok":false,"error":"..."}
   */
  @JvmStatic
  external fun setPusher(dataDir: String, configJson: String): String

  /**
   * Unregisters an HTTP pusher from the Matrix homeserver.
   *
   * @param dataDir The path to the app's files directory
   * @param configJson JSON with app_id, pushkey, gateway_url, app_display_name, device_display_name, lang
   * @return JSON response: {"ok":true} or {"ok":false,"error":"..."}
   */
  @JvmStatic
  external fun removePusher(dataDir: String, configJson: String): String
}
