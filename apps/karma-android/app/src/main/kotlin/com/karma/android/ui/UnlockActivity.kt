package com.karma.android.ui

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.karma.android.AppLanguage
import com.karma.android.R
import com.karma.android.guard.GuardState
import com.karma.android.security.CoreBootstrap

/**
 * Password prompt raised by the settings watchdog (to open a maintenance window) and by
 * the block overlay (to end a package's cooldown early). A recovery code leads to the
 * admin area so a new password can be set.
 */
class UnlockActivity : ComponentActivity() {
    override fun attachBaseContext(newBase: Context) {
        super.attachBaseContext(AppLanguage.localized(newBase))
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val core = CoreBootstrap.get(this)
        val purpose = intent.getStringExtra(EXTRA_PURPOSE) ?: PURPOSE_MAINTENANCE
        val packageName = intent.getStringExtra(EXTRA_PACKAGE)
        setContent {
            KarmaTheme {
                Surface {
                    Column(modifier = Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Text(stringResource(R.string.unlock_title), style = MaterialTheme.typography.headlineSmall)
                        Text(
                            stringResource(
                                if (purpose == PURPOSE_MAINTENANCE) R.string.unlock_maintenance_explanation
                                else R.string.unlock_open_app_explanation,
                            ),
                        )
                        UnlockForm(core) { session, recovered ->
                            val now = System.currentTimeMillis()
                            when {
                                recovered -> startActivity(
                                    Intent(this@UnlockActivity, MainActivity::class.java)
                                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                                )
                                purpose == PURPOSE_MAINTENANCE -> runCatching {
                                    core.authorizePrivileged(session.token, "maintenance", now)
                                    GuardState.openMaintenance()
                                    Toast.makeText(this@UnlockActivity, R.string.maintenance_opened, Toast.LENGTH_LONG).show()
                                }
                                packageName != null -> runCatching {
                                    core.clearCooldown(session.token, packageName, now)
                                }
                            }
                            if (!recovered) {
                                core.endSession(session.token)
                                AdminSessionHolder.clear()
                            }
                            finish()
                        }
                        TextButton(onClick = { finish() }) { Text(stringResource(R.string.cancel)) }
                    }
                }
            }
        }
    }

    companion object {
        const val EXTRA_PURPOSE = "com.karma.android.extra.PURPOSE"
        const val EXTRA_PACKAGE = "com.karma.android.extra.PACKAGE"
        const val PURPOSE_MAINTENANCE = "maintenance"
        const val PURPOSE_OPEN_APP = "open_app"
    }
}
