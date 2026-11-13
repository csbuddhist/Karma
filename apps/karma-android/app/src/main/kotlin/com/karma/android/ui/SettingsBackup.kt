package com.karma.android.ui

import android.content.Context
import android.net.Uri
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.karma.android.R
import java.io.IOException
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.karma_mobile_ffi.AuthException
import uniffi.karma_mobile_ffi.KarmaCore
import uniffi.karma_mobile_ffi.SettingsException

/**
 * Export and import of the rules, open to anyone using the phone. The Rust core seals the
 * file with a key derived from the administrator password, so it can be neither read nor
 * changed. A file sealed under another password (another phone, an earlier password)
 * needs the administrator password of this phone.
 */
@Composable
fun SettingsBackupSection(core: KarmaCore, onImported: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var busy by remember { mutableStateOf(false) }
    var pending by remember { mutableStateOf<ByteArray?>(null) }
    var needsAdministrator by remember { mutableStateOf<ByteArray?>(null) }
    var exported by remember { mutableStateOf<ByteArray?>(null) }

    fun toast(message: String) = Toast.makeText(context, message, Toast.LENGTH_LONG).show()

    val exportLauncher = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument(MIME_TYPE)) { uri ->
        val file = exported
        exported = null
        if (uri == null || file == null) return@rememberLauncherForActivityResult
        busy = true
        scope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    context.contentResolver.openOutputStream(uri, "wt")?.use { it.write(file) } ?: throw IOException()
                }
            }
            busy = false
            result.onSuccess { toast(context.getString(R.string.backup_exported)) }
                .onFailure { toast(errorMessage(context, it)) }
        }
    }
    val importLauncher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri ?: return@rememberLauncherForActivityResult
        busy = true
        scope.launch {
            val result = withContext(Dispatchers.IO) { runCatching { readLimited(context, uri) } }
            busy = false
            result.onSuccess { pending = it }.onFailure { toast(errorMessage(context, it)) }
        }
    }

    Text(stringResource(R.string.backup_explanation), style = MaterialTheme.typography.bodySmall)
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedButton(enabled = !busy, onClick = {
            // Seal first, so a refusal does not leave an empty file behind.
            busy = true
            scope.launch {
                val result = withContext(Dispatchers.Default) { runCatching { core.exportSettings(System.currentTimeMillis()) } }
                busy = false
                result.onSuccess {
                    exported = it
                    exportLauncher.launch(defaultFileName())
                }.onFailure { toast(errorMessage(context, it)) }
            }
        }) {
            Text(stringResource(R.string.backup_export))
        }
        OutlinedButton(enabled = !busy, onClick = { importLauncher.launch(arrayOf("*/*")) }) {
            Text(stringResource(R.string.backup_import))
        }
    }

    pending?.let { file ->
        AlertDialog(
            onDismissRequest = { pending = null },
            title = { Text(stringResource(R.string.backup_import_confirm_title)) },
            text = { Text(stringResource(R.string.backup_import_confirm_text)) },
            confirmButton = {
                Button(onClick = {
                    pending = null
                    busy = true
                    scope.launch {
                        val result = withContext(Dispatchers.Default) {
                            runCatching { core.importSettings(file, System.currentTimeMillis()) }
                        }
                        busy = false
                        result.onSuccess {
                            toast(context.getString(R.string.backup_imported))
                            onImported()
                        }.onFailure {
                            if (it is SettingsException.NeedsAdministrator) needsAdministrator = file else toast(errorMessage(context, it))
                        }
                    }
                }) { Text(stringResource(R.string.backup_import)) }
            },
            dismissButton = { TextButton(onClick = { pending = null }) { Text(stringResource(R.string.cancel)) } },
        )
    }
    needsAdministrator?.let { file ->
        AdministratorImportDialog(core, file, onDismiss = { needsAdministrator = null }) {
            needsAdministrator = null
            toast(context.getString(R.string.backup_imported))
            onImported()
        }
    }
}

/** Asks for this phone's administrator password and, if it differs, the file's password. */
@Composable
private fun AdministratorImportDialog(core: KarmaCore, file: ByteArray, onDismiss: () -> Unit, onImported: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var current by remember { mutableStateOf("") }
    var filePassword by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }

    AlertDialog(
        onDismissRequest = { if (!busy) onDismiss() },
        title = { Text(stringResource(R.string.backup_admin_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.backup_admin_text), style = MaterialTheme.typography.bodySmall)
                PasswordField(current, { current = it }, stringResource(R.string.backup_current_password))
                PasswordField(filePassword, { filePassword = it }, stringResource(R.string.backup_file_password))
                error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            }
        },
        confirmButton = {
            Button(enabled = !busy && current.isNotEmpty(), onClick = {
                busy = true
                error = null
                scope.launch {
                    val result = withContext(Dispatchers.Default) {
                        runCatching {
                            val session = core.verifyAdminPassword(current, System.currentTimeMillis())
                            try {
                                core.importSettingsAsAdministrator(
                                    session.token,
                                    filePassword.ifEmpty { current },
                                    file,
                                    System.currentTimeMillis(),
                                )
                            } finally {
                                core.endSession(session.token)
                            }
                        }
                    }
                    busy = false
                    result.onSuccess { onImported() }.onFailure {
                        error = if (it is SettingsException.Rejected) context.getString(R.string.backup_error_rejected_password) else errorMessage(context, it)
                    }
                }
            }) { Text(stringResource(R.string.backup_import)) }
        },
        dismissButton = { TextButton(enabled = !busy, onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}

private const val MIME_TYPE = "application/octet-stream"

/** Settings files are far below this; anything larger is not one. */
private const val MAX_FILE_BYTES = 1024 * 1024

private fun defaultFileName(): String =
    "karma-settings-${SimpleDateFormat("yyyyMMdd-HHmm", Locale.US).format(Date())}.karma"

private class FileTooLargeException : IOException()

private fun readLimited(context: Context, uri: Uri): ByteArray {
    val stream = context.contentResolver.openInputStream(uri) ?: throw IOException()
    return stream.use { input ->
        val output = java.io.ByteArrayOutputStream()
        val buffer = ByteArray(16 * 1024)
        while (true) {
            val read = input.read(buffer)
            if (read < 0) break
            output.write(buffer, 0, read)
            if (output.size() > MAX_FILE_BYTES) throw FileTooLargeException()
        }
        output.toByteArray()
    }
}

private fun errorMessage(context: Context, error: Throwable): String {
    if (error is AuthException.RateLimited) {
        return context.getString(R.string.error_rate_limited, (error.retryAfterMs + 999) / 1000)
    }
    return context.getString(errorResource(error))
}

private fun errorResource(error: Throwable): Int =
    when (error) {
        is SettingsException.NotEnrolled, is SettingsException.KeyNotReady -> R.string.backup_error_key_not_ready
        is SettingsException.InvalidFile, is FileTooLargeException -> R.string.backup_error_invalid_file
        is SettingsException.NeedsAdministrator -> R.string.backup_error_other_password
        is SettingsException.Rejected -> R.string.backup_error_rejected
        is SettingsException.InvalidPolicy -> R.string.backup_error_invalid_policy
        is AuthException.Failed -> R.string.error_wrong_secret
        else -> R.string.backup_error_storage
    }
