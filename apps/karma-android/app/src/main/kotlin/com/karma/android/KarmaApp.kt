package com.karma.android

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.content.res.Configuration
import android.os.LocaleList
import com.karma.android.service.MonitorForegroundService
import com.karma.android.service.WatchdogWorker
import com.karma.android.update.UpdateWorker

class KarmaApp : Application() {
    private var locales: LocaleList? = null

    override fun onCreate() {
        super.onCreate()
        AppLanguage.applyDefaultLocale(this)
        locales = resources.configuration.locales
        createChannels(this)
        WatchdogWorker.schedule(this)
        UpdateWorker.schedule(this)
    }

    /** Android 13+ applies a new app or system language here; renames channels and the status notification. */
    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        if (newConfig.locales != locales) {
            locales = newConfig.locales
            AppLanguage.applyDefaultLocale(this)
            onLanguageChanged(this)
        }
    }

    companion object {
        const val CHANNEL_STATUS = "karma.status"
        const val CHANNEL_ALERTS = "karma.alerts"

        /** Shows the new language in notification channel names and the ongoing status notification. */
        fun onLanguageChanged(context: Context) {
            createChannels(context)
            MonitorForegroundService.start(context)
        }

        /** Creates the channels, or renames existing ones to the current language. */
        private fun createChannels(context: Context) {
            val strings = AppLanguage.localized(context)
            val manager = context.getSystemService(NotificationManager::class.java)
            manager.createNotificationChannel(
                NotificationChannel(
                    CHANNEL_STATUS,
                    strings.getString(R.string.channel_status),
                    NotificationManager.IMPORTANCE_LOW,
                ).apply { setShowBadge(false) },
            )
            manager.createNotificationChannel(
                NotificationChannel(
                    CHANNEL_ALERTS,
                    strings.getString(R.string.channel_alerts),
                    NotificationManager.IMPORTANCE_HIGH,
                ),
            )
        }
    }
}
