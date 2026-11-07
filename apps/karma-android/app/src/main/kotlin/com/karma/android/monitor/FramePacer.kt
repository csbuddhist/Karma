package com.karma.android.monitor

import uniffi.karma_mobile_ffi.PackageClass

/**
 * When to take the next screenshot (technical plan §4.2). Pure logic with the clock
 * passed in, so it is unit-tested without a device.
 *
 * - Exempt, allowed and blocked packages are never captured.
 * - Video players are sampled at a fixed 1 fps.
 * - Other apps (browsers included) are captured on content events at most 2 fps,
 *   fall back to one frame every 2 s while the screen is still, and pause after 30 s
 *   without events until the next event.
 */
class FramePacer {
    private var packageClass: PackageClass = PackageClass.EXEMPT
    private var lastEventMs = Long.MIN_VALUE / 2
    private var lastCaptureMs = Long.MIN_VALUE / 2
    private var dirty = false
    private var inFlight = false

    fun onForeground(packageClass: PackageClass, nowMs: Long) {
        this.packageClass = packageClass
        dirty = true
        lastEventMs = nowMs
        lastCaptureMs = Long.MIN_VALUE / 2
    }

    fun onContentEvent(nowMs: Long) {
        dirty = true
        lastEventMs = nowMs
    }

    /** Whether a capture should start now. Call [onCaptureStarted] when one does. */
    fun shouldCapture(nowMs: Long): Boolean {
        if (inFlight) return false
        val sinceCapture = nowMs - lastCaptureMs
        return when (packageClass) {
            PackageClass.EXEMPT, PackageClass.ALLOWED, PackageClass.BLOCKED -> false
            PackageClass.VIDEO -> sinceCapture >= VIDEO_INTERVAL_MS
            PackageClass.BROWSER, PackageClass.MONITORED -> when {
                dirty && sinceCapture >= MIN_INTERVAL_MS -> true
                nowMs - lastEventMs < IDLE_PAUSE_MS && sinceCapture >= STILL_INTERVAL_MS -> true
                else -> false
            }
        }
    }

    fun onCaptureStarted(nowMs: Long) {
        inFlight = true
        dirty = false
        lastCaptureMs = nowMs
    }

    fun onCaptureFinished() {
        inFlight = false
    }

    companion object {
        const val MIN_INTERVAL_MS = 500L
        const val VIDEO_INTERVAL_MS = 1_000L
        const val STILL_INTERVAL_MS = 2_000L
        const val IDLE_PAUSE_MS = 30_000L
        /** How often the service asks [shouldCapture]. */
        const val TICK_MS = 250L
    }
}
