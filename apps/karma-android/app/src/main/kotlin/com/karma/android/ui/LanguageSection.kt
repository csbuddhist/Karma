package com.karma.android.ui

import android.app.Activity
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import com.karma.android.AppLanguage
import com.karma.android.KarmaApp
import com.karma.android.R

/**
 * Display language picker. Anyone using the phone may change it: it only affects how Karma
 * reads, never what it blocks, so it needs no administrator password.
 */
@Composable
fun LanguageSection() {
    val context = LocalContext.current
    var selected by remember { mutableStateOf(AppLanguage.current(context)) }
    Column(modifier = Modifier.selectableGroup()) {
        AppLanguage.Choice.entries.forEach { choice ->
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier
                    .fillMaxWidth()
                    .selectable(selected = choice == selected, role = Role.RadioButton) {
                        if (choice == selected) return@selectable
                        selected = choice
                        if (AppLanguage.select(context, choice)) {
                            // Android 11–12: no per-app language, so refresh what is already shown.
                            KarmaApp.onLanguageChanged(context.applicationContext)
                            (context as? Activity)?.recreate()
                        }
                    }
                    .padding(vertical = 4.dp),
            ) {
                RadioButton(selected = choice == selected, onClick = null)
                Text(
                    stringResource(
                        when (choice) {
                            AppLanguage.Choice.SYSTEM -> R.string.language_system
                            AppLanguage.Choice.ENGLISH -> R.string.language_english
                            AppLanguage.Choice.CHINESE -> R.string.language_chinese
                        },
                    ),
                    style = MaterialTheme.typography.bodyLarge,
                    modifier = Modifier.padding(start = 12.dp),
                )
            }
        }
    }
    Text(stringResource(R.string.language_explanation), style = MaterialTheme.typography.bodySmall)
}
