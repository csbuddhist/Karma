import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

val repoRoot: File = rootDir.parentFile.parentFile
val modelDir: File = (findProperty("karma.modelDir") as String? ?: "target/model-assets/viddexa-nano")
    .let { File(it).takeIf(File::isAbsolute) ?: File(repoRoot, it) }
val generatedAssets = layout.buildDirectory.dir("generated/karma-assets")

// Release signing reads storeFile, storePassword, keyAlias and keyPassword from the properties
// file named by karma.signing (kept outside the repository). Without it the release APK is unsigned.
val releaseSigning: Properties? = (findProperty("karma.signing") as String?)
    ?.let(::File)
    ?.takeIf(File::isFile)
    ?.let { file -> Properties().apply { file.inputStream().use { load(it) } } }

android {
    namespace = "com.karma.android"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.karma.android"
        minSdk = 30
        targetSdk = 35
        versionCode = 5
        versionName = "0.1.4"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    signingConfigs {
        if (releaseSigning != null) {
            create("release") {
                storeFile = File(releaseSigning.getProperty("storeFile"))
                storePassword = releaseSigning.getProperty("storePassword")
                keyAlias = releaseSigning.getProperty("keyAlias")
                keyPassword = releaseSigning.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            signingConfig = signingConfigs.findByName("release")
            isMinifyEnabled = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    sourceSets.getByName("main") {
        assets.srcDir(generatedAssets)
    }

    androidResources {
        // Stored uncompressed so the model is read without inflating it first.
        noCompress += "onnx"
        // Lists English and Simplified Chinese in the system's per-app language setting.
        generateLocaleConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    packaging {
        jniLibs.useLegacyPackaging = false
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

/**
 * Copies the exported viddexa-nano model and its manifest into the APK assets. The APK
 * still builds without them; the app then reports the model as missing and enforces
 * context rules (applications, websites, titles, schedules) only.
 */
val copyModelAssets by tasks.registering(Copy::class) {
    group = "karma"
    description = "Copies model.onnx, manifest.json and LICENSE from karma.modelDir."
    from(modelDir) {
        include("model.onnx", "manifest.json", "LICENSE")
    }
    from(File(repoRoot, "assets/image/viddexa-nano")) {
        include("LICENSE")
    }
    into(generatedAssets.map { it.dir("models/viddexa-nano") })
    duplicatesStrategy = DuplicatesStrategy.EXCLUDE
    doLast {
        if (!File(modelDir, "model.onnx").isFile) {
            logger.warn("karma: no model at $modelDir; the APK will run context-only enforcement")
        }
    }
}

tasks.named("preBuild") { dependsOn(copyModelAssets) }

dependencies {
    implementation(project(":core"))
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.work.runtime.ktx)
    implementation(libs.onnxruntime.android)
    debugImplementation(libs.androidx.compose.ui.tooling)

    testImplementation(libs.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.ext.junit)
}
