package com.karma.android.monitor

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import uniffi.karma_mobile_ffi.MobileReason

/** Live monitor state for the status screen. Holds no content, only states and counters. */
object MonitorStatus {
    enum class Model { LOADING, READY, MISSING, REJECTED }

    data class State(
        val serviceConnected: Boolean = false,
        val model: Model = Model.LOADING,
        val modelDetail: String? = null,
        val framesScored: Long = 0,
        val closures: Long = 0,
        val lastClosureReason: MobileReason? = null,
        val lastClosureAtMs: Long? = null,
    )

    private val state = MutableStateFlow(State())
    val flow: StateFlow<State> = state.asStateFlow()

    fun update(transform: (State) -> State) = state.update(transform)
}
