package vn.hce.alloy

import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.view.View
import android.webkit.WebView
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  companion object {
    private const val TAG = "MainActivity"
  }

  private var pendingRoomId: String? = null
  private var webViewReady = false
  private lateinit var pushBridge: PushBridge
  private var webView: WebView? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    // The system bars sit on the dark window background with light icons in every app theme.
    enableEdgeToEdge(
      statusBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
      navigationBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
    )
    super.onCreate(savedInstanceState)
    // The WebView does not report safe-area insets, and edge-to-edge windows ignore adjustResize.
    // Inset the content by the system bars, cutout and soft keyboard so nothing is hidden.
    val content = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout(),
      )
      val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
      view.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, ime.bottom))
      insets
    }

    // Create notification channel (done here so it exists from app start)
    AlloyPushService.createNotificationChannel(this)

    // Re-register push if enabled (distributor may have been reinstalled or restarted)
    val pushPrefs = getSharedPreferences("alloy_push", MODE_PRIVATE)
    if (pushPrefs.getBoolean("enabled", false)) {
      tryUseCurrentOrDefaultDistributor()
    }

    // Check if this activity was launched with a room_id extra
    handlePendingRoom(intent)

    // Register for runtime permission result
    registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
      if (granted) {
        android.util.Log.d(TAG, "POST_NOTIFICATIONS permission granted")
      } else {
        android.util.Log.d(TAG, "POST_NOTIFICATIONS permission denied")
        if (::pushBridge.isInitialized) {
          pushBridge.onPermissionResult(this, false)
        }
      }
    }
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    handlePendingRoom(intent)
  }

  /**
   * Mount the PushBridge and DownloadBridge JS interfaces when the WebView is created.
   * Do NOT replace the webViewClient; that breaks Wry's RustWebViewClient.
   * Instead, we use evaluateJavascript to dispatch events when ready.
   */
  override fun onWebViewCreate(webView: WebView) {
    this.webView = webView
    webView.addJavascriptInterface(DownloadBridge(applicationContext), "AlloyAndroid")
    webView.addJavascriptInterface(ClipboardBridge(applicationContext), "AlloyClipboard")
    webView.addJavascriptInterface(PermissionsBridge(this), "AlloyPermissions")
    pushBridge = PushBridge(this)
    webView.addJavascriptInterface(pushBridge, "AlloyPush")

    // Dispatch initial state and pending room via evaluateJavascript on next event loop.
    // This is called after Wry has set the RustWebViewClient.
    // We mark webViewReady to indicate the view is available for evaluateJavascript,
    // but the actual page load is handled via onPageFinished in Wry.
    webView.post {
      webViewReady = true
      dispatchPushStateEvent()
      dispatchPendingRoomIfPresent()
    }
  }

  private fun handlePendingRoom(intent: Intent) {
    val roomId = intent.getStringExtra("room_id") ?: return
    intent.removeExtra("room_id") // Clear the extra so it's not processed again
    pendingRoomId = roomId
    android.util.Log.d(TAG, "Pending room set: $roomId")
    // If WebView is already ready, dispatch immediately
    if (webViewReady) {
      dispatchPendingRoomIfPresent()
    }
  }

  /**
   * Tells the web UI to open the room. The id stays pending until the UI calls `AlloyPush.pendingRoom()`:
   * on a cold start the page has not mounted its listener yet, so this event is lost and the UI picks the
   * room up from the pending value instead; with the page already running it handles the event and then
   * clears the pending value the same way.
   */
  private fun dispatchPendingRoomIfPresent() {
    if (pendingRoomId != null && webViewReady && webView != null) {
      val roomId = pendingRoomId!!
      android.util.Log.d(TAG, "Dispatching alloy-open-room: $roomId")
      val escapedRoomId = roomId.replace("\\", "\\\\").replace("'", "\\'")
      webView!!.evaluateJavascript(
        "javascript:void(window.dispatchEvent(new CustomEvent('alloy-open-room', {detail:{roomId:'$escapedRoomId'}})))"
      ) { _ -> }
    }
  }

  fun takePendingRoom(): String {
    val room = pendingRoomId ?: return ""
    pendingRoomId = null
    return room
  }

  fun dispatchPushStateEvent() {
    if (!webViewReady || webView == null) return
    val stateJson = PushBridge.buildStateJson(this)
    webView!!.evaluateJavascript(
      "javascript:void(window.dispatchEvent(new CustomEvent('alloy-push-state', {detail:$stateJson})))"
    ) { _ -> }
  }

  /** Asks the system for one runtime permission (through Wry's helper) and tells the page the answer. */
  fun requestPermissionFor(name: String, permission: String) {
    val permissions = if (name == "microphone") {
      arrayOf(permission, android.Manifest.permission.MODIFY_AUDIO_SETTINGS)
    } else if (name == "location") {
      arrayOf(permission, android.Manifest.permission.ACCESS_COARSE_LOCATION)
    } else {
      arrayOf(permission)
    }
    requestPermissions(permissions) { _ -> dispatchPermissionChanged(name) }
  }

  /** Lets the Permissions screen re-read its states: after an answer, or on return from a settings page. */
  fun dispatchPermissionChanged(name: String) {
    if (!webViewReady || webView == null) return
    val safe = name.replace("\\", "\\\\").replace("'", "\\'")
    webView!!.post {
      webView!!.evaluateJavascript(
        "javascript:void(window.dispatchEvent(new CustomEvent('alloy-permission-changed', {detail:{name:'$safe'}})))"
      ) { _ -> }
    }
  }

  fun requestNotificationPermission(callback: (Boolean) -> Unit) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
      callback(true)
      return
    }
    if (ContextCompat.checkSelfPermission(this, android.Manifest.permission.POST_NOTIFICATIONS) ==
      PackageManager.PERMISSION_GRANTED) {
      callback(true)
      return
    }
    // Request permission via the launcher (registered in onCreate)
    requestPermissionLauncher.launch(android.Manifest.permission.POST_NOTIFICATIONS)
    // Note: result will be handled via the registered ActivityResultContract callback
  }

  private val requestPermissionLauncher = registerForActivityResult(
    ActivityResultContracts.RequestPermission()
  ) { isGranted: Boolean ->
    android.util.Log.d(TAG, "Permission callback: $isGranted")
  }

  fun tryUseCurrentOrDefaultDistributor() {
    org.unifiedpush.android.connector.UnifiedPush.tryUseCurrentOrDefaultDistributor(this) { success ->
      android.util.Log.d(TAG, "tryUseCurrentOrDefaultDistributor result: $success")
      if (success) {
        org.unifiedpush.android.connector.UnifiedPush.register(this)
      }
    }
  }

  fun tryPickDistributor() {
    org.unifiedpush.android.connector.UnifiedPush.tryPickDistributor(this) { success ->
      android.util.Log.d(TAG, "tryPickDistributor result: $success")
      if (success) {
        org.unifiedpush.android.connector.UnifiedPush.register(this)
      }
    }
  }
}
