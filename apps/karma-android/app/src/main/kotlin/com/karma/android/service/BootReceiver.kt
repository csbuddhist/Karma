package com.karma.android.service

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/** Restores the status service and the scheduled checks after a reboot or an app update. */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED && intent.action != Intent.ACTION_MY_PACKAGE_REPLACED) return
        MonitorForegroundService.start(context)
        WatchdogWorker.schedule(context)
        com.karma.android.update.UpdateWorker.schedule(context)
        if (intent.action == Intent.ACTION_MY_PACKAGE_REPLACED) {
            com.karma.android.update.Updater.clear(context)
            com.karma.android.update.UpdateSettings.recordInstalled(context)
            com.karma.android.update.Updater.audit(context, "update_install", "installed")
        }
    }
}
