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

    /** The watchdog acts only when armed and outside a maintenance window. */
    fun watchdogActive(context: Context): Boolean = isArmed(context) && !inMaintenance()
}
