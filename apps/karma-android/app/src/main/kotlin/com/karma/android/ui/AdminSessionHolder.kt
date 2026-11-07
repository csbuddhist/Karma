package com.karma.android.ui

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.karma_mobile_ffi.AdminSession

/** The administrator session of this process, kept in memory only and cleared on lock. */
object AdminSessionHolder {
    private val state = MutableStateFlow<AdminSession?>(null)
    val session: StateFlow<AdminSession?> = state.asStateFlow()

    fun set(session: AdminSession) {
        state.value = session
    }

    /** The token while the session is still valid, else null (and the session is dropped). */
    fun token(): String? {
        val current = state.value ?: return null
        if (current.expiresAtMs <= System.currentTimeMillis()) {
            state.value = null
            return null
        }
        return current.token
    }

    fun clear() {
        state.value = null
    }
}
