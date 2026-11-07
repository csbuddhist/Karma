package com.karma.android

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import com.karma.android.service.WatchdogWorker
import com.karma.android.update.UpdateWorker

class KarmaApp : Application() {
    override fun onCreate() {
        super.onCreate()
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_STATUS,
                getString(R.string.channel_status),
                NotificationManager.IMPORTANCE_LOW,
            ).apply { setShowBadge(false) },
        )
        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_ALERTS,
                getString(R.string.channel_alerts),
                NotificationManager.IMPORTANCE_HIGH,
            ),
        )
        WatchdogWorker.schedule(this)
        UpdateWorker.schedule(this)
    }

    companion object {
        const val CHANNEL_STATUS = "karma.status"
        const val CHANNEL_ALERTS = "karma.alerts"
    }
}
