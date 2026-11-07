package com.karma.android.monitor

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.karma_mobile_ffi.PackageClass

class FramePacerTest {
    private fun captureAt(pacer: FramePacer, now: Long): Boolean {
        val capture = pacer.shouldCapture(now)
        if (capture) {
            pacer.onCaptureStarted(now)
            pacer.onCaptureFinished()
        }
        return capture
    }

    @Test
    fun exemptAndBlockedPackagesAreNeverCaptured() {
        for (packageClass in listOf(PackageClass.EXEMPT, PackageClass.ALLOWED, PackageClass.BLOCKED)) {
            val pacer = FramePacer()
            pacer.onForeground(packageClass, 0)
            assertFalse(pacer.shouldCapture(10_000))
        }
    }

    @Test
    fun contentEventsAreCapturedAtMostTwiceASecond() {
        val pacer = FramePacer()
        pacer.onForeground(PackageClass.MONITORED, 0)
        assertTrue(captureAt(pacer, 0))
        pacer.onContentEvent(100)
        assertFalse(captureAt(pacer, 300))
        assertTrue(captureAt(pacer, 500))
    }

    @Test
    fun stillScreenFallsBackToEveryTwoSecondsThenPauses() {
        val pacer = FramePacer()
        pacer.onForeground(PackageClass.BROWSER, 0)
        assertTrue(captureAt(pacer, 0))
        assertFalse(captureAt(pacer, 1_000))
        assertTrue(captureAt(pacer, 2_000))
        assertFalse(captureAt(pacer, 31_000))
        assertFalse(captureAt(pacer, 40_000))
        pacer.onContentEvent(41_000)
        assertTrue(captureAt(pacer, 41_000))
    }

    @Test
    fun videoIsSampledEverySecondWithoutEvents() {
        val pacer = FramePacer()
        pacer.onForeground(PackageClass.VIDEO, 0)
        assertTrue(captureAt(pacer, 0))
        assertFalse(captureAt(pacer, 900))
        assertTrue(captureAt(pacer, 1_000))
        assertTrue(captureAt(pacer, 60_000))
    }

    @Test
    fun noSecondCaptureWhileOneIsInFlight() {
        val pacer = FramePacer()
        pacer.onForeground(PackageClass.VIDEO, 0)
        assertTrue(pacer.shouldCapture(0))
        pacer.onCaptureStarted(0)
        assertFalse(pacer.shouldCapture(5_000))
        pacer.onCaptureFinished()
        assertTrue(pacer.shouldCapture(5_000))
    }

    @Test
    fun motionKeepsBrowsersSampledAtOneFrameASecond() {
        val pacer = FramePacer()
        pacer.onForeground(PackageClass.BROWSER, 0)
        assertTrue(captureAt(pacer, 0))
        var now = 1_000L
        while (now < 120_000L) {
            pacer.onMotion(true, now)
            assertTrue("capture at $now", captureAt(pacer, now))
            now += FramePacer.VIDEO_INTERVAL_MS
        }
        pacer.onMotion(false, now)
        assertFalse(captureAt(pacer, now + FramePacer.IDLE_PAUSE_MS + 5_000))
    }
}
