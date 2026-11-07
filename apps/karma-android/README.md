# Karma Android

Android client of Karma. Open this directory directly in Android Studio.

Status: **developer preview** — the code for milestones M0–M4 of `docs/android-technical-plan.md`
(Chinese) is in place. The first APK (`android-v0.1.0`) was verified on an Android 15 emulator; it
has not yet passed `docs/android-acceptance.md` on real devices.

## Layout

```
app/   Application module (Kotlin + Jetpack Compose)
       ui/         Setup, status checklist, administrator area, unlock prompt
       monitor/    KarmaAccessibilityService, FramePacer, context/ (address bar, title), enforce/ (overlay)
       inference/  ModelManifest, Preprocessor (bit-exact desktop scaling), ImageClassifier (ORT + XNNPACK)
       guard/      Device admin receiver, Device Owner policies, settings watchdog
       security/   Keystore-wrapped state key, core bootstrap
       service/    Foreground status service, boot receiver, WorkManager watchdog
core/  Android library wrapping crates/karma-mobile-ffi via UniFFI (jniLibs built by cargo-ndk)
```

## Build

Requirements: JDK 17, Android SDK 35, NDK r27 (`ANDROID_NDK_HOME`), Gradle 8.11.1 (or Android
Studio), Rust 1.85 with `aarch64-linux-android` and `x86_64-linux-android`, and `cargo-ndk`.

```bash
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk --version 3.5.4 --locked
gradle assembleDebug testDebugUnitTest
```

`core:preBuild` runs `cargoBuild` (cargo-ndk → `core/src/main/jniLibs`) and `uniffiBindgen`
(Kotlin bindings → `core/build/generated/uniffi`). Set `karma.buildRust=false` in
`gradle.properties` to package prebuilt libraries instead.

`app:preBuild` copies `model.onnx`, `manifest.json` and `LICENSE` from `karma.modelDir`
(default `target/model-assets/viddexa-nano`, see `docs/model-assets.md`). Without a model the APK
still builds and enforces application, website, title and schedule rules only.

The repository does not ship `gradle-wrapper.jar`; run `gradle wrapper` once if you want `./gradlew`.

## Release build

`assembleRelease` minifies with R8 and signs with the key described by the properties file named in
`karma.signing` (`storeFile`, `storePassword`, `keyAlias`, `keyPassword`); keep that file and the
keystore outside the repository. Without `karma.signing` the release APK is unsigned.

```bash
gradle -Pkarma.signing=$HOME/.config/karma/android-signing.properties assembleRelease lintRelease
```

Published APKs are signed by the certificate with SHA-256
`2b6e9836d496091e8607bbabf6b4f7dda856e656cddefa5f6d6adb1936129bd7`. Android only installs updates
signed by the same key, so a debug build must be uninstalled before installing a release build.

## Key decisions

- Screen capture: `AccessibilityService.takeScreenshot()` (minSdk 30), not MediaProjection.
- Inference: ONNX Runtime Android with the XNNPACK execution provider, reusing
  `assets/image/viddexa-nano` and its `manifest.json` for preprocessing.
- Policy, risk fusion, cooldowns, admin password and audit live in Rust (`crates/karma-mobile-ffi`);
  only scores and metadata cross the Kotlin/Rust boundary — never pixels.
- Tamper resistance: L1 (device admin + settings watchdog) and L2 (Device Owner).
