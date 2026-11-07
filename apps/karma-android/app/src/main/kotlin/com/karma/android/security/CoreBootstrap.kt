package com.karma.android.security

import android.content.Context
import java.io.File
import uniffi.karma_mobile_ffi.KarmaCore

/**
 * Process-wide access to the Rust decision core. The accessibility service, the UI and
 * the workers all run in the app's single process and share this instance.
 */
object CoreBootstrap {
    @Volatile
    private var core: KarmaCore? = null

    fun get(context: Context): KarmaCore {
        core?.let { return it }
        return synchronized(this) {
            core ?: open(context.applicationContext).also { core = it }
        }
    }

    private fun open(context: Context): KarmaCore {
        val key = KeystoreKeys.stateKey(context)
        try {
            val directory = File(context.noBackupFilesDir, "core")
            return KarmaCore.open(directory.absolutePath, key)
        } finally {
            key.fill(0)
        }
    }
}
