package com.karma.android.inference

import org.json.JSONObject

/**
 * The viddexa-nano manifest, checked by the same rules as the desktop
 * `ImageModelManifest::validate` in `crates/karma-ai`. Preprocessing parameters are read
 * from here only, never hard-coded, so desktop and phone scores cannot drift apart.
 */
data class ModelManifest(
    val version: String,
    val sha256: String,
    val fileName: String,
    val fileBytes: Long,
    val inputName: String,
    val outputName: String,
    val height: Int,
    val width: Int,
    val scale: Float,
    val mean: FloatArray,
    val std: FloatArray,
    /** Output index of each label name. */
    val labelIndex: Map<String, Int>,
) {
    class Invalid(message: String) : Exception(message)

    companion object {
        const val REPOSITORY = "https://huggingface.co/viddexa/nsfw-detection-2-nano"
        const val REVISION = "913bc502e69fa3edfe2cfce72c98cad4ddc6149b"
        val LABELS = listOf("normal", "hentai", "porn", "sexy", "drawing")
        private const val SCALE = 1.0f / 255.0f
        private val MEAN = floatArrayOf(0.485f, 0.456f, 0.406f)
        private val STD = floatArrayOf(0.47853944f, 0.4732864f, 0.47434163f)
        private val SHA256 = Regex("^[0-9a-f]{64}$")
        private val SAFE_FILE = Regex("^[A-Za-z0-9._-]{1,128}$")

        fun parse(json: String): ModelManifest {
            val root = JSONObject(json)
            val asset = root.getJSONObject("asset")
            require(asset.getString("kind") == "image_classifier", "asset kind")
            val sha256 = asset.getString("sha256")
            require(SHA256.matches(sha256), "asset sha256")
            require(asset.getString("license") == "Apache-2.0", "asset license")
            require(root.getString("source_repository") == REPOSITORY, "source repository")
            require(root.getString("source_revision") == REVISION, "source revision")
            val fileName = root.getString("file_name")
            require(SAFE_FILE.matches(fileName) && fileName != "." && fileName != "..", "file name")
            val fileBytes = root.getLong("file_bytes")
            require(fileBytes > 0, "file bytes")
            require(root.getInt("opset") == 18, "opset")
            require(root.getString("minimum_runtime_version") == "1.22", "runtime version")

            val input = root.getJSONObject("input")
            val shape = input.getJSONArray("shape").let { array -> IntArray(array.length()) { array.getInt(it) } }
            require(shape.contentEquals(intArrayOf(1, 3, 224, 224)), "input shape")
            require(input.getString("layout") == "nchw", "input layout")
            require(input.getString("color_order") == "rgb", "color order")
            val scale = input.getDouble("scale").toFloat()
            val mean = input.getJSONArray("mean").let { a -> FloatArray(a.length()) { a.getDouble(it).toFloat() } }
            val std = input.getJSONArray("std").let { a -> FloatArray(a.length()) { a.getDouble(it).toFloat() } }
            require(scale == SCALE && mean.contentEquals(MEAN) && std.contentEquals(STD), "normalization")
            val inputName = input.getString("name")
            val outputName = root.getString("output_name")
            require(inputName.isNotBlank() && outputName.isNotBlank(), "tensor names")

            val labels = root.getJSONArray("labels")
            require(labels.length() == LABELS.size, "label count")
            val labelIndex = HashMap<String, Int>()
            for (position in 0 until labels.length()) {
                val label = labels.getJSONObject(position)
                val index = label.getInt("index")
                val name = label.getString("name")
                require(index == position && name in LABELS && labelIndex.put(name, index) == null, "labels")
            }

            return ModelManifest(
                version = asset.getString("version"),
                sha256 = sha256,
                fileName = fileName,
                fileBytes = fileBytes,
                inputName = inputName,
                outputName = outputName,
                height = shape[2],
                width = shape[3],
                scale = scale,
                mean = mean,
                std = std,
                labelIndex = labelIndex,
            )
        }

        private fun require(condition: Boolean, field: String) {
            if (!condition) throw Invalid("model manifest field is invalid: $field")
        }
    }
}
