package com.karma.android.inference

/**
 * Turns ARGB screen pixels into the classifier's NCHW float tensor with the same two
 * steps as the desktop agent (`crates/karma-ai`):
 *
 * 1. `FramePreparer`: scale so the longer edge is at most 640 px, preserving aspect
 *    ratio, with corner-aligned integer bilinear interpolation and rounding.
 * 2. `ImageTensorBuilder`: stretch to the model's 224×224 input with corner-aligned
 *    float bilinear interpolation, then `(value * scale - mean) / std` per RGB channel.
 *
 * Keeping the arithmetic identical keeps desktop and phone scores comparable for the
 * same pixels. Buffers are reused between frames and cleared after use.
 */
class Preprocessor(private val manifest: ModelManifest) {
    private var prepared = IntArray(0)
    val tensor = FloatArray(3 * manifest.height * manifest.width)

    /**
     * Fills [tensor] from [pixels] (`0xAARRGGBB`, row-major, `width * height`). [onPrepared]
     * sees the prepared frame (longer edge at most 640 px) before its buffer is cleared.
     */
    fun fill(
        pixels: IntArray,
        width: Int,
        height: Int,
        onPrepared: ((prepared: IntArray, width: Int, height: Int) -> Unit)? = null,
    ) {
        require(width > 0 && height > 0 && pixels.size >= width * height)
        val (preparedWidth, preparedHeight) = preparedSize(width, height)
        val source: IntArray
        if (preparedWidth == width && preparedHeight == height) {
            source = pixels
        } else {
            if (prepared.size < preparedWidth * preparedHeight) {
                prepared = IntArray(preparedWidth * preparedHeight)
            }
            scaleInteger(pixels, width, height, prepared, preparedWidth, preparedHeight)
            source = prepared
        }
        buildTensor(source, preparedWidth, preparedHeight)
        try {
            onPrepared?.invoke(source, preparedWidth, preparedHeight)
        } finally {
            prepared.fill(0)
        }
    }

    private fun buildTensor(source: IntArray, width: Int, height: Int) {
        val targetHeight = manifest.height
        val targetWidth = manifest.width
        val plane = targetWidth * targetHeight
        for (targetY in 0 until targetHeight) {
            val y = floatAxis(height, targetHeight, targetY)
            for (targetX in 0 until targetWidth) {
                val x = floatAxis(width, targetWidth, targetX)
                val destination = targetY * targetWidth + targetX
                for (channel in 0 until 3) {
                    // Channel 0 = red, 1 = green, 2 = blue.
                    val shift = 16 - 8 * channel
                    val topLeft = channelOf(source[y.low * width + x.low], shift)
                    val topRight = channelOf(source[y.low * width + x.high], shift)
                    val bottomLeft = channelOf(source[y.high * width + x.low], shift)
                    val bottomRight = channelOf(source[y.high * width + x.high], shift)
                    val top = topLeft + (topRight - topLeft) * x.weight
                    val bottom = bottomLeft + (bottomRight - bottomLeft) * x.weight
                    val value = top + (bottom - top) * y.weight
                    tensor[channel * plane + destination] =
                        (value * manifest.scale - manifest.mean[channel]) / manifest.std[channel]
                }
            }
        }
    }

    private class FloatAxis(val low: Int, val high: Int, val weight: Float)

    private fun floatAxis(source: Int, target: Int, index: Int): FloatAxis {
        if (source == 1 || target == 1) return FloatAxis(0, 0, 0f)
        val position = index.toFloat() * (source - 1).toFloat() / (target - 1).toFloat()
        val low = kotlin.math.floor(position).toInt()
        return FloatAxis(low, minOf(low + 1, source - 1), position - low.toFloat())
    }

    private fun channelOf(pixel: Int, shift: Int): Float = ((pixel ushr shift) and 0xFF).toFloat()

    companion object {
        const val MAXIMUM_EDGE = 640

        /** Desktop `FramePreparationConfig::target` with the default 640 px edge. */
        fun preparedSize(width: Int, height: Int): Pair<Int, Int> {
            if (maxOf(width, height) <= MAXIMUM_EDGE) return width to height
            return if (width >= height) {
                val scaled = (height.toLong() * MAXIMUM_EDGE + width / 2) / width
                MAXIMUM_EDGE to maxOf(scaled.toInt(), 1)
            } else {
                val scaled = (width.toLong() * MAXIMUM_EDGE + height / 2) / height
                maxOf(scaled.toInt(), 1) to MAXIMUM_EDGE
            }
        }

        /** Desktop `scale_bgra`: corner-aligned integer bilinear, rounded per step. */
        fun scaleInteger(
            input: IntArray,
            inputWidth: Int,
            inputHeight: Int,
            output: IntArray,
            outputWidth: Int,
            outputHeight: Int,
        ) {
            for (targetY in 0 until outputHeight) {
                val y = IntAxis.of(inputHeight, outputHeight, targetY)
                for (targetX in 0 until outputWidth) {
                    val x = IntAxis.of(inputWidth, outputWidth, targetX)
                    val topLeft = input[y.low * inputWidth + x.low]
                    val topRight = input[y.low * inputWidth + x.high]
                    val bottomLeft = input[y.high * inputWidth + x.low]
                    val bottomRight = input[y.high * inputWidth + x.high]
                    var result = 0
                    for (shift in intArrayOf(24, 16, 8, 0)) {
                        val top = lerp(
                            (topLeft ushr shift) and 0xFF,
                            (topRight ushr shift) and 0xFF,
                            x.weight,
                            x.denominator,
                        )
                        val bottom = lerp(
                            (bottomLeft ushr shift) and 0xFF,
                            (bottomRight ushr shift) and 0xFF,
                            x.weight,
                            x.denominator,
                        )
                        result = result or (lerp(top, bottom, y.weight, y.denominator) shl shift)
                    }
                    output[targetY * outputWidth + targetX] = result
                }
            }
        }

        private fun lerp(first: Int, second: Int, weight: Long, denominator: Long): Int {
            val inverse = denominator - weight
            return ((first * inverse + second * weight + denominator / 2) / denominator).toInt()
        }
    }

    private class IntAxis(val low: Int, val high: Int, val weight: Long, val denominator: Long) {
        companion object {
            fun of(source: Int, target: Int, index: Int): IntAxis {
                if (source == 1 || target == 1) return IntAxis(0, 0, 0, 1)
                val denominator = (target - 1).toLong()
                val numerator = index.toLong() * (source - 1).toLong()
                val low = numerator / denominator
                return IntAxis(
                    low.toInt(),
                    minOf(low + 1, (source - 1).toLong()).toInt(),
                    numerator % denominator,
                    denominator,
                )
            }
        }
    }
}
