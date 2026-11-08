package com.karma.android.update

import android.content.Context
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Automatic-update switch and the result of the last check. */
object UpdateSettings {
    private const val PREFS = "karma.update"
    private const val KEY_AUTO = "auto"
    private const val KEY_CHECKED_AT = "checked_at"
    private const val KEY_OUTCOME = "outcome"
    private const val KEY_VERSION = "version"

    data class Status(val checkedAtMs: Long, val outcome: Updater.Outcome?, val version: String?)

    private fun prefs(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    fun isAutomatic(context: Context): Boolean = prefs(context).getBoolean(KEY_AUTO, true)

    fun setAutomatic(context: Context, automatic: Boolean) {
        prefs(context).edit().putBoolean(KEY_AUTO, automatic).apply()
        UpdateWorker.schedule(context)
    }

    fun status(context: Context): Status {
        val prefs = prefs(context)
        return Status(
            prefs.getLong(KEY_CHECKED_AT, 0),
            prefs.getString(KEY_OUTCOME, null)?.let { runCatching { Updater.Outcome.valueOf(it) }.getOrNull() },
            prefs.getString(KEY_VERSION, null),
        )
    }

    internal fun recordCheck(context: Context, outcome: Updater.Outcome, version: String?) {
        prefs(context).edit()
            .putLong(KEY_CHECKED_AT, System.currentTimeMillis())
            .putString(KEY_OUTCOME, outcome.name)
            .putString(KEY_VERSION, version)
            .apply()
    }

    /** Called once Android has replaced the app, so the status no longer says "installing". */
    internal fun recordInstalled(context: Context) {
        prefs(context).edit()
            .putString(KEY_OUTCOME, Updater.Outcome.INSTALLED.name)
            .putString(KEY_VERSION, com.karma.android.BuildConfig.VERSION_NAME)
            .apply()
    }

    internal fun recordInstallFailure(context: Context) {
        prefs(context).edit().putString(KEY_OUTCOME, Updater.Outcome.INSTALL_BLOCKED.name).apply()
    }
}

/**
 * Checks for updates twice a day on an unmetered network (an APK is about 64 MB), and once
 * on demand on any network.
 */
class UpdateWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result = withContext(Dispatchers.IO) {
        Updater.run(applicationContext)
        Result.success()
    }

    companion object {
        private const val PERIODIC = "karma.update"
        private const val NOW = "karma.update.now"

        fun schedule(context: Context) {
            val manager = WorkManager.getInstance(context)
            if (!UpdateSettings.isAutomatic(context)) {
                manager.cancelUniqueWork(PERIODIC)
                return
            }
            val request = PeriodicWorkRequestBuilder<UpdateWorker>(12, TimeUnit.HOURS)
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.UNMETERED).build())
                .setInitialDelay(10, TimeUnit.MINUTES)
                .build()
            manager.enqueueUniquePeriodicWork(PERIODIC, ExistingPeriodicWorkPolicy.KEEP, request)
        }

        /** Starts one check unless one is already waiting or running; repeated taps do not restart a download. */
        fun checkNow(context: Context) {
            val request = OneTimeWorkRequestBuilder<UpdateWorker>()
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .build()
            WorkManager.getInstance(context).enqueueUniqueWork(NOW, ExistingWorkPolicy.KEEP, request)
        }
    }
}
