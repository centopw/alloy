package vn.hce.alloy

import android.graphics.Color
import android.os.Bundle
import android.view.View
import android.webkit.WebView
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {

  private var webViewReady = false
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

    // The connectors (Telegram in-process, Messenger in the sidecar) only receive messages while the
    // process lives, so the foreground service keeps the process - and with it the Rust engine -
    // alive. The app only stops it when Android's data-sync time limit requires it; the user can
    // disable it from the system's active-apps screen.
    ConnectorService.start(this)
  }

  /**
   * Mount the DownloadBridge and Permissions JS interfaces when the WebView is created.
   * Do NOT replace the webViewClient; that breaks Wry's RustWebViewClient.
   * Instead, we use evaluateJavascript to dispatch events when ready.
   */
  override fun onWebViewCreate(webView: WebView) {
    this.webView = webView
    webView.addJavascriptInterface(DownloadBridge(applicationContext), "AlloyAndroid")
    webView.addJavascriptInterface(ClipboardBridge(applicationContext), "AlloyClipboard")
    webView.addJavascriptInterface(PermissionsBridge(this), "AlloyPermissions")

    webView.post {
      webViewReady = true
    }
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
}
