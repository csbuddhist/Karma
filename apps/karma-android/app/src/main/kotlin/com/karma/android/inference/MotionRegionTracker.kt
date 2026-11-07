package com.karma.android.inference

/**
 * Finds the parts of the screen that keep changing, such as a video playing in a small
 * player inside a web page, so they can be classified at their own scale. A port of the
 * desktop `MotionRegionTracker` (`crates/karma-ai/src/motion_focus.rs`) with the same
 * constants, run on the prepared frame (longer edge at most 640 px).
 *
 * Only one 8×8-cell luma grid and a 16-sample change history per cell are kept; no pixels.
 */
class MotionRegionTracker {
    /** An `(x, y, width, height)` rectangle in pixels. */
    data class Region(val x: Int, val y: Int, val width: Int, val height: Int)

    private var width = 0
    private var height = 0
    private var columns = 0
    private var rows = 0
    private var lumas = IntArray(0)
    private var history = IntArray(0)

    /** Whether any cell changed in the recent samples: the screen is playing something. */
    var moving = false
        private set

    fun reset() {
        width = 0
        height = 0
        lumas.fill(0)
        history.fill(0)
        moving = false
    }

    /**
     * Records a prepared frame (`0xAARRGGBB`, `width * height`) and returns its densest moving
     * regions, largest first, in pixels of the prepared frame. Sizes are judged in pixels of
     * the `captureWidth × captureHeight` screenshot it was scaled down from. A resolution
     * change restarts the history, so the first frame of a new sequence has no regions.
     */
    fun observe(pixels: IntArray, width: Int, height: Int, captureWidth: Int, captureHeight: Int): List<Region> {
        require(width > 0 && height > 0 && pixels.size >= width * height)
        val columns = (width + CELL_EDGE - 1) / CELL_EDGE
        val rows = (height + CELL_EDGE - 1) / CELL_EDGE
        val current = cellLumas(pixels, width, height, columns, rows)
        if (width != this.width || height != this.height) {
            this.width = width
            this.height = height
            this.columns = columns
            this.rows = rows
            lumas = current
            history = IntArray(columns * rows)
            moving = false
            return emptyList()
        }
        var anyMoving = false
        for (index in history.indices) {
            val changed = kotlin.math.abs(lumas[index] - current[index]) >= CHANGE_THRESHOLD
            history[index] = ((history[index] shl 1) or (if (changed) 1 else 0)) and HISTORY_MASK
            if (Integer.bitCount(history[index]) >= MOTION_CHANGES) anyMoving = true
        }
        lumas.fill(0)
        lumas = current
        moving = anyMoving
        return regions(captureWidth, captureHeight)
    }

    private fun cellLumas(pixels: IntArray, width: Int, height: Int, columns: Int, rows: Int): IntArray {
        val sums = IntArray(columns * rows)
        for (y in 0 until height) {
            val rowOffset = (y / CELL_EDGE) * columns
            val pixelOffset = y * width
            for (x in 0 until width) {
                val pixel = pixels[pixelOffset + x]
                val red = (pixel ushr 16) and 0xFF
                val green = (pixel ushr 8) and 0xFF
                val blue = pixel and 0xFF
                sums[rowOffset + x / CELL_EDGE] += (77 * red + 150 * green + 29 * blue) ushr 8
            }
        }
        for (index in sums.indices) {
            val cellWidth = minOf(CELL_EDGE, width - (index % columns) * CELL_EDGE)
            val cellHeight = minOf(CELL_EDGE, height - (index / columns) * CELL_EDGE)
            sums[index] = sums[index] / (cellWidth * cellHeight)
        }
        return sums
    }

    private class CellBounds(var left: Int, var top: Int, var right: Int, var bottom: Int) {
        fun include(column: Int, row: Int) {
            left = minOf(left, column)
            top = minOf(top, row)
            right = maxOf(right, column)
            bottom = maxOf(bottom, row)
        }

        val columns get() = right - left + 1
        val rows get() = bottom - top + 1
        val cells get() = columns * rows
    }

    private fun regions(captureWidth: Int, captureHeight: Int): List<Region> {
        val movingCells = BooleanArray(history.size) { Integer.bitCount(history[it]) >= MOTION_CHANGES }
        // Growing every moving cell by one joins the still patches inside a video into one
        // region and leaves a one-cell margin around it.
        val grown = grow(movingCells)
        val visited = BooleanArray(grown.size)
        val candidates = mutableListOf<Pair<Int, Region>>()
        val pending = ArrayDeque<Int>()
        for (start in grown.indices) {
            if (!grown[start] || visited[start]) continue
            visited[start] = true
            pending.addLast(start)
            val crop = CellBounds(start % columns, start / columns, start % columns, start / columns)
            var motion: CellBounds? = null
            var count = 0
            while (pending.isNotEmpty()) {
                val index = pending.removeLast()
                val column = index % columns
                val row = index / columns
                crop.include(column, row)
                if (movingCells[index]) {
                    count++
                    (motion ?: CellBounds(column, row, column, row).also { motion = it }).include(column, row)
                }
                if (column > 0) visit(index - 1, grown, visited, pending)
                if (column + 1 < columns) visit(index + 1, grown, visited, pending)
                if (row > 0) visit(index - columns, grown, visited, pending)
                if (row + 1 < rows) visit(index + columns, grown, visited, pending)
            }
            val bounds = motion ?: continue
            focusRegion(crop, bounds, count, captureWidth, captureHeight)?.let { candidates += count to it }
        }
        return candidates.sortedByDescending { it.first }.take(MAXIMUM_FOCUS_REGIONS).map { it.second }
    }

