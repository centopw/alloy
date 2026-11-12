package vn.hce.alloy

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat

/**
 * Keeps Alloy's process - and with it the Rust engine driving the connectors - alive while the app
 * is not in the foreground.
 *
 * Telegram talks MTProto over a socket this process owns and Messenger is a child process of it, so
 * without a foreground service Android freezes or kills both as soon as the activity is backgrounded
 * and no message arrives until the user opens the app again. There is no homeserver to register a
 * pusher with (ADR 0016), so the service is the replacement for ADR 0009's Matrix push path.
 *
 * The notification is deliberately low importance and ongoing: it is the price of staying connected,
 * not a message. The app stops the service only when Android's data-sync time limit requires it
 * ([onTimeout]); the user can stop it from the system's active-apps screen, in which case messages
 * are only fetched the next time Alloy starts.
 */
class ConnectorService : Service() {
  companion object {
    private const val CHANNEL_ID = "connection"
    private const val CHANNEL_NAME = "Connection"
    private const val NOTIFICATION_ID = 1

    /** Asks the system to (re)start the service; safe to call on every activity creation. */
    fun start(context: Context) {
      ContextCompat.startForegroundService(context, Intent(context, ConnectorService::class.java))
    }
  }

  override fun onCreate() {
    super.onCreate()
    createNotificationChannel()
    startForeground(NOTIFICATION_ID, buildNotification())
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    // A START_STICKY restart can arrive without onCreate's notification already posted.
    startForeground(NOTIFICATION_ID, buildNotification())
    return START_STICKY
  }

  override fun onBind(intent: Intent?): IBinder? = null

  /**
   * Android 15 caps a `dataSync` foreground service at six hours in any 24; the system asks the app
   * to stop when the cap is reached. Stopping is the only supported answer, and the next launch
   * starts the service again. Android kills the process when a service ignores this callback.
   */
  override fun onTimeout(startId: Int) {
    stopSelf(startId)
  }

  private fun createNotificationChannel() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val channel = NotificationChannel(
      CHANNEL_ID,
      CHANNEL_NAME,
      NotificationManager.IMPORTANCE_LOW,
    ).apply {
      description = "Keeps Alloy connected so new messages arrive in the background"
      setShowBadge(false)
    }
    getSystemService(NotificationManager::class.java)?.createNotificationChannel(channel)
  }

  private fun buildNotification(): Notification {
    val contentIntent = PendingIntent.getActivity(
      this,
      0,
      Intent(this, MainActivity::class.java).apply {
        action = Intent.ACTION_MAIN
        addCategory(Intent.CATEGORY_LAUNCHER)
        flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
      },
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )
    return NotificationCompat.Builder(this, CHANNEL_ID)
      .setSmallIcon(smallIcon())
      .setContentTitle(getString(R.string.app_name))
      .setContentText("Alloy is connected")
      .setContentIntent(contentIntent)
      .setOngoing(true)
      .setShowWhen(false)
      .setSilent(true)
      .setPriority(NotificationCompat.PRIORITY_LOW)
      .setCategory(NotificationCompat.CATEGORY_SERVICE)
      .build()
  }

  /** The launcher foreground or a dedicated notification icon, whichever the resources provide. */
  private fun smallIcon(): Int {
    val notifIcon = resources.getIdentifier("ic_notification", "drawable", packageName)
    if (notifIcon != 0) return notifIcon
    val launcher = resources.getIdentifier("ic_launcher_foreground", "drawable", packageName)
    if (launcher != 0) return launcher
    return applicationInfo.icon.takeIf { it != 0 } ?: android.R.drawable.ic_dialog_info
  }
}
