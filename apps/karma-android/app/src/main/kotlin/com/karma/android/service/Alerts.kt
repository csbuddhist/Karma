package com.karma.android.service

import android.app.Notification
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import com.karma.android.KarmaApp
import com.karma.android.R
import com.karma.android.ui.MainActivity

/** High-priority notifications for when protection stops working. */
object Alerts {
    private const val WEAKENED_ID = 2

    fun protectionWeakened(context: Context, message: String) {
        val manager = context.getSystemService(NotificationManager::class.java)
        if (!manager.areNotificationsEnabled()) return
        val notification = Notification.Builder(context, KarmaApp.CHANNEL_ALERTS)
            .setSmallIcon(R.drawable.ic_stat_karma)
            .setContentTitle(context.getString(R.string.alert_title))
            .setContentText(message)
            .setStyle(Notification.BigTextStyle().bigText(message))
            .setContentIntent(openApp(context))
            .setAutoCancel(true)
            .build()
        manager.notify(WEAKENED_ID, notification)
    }

    fun openApp(context: Context): PendingIntent = PendingIntent.getActivity(
        context,
        0,
        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )
}
