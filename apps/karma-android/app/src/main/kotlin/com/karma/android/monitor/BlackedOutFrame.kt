package com.karma.android.monitor

/**
 * Recognizes a display screenshot in which the system blacked out the foreground window.
 *
 * `takeScreenshot()` does not fail on windows with `FLAG_SECURE` (such as Chrome Incognito
 * tabs); it draws them pure black, leaving only the status and navigation bars. A frame
 * counts as blacked out when at least [MIN_BLACK_ROW_FRACTION] of the sampled rows are pure
 * black. Real content is rarely exactly `#000000` across almost the whole screen, and the
 * caller confirms the match before acting on it.
 */
object BlackedOutFrame {
    const val MIN_BLACK_ROW_FRACTION = 0.85
    private const val STRIDE = 8

    fun matches(pixels: IntArray, width: Int, height: Int): Boolean {
        if (width <= 0 || height <= 0 || pixels.size < width * height) return false
        var rows = 0
        var blackRows = 0
        for (y in 0 until height step STRIDE) {
            rows++
            val offset = y * width
            var black = true
            for (x in 0 until width step STRIDE) {
                if (pixels[offset + x] and 0x00FFFFFF != 0) {
                    black = false
                    break
                }
            }
            if (black) blackRows++
        }
        return blackRows >= rows * MIN_BLACK_ROW_FRACTION
    }
}
