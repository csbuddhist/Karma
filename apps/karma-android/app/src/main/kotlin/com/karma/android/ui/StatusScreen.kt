package com.karma.android.ui

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import com.karma.android.BuildConfig
import com.karma.android.R
import com.karma.android.guard.DeviceOwnerPolicies
import com.karma.android.guard.GuardState
import com.karma.android.monitor.MonitorStatus
import com.karma.android.monitor.enforce.label
import com.karma.android.service.MonitorForegroundService
import com.karma.android.service.WatchdogWorker
import com.karma.android.update.UpdateInstaller
import com.karma.android.update.UpdateSettings
import com.karma.android.update.UpdateWorker
import com.karma.android.update.Updater
import java.text.DateFormat
import java.util.Date
import uniffi.karma_mobile_ffi.KarmaCore

/** Protection state and the onboarding checklist. [refresh] changes on every resume. */
@Composable
fun StatusScreen(core: KarmaCore, refresh: Int) {
    val context = LocalContext.current
    val monitor by MonitorStatus.flow.collectAsState()
    var localRefresh by remember { mutableIntStateOf(0) }
    val notificationLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        localRefresh++
    }
    // System settings change outside the app; re-read them on every resume or action.
    val tick = refresh + localRefresh
    val coreStatus = remember(tick) { core.status() }
    val accessibility = remember(tick) { WatchdogWorker.isAccessibilityEnabled(context) }
    val admin = remember(tick) { DeviceOwnerPolicies.isAdminActive(context) }
    val owner = remember(tick) { DeviceOwnerPolicies.isDeviceOwner(context) }
    val battery = remember(tick) { SystemSettings.isIgnoringBatteryOptimizations(context) }
    val notifications = remember(tick) {
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED
    }
    val armed = remember(tick) { GuardState.isArmed(context) }

    Column(modifier = Modifier.padding(16.dp).verticalScroll(rememberScrollState())) {
        Text(
            stringResource(
                when {
                    !coreStatus.protectionEnabled -> R.string.status_protection_off
                    accessibility && monitor.serviceConnected -> R.string.status_protection_on
                    else -> R.string.status_protection_incomplete
                },
            ),
            style = MaterialTheme.typography.headlineSmall,
        )
        Text(
            stringResource(
                when (monitor.model) {
                    MonitorStatus.Model.READY -> R.string.status_model_ready
                    MonitorStatus.Model.MISSING -> R.string.status_model_missing
                    MonitorStatus.Model.REJECTED -> R.string.status_model_rejected
                    MonitorStatus.Model.LOADING -> R.string.status_model_loading
                },
            ),
            style = MaterialTheme.typography.bodyMedium,
        )
        Text(stringResource(R.string.status_counters, monitor.framesScored, monitor.closures), style = MaterialTheme.typography.bodySmall)
        monitor.lastClosureReason?.let { reason ->
            val at = monitor.lastClosureAtMs?.let { DateFormat.getTimeInstance().format(Date(it)) } ?: ""
            Text(stringResource(R.string.status_last_closure, stringResource(reason.label()), at), style = MaterialTheme.typography.bodySmall)
        }

        SectionTitle(stringResource(R.string.checklist_title))
        ChecklistRow(
            stringResource(R.string.check_accessibility),
            stringResource(R.string.check_accessibility_detail),
            accessibility,
            stringResource(R.string.open),
        ) { start(context, SystemSettings.accessibility()) }
        if (!accessibility) {
            ChecklistRow(
                stringResource(R.string.check_restricted_settings),
                stringResource(R.string.check_restricted_settings_detail),
                null,
                stringResource(R.string.open),
            ) { start(context, SystemSettings.appDetails(context)) }
        }
        ChecklistRow(stringResource(R.string.check_device_admin), stringResource(R.string.check_device_admin_detail), admin, stringResource(R.string.enable)) {
            start(context, SystemSettings.addDeviceAdmin(context))
        }
        ChecklistRow(stringResource(R.string.check_battery), stringResource(R.string.check_battery_detail), battery, stringResource(R.string.open)) {
            start(context, SystemSettings.requestIgnoreBatteryOptimizations(context))
        }
        ChecklistRow(stringResource(R.string.check_notifications), null, notifications, stringResource(R.string.enable)) {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                notificationLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
            }
        }
        ChecklistRow(stringResource(R.string.check_autostart), stringResource(R.string.check_autostart_detail), null, stringResource(R.string.open)) {
            start(context, *SystemSettings.vendorAutostart(context).toTypedArray())
        }
        ChecklistRow(
            stringResource(R.string.check_device_owner),
            stringResource(if (owner) R.string.check_device_owner_active else R.string.check_device_owner_detail),
            if (owner) true else null,
            null,
        ) {}
        if (!owner) {
            Text(SystemSettings.DEVICE_OWNER_COMMAND, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
        }

        SectionTitle(stringResource(R.string.guard_title))
        Text(stringResource(if (armed) R.string.guard_armed else R.string.guard_disarmed))
        if (!armed) {
            Button(
                enabled = accessibility && admin,
                modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
                onClick = {
                    GuardState.setArmed(context, true)
                    MonitorForegroundService.start(context)
                    localRefresh++
                },
            ) { Text(stringResource(R.string.guard_arm)) }
        }

        SectionTitle(stringResource(R.string.updates_title))
        UpdateStatusSection(tick) { localRefresh++ }

        Text(stringResource(R.string.supervisor_advice), style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(top = 16.dp))
    }
}

