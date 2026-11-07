package com.karma.android.inference

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MotionRegionTrackerTest {
    /** A portrait phone screen prepared to 288×640 from a 1080×2400 screenshot. */
    private val width = 288
    private val height = 640
    private val captureWidth = 1080
    private val captureHeight = 2400

    private fun frame(videoTop: Int, videoHeight: Int, videoWidth: Int, shade: Int): IntArray {
        val pixels = IntArray(width * height) { 0xFFF0F0F0.toInt() }
        for (y in videoTop until videoTop + videoHeight) {
            for (x in 0 until videoWidth) {
                val value = (shade + x + y) and 0xFF
                pixels[y * width + x] = (0xFF shl 24) or (value shl 16) or (value shl 8) or value
            }
        }
        return pixels
    }

    @Test
    fun smallPlayerInAPageIsFound() {
        val tracker = MotionRegionTracker()
        var regions = emptyList<MotionRegionTracker.Region>()
        for (sample in 0 until 4) {
            regions = tracker.observe(frame(80, 160, width, sample * 40), width, height, captureWidth, captureHeight)
        }
        assertTrue(tracker.moving)
        assertEquals(1, regions.size)
        val region = regions.single()
        assertTrue(region.y <= 80 && region.y + region.height >= 240)
        assertTrue(region.width >= width - MotionRegionTracker.CELL_EDGE)
    }

    @Test
    fun stillScreenHasNoMotion() {
        val tracker = MotionRegionTracker()
        repeat(5) {
            assertTrue(tracker.observe(frame(80, 160, width, 0), width, height, captureWidth, captureHeight).isEmpty())
        }
        assertFalse(tracker.moving)
    }

    @Test
    fun fullScreenVideoMovesWithoutAFocusRegion() {
        val tracker = MotionRegionTracker()
        var regions = emptyList<MotionRegionTracker.Region>()
        for (sample in 0 until 4) {
            regions = tracker.observe(frame(0, height, width, sample * 40), width, height, captureWidth, captureHeight)
        }
        assertTrue(tracker.moving)
        assertTrue(regions.isEmpty())
    }

    @Test
    fun resetForgetsTheHistory() {
        val tracker = MotionRegionTracker()
        for (sample in 0 until 4) tracker.observe(frame(80, 160, width, sample * 40), width, height, captureWidth, captureHeight)
        tracker.reset()
        assertFalse(tracker.moving)
        assertTrue(tracker.observe(frame(80, 160, width, 200), width, height, captureWidth, captureHeight).isEmpty())
    }

    @Test
    fun regionsScaleToTheScreenshot() {
        val scaled = MotionRegionTracker.scaled(MotionRegionTracker.Region(0, 80, 288, 160), 288, 640, 1080, 2400)
        assertEquals(MotionRegionTracker.Region(0, 300, 1080, 600), scaled)
    }
}
