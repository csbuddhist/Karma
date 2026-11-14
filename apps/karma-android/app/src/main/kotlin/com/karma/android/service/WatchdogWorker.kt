package com.karma.android.service

import android.content.ComponentName
import android.content.Context
import android.provider.Settings
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import com.karma.android.AppLanguage
import com.karma.android.R
import com.karma.android.guard.DeviceOwnerPolicies
import com.karma.android.guard.GuardState
import com.karma.android.monitor.KarmaAccessibilityService
import com.karma.android.security.CoreBootstrap
import java.util.concurrent.TimeUnit
import uniffi.karma_mobile_ffi.AuditEventRecord

/**
 * Periodic self-check (every 15 minutes, the WorkManager minimum): restarts the status
 * service, re-applies Device Owner restrictions, and alerts when the accessibility
 * service or device admin was turned off while the guard is armed.
 */
class WatchdogWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val context = applicationContext
        MonitorForegroundService.start(context)
        runCatching { DeviceOwnerPolicies.apply(context) }

        if (GuardState.isArmed(context) && !GuardState.inMaintenance()) {
            val problems = buildList {
                if (!isAccessibilityEnabled(context)) add(R.string.alert_accessibility_disabled)
                if (!DeviceOwnerPolicies.isAdminActive(context)) add(R.string.alert_device_admin_disabled)
            }
            val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            val signature = problems.joinToString(",")
            if (problems.isNotEmpty() && prefs.getString(KEY_LAST, "") != signature) {
                Alerts.protectionWeakened(context, problems.joinToString("\n") { AppLanguage.localized(context).getString(it) })
                runCatching {
                    CoreBootstrap.get(context).recordAudit(
                        AuditEventRecord(System.currentTimeMillis(), "watchdog_check", "protection_weakened", null, null, null),
                    )
                }
            }
            prefs.edit().putString(KEY_LAST, signature).apply()
        }
        return Result.success()
    }

    companion object {
        private const val NAME = "karma.watchdog"
        private const val PREFS = "karma.watchdog"
        private const val KEY_LAST = "last_problems"

        fun schedule(context: Context) {
            val request = PeriodicWorkRequestBuilder<WatchdogWorker>(15, TimeUnit.MINUTES).build()
            WorkManager.getInstance(context)
                .enqueueUniquePeriodicWork(NAME, ExistingPeriodicWorkPolicy.KEEP, request)
        }

        fun isAccessibilityEnabled(context: Context): Boolean {
            val expected = ComponentName(context, KarmaAccessibilityService::class.java)
            val enabled = Settings.Secure.getString(
                context.contentResolver,
                Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES,
            ) ?: return false
            return enabled.split(':').any { ComponentName.unflattenFromString(it) == expected }
        }
    }
}
