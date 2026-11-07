package com.karma.android.guard

import android.content.Context
import android.os.SystemClock

/**
 * Whether the settings watchdog is armed, and the short maintenance window an
 * administrator opens with the password (to change system settings for Karma).
 *
 * Arming is persisted; the maintenance window lives only in memory, so a restart or a
 * killed process always re-arms the guard.
 */
object GuardState {
    private const val PREFS = "karma.guard"
    private const val KEY_ARMED = "armed"
    const val MAINTENANCE_MS = 10 * 60 * 1000L

    @Volatile
    private var maintenanceUntilElapsed = 0L

    fun isArmed(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(KEY_ARMED, false)

    /** Arming needs no password; disarming goes through [setArmed] after authorization. */
    fun setArmed(context: Context, armed: Boolean) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean(KEY_ARMED, armed).apply()
    }

    fun openMaintenance() {
        maintenanceUntilElapsed = SystemClock.elapsedRealtime() + MAINTENANCE_MS
    }

    fun closeMaintenance() {
        maintenanceUntilElapsed = 0L
    }

    fun inMaintenance(): Boolean = SystemClock.elapsedRealtime() < maintenanceUntilElapsed

    fun maintenanceRemainingMs(): Long = (maintenanceUntilElapsed - SystemClock.elapsedRealtime()).coerceAtLeast(0)

    /** How long installer windows are let through after Karma commits a verified update. */
    const val INSTALLER_WINDOW_MS = 10 * 60 * 1000L

    @Volatile
    private var installerUntilElapsed = 0L

    /**
     * Lets package-installer windows through for [INSTALLER_WINDOW_MS], so that the
     * confirmation of Karma's own update is not backed out of. Settings pages stay guarded,
     * and device admin still blocks uninstalling. Memory only, like maintenance.
     */
    fun allowInstaller() {
        installerUntilElapsed = SystemClock.elapsedRealtime() + INSTALLER_WINDOW_MS
    }

    fun closeInstaller() {
        installerUntilElapsed = 0L
    }

    fun installerAllowed(): Boolean = SystemClock.elapsedRealtime() < installerUntilElapsed

    /** The watchdog acts only when armed and outside a maintenance window. */
    fun watchdogActive(context: Context): Boolean = isArmed(context) && !inMaintenance()
}
