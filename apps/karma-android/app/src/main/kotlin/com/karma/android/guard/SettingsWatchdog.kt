package com.karma.android.guard

import android.accessibilityservice.AccessibilityService
import android.content.Intent
import android.view.accessibility.AccessibilityNodeInfo
import com.karma.android.R
import com.karma.android.ui.UnlockActivity
import uniffi.karma_mobile_ffi.AuditEventRecord
import uniffi.karma_mobile_ffi.KarmaCore

/**
 * Tier L1 tamper resistance: while armed, any system settings, security-center or
 * package-installer window that shows Karma (its app info, accessibility switch,
 * device-admin page, force-stop or uninstall dialog) is backed out of immediately and
 * the administrator password is requested. A correct password opens a short
 * maintenance window during which the watchdog stands down.
 */
class SettingsWatchdog(private val service: AccessibilityService, private val core: KarmaCore) {
    private var lastScanMs = 0L
    private val appLabel = service.getString(R.string.app_name)
    private val serviceLabel = service.getString(R.string.accessibility_service_label)

    fun isGuardedPackage(packageName: String): Boolean = GUARDED_PACKAGES.contains(packageName)

    /** Returns true when the window was blocked. [nowMs] is `elapsedRealtime`, used for throttling. */
    fun inspect(packageName: String, root: AccessibilityNodeInfo?, nowMs: Long): Boolean {
        if (!isGuardedPackage(packageName) || !GuardState.watchdogActive(service)) return false
        if (packageName in INSTALLER_PACKAGES && GuardState.installerAllowed()) return false
        if (nowMs - lastScanMs < SCAN_INTERVAL_MS) return false
        lastScanMs = nowMs
        root ?: return false
        if (!mentionsKarma(root)) return false

        service.performGlobalAction(AccessibilityService.GLOBAL_ACTION_BACK)
        service.performGlobalAction(AccessibilityService.GLOBAL_ACTION_BACK)
        runCatching {
            core.recordAudit(
                AuditEventRecord(System.currentTimeMillis(), "settings_watchdog", "blocked", packageName, null, null),
            )
        }
        service.startActivity(
            Intent(service, UnlockActivity::class.java)
                .putExtra(UnlockActivity.EXTRA_PURPOSE, UnlockActivity.PURPOSE_MAINTENANCE)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP),
        )
        return true
    }

    private fun mentionsKarma(root: AccessibilityNodeInfo): Boolean {
        val queue = ArrayDeque<AccessibilityNodeInfo>()
        queue.add(root)
        var scanned = 0
        while (queue.isNotEmpty() && scanned < MAX_SCANNED_NODES) {
            val node = queue.removeFirst()
            scanned++
            val text = node.text?.toString()?.trim()
            val description = node.contentDescription?.toString()?.trim()
            if (matches(text) || matches(description)) return true
            for (index in 0 until node.childCount) node.getChild(index)?.let(queue::add)
        }
        return false
    }

    private fun matches(value: String?): Boolean =
        value != null && (
            value.equals(appLabel, ignoreCase = true) ||
                value.equals(serviceLabel, ignoreCase = true) ||
                value.contains(service.packageName)
            )

    companion object {
        private const val SCAN_INTERVAL_MS = 300L
        private const val MAX_SCANNED_NODES = 600

        /** Package installers, which show the confirmation of Karma's own update. */
        val INSTALLER_PACKAGES = setOf(
            "com.android.packageinstaller",
            "com.google.android.packageinstaller",
            "com.miui.packageinstaller",
        )

        /** System settings, vendor security centers and package installers. */
        val GUARDED_PACKAGES = setOf(
            "com.android.settings",
            "com.google.android.settings.intelligence",
            "com.android.packageinstaller",
            "com.google.android.packageinstaller",
            "com.miui.securitycenter",
            "com.miui.packageinstaller",
            "com.miui.powerkeeper",
            "com.xiaomi.misettings",
            "com.coloros.safecenter",
            "com.oplus.safecenter",
            "com.coloros.oppoguardelf",
            "com.oplus.battery",
            "com.iqoo.secure",
            "com.vivo.permissionmanager",
            "com.vivo.abe",
            "com.huawei.systemmanager",
            "com.hihonor.systemmanager",
            "com.samsung.android.lool",
            "com.samsung.android.sm",
            "com.oneplus.security",
            "com.transsion.phonemaster",
        )
    }
}
