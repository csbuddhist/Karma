package com.karma.android.update

import android.app.Notification
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.os.Build
import com.karma.android.AppLanguage
import com.karma.android.KarmaApp
import com.karma.android.R
import com.karma.android.guard.DeviceOwnerPolicies
import com.karma.android.guard.GuardState
import java.io.File

/**
 * Installs a verified update through [PackageInstaller]. Under Device Owner, and on
 * Android 12+ once Karma itself installed the previous version, Android installs it
 * without asking; otherwise the system confirmation appears (and a notification, in case
 * it cannot be shown at once). The settings guard lets the installer window through for
 * [GuardState.INSTALLER_WINDOW_MS] so that it does not back out of the confirmation.
 */
object UpdateInstaller {
    private const val NOTIFICATION_ID = 3

    /** Whether Android lets Karma install packages at all. */
    fun canInstall(context: Context): Boolean =
        DeviceOwnerPolicies.isDeviceOwner(context) || context.packageManager.canRequestPackageInstalls()

    fun install(context: Context, apk: File): Boolean = runCatching {
        val installer = context.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {
            setAppPackageName(context.packageName)
            setSize(apk.length())
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED)
            }
            // No package source: "downloaded file" would make Android 13+ treat Karma as newly
            // sideloaded and refuse its accessibility service until restricted settings are
            // allowed again, which would switch protection off after every update.
        }
        val sessionId = installer.createSession(params)
        installer.openSession(sessionId).use { session ->
            apk.inputStream().use { input ->
                session.openWrite("karma.apk", 0, apk.length()).use { output ->
                    input.copyTo(output)
                    session.fsync(output)
                }
            }
            GuardState.allowInstaller()
            val callback = PendingIntent.getBroadcast(
                context,
                sessionId,
                Intent(context, UpdateStatusReceiver::class.java),
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE,
            )
            session.commit(callback.intentSender)
        }
        true
    }.getOrElse { false }

    internal fun askToConfirm(context: Context, confirm: Intent) {
        GuardState.allowInstaller()
        confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        // The bound accessibility service lets Karma start the confirmation from the background.
        runCatching { context.startActivity(confirm) }
        val manager = context.getSystemService(NotificationManager::class.java)
        if (!manager.areNotificationsEnabled()) return
        val open = PendingIntent.getActivity(context, 1, confirm, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val strings = AppLanguage.localized(context)
        manager.notify(
            NOTIFICATION_ID,
            Notification.Builder(context, KarmaApp.CHANNEL_ALERTS)
                .setSmallIcon(R.drawable.ic_stat_karma)
                .setContentTitle(strings.getString(R.string.update_ready_title))
                .setContentText(strings.getString(R.string.update_ready_text))
                .setContentIntent(open)
                .setAutoCancel(true)
                .build(),
        )
    }
}

/** Receives the [PackageInstaller] session result. */
class UpdateStatusReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                @Suppress("DEPRECATION")
                val confirm = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
                } else {
                    intent.getParcelableExtra(Intent.EXTRA_INTENT)
                }
                confirm?.let { UpdateInstaller.askToConfirm(context, it) }
                Updater.audit(context, "update_install", "pending_user_action")
            }
            // On success Android replaces and restarts Karma; BootReceiver cleans up.
            PackageInstaller.STATUS_SUCCESS -> Unit
            else -> {
                GuardState.closeInstaller()
                UpdateSettings.recordInstallFailure(context)
                Updater.audit(context, "update_install", "failed")
            }
        }
    }
}
