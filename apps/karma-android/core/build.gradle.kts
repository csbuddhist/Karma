// Android library wrapping crates/karma-mobile-ffi.
//
// cargoBuild    cargo-ndk builds libkarma_mobile_ffi.so for arm64-v8a (release ABI) and
//               x86_64 (emulator only) into src/main/jniLibs.
// uniffiBindgen generates the Kotlin bindings from the arm64 library's UniFFI metadata.
//
// Set karma.buildRust=false to package prebuilt jniLibs and bindings instead.

plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
}

val repoRoot: File = rootDir.parentFile.parentFile
val jniLibsDir = layout.projectDirectory.dir("src/main/jniLibs")
val bindingsDir = layout.buildDirectory.dir("generated/uniffi/kotlin")
val buildRust = (findProperty("karma.buildRust") as String?)?.toBoolean() ?: true
val cargo = System.getenv("CARGO") ?: "cargo"

android {
    namespace = "com.karma.android.core"
    compileSdk = 35

    defaultConfig {
        minSdk = 30
        consumerProguardFiles("consumer-rules.pro")
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    sourceSets.getByName("main") {
        java.srcDir(bindingsDir)
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

dependencies {
    // UniFFI's Kotlin bindings load the library through JNA.
    api("net.java.dev.jna:jna:${libs.versions.jna.get()}@aar")
}

val cargoBuild by tasks.registering(Exec::class) {
    group = "rust"
    description = "Builds karma-mobile-ffi for Android with cargo-ndk."
    workingDir = repoRoot
    inputs.dir(File(repoRoot, "crates"))
    inputs.file(File(repoRoot, "Cargo.lock"))
    outputs.dir(jniLibsDir)
    commandLine(
        cargo, "ndk",
        "-t", "arm64-v8a",
        "-t", "x86_64",
        "-P", "30",
        "-o", jniLibsDir.asFile.absolutePath,
        "build", "--release", "--locked", "-p", "karma-mobile-ffi",
    )
}

val uniffiBindgen by tasks.registering(Exec::class) {
    group = "rust"
    description = "Generates the Kotlin bindings of karma-mobile-ffi."
    dependsOn(cargoBuild)
    workingDir = repoRoot
    val library = jniLibsDir.file("arm64-v8a/libkarma_mobile_ffi.so").asFile
    inputs.file(library)
    outputs.dir(bindingsDir)
    commandLine(
        cargo, "run", "--release", "--locked",
        "-p", "karma-mobile-ffi", "--features", "bindgen", "--bin", "uniffi-bindgen", "--",
        "generate", "--library", library.absolutePath,
        "--language", "kotlin", "--no-format",
        "--out-dir", bindingsDir.get().asFile.absolutePath,
    )
}

if (buildRust) {
    tasks.named("preBuild") { dependsOn(uniffiBindgen) }
}
