package com.karma.android.inference

import ai.onnxruntime.OnnxTensor
import ai.onnxruntime.OrtEnvironment
import ai.onnxruntime.OrtSession
import ai.onnxruntime.TensorInfo
import android.content.Context
import java.io.FileNotFoundException
import java.nio.FloatBuffer
import java.security.MessageDigest
import kotlin.math.exp
import uniffi.karma_mobile_ffi.ClassifierProbabilities

/**
 * ONNX Runtime session for the verified viddexa-nano model, using the XNNPACK CPU
 * execution provider (NNAPI is deprecated and inconsistent across vendor SoCs).
 *
 * The model bytes are read from the APK, checked against the manifest's length and
 * SHA-256, and the session is built from those verified bytes. Graph input/output names
 * and shapes are checked before the first frame.
 */
class ImageClassifier private constructor(
    val manifest: ModelManifest,
    private val environment: OrtEnvironment,
    private val session: OrtSession,
) : AutoCloseable {
    private val preprocessor = Preprocessor(manifest)
    private val shape = longArrayOf(1, 3, manifest.height.toLong(), manifest.width.toLong())

    private val motion = MotionRegionTracker()
    private var crop = IntArray(0)

    /** The whole frame's scores and those of its moving regions, see [classifyWithFocus]. */
    class FrameScores(val whole: ClassifierProbabilities, val focus: List<ClassifierProbabilities>)

    /** Whether the last frame given to [classifyWithFocus] showed motion, such as a playing video. */
    val moving: Boolean get() = motion.moving

    /** Forgets the motion history, for example when another app comes to the front. */
    fun resetMotion() = motion.reset()

    /**
     * Scores the whole frame, then each moving region on its own (desktop
     * `MotionFocusClassifier`). The whole-frame inference shrinks a video that covers part
     * of the screen to a small patch of the model input among unrelated page content, which
     * dilutes its score; each crop is read from the full-resolution screenshot and given to
     * the model at its own scale. Not thread-safe; call from one thread.
     */
    fun classifyWithFocus(pixels: IntArray, width: Int, height: Int): FrameScores {
        var regions: List<MotionRegionTracker.Region> = emptyList()
        var preparedWidth = width
        var preparedHeight = height
        val whole = classify(pixels, width, height) { prepared, w, h ->
            preparedWidth = w
            preparedHeight = h
            regions = motion.observe(prepared, w, h, width, height)
        }
        val focus = regions.mapNotNull { region ->
            val area = MotionRegionTracker.scaled(region, preparedWidth, preparedHeight, width, height)
            if (area.width <= 0 || area.height <= 0) return@mapNotNull null
            val size = area.width * area.height
            if (crop.size < size) crop = IntArray(size)
            for (row in 0 until area.height) {
                System.arraycopy(pixels, (area.y + row) * width + area.x, crop, row * area.width, area.width)
            }
            try {
                classify(crop, area.width, area.height)
            } finally {
                crop.fill(0)
            }
        }
        return FrameScores(whole, focus)
    }

    /** Scores one frame of `0xAARRGGBB` pixels. Not thread-safe; call from one thread. */
    fun classify(
        pixels: IntArray,
        width: Int,
        height: Int,
        onPrepared: ((prepared: IntArray, width: Int, height: Int) -> Unit)? = null,
    ): ClassifierProbabilities {
        preprocessor.fill(pixels, width, height, onPrepared)
        val logits = OnnxTensor.createTensor(environment, FloatBuffer.wrap(preprocessor.tensor), shape).use { input ->
            session.run(mapOf(manifest.inputName to input)).use { result ->
                @Suppress("UNCHECKED_CAST")
                val output = result.get(manifest.outputName).get().value as Array<FloatArray>
                output[0].copyOf()
            }
        }
        preprocessor.tensor.fill(0f)
        val probabilities = softmax(logits)
        fun label(name: String) = probabilities[manifest.labelIndex.getValue(name)]
        return ClassifierProbabilities(
            normal = label("normal"),
            hentai = label("hentai"),
            porn = label("porn"),
            sexy = label("sexy"),
            drawing = label("drawing"),
        )
    }

    override fun close() {
        session.close()
    }

    sealed class LoadResult {
        data class Ready(val classifier: ImageClassifier) : LoadResult()
        /** No model was packaged; the app enforces context rules only. */
        data object Missing : LoadResult()
        data class Rejected(val reason: String) : LoadResult()
    }

    companion object {
        private const val ASSET_DIR = "models/viddexa-nano"

        fun load(context: Context): LoadResult {
            val assets = context.assets
            val manifest = try {
                ModelManifest.parse(assets.open("$ASSET_DIR/manifest.json").use { it.readBytes().decodeToString() })
            } catch (_: FileNotFoundException) {
                return LoadResult.Missing
            } catch (error: Exception) {
                return LoadResult.Rejected(error.message ?: "manifest")
            }
            val bytes = try {
                assets.open("$ASSET_DIR/${manifest.fileName}").use { it.readBytes() }
            } catch (_: FileNotFoundException) {
                return LoadResult.Missing
            }
            if (bytes.size.toLong() != manifest.fileBytes) return LoadResult.Rejected("model length")
            val digest = MessageDigest.getInstance("SHA-256").digest(bytes)
                .joinToString("") { "%02x".format(it) }
            if (digest != manifest.sha256) return LoadResult.Rejected("model sha256")

            val environment = OrtEnvironment.getEnvironment()
            val options = OrtSession.SessionOptions().apply {
                // XNNPACK owns the threads; keep ORT's own pool minimal as it recommends.
                setIntraOpNumThreads(1)
                addXnnpack(mapOf("intra_op_num_threads" to "2"))
                setOptimizationLevel(OrtSession.SessionOptions.OptLevel.ALL_OPT)
            }
            val session = try {
                environment.createSession(bytes, options)
            } catch (error: Exception) {
                return LoadResult.Rejected("session: ${error.message}")
            } finally {
                options.close()
            }
            val input = session.inputInfo[manifest.inputName]?.info as? TensorInfo
            val output = session.outputInfo[manifest.outputName]?.info as? TensorInfo
            val expectedInput = longArrayOf(1, 3, manifest.height.toLong(), manifest.width.toLong())
            if (session.inputInfo.size != 1 || input == null || !input.shape.contentEquals(expectedInput) ||
                output == null || output.shape.last() != ModelManifest.LABELS.size.toLong()
            ) {
                session.close()
                return LoadResult.Rejected("graph contract")
            }
            return LoadResult.Ready(ImageClassifier(manifest, environment, session))
        }

        fun softmax(logits: FloatArray): FloatArray {
            val max = logits.max()
            val exps = DoubleArray(logits.size) { exp((logits[it] - max).toDouble()) }
            val sum = exps.sum()
            return FloatArray(logits.size) { (exps[it] / sum).toFloat() }
        }
    }
}
