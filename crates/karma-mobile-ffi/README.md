# karma-mobile-ffi

UniFFI export layer of the Karma core for mobile platforms (Android now, iOS later).

Status: **planning** — not yet a workspace member. Add it to `[workspace] members` in the root
`Cargo.toml` once it builds (milestone M2 in `docs/android-technical-plan.md`).

Wraps:

- `karma-ai`      — `ViddexaRiskMapper`: classifier probabilities → `RiskObservation`
- `karma-policy`  — `DecisionEngine`, `RiskState`, `ContextPolicy`
- `karma-domain`  — `Decision`, `Action`, `ReasonCode`, `AuditEvent`
- platform-independent password (Argon2id) and store logic extracted from `karma-service-core`

Design rule: the FFI surface carries scores and metadata only. Image capture, preprocessing and
ONNX inference stay on the platform side.

Build (planned):

```bash
cargo ndk -t arm64-v8a -t x86_64 \
  -o ../../apps/karma-android/core/src/main/jniLibs \
  build --release -p karma-mobile-ffi
uniffi-bindgen generate --library target/.../libkarma_mobile_ffi.so \
  --language kotlin --out-dir ../../apps/karma-android/core/src/main/kotlin
```
