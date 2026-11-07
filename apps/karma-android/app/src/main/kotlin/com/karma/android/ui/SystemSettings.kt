package com.karma.android.ui

import android.app.admin.DevicePolicyManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.PowerManager
import android.provider.Settings
import com.karma.android.R
import com.karma.android.guard.DeviceOwnerPolicies

/** Intents for the onboarding checklist, including vendor autostart pages. */
object SystemSettings {
    fun accessibility() = Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)

    /** App info, where Android 13+ offers "Allow restricted settings" for sideloaded apps. */
    fun appDetails(context: Context) =
        Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${context.packageName}"))

    fun addDeviceAdmin(context: Context) = Intent(DevicePolicyManager.ACTION_ADD_DEVICE_ADMIN)
        .putExtra(DevicePolicyManager.EXTRA_DEVICE_ADMIN, DeviceOwnerPolicies.admin(context))
        .putExtra(DevicePolicyManager.EXTRA_ADD_EXPLANATION, context.getString(R.string.device_admin_explanation))

    fun isIgnoringBatteryOptimizations(context: Context): Boolean =
        context.getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(context.packageName)

    @android.annotation.SuppressLint("BatteryLife")
    fun requestIgnoreBatteryOptimizations(context: Context) =
        Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:${context.packageName}"))

    /** Known autostart / background-management pages of Chinese vendor ROMs. */
    private val VENDOR_AUTOSTART = listOf(
        ComponentName("com.miui.securitycenter", "com.miui.permcenter.autostart.AutoStartManagementActivity"),
        ComponentName("com.coloros.safecenter", "com.coloros.safecenter.permission.startup.StartupAppListActivity"),
        ComponentName("com.oplus.safecenter", "com.oplus.safecenter.permission.startup.StartupAppListActivity"),
        ComponentName("com.vivo.permissionmanager", "com.vivo.permissionmanager.activity.BgStartUpManagerActivity"),
        ComponentName("com.iqoo.secure", "com.iqoo.secure.ui.phoneoptimize.BgStartUpManager"),
        ComponentName("com.huawei.systemmanager", "com.huawei.systemmanager.startupmgr.ui.StartupNormalAppListActivity"),
        ComponentName("com.hihonor.systemmanager", "com.hihonor.systemmanager.startupmgr.ui.StartupNormalAppListActivity"),
        ComponentName("com.samsung.android.lool", "com.samsung.android.sm.battery.ui.BatteryActivity"),
    )

    /** The first vendor autostart page that exists on this device, or app info. */
    fun vendorAutostart(context: Context): Intent {
        val pm = context.packageManager
        return VENDOR_AUTOSTART
            .map { Intent().setComponent(it) }
            .firstOrNull { it.resolveActivity(pm) != null }
            ?: appDetails(context)
    }

    const val DEVICE_OWNER_COMMAND =
        "adb shell dpm set-device-owner com.karma.android/.guard.KarmaDeviceAdminReceiver"
}
