package com.karma.android.monitor

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class BlackedOutFrameTest {
    private val width = 108
    private val height = 240
    private val black = 0xFF000000.toInt()

    private fun frame(fill: Int) = IntArray(width * height) { fill }

    /** Paints rows [from, to) with [color], like the status bar or navigation bar. */
    private fun IntArray.rows(from: Int, to: Int, color: Int) = apply {
        for (y in from until to) fill(color, y * width, (y + 1) * width)
    }

    @Test
    fun secureWindowWithSystemBarsIsBlackedOut() {
        val pixels = frame(black).rows(0, 12, 0xFF1E1E1E.toInt()).rows(height - 10, height, 0xFFFFFFFF.toInt())
        assertTrue(BlackedOutFrame.matches(pixels, width, height))
    }

    @Test
    fun darkThemeContentIsNotBlackedOut() {
        assertFalse(BlackedOutFrame.matches(frame(0xFF202124.toInt()), width, height))
    }

    @Test
    fun letterboxedVideoIsNotBlackedOut() {
        val pixels = frame(black).rows(height / 4, height * 3 / 4, 0xFF3A6EA5.toInt())
        assertFalse(BlackedOutFrame.matches(pixels, width, height))
    }

    @Test
    fun secureWindowBehindAnOpenKeyboardIsNotMatched() {
        val pixels = frame(black).rows(height / 2, height, 0xFFECEFF1.toInt())
        assertFalse(BlackedOutFrame.matches(pixels, width, height))
    }

    @Test
    fun emptyOrShortBuffersAreRejected() {
        assertFalse(BlackedOutFrame.matches(IntArray(0), 0, 0))
        assertFalse(BlackedOutFrame.matches(IntArray(10) { black }, width, height))
    }
}
