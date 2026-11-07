package com.karma.android.guard

import android.app.admin.DevicePolicyManager
import android.content.ComponentName
import android.content.Context
import android.os.UserManager

/**
 * Tier L2 (technical plan §6): restrictions applied when Karma is the Device Owner,
 * provisioned on a device without accounts with
 * `adb shell dpm set-device-owner com.karma.android/.guard.KarmaDeviceAdminReceiver`.
 */
object DeviceOwnerPolicies {
    private val RESTRICTIONS = listOf(
        UserManager.DISALLOW_SAFE_BOOT,
        UserManager.DISALLOW_FACTORY_RESET,
        UserManager.DISALLOW_ADD_USER,
        UserManager.DISALLOW_DEBUGGING_FEATURES,
        UserManager.DISALLOW_APPS_CONTROL,
    )

    fun admin(context: Context) = ComponentName(context, KarmaDeviceAdminReceiver::class.java)

    private fun manager(context: Context) = context.getSystemService(DevicePolicyManager::class.java)

    fun isDeviceOwner(context: Context): Boolean = manager(context).isDeviceOwnerApp(context.packageName)

    fun isAdminActive(context: Context): Boolean = manager(context).isAdminActive(admin(context))

    /** Applies the L2 restrictions; a no-op unless Karma is the Device Owner. */
    fun apply(context: Context) {
        if (!isDeviceOwner(context)) return
        val manager = manager(context)
        val admin = admin(context)
        manager.setUninstallBlocked(admin, context.packageName, true)
        RESTRICTIONS.forEach { manager.addUserRestriction(admin, it) }
        // Only Karma (plus system services, which are always allowed) may use accessibility,
        // so another app cannot automate turning Karma off.
        manager.setPermittedAccessibilityServices(admin, listOf(context.packageName))
    }

    /** Suspends [packageName] (Terminate under Device Owner). Returns whether it was suspended. */
    fun suspend(context: Context, packageName: String, suspended: Boolean): Boolean {
        if (!isDeviceOwner(context)) return false
        val failed = manager(context).setPackagesSuspended(admin(context), arrayOf(packageName), suspended)
        return failed.isEmpty()
    }

    /**
     * Removes every restriction and gives up Device Owner and device admin, so Karma can
     * be uninstalled. Call only after [uniffi.karma_mobile_ffi.KarmaCore.authorizePrivileged].
     */
    @Suppress("DEPRECATION")
    fun releaseForUninstall(context: Context) {
        val manager = manager(context)
        val admin = admin(context)
        if (isDeviceOwner(context)) {
            RESTRICTIONS.forEach { manager.clearUserRestriction(admin, it) }
            manager.setPermittedAccessibilityServices(admin, null)
            manager.setUninstallBlocked(admin, context.packageName, false)
            manager.clearDeviceOwnerApp(context.packageName)
        }
        if (manager.isAdminActive(admin)) manager.removeActiveAdmin(admin)
    }
}
