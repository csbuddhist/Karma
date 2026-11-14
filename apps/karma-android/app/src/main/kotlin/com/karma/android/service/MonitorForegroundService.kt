package com.karma.android.service

import android.app.ForegroundServiceStartNotAllowedException
import android.app.Notification
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log
import com.karma.android.AppLanguage
import com.karma.android.KarmaApp
import com.karma.android.R

/**
 * The always-visible status notification. It raises the process priority so
 * aggressive vendor task killers are less likely to stop the monitor, and tells the
 * device user that monitoring is active (a Play policy requirement too).
 */
class MonitorForegroundService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val strings = AppLanguage.localized(this)
        val builder = Notification.Builder(this, KarmaApp.CHANNEL_STATUS)
            .setSmallIcon(R.drawable.ic_stat_karma)
            .setContentTitle(strings.getString(R.string.status_notification_title))
            .setContentText(strings.getString(R.string.status_notification_text))
            .setContentIntent(Alerts.openApp(this))
            .setOngoing(true)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            builder.setForegroundServiceBehavior(Notification.FOREGROUND_SERVICE_IMMEDIATE)
        }
        val notification = builder.build()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
        return START_STICKY
    }

    companion object {
        private const val NOTIFICATION_ID = 1
        private const val TAG = "KarmaFgs"

        fun start(context: Context) {
            try {
                context.startForegroundService(Intent(context, MonitorForegroundService::class.java))
            } catch (error: IllegalStateException) {
                // Background start refused (Android 12+); the watchdog worker retries.
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && error is ForegroundServiceStartNotAllowedException) {
                    Log.w(TAG, "foreground service start deferred")
                } else {
                    throw error
                }
            }
        }
    }
}
