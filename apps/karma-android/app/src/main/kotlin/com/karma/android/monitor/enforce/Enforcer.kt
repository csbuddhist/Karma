package com.karma.android.monitor.enforce

import android.accessibilityservice.AccessibilityService
import android.app.ActivityManager
import android.content.Context
import android.os.Handler
import android.os.Looper
import android.widget.Toast
import com.karma.android.guard.DeviceOwnerPolicies
import uniffi.karma_mobile_ffi.AuditEventRecord
import uniffi.karma_mobile_ffi.KarmaCore
import uniffi.karma_mobile_ffi.MobileAction
import uniffi.karma_mobile_ffi.MobileDecision

/**
 * Carries out decisions (technical plan §4.4). Android cannot kill a foreground
 * third-party process, so "close" means: go home, cover the screen, then kill the now
 * background process; under Device Owner, `Terminate` also suspends the package until
 * the cooldown ends.
 */
class Enforcer(private val service: AccessibilityService, private val core: KarmaCore) {
    private val handler = Handler(Looper.getMainLooper())
    private val overlay = BlockOverlay(service)
    private val activityManager = service.getSystemService(ActivityManager::class.java)
    private val suspensions = service.getSharedPreferences(SUSPENSIONS, Context.MODE_PRIVATE)

    fun apply(packageName: String, decision: MobileDecision) {
        when (decision.action) {
            MobileAction.ALLOW, MobileAction.BLOCK_NETWORK -> Unit
            MobileAction.WARN -> handler.post {
                Toast.makeText(service, decision.reason.label(), Toast.LENGTH_SHORT).show()
            }
            MobileAction.CLOSE_GRACEFULLY, MobileAction.TERMINATE -> close(packageName, decision)
        }
    }

    private fun close(packageName: String, decision: MobileDecision) {
        val wentHome = service.performGlobalAction(AccessibilityService.GLOBAL_ACTION_HOME)
        overlay.show(packageName, decision)
        handler.postDelayed({
            runCatching { activityManager.killBackgroundProcesses(packageName) }
        }, KILL_DELAY_MS)
        var suspended = false
        if (decision.action == MobileAction.TERMINATE && decision.expiresAtMs != null) {
            suspended = DeviceOwnerPolicies.suspend(service, packageName, true)
            if (suspended) {
                suspensions.edit().putLong(packageName, decision.expiresAtMs!!).apply()
                scheduleUnsuspend(packageName, decision.expiresAtMs!!)
            }
        }
        if (!decision.fromCooldown) {
            val outcome = when {
                !wentHome -> "home_failed"
                suspended -> "closed_suspended"
                else -> "closed"
            }
            runCatching {
                core.recordAudit(
                    AuditEventRecord(System.currentTimeMillis(), "enforcement", outcome, packageName, decision.action, decision.reason),
                )
            }
        }
    }

    /** Lifts suspensions whose cooldown ended, including ones from before a restart. */
    fun restoreSuspensions() {
        val now = System.currentTimeMillis()
        for ((packageName, value) in suspensions.all) {
            val until = value as? Long ?: continue
            if (until <= now) liftSuspension(packageName) else scheduleUnsuspend(packageName, until)
        }
    }

    private fun scheduleUnsuspend(packageName: String, untilMs: Long) {
        handler.postDelayed({ liftSuspension(packageName) }, (untilMs - System.currentTimeMillis()).coerceAtLeast(0))
    }

    private fun liftSuspension(packageName: String) {
        val until = suspensions.getLong(packageName, 0)
        if (until > System.currentTimeMillis()) return
        DeviceOwnerPolicies.suspend(service, packageName, false)
        suspensions.edit().remove(packageName).apply()
    }

    fun hideOverlay() = overlay.hide()

    companion object {
        private const val SUSPENSIONS = "karma.suspensions"
        private const val KILL_DELAY_MS = 1_500L
    }
}
