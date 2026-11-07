package com.karma.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.karma.android.R
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.karma_mobile_ffi.AuthException
import uniffi.karma_mobile_ffi.KarmaCore

/** First run: set the administrator password and record the one-time recovery codes. */
@Composable
fun SetupScreen(core: KarmaCore, onEnrolled: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var password by remember { mutableStateOf("") }
    var confirm by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    var codes by remember { mutableStateOf<List<String>?>(null) }

    Column(
        modifier = Modifier.padding(24.dp).verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(stringResource(R.string.setup_title), style = MaterialTheme.typography.headlineSmall)
        Text(stringResource(R.string.setup_explanation))
        Text(stringResource(R.string.privacy_promise), style = MaterialTheme.typography.bodySmall)
        PasswordField(password, { password = it }, stringResource(R.string.new_password))
        PasswordField(confirm, { confirm = it }, stringResource(R.string.confirm_password))
        error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        Button(
            enabled = !busy,
            modifier = Modifier.fillMaxWidth(),
            onClick = {
                if (password != confirm) {
                    error = context.getString(R.string.error_password_mismatch)
                    return@Button
                }
                busy = true
                scope.launch {
                    val now = System.currentTimeMillis()
                    val result = withContext(Dispatchers.Default) { runCatching { core.enrollAdmin(password, now) } }
                    busy = false
                    result.onSuccess {
                        password = ""
                        confirm = ""
                        AdminSessionHolder.set(it.session)
                        codes = it.recoveryCodes
                    }.onFailure {
                        error = context.getString(
                            if (it is AuthException.WeakPassword) R.string.error_weak_password else R.string.error_generic,
                        )
                    }
                }
            },
        ) { Text(stringResource(R.string.setup_submit)) }
    }

    codes?.let { RecoveryCodesDialog(it) { codes = null; onEnrolled() } }
}
