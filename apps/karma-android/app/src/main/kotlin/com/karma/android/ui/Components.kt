package com.karma.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import com.karma.android.R
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.karma_mobile_ffi.AdminSession
import uniffi.karma_mobile_ffi.AuthException
import uniffi.karma_mobile_ffi.KarmaCore

@Composable
fun PasswordField(value: String, onValueChange: (String) -> Unit, label: String, modifier: Modifier = Modifier) {
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        label = { Text(label) },
        singleLine = true,
        visualTransformation = PasswordVisualTransformation(),
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
        modifier = modifier.fillMaxWidth(),
    )
}

@Composable
fun SectionTitle(text: String) {
    Text(text, style = MaterialTheme.typography.titleMedium, modifier = Modifier.padding(top = 16.dp, bottom = 8.dp))
}

@Composable
fun ChecklistRow(title: String, detail: String?, ok: Boolean?, action: String?, onAction: () -> Unit) {
    Card(modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
        Row(
            modifier = Modifier.padding(12.dp).fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                when (ok) {
                    true -> "✓"
                    false -> "!"
                    null -> "•"
                },
                style = MaterialTheme.typography.titleLarge,
                color = if (ok == false) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary,
            )
            Column(modifier = Modifier.weight(1f)) {
                Text(title, style = MaterialTheme.typography.bodyLarge)
                if (detail != null) Text(detail, style = MaterialTheme.typography.bodySmall)
            }
            if (action != null) TextButton(onClick = onAction) { Text(action) }
        }
    }
}

/** Password (or one-time recovery code) entry that yields an administrator session. */
@Composable
fun UnlockForm(core: KarmaCore, onSession: (AdminSession, recovered: Boolean) -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var secret by remember { mutableStateOf("") }
    var recovery by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }

    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (recovery) {
            OutlinedTextField(
                value = secret,
                onValueChange = { secret = it },
                label = { Text(stringResource(R.string.recovery_code)) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
        } else {
            PasswordField(secret, { secret = it }, stringResource(R.string.admin_password))
        }
        error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        Button(
            enabled = !busy && secret.isNotEmpty(),
            modifier = Modifier.fillMaxWidth(),
            onClick = {
                busy = true
                error = null
                scope.launch {
                    val now = System.currentTimeMillis()
                    val result = withContext(Dispatchers.Default) {
                        runCatching {
                            if (recovery) core.redeemRecoveryCode(secret, now) else core.verifyAdminPassword(secret, now)
                        }
                    }
                    busy = false
                    result.onSuccess {
                        secret = ""
                        AdminSessionHolder.set(it)
                        onSession(it, recovery)
                    }.onFailure {
                        error = when (it) {
                            is AuthException.RateLimited ->
                                context.getString(R.string.error_rate_limited, (it.retryAfterMs + 999) / 1000)
                            is AuthException.Failed -> context.getString(R.string.error_wrong_secret)
                            else -> context.getString(R.string.error_generic)
                        }
                    }
                }
            },
        ) { Text(stringResource(R.string.unlock)) }
        TextButton(onClick = {
            recovery = !recovery
            secret = ""
            error = null
        }) {
            Text(stringResource(if (recovery) R.string.use_password else R.string.use_recovery_code))
        }
    }
}

@Composable
fun RecoveryCodesDialog(codes: List<String>, onDone: () -> Unit) {
    AlertDialog(
        onDismissRequest = {},
        title = { Text(stringResource(R.string.recovery_codes_title)) },
        text = {
            Column {
                Text(stringResource(R.string.recovery_codes_explanation))
                codes.forEach {
                    Text(it, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.titleMedium)
                }
            }
        },
        confirmButton = { Button(onClick = onDone) { Text(stringResource(R.string.recovery_codes_saved)) } },
    )
}
