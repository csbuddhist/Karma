package com.karma.android.guard

import android.app.admin.DeviceAdminReceiver
import android.content.Context
import android.content.Intent
import com.karma.android.R
import com.karma.android.security.CoreBootstrap
import com.karma.android.service.Alerts
import uniffi.karma_mobile_ffi.AuditEventRecord

/**
 * Being an active device admin makes Android refuse to uninstall Karma until the admin
 * is deactivated, and the settings watchdog blocks the deactivation page (tier L1).
 */
class KarmaDeviceAdminReceiver : DeviceAdminReceiver() {
    override fun onEnabled(context: Context, intent: Intent) {
        audit(context, "device_admin_enabled")
        DeviceOwnerPolicies.apply(context)
    }

    override fun onProfileProvisioningComplete(context: Context, intent: Intent) {
        DeviceOwnerPolicies.apply(context)
    }

    override fun onDisableRequested(context: Context, intent: Intent): CharSequence =
        context.getString(R.string.device_admin_disable_warning)

    override fun onDisabled(context: Context, intent: Intent) {
        audit(context, "device_admin_disabled")
        if (!GuardState.inMaintenance()) {
            Alerts.protectionWeakened(context, context.getString(R.string.alert_device_admin_disabled))
        }
    }

    private fun audit(context: Context, kind: String) {
        runCatching {
            CoreBootstrap.get(context).recordAudit(
                AuditEventRecord(System.currentTimeMillis(), kind, "observed", null, null, null),
            )
        }
    }
}
