package com.karma.android.inference

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Golden values come from the desktop pipeline (`FramePreparer` + `ImageTensorBuilder` in
 * `crates/karma-ai`) run on the same synthetic image, so this test fails if the phone's
 * preprocessing drifts from the desktop's.
 */
class PreprocessorTest {
    private val manifest = ModelManifest(
        version = "viddexa-nano-1",
        sha256 = "0".repeat(64),
        fileName = "model.onnx",
        fileBytes = 1,
        inputName = "pixel_values",
        outputName = "logits",
        height = 224,
        width = 224,
        scale = 1.0f / 255.0f,
        mean = floatArrayOf(0.485f, 0.456f, 0.406f),
        std = floatArrayOf(0.47853944f, 0.4732864f, 0.47434163f),
        labelIndex = ModelManifest.LABELS.withIndex().associate { it.value to it.index },
    )

    private fun synthetic(width: Int, height: Int): IntArray = IntArray(width * height) { index ->
        val x = index % width
        val y = index / width
        val r = (x * 7 + y * 3) % 256
        val g = (x * y) % 256
        val b = (x + 2 * y) % 256
        (0xFF shl 24) or (r shl 16) or (g shl 8) or b
    }

    private fun check(width: Int, height: Int, sum: Double, expected: Map<Int, Float>) {
        val preprocessor = Preprocessor(manifest)
        preprocessor.fill(synthetic(width, height), width, height)
        val tensor = preprocessor.tensor
        assertEquals(sum, tensor.sumOf { it.toDouble() }, 0.01)
        for ((index, value) in expected) {
            assertEquals("element $index", value, tensor[index], 1e-5f)
        }
    }

    @Test
    fun phoneSizedFrameMatchesDesktopPipeline() = check(
        1080, 2400, 14973.4184,
        mapOf(
            0 to -1.013501f, 1 to -0.736933f, 777 to -0.447575f, 50_175 to 0.281289f,
            50_176 to -0.963476f, 100_000 to 0.308227f, 150_527 to 1.169588f,
        ),
    )

    @Test
    fun smallFrameSkipsThePreparationScaleAndMatchesDesktop() = check(
        300, 200, 14026.1081,
        mapOf(
            0 to -1.013501f, 1 to -0.936586f, 777 to 0.834645f, 50_175 to 0.051833f,
            50_176 to -0.963476f, 100_000 to 0.164615f, 150_527 to 0.673545f,
        ),
    )

    @Test
    fun preparedSizeKeepsAspectRatioLikeDesktop() {
        assertEquals(288 to 640, Preprocessor.preparedSize(1080, 2400))
        assertEquals(640 to 360, Preprocessor.preparedSize(1920, 1080))
        assertEquals(300 to 200, Preprocessor.preparedSize(300, 200))
    }
}
