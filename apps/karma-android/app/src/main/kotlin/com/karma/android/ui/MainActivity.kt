package com.karma.android.ui

import android.content.Context
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRow
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.karma.android.AppLanguage
import com.karma.android.R
import com.karma.android.security.CoreBootstrap
import com.karma.android.service.MonitorForegroundService
import uniffi.karma_mobile_ffi.KarmaCore

class MainActivity : ComponentActivity() {
    private var resumes by mutableIntStateOf(0)

    override fun attachBaseContext(newBase: Context) {
        super.attachBaseContext(AppLanguage.localized(newBase))
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        MonitorForegroundService.start(this)
        setContent {
            KarmaTheme {
                Scaffold { padding ->
                    Column(modifier = Modifier.padding(padding).fillMaxSize()) {
                        KarmaRoot(resumes)
                    }
                }
            }
        }
    }

    override fun onResume() {
        super.onResume()
        resumes++
    }

    override fun onStop() {
        super.onStop()
        // Leaving the app ends administrator access.
        if (!isChangingConfigurations) AdminSessionHolder.clear()
    }
}

@Composable
private fun KarmaRoot(resumes: Int) {
    val context = LocalContext.current
    val core: KarmaCore = remember { CoreBootstrap.get(context) }
    var enrolled by remember { mutableStateOf(core.status().enrolled) }
    if (!enrolled) {
        SetupScreen(core) { enrolled = true }
        return
    }
    var tab by remember { mutableIntStateOf(0) }
    val session by AdminSessionHolder.session.collectAsState()
    TabRow(selectedTabIndex = tab) {
        Tab(selected = tab == 0, onClick = { tab = 0 }, text = { Text(stringResource(R.string.tab_status)) })
        Tab(selected = tab == 1, onClick = { tab = 1 }, text = { Text(stringResource(R.string.tab_admin)) })
    }
    when (tab) {
        0 -> StatusScreen(core, resumes)
        else -> {
            val token = session?.let { AdminSessionHolder.token() }
            if (token == null) {
                Column(modifier = Modifier.padding(24.dp)) {
                    Text(stringResource(R.string.admin_locked), style = MaterialTheme.typography.titleMedium)
                    UnlockForm(core) { _, recovered ->
                        if (recovered) tab = 1
                    }
                }
            } else {
                AdminScreen(core, token) { AdminSessionHolder.clear() }
            }
        }
    }
}
