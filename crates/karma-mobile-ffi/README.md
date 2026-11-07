# karma-mobile-ffi

UniFFI export layer of the Karma core for mobile platforms (Android now, iOS later). A workspace
member since milestone M2 of `docs/android-technical-plan.md`.

Wraps:

- `karma-ai`      — `ViddexaRiskMapper`: classifier probabilities → risk score and labels
- `karma-policy`  — `RiskState` (multi-frame fusion), `ApplicationPolicy`, `ContextPolicy`,
  `NetworkSchedulePolicy`, `context_enforcement`
- `karma-domain`  — `Action`, `ReasonCode`, `RiskObservation`

and adds what the phone needs on top: per-package cooldowns, built-in Android browser/video package
categories, Argon2id administrator password with one-time recovery codes and rate limiting
(desktop parameters), short-lived sessions, and a bounded audit trail — all persisted in one
AES-256-GCM sealed state file whose key the platform supplies.

Design rules: the FFI surface carries scores and metadata only. Image capture, preprocessing and
ONNX inference stay on the platform side. Every call takes its timestamp from the caller, so a
recorded score sequence yields the same decisions on desktop tests and on a phone
(`tests/core_lifecycle.rs`).

## Build

The Android Gradle project runs these steps itself (`core:cargoBuild`, `core:uniffiBindgen`):

```bash
cargo ndk -t arm64-v8a -t x86_64 --platform 30 \
  -o apps/karma-android/core/src/main/jniLibs \
  build --release --locked -p karma-mobile-ffi
cargo run --release -p karma-mobile-ffi --features bindgen --bin uniffi-bindgen -- \
  generate --library target/aarch64-linux-android/release/libkarma_mobile_ffi.so \
  --language kotlin --out-dir apps/karma-android/core/build/generated/uniffi/kotlin
```

Generate the bindings from the unstripped library in `target/`: cargo-ndk strips the
jniLibs copy, and the stripped file carries no UniFFI metadata.

Host tests: `cargo test -p karma-mobile-ffi`.
