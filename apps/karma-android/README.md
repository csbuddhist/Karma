# Karma Android

Android client of Karma. Open this directory directly in Android Studio.

Status: **planning** — see `docs/android-technical-plan.md` (Chinese) for the full technical
plan, module layout, shared-core interface and milestones.

## Planned layout

```
app/   Application module (Kotlin + Jetpack Compose)
       ui/ monitor/ inference/ guard/ security/ service/
core/  Android library wrapping crates/karma-mobile-ffi via UniFFI (jniLibs built by cargo-ndk)
```

## Key decisions

- Screen capture: `AccessibilityService.takeScreenshot()` (minSdk 30), not MediaProjection.
- Inference: ONNX Runtime Android with the XNNPACK execution provider, reusing
  `assets/image/viddexa-nano` and its `manifest.json` for preprocessing.
- Policy, risk fusion, admin password and audit live in Rust (`crates/karma-mobile-ffi`);
  only scores and metadata cross the Kotlin/Rust boundary — never pixels.
- Tamper resistance: L1 (Device Admin + settings watchdog) and L2 (Device Owner).

## Toolchain (target)

JDK 17 · AGP 8.x · Kotlin 2.x · NDK r27 · Rust `aarch64-linux-android` · `cargo-ndk` · `uniffi-bindgen`