    private fun visit(index: Int, grown: BooleanArray, visited: BooleanArray, pending: ArrayDeque<Int>) {
        if (grown[index] && !visited[index]) {
            visited[index] = true
            pending.addLast(index)
        }
    }

    private fun grow(cells: BooleanArray): BooleanArray {
        val grown = BooleanArray(cells.size)
        for (index in cells.indices) {
            if (!cells[index]) continue
            val column = index % columns
            val row = index / columns
            for (neighborRow in maxOf(row - 1, 0)..minOf(row + 1, rows - 1)) {
                for (neighborColumn in maxOf(column - 1, 0)..minOf(column + 1, columns - 1)) {
                    grown[neighborRow * columns + neighborColumn] = true
                }
            }
        }
        return grown
    }

    /** Judges the moving cells of one group and returns the area to crop around them. */
    private fun focusRegion(
        crop: CellBounds,
        motion: CellBounds,
        movingCells: Int,
        captureWidth: Int,
        captureHeight: Int,
    ): Region? {
        if (movingCells * 100 < motion.cells * MINIMUM_MOTION_FILL_PERCENT ||
            minOf(motion.columns, motion.rows) < MINIMUM_MOTION_CELLS
        ) {
            return null
        }
        val motionPixels = pixels(motion)
        val shortEdge = minOf(motionPixels.width, motionPixels.height)
        val captureShortEdge = minOf(
            motionPixels.width.toLong() * captureWidth / width,
            motionPixels.height.toLong() * captureHeight / height,
        )
        if (captureShortEdge < MINIMUM_MOTION_EDGE ||
            maxOf(motionPixels.width, motionPixels.height) > shortEdge * MAXIMUM_MOTION_ASPECT_RATIO ||
            motionPixels.width.toLong() * motionPixels.height * 100 >
            width.toLong() * height * MAXIMUM_MOTION_AREA_PERCENT
        ) {
            return null
        }
        return pixels(crop)
    }

    private fun pixels(bounds: CellBounds): Region {
        val x = bounds.left * CELL_EDGE
        val y = bounds.top * CELL_EDGE
        val right = minOf((bounds.right + 1) * CELL_EDGE, width)
        val bottom = minOf((bounds.bottom + 1) * CELL_EDGE, height)
        return Region(x, y, right - x, bottom - y)
    }

    companion object {
        /** Edge of a square motion cell, in prepared-frame pixels. */
        const val CELL_EDGE = 8
        /** Mean-luma difference at which a cell counts as changed between two samples. */
        const val CHANGE_THRESHOLD = 4
        /** A cell is moving when it changed in at least this many of its last 16 samples. */
        const val MOTION_CHANGES = 2
        private const val HISTORY_MASK = 0xFFFF
        /** Narrower or shorter motion, in capture pixels, is mostly avatars, emoji and icons. */
        const val MINIMUM_MOTION_EDGE = 64
        /** Motion must also span this many cells of the prepared frame to be told apart from noise. */
        const val MINIMUM_MOTION_CELLS = 2
        /** Larger motion is already legible in the whole-frame inference. */
        const val MAXIMUM_MOTION_AREA_PERCENT = 50
        /** Moving cells must fill this share of their bounds, which rejects cursor trails and sparse UI. */
        const val MINIMUM_MOTION_FILL_PERCENT = 50
        /** Longer, thinner motion is a ticker or a text line rather than video. */
        const val MAXIMUM_MOTION_ASPECT_RATIO = 4
        /** Bounds the extra inferences that one frame can cost (core `MAX_FOCUS_REGIONS`). */
        const val MAXIMUM_FOCUS_REGIONS = 2

        /**
         * The smallest region of a `toWidth × toHeight` frame that covers [region] of a
         * `fromWidth × fromHeight` frame showing the same picture (desktop `FrameRegion::scaled`).
         */
        fun scaled(region: Region, fromWidth: Int, fromHeight: Int, toWidth: Int, toHeight: Int): Region {
            val x = (region.x.toLong() * toWidth / fromWidth).toInt()
            val y = (region.y.toLong() * toHeight / fromHeight).toInt()
            val right = minOf(ceilDiv((region.x + region.width).toLong() * toWidth, fromWidth.toLong()), toWidth.toLong()).toInt()
            val bottom = minOf(ceilDiv((region.y + region.height).toLong() * toHeight, fromHeight.toLong()), toHeight.toLong()).toInt()
            return Region(x, y, right - x, bottom - y)
        }

        private fun ceilDiv(value: Long, divisor: Long): Long = (value + divisor - 1) / divisor
    }
}
