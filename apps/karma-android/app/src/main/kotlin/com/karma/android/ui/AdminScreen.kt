package com.karma.android.ui

import android.app.TimePickerDialog
import android.content.Context
import android.content.Intent
import android.widget.Toast
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.karma.android.R
import com.karma.android.guard.DeviceOwnerPolicies
import com.karma.android.guard.GuardState
import com.karma.android.monitor.enforce.label
import com.karma.android.update.UpdateInstaller
import com.karma.android.update.UpdateSettings
import java.text.DateFormat
import java.util.Date
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.karma_mobile_ffi.KarmaCore

/** Administrator area. Every change is authorized by the session token in the Rust core. */
@Composable
fun AdminScreen(core: KarmaCore, token: String, onLocked: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var revision by remember { mutableIntStateOf(0) }
    val policy = remember(revision) { PolicyDocument(core.getPolicy()) }
    var protection by remember(revision) { mutableStateOf(policy.protectionEnabled) }
    var image by remember(revision) { mutableStateOf(policy.imageEnabled) }
    var secureClose by remember(revision) { mutableStateOf(policy.closeSecureBrowserWindows) }
    var cooldown by remember(revision) { mutableStateOf(policy.cooldownSeconds.toFloat()) }
    var blockedSite by remember { mutableStateOf("") }
    var allowedSite by remember { mutableStateOf("") }
    var pickApp by remember { mutableStateOf(false) }
    var scheduling by remember { mutableStateOf<PolicyDocument.AppSchedule?>(null) }
    var newPassword by remember { mutableStateOf("") }
    var confirmPassword by remember { mutableStateOf("") }
    var codes by remember { mutableStateOf<List<String>?>(null) }
    var confirmUninstall by remember { mutableStateOf(false) }

    fun toast(resource: Int) = Toast.makeText(context, resource, Toast.LENGTH_SHORT).show()

    /** Saves the edited policy; an expired session locks the admin area again. */
    fun save(edit: PolicyDocument.() -> Unit) {
        val document = PolicyDocument(core.getPolicy()).apply(edit)
        runCatching { core.setPolicy(token, document.toJson(), System.currentTimeMillis()) }
            .onSuccess { revision++; toast(R.string.saved) }
            .onFailure {
                if (it is uniffi.karma_mobile_ffi.CoreException.Unauthorized) onLocked() else toast(R.string.error_invalid_policy)
            }
    }

    fun authorize(operation: String): Boolean =
        runCatching { core.authorizePrivileged(token, operation, System.currentTimeMillis()) }
            .onFailure { onLocked() }
            .isSuccess

    Column(modifier = Modifier.padding(16.dp).verticalScroll(rememberScrollState())) {
        SectionTitle(stringResource(R.string.admin_protection))
        SwitchRow(stringResource(R.string.policy_protection), protection) { protection = it }
        SwitchRow(stringResource(R.string.policy_image), image) { image = it }
        SwitchRow(stringResource(R.string.policy_secure_window), secureClose) { secureClose = it }
        Text(stringResource(R.string.policy_cooldown, cooldown.toInt()))
        Slider(
            value = cooldown,
            onValueChange = { cooldown = it },
            valueRange = PolicyDocument.MIN_COOLDOWN.toFloat()..600f,
            steps = 58,
        )
        Button(modifier = Modifier.fillMaxWidth(), onClick = {
            save {
                protectionEnabled = protection
                imageEnabled = image
                closeSecureBrowserWindows = secureClose
                cooldownSeconds = cooldown.toInt()
            }
        }) { Text(stringResource(R.string.save)) }

        SectionTitle(stringResource(R.string.admin_apps))
        policy.appRules.forEach { rule ->
            RuleRow(
                "${rule.label} (${rule.packageName})",
                stringResource(if (rule.block) R.string.rule_block else R.string.rule_allow),
            ) { save { removeAppRule(rule.packageName) } }
        }
        policy.appSchedules.forEach { schedule ->
            RuleRow(
                "${schedule.label} (${schedule.packageName})",
                scheduleSummary(schedule),
                onEdit = { scheduling = schedule },
            ) { save { removeAppSchedule(schedule.packageName) } }
        }
        OutlinedButton(onClick = { pickApp = true }) { Text(stringResource(R.string.add_app)) }

        SectionTitle(stringResource(R.string.admin_websites))
        Text(stringResource(R.string.website_hint), style = MaterialTheme.typography.bodySmall)
        SiteList(
            title = stringResource(R.string.website_blocklist),
            hosts = policy.siteRules.filter { it.block }.map { it.host },
            input = blockedSite,
            onInput = { blockedSite = it },
            addLabel = stringResource(R.string.website_add_block),
            onAdd = { save { setSiteRule(blockedSite, block = true) }; blockedSite = "" },
            onRemove = { host -> save { removeSiteRule(host) } },
        )
        SiteList(
            title = stringResource(R.string.website_allowlist),
            hosts = policy.siteRules.filter { !it.block }.map { it.host },
            input = allowedSite,
            onInput = { allowedSite = it },
            addLabel = stringResource(R.string.website_add_allow),
            onAdd = { save { setSiteRule(allowedSite, block = false) }; allowedSite = "" },
            onRemove = { host -> save { removeSiteRule(host) } },
        )

        SectionTitle(stringResource(R.string.admin_audit))
        AuditList(core)

        SectionTitle(stringResource(R.string.admin_account))
        PasswordField(newPassword, { newPassword = it }, stringResource(R.string.new_password))
        PasswordField(confirmPassword, { confirmPassword = it }, stringResource(R.string.confirm_password))
        if (confirmPassword.isNotEmpty() && confirmPassword != newPassword) {
            Text(stringResource(R.string.error_password_mismatch), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
        }
        Button(enabled = newPassword.isNotEmpty() && confirmPassword == newPassword, onClick = {
            val password = newPassword
            newPassword = ""
            confirmPassword = ""
            scope.launch {
                val result = withContext(Dispatchers.Default) {
                    runCatching { core.changeAdminPassword(token, password, System.currentTimeMillis()) }
                }
                result.onSuccess { toast(R.string.password_changed) }.onFailure {
                    if (it is uniffi.karma_mobile_ffi.AuthException.SessionExpired) onLocked()
                    else toast(R.string.error_weak_password)
                }
            }
        }) { Text(stringResource(R.string.change_password)) }
        OutlinedButton(onClick = {
            scope.launch {
                withContext(Dispatchers.Default) { runCatching { core.regenerateRecoveryCodes(token, System.currentTimeMillis()) } }
                    .onSuccess { codes = it }
                    .onFailure { onLocked() }
            }
        }) { Text(stringResource(R.string.regenerate_codes)) }

        SectionTitle(stringResource(R.string.updates_title))
        UpdatesSection(onAuthorize = ::authorize)

        SectionTitle(stringResource(R.string.admin_guard))
        Text(stringResource(R.string.maintenance_explanation), style = MaterialTheme.typography.bodySmall)
        OutlinedButton(onClick = {
            if (authorize("maintenance")) {
                GuardState.openMaintenance()
                toast(R.string.maintenance_opened)
            }
        }) { Text(stringResource(R.string.open_maintenance)) }
        if (GuardState.isArmed(context)) {
            OutlinedButton(onClick = {
                if (authorize("disarm_guard")) {
                    GuardState.setArmed(context, false)
                    revision++
                }
            }) { Text(stringResource(R.string.guard_disarm)) }
        }
        OutlinedButton(onClick = { confirmUninstall = true }) { Text(stringResource(R.string.prepare_uninstall)) }
        HorizontalDivider(modifier = Modifier.padding(vertical = 12.dp))
        TextButton(onClick = {
            core.endSession(token)
            AdminSessionHolder.clear()
            onLocked()
        }) { Text(stringResource(R.string.lock_admin)) }
    }

    if (pickApp) {
        AppPickerDialog(
            onDismiss = { pickApp = false },
            onPick = { packageName, label, block ->
                pickApp = false
                save { setAppRule(packageName, label, block) }
            },
            onSchedule = { packageName, label ->
                pickApp = false
                scheduling = policy.appSchedules.firstOrNull { it.packageName == packageName }
                    ?: PolicyDocument.AppSchedule(packageName, label, allow = true, windows = listOf(PolicyDocument.TimeWindow("08:00", "09:00")))
            },
        )
    }
    scheduling?.let { initial ->
        AppScheduleDialog(initial, onDismiss = { scheduling = null }) { schedule ->
            scheduling = null
            save { setAppSchedule(schedule) }
        }
    }
    codes?.let { RecoveryCodesDialog(it) { codes = null } }
    if (confirmUninstall) {
        AlertDialog(
            onDismissRequest = { confirmUninstall = false },
            title = { Text(stringResource(R.string.prepare_uninstall)) },
            text = { Text(stringResource(R.string.prepare_uninstall_explanation)) },
            confirmButton = {
                Button(onClick = {
                    confirmUninstall = false
                    if (authorize("prepare_uninstall")) prepareUninstall(context)
                }) { Text(stringResource(R.string.confirm)) }
            },
            dismissButton = { TextButton(onClick = { confirmUninstall = false }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}

private fun prepareUninstall(context: Context) {
    GuardState.openMaintenance()
    GuardState.setArmed(context, false)
    runCatching { DeviceOwnerPolicies.releaseForUninstall(context) }
    context.startActivity(
        Intent(Intent.ACTION_DELETE, android.net.Uri.parse("package:${context.packageName}"))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
    )
}

/** Update preferences. Checking and installing a newer version is on the status screen. */
@Composable
private fun UpdatesSection(onAuthorize: (String) -> Boolean) {
    val context = LocalContext.current
    var automatic by remember { mutableStateOf(UpdateSettings.isAutomatic(context)) }
    val canInstall = remember { UpdateInstaller.canInstall(context) }

    SwitchRow(stringResource(R.string.update_automatic), automatic) {
        automatic = it
        UpdateSettings.setAutomatic(context, it)
    }
    Text(stringResource(R.string.update_explanation), style = MaterialTheme.typography.bodySmall)
    if (!canInstall) {
        Text(stringResource(R.string.update_needs_permission), style = MaterialTheme.typography.bodySmall)
        OutlinedButton(onClick = {
            // The "install unknown apps" page names Karma, so the guard stands down briefly.
            if (onAuthorize("maintenance")) {
                GuardState.openMaintenance()
                runCatching {
                    context.startActivity(
                        Intent(android.provider.Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, android.net.Uri.parse("package:${context.packageName}")),
                    )
                }
            }
        }) { Text(stringResource(R.string.update_allow_installs)) }
    }
}

@Composable
private fun SwitchRow(label: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(label, modifier = Modifier.weight(1f))
        Switch(checked = checked, onCheckedChange = onChange)
    }
}

@Composable
private fun RuleRow(title: String, effect: String, onEdit: (() -> Unit)? = null, onRemove: () -> Unit) {
    Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Column(modifier = Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyMedium)
            Text(effect, style = MaterialTheme.typography.bodySmall)
        }
        onEdit?.let { TextButton(onClick = it) { Text(stringResource(R.string.edit)) } }
        TextButton(onClick = onRemove) { Text(stringResource(R.string.remove)) }
    }
}

/** One website list: its rules, then a field and button that add a domain to it. */
@Composable
private fun SiteList(
    title: String,
    hosts: List<String>,
    input: String,
    onInput: (String) -> Unit,
    addLabel: String,
    onAdd: () -> Unit,
    onRemove: (String) -> Unit,
) {
    Text(title, style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(top = 12.dp))
    if (hosts.isEmpty()) Text(stringResource(R.string.website_list_empty), style = MaterialTheme.typography.bodySmall)
    hosts.forEach { host ->
        Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(host, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
            TextButton(onClick = { onRemove(host) }) { Text(stringResource(R.string.remove)) }
        }
    }
    OutlinedTextField(
        value = input,
        onValueChange = onInput,
        label = { Text(stringResource(R.string.website_host)) },
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
    )
    OutlinedButton(enabled = input.isNotBlank(), onClick = onAdd) { Text(addLabel) }
}

@Composable
private fun AuditList(core: KarmaCore) {
    val events = remember { core.queryAudit(0, 100u) }
    val format = remember { DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT) }
    if (events.isEmpty()) Text(stringResource(R.string.audit_empty), style = MaterialTheme.typography.bodySmall)
    Column(modifier = Modifier.heightIn(max = 360.dp).verticalScroll(rememberScrollState())) {
        events.forEach { event ->
            Text(
                buildString {
                    append(format.format(Date(event.occurredAtMs)))
                    append("  ")
                    append(event.kind)
                    append(" · ")
                    append(event.outcome)
                    event.packageName?.let { append(" · ").append(it) }
                },
                style = MaterialTheme.typography.bodySmall,
            )
            event.reason?.let { Text(stringResource(it.label()), style = MaterialTheme.typography.labelSmall) }
        }
    }
}

@Composable
private fun AppPickerDialog(onDismiss: () -> Unit, onPick: (String, String, Boolean) -> Unit, onSchedule: (String, String) -> Unit) {
    val context = LocalContext.current
    val apps = remember {
        val pm = context.packageManager
        pm.queryIntentActivities(Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER), 0)
            .map { it.activityInfo.packageName to it.loadLabel(pm).toString() }
            .filter { it.first != context.packageName }
            .distinctBy { it.first }
            .sortedBy { it.second.lowercase() }
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.add_app)) },
        text = {
            LazyColumn(modifier = Modifier.heightIn(max = 420.dp)) {
                items(apps) { (packageName, label) ->
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(label, modifier = Modifier.weight(1f))
                        TextButton(onClick = { onPick(packageName, label, true) }) { Text(stringResource(R.string.rule_block)) }
                        TextButton(onClick = { onPick(packageName, label, false) }) { Text(stringResource(R.string.rule_allow)) }
                        TextButton(onClick = { onSchedule(packageName, label) }) { Text(stringResource(R.string.rule_schedule)) }
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}

@Composable
private fun scheduleSummary(schedule: PolicyDocument.AppSchedule): String {
    val windows = schedule.windows.joinToString(stringResource(R.string.schedule_separator)) { "${it.start}–${it.end}" }
    return stringResource(if (schedule.allow) R.string.schedule_summary_allow else R.string.schedule_summary_block, windows)
}

/** Edits the daily windows of one app, to the minute. */
@Composable
private fun AppScheduleDialog(
    initial: PolicyDocument.AppSchedule,
    onDismiss: () -> Unit,
    onSave: (PolicyDocument.AppSchedule) -> Unit,
) {
    val context = LocalContext.current
    var allow by remember { mutableStateOf(initial.allow) }
    var windows by remember { mutableStateOf(initial.windows) }
    val valid = windows.isNotEmpty() && windows.all { it.start != it.end }

    /** Opens the system time picker (24-hour) for one end of window [index]. */
    fun pick(index: Int, start: Boolean) {
        val current = if (start) windows[index].start else windows[index].end
        TimePickerDialog(
            context,
            { _, hour, minute ->
                val value = "%02d:%02d".format(hour, minute)
                windows = windows.mapIndexed { i, window ->
                    if (i != index) window else if (start) window.copy(start = value) else window.copy(end = value)
                }
            },
            current.substringBefore(':').toIntOrNull() ?: 0,
            current.substringAfter(':').toIntOrNull() ?: 0,
            true,
        ).show()
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(initial.label) },
        text = {
            Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
                listOf(true to R.string.schedule_mode_allow, false to R.string.schedule_mode_block).forEach { (mode, label) ->
                    Row(
                        modifier = Modifier.fillMaxWidth().selectable(selected = allow == mode, onClick = { allow = mode }),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        RadioButton(selected = allow == mode, onClick = { allow = mode })
                        Text(stringResource(label))
                    }
                }
                windows.forEachIndexed { index, window ->
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        OutlinedButton(onClick = { pick(index, start = true) }) { Text(window.start) }
                        Text("–", modifier = Modifier.padding(horizontal = 4.dp))
                        OutlinedButton(onClick = { pick(index, start = false) }) { Text(window.end) }
                        if (windows.size > 1) {
                            TextButton(onClick = { windows = windows.filterIndexed { i, _ -> i != index } }) {
                                Text(stringResource(R.string.remove))
                            }
                        }
                    }
                }
                if (windows.size < PolicyDocument.MAX_WINDOWS) {
                    TextButton(onClick = { windows = windows + PolicyDocument.TimeWindow("08:00", "09:00") }) {
                        Text(stringResource(R.string.schedule_add_window))
                    }
                }
                Text(stringResource(R.string.schedule_hint), style = MaterialTheme.typography.bodySmall)
                if (!valid) {
                    Text(stringResource(R.string.schedule_invalid), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
                }
            }
        },
        confirmButton = {
            Button(enabled = valid, onClick = { onSave(initial.copy(allow = allow, windows = windows)) }) { Text(stringResource(R.string.save)) }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}
