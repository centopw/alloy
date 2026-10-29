package vn.hce.alloy

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.lifecycle.ProcessLifecycleOwner
import java.util.concurrent.Executors
import kotlin.math.absoluteValue
import org.unifiedpush.android.connector.FailedReason
import org.unifiedpush.android.connector.PushService
import org.unifiedpush.android.connector.data.PushEndpoint
import org.unifiedpush.android.connector.data.PushMessage

/**
 * UnifiedPush service that receives push notifications for the Alloy Matrix client.
 * When the app is in the foreground, messages are ignored (the timeline updates via sync).
 * When the app is closed or backgrounded, a system notification is posted.
 */
class AlloyPushService : PushService() {
  companion object {
    private const val TAG = "AlloyPushService"
    const val CHANNEL_ID = "messages"
    const val CHANNEL_NAME = "Messages"
    private const val NOTIF_GROUP = "chat.alloy.messages"
    private val executor = Executors.newSingleThreadExecutor()

    fun createNotificationChannel(context: Context) {
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        val channel = NotificationChannel(
          CHANNEL_ID,
          CHANNEL_NAME,
          NotificationManager.IMPORTANCE_HIGH,
        ).apply {
          description = "Direct messages and mentions"
          enableVibration(true)
          enableLights(true)
        }
        val manager = context.getSystemService(NotificationManager::class.java)
        manager?.createNotificationChannel(channel)
      }
    }
  }

  override fun onCreate() {
    super.onCreate()
    createNotificationChannel(this)
  }

  /**
   * Called when a new endpoint is allocated by the distributor.
   * Runs gateway discovery and registers an HTTP pusher with the homeserver.
   */
  override fun onNewEndpoint(endpoint: PushEndpoint, instance: String) {
    Log.d(TAG, "onNewEndpoint: ${endpoint.url}")
    executor.execute {
      try {
        val gateway = discoverGateway(endpoint.url)
        val config = buildPusherConfig(endpoint.url, gateway)
        val result = PushNative.setPusher(dataDir.absolutePath, config)
        Log.d(TAG, "setPusher result: $result")
        // Check if setPusher succeeded (e.g. {"ok":true})
        val ok = result.contains("\"ok\":true")
        PushBridge.onEndpointChanged(this@AlloyPushService, endpoint.url, gateway, ok, if (ok) null else result)
      } catch (e: Exception) {
        Log.e(TAG, "Error in onNewEndpoint", e)
        PushBridge.onError(this@AlloyPushService, "Registration failed: ${e.message}")
      }
    }
  }

  /**
   * Called when a message is received from the distributor.
   * If the app is in the foreground, the message is ignored (sync is active).
   * Otherwise, a system notification is posted.
   */
  override fun onMessage(message: PushMessage, instance: String) {
    Log.d(TAG, "onMessage: content length=${message.content.size}, decrypted=${message.decrypted}")
    val isAppVisible = ProcessLifecycleOwner.get().lifecycle.currentState
      .isAtLeast(androidx.lifecycle.Lifecycle.State.STARTED)
    if (isAppVisible) {
      Log.d(TAG, "App in foreground; ignoring message")
      return
    }

    executor.execute {
      try {
        val payloadJson = String(message.content, Charsets.UTF_8)
        val response = PushNative.fetchNotification(dataDir.absolutePath, payloadJson)
        Log.d(TAG, "fetchNotification result: $response")
        handleNotificationResponse(response)
      } catch (e: Exception) {
        Log.e(TAG, "Error processing notification", e)
        postGenericNotification()
      }
    }
  }

  /**
   * Called when the user unregisters, uninstalls the distributor, or resets the endpoint.
   */
  override fun onUnregistered(instance: String) {
    Log.d(TAG, "onUnregistered")
    executor.execute {
      try {
        val endpoint = PushBridge.getStoredEndpoint(this@AlloyPushService)
        if (endpoint != null) {
          val config = buildPusherConfig(endpoint, PushBridge.getStoredGateway(this@AlloyPushService) ?: defaultGateway())
          PushNative.removePusher(dataDir.absolutePath, config)
        }
        PushBridge.onUnregistered(this@AlloyPushService)
      } catch (e: Exception) {
        Log.e(TAG, "Error in onUnregistered", e)
      }
    }
  }

  /**
   * Called when registration fails temporarily (e.g., no network).
   */
  override fun onTempUnavailable(instance: String) {
    Log.d(TAG, "onTempUnavailable for instance=$instance")
  }

  /**
   * Called when registration fails permanently.
   */
  override fun onRegistrationFailed(reason: FailedReason, instance: String) {
    Log.d(TAG, "onRegistrationFailed: reason=$reason")
    PushBridge.onError(this, "Registration failed: $reason")
  }

  /**
   * Gateway discovery: fetch /_matrix/push/v1/notify from the endpoint origin.
   * Returns the gateway URL from the response, or the default if not specified.
   */
  private fun discoverGateway(endpoint: String): String {
    return try {
      val origin = endpoint.substringBeforeLast('/').let { base ->
        val proto = if (base.startsWith("https://")) "https://" else "http://"
        base.substringAfter("://").substringBefore('/').let { "$proto$it" }
      }
      val url = "$origin/_matrix/push/v1/notify"
      val json = fetchUrlJson(url)
      Log.d(TAG, "Gateway discovery result: $json")
      val obj = org.json.JSONObject(json)
      if (obj.has("unifiedpush")) {
        val up = obj.getJSONObject("unifiedpush")
        if (up.optString("gateway") == "matrix") {
          return url
        }
      }
      defaultGateway()
    } catch (e: Exception) {
      Log.w(TAG, "Gateway discovery failed, using default", e)
      defaultGateway()
    }
  }

  /**
   * Fetch a URL and return the response body as a string. Uses HttpURLConnection with a 5-second timeout.
   */
  private fun fetchUrlJson(urlString: String): String {
    val url = java.net.URL(urlString)
    val conn = url.openConnection() as java.net.HttpURLConnection
    conn.requestMethod = "GET"
    conn.connectTimeout = 5000
    conn.readTimeout = 5000
    conn.connect()
    return conn.inputStream.bufferedReader().use { it.readText() }
  }

  /**
   * Build the pusher configuration JSON for setPusher/removePusher.
   */
  private fun buildPusherConfig(endpoint: String, gateway: String): String {
    val deviceName = "${Build.MANUFACTURER} ${Build.MODEL}".take(100)
    val obj = org.json.JSONObject().apply {
      put("app_id", "vn.hce.alloy.android")
      put("pushkey", endpoint)
      put("gateway_url", gateway)
      put("app_display_name", "Alloy")
      put("device_display_name", deviceName)
      put("lang", "en")
    }
    return obj.toString()
  }

  /**
   * Handle the response from fetchNotification.
   */
  private fun handleNotificationResponse(responseJson: String) {
    try {
      val obj = org.json.JSONObject(responseJson)
      val status = obj.optString("status")
      when (status) {
        "event" -> postEventNotification(obj)
        "generic" -> postGenericNotification(obj)
        "skip" -> Log.d(TAG, "Skipping notification (filtered or own message)")
        "error" -> {
          Log.w(TAG, "Notification error: $responseJson")
          postGenericNotification(obj)
        }
        else -> postGenericNotification(null)
      }
    } catch (e: Exception) {
      Log.e(TAG, "Error parsing notification response", e)
      postGenericNotification(null)
    }
  }

  /**
   * Post a notification for a decrypted Matrix event.
   * One notification per room using the room id hash as id, plus a group summary.
   */
  private fun postEventNotification(obj: org.json.JSONObject) {
    val title = obj.optString("title").ifBlank { "Alloy" }
    // With "Show message preview" off the text stays out of the notification, as for in-app ones.
    val body = if (PushBridge.showPreview(this)) obj.optString("body").ifBlank { "New message" } else "New message"
    val roomId = obj.optString("room_id")
    val eventId = obj.optString("event_id")

    // Use the room_id hash as the notification ID to group by room.
    val notificationId = if (roomId.isNotBlank()) roomId.hashCode().absoluteValue else 1

    val intent = Intent(this, MainActivity::class.java).apply {
      action = "android.intent.action.MAIN"
      addCategory("android.intent.category.LAUNCHER")
      if (roomId.isNotBlank()) {
        putExtra("room_id", roomId)
      }
      if (eventId.isNotBlank()) {
        putExtra("event_id", eventId)
      }
      flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
    }
    val pendingIntent = PendingIntent.getActivity(
      this,
      notificationId,
      intent,
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    val smallIcon = getSmallIconResId()

    val notification = NotificationCompat.Builder(this, CHANNEL_ID)
      .setSmallIcon(smallIcon)
      .setContentTitle(title)
      .setContentText(body)
      .setStyle(NotificationCompat.BigTextStyle().bigText(body))
      .setContentIntent(pendingIntent)
      .setAutoCancel(true)
      .setGroup(NOTIF_GROUP)
      .setGroupSummary(false)
      .setPriority(NotificationCompat.PRIORITY_HIGH)
      .build()

    val manager = NotificationManagerCompat.from(this)
    try {
      manager.notify(notificationId, notification)

      // Post group summary notification
      val summaryNotification = NotificationCompat.Builder(this, CHANNEL_ID)
        .setSmallIcon(smallIcon)
        .setContentTitle("Alloy")
        .setContentText("New messages")
        .setGroup(NOTIF_GROUP)
        .setGroupSummary(true)
        .setAutoCancel(true)
        .setPriority(NotificationCompat.PRIORITY_HIGH)
        .build()

      manager.notify(0, summaryNotification)
    } catch (e: SecurityException) {
      Log.w(TAG, "Notification permission missing", e)
    }
  }

  /**
   * Post a generic "New message" notification when decryption or fetching fails.
   */
  private fun postGenericNotification(obj: org.json.JSONObject? = null) {
    val title = obj?.optString("title")?.ifBlank { "Alloy" } ?: "Alloy"
    val body = obj?.optString("body")?.ifBlank { "New message" } ?: "New message"
    val roomId = obj?.optString("room_id") ?: ""

    val notificationId = if (roomId.isNotBlank()) roomId.hashCode().absoluteValue else 1

    val intent = Intent(this, MainActivity::class.java).apply {
      action = "android.intent.action.MAIN"
      addCategory("android.intent.category.LAUNCHER")
      if (roomId.isNotBlank()) {
        putExtra("room_id", roomId)
      }
      flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
    }
    val pendingIntent = PendingIntent.getActivity(
      this,
      notificationId,
      intent,
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    val smallIcon = getSmallIconResId()

    val notification = NotificationCompat.Builder(this, CHANNEL_ID)
      .setSmallIcon(smallIcon)
      .setContentTitle(title)
      .setContentText(body)
      .setStyle(NotificationCompat.BigTextStyle().bigText(body))
      .setContentIntent(pendingIntent)
      .setAutoCancel(true)
      .setGroup(NOTIF_GROUP)
      .setPriority(NotificationCompat.PRIORITY_HIGH)
      .build()

    val manager = NotificationManagerCompat.from(this)
    try {
      manager.notify(notificationId, notification)
    } catch (e: SecurityException) {
      Log.w(TAG, "Notification permission missing", e)
    }
  }

  private fun getSmallIconResId(): Int {
    // Check if ic_notification or ic_launcher exists
    val notifIcon = resources.getIdentifier("ic_notification", "drawable", packageName)
    if (notifIcon != 0) return notifIcon
    val launcher = resources.getIdentifier("ic_launcher_foreground", "drawable", packageName)
    if (launcher != 0) return launcher
    return applicationInfo.icon.takeIf { it != 0 } ?: android.R.drawable.ic_dialog_info
  }

  private fun defaultGateway(): String = "https://matrix.gateway.unifiedpush.org/_matrix/push/v1/notify"
}