/**
 * Installed version, the last check, and a check that anyone using the phone may start: the
 * updater only installs a higher version signed by the same certificate, so no password is
 * needed. Turning automatic updates off and allowing installs stay with the administrator.
 */
@Composable
private fun UpdateStatusSection(tick: Int, onRefresh: () -> Unit) {
    val context = LocalContext.current
    val status = remember(tick) { UpdateSettings.status(context) }
    val canInstall = remember(tick) { UpdateInstaller.canInstall(context) }
    val format = remember { DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT) }

    Text(stringResource(R.string.update_installed_version, BuildConfig.VERSION_NAME))
    Text(
        if (status.checkedAtMs == 0L) {
            stringResource(R.string.update_never_checked)
        } else {
            // An update installed before this fix may still read "installing" its own version.
            val outcome = if (status.outcome == Updater.Outcome.INSTALLING && status.version == BuildConfig.VERSION_NAME) Updater.Outcome.INSTALLED else status.outcome
            stringResource(R.string.update_last_check, format.format(Date(status.checkedAtMs)), stringResource(updateOutcomeLabel(outcome, status.version != null)), status.version ?: "")
        },
        style = MaterialTheme.typography.bodySmall,
    )
    Text(stringResource(R.string.update_anyone_explanation), style = MaterialTheme.typography.bodySmall)
    if (!canInstall) {
        Text(stringResource(R.string.update_needs_administrator), style = MaterialTheme.typography.bodySmall)
    }
    OutlinedButton(onClick = {
        UpdateWorker.checkNow(context)
        Toast.makeText(context, R.string.update_checking, Toast.LENGTH_SHORT).show()
        onRefresh()
    }) { Text(stringResource(R.string.update_check_now)) }
    TextButton(onClick = onRefresh) { Text(stringResource(R.string.update_refresh)) }
}

private fun updateOutcomeLabel(outcome: Updater.Outcome?, found: Boolean): Int = when (outcome) {
    Updater.Outcome.UP_TO_DATE -> R.string.update_outcome_up_to_date
    Updater.Outcome.INSTALLING -> R.string.update_outcome_installing
    Updater.Outcome.INSTALLED -> R.string.update_outcome_installed
    Updater.Outcome.NETWORK_FAILED -> if (found) R.string.update_outcome_download_failed else R.string.update_outcome_network
    Updater.Outcome.VERIFICATION_FAILED -> R.string.update_outcome_verification
    Updater.Outcome.INSTALL_BLOCKED -> R.string.update_outcome_blocked
    null -> R.string.update_outcome_unknown
}

/** Opens the first of [intents] the system accepts, or explains that none could be opened. */
private fun start(context: Context, vararg intents: Intent) {
    val opened = intents.any { intent ->
        // ADD_DEVICE_ADMIN finishes at once when started as a new task, so stay in our task.
        if (context !is Activity) intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        runCatching { context.startActivity(intent) }.isSuccess
    }
    if (!opened) Toast.makeText(context, R.string.open_failed, Toast.LENGTH_LONG).show()
}
