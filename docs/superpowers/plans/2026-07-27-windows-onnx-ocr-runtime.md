# Windows ONNX OCR Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add privacy-preserving PP-OCRv5 detection and recognition to the existing Windows multi-monitor frame pipeline, with lightweight/accurate profiles and deterministic local benchmarking.

**Architecture:** `karma-ai` owns portable manifests, bounded image geometry, CTC decoding, the `OcrEngine` contract, and profile selection. `karma-onnx` verifies detector/recognizer/dictionary bytes once, creates CPU-only ONNX Runtime sessions from verified memory, and converts short-lived OCR text directly into `OcrMatchSummary`. The Windows Agent gives every monitor an independent OCR engine and consumes the existing `FrameWork.run_ocr` schedule.

**Tech Stack:** Rust 1.85, edition 2024, `ort = 2.0.0-rc.10`, ONNX Runtime 1.22 CPU, `ndarray = 0.16`, `image = 0.25`, `imageproc = 0.25`, `zeroize = 1.8`, PP-OCRv5, Python 3.11 export tooling.

## Global Constraints

- Implement from `docs/superpowers/specs/2026-07-24-windows-onnx-ocr-design.md`.
- Lightweight is `PP-OCRv5_mobile_det` plus `PP-OCRv5_mobile_rec`; accurate is the server pair.
- Runtime never downloads assets; this plan accepts only already-verified local bundle directories.
- Each monitor owns separate mutable detector and recognizer sessions; verified immutable bytes may be shared.
- OCR runs only when the existing scheduler sets `run_ocr=true`, at most once per changed display per second.
- No public API, log, error, serialization, health state, fixture name, or snapshot may contain OCR text.
- Raw text, maps, crops, and tensors use zeroizing ownership and are dropped before `classify` returns.
- Limits are 64 boxes/frame, crop height 48, crop width 320, batch 8, 128 Unicode scalars/line, and 4,096 scalars/frame.
- Default thresholds are DB pixel `0.3`, box `0.6`, unclip `1.5`, and recognition confidence `0.5`.
- Invalid model contracts and reference mismatches fail before the first captured frame.
- Do not claim production accuracy until the Windows acceptance matrix is executed.

## File Map

- `crates/karma-ai/src/ocr_manifest.rs`: OCR bundle schema and semantic validation.
- `crates/karma-ai/src/ocr_text.rs`: redacted, zeroizing decoded text batch.
- `crates/karma-ai/src/ocr_geometry.rs`: quadrilaterals, ordering, limits, and crop contracts.
- `crates/karma-ai/src/ocr_tensor.rs`: detector/recognizer preprocessing.
- `crates/karma-ai/src/ctc.rs`: bounded CTC decoding.
- `crates/karma-ai/src/ocr_engine.rs`: engine trait and profile/benchmark policy.
- `crates/karma-onnx/src/ocr_model.rs`: bounded bundle verification.
- `crates/karma-onnx/src/db_postprocess.rs`: DB probability-map postprocessing.
- `crates/karma-onnx/src/ocr.rs`: ONNX sessions and end-to-end OCR classification.
- `crates/karma-onnx/src/ocr_health.rs`: privacy-safe counters.
- `apps/karma-agent-windows/src/inference_consumer.rs`: independent image/OCR scheduled consumer.
- `apps/karma-agent-windows/src/ocr_profile.rs`: local profile resolution and benchmark cache.
- `tools/ocr-export/`: pinned PaddleOCR download, export, validation, and packaging.
- `assets/ocr/pp-ocrv5-mobile/`: lightweight manifest, license, dictionary, and packaging metadata.

---

### Task 1: Define and validate the portable OCR contract

**Files:**
- Modify: `crates/karma-ai/Cargo.toml`
- Modify: `crates/karma-ai/src/lib.rs`
- Modify: `crates/karma-ai/src/manifest.rs`
- Create: `crates/karma-ai/src/ocr_manifest.rs`
- Create: `crates/karma-ai/src/ocr_engine.rs`
- Create: `crates/karma-ai/src/ocr_text.rs`

**Interfaces:**
- Produces `OcrModelProfile::{Lightweight, Accurate}`, `OcrBundleManifest`, `OcrTensorContract`, `OcrResourceLimits`, `OcrEngine`, and `OcrTextBatch`.
- `OcrEngine::classify(&mut self, &PreparedFrame, &WordPack) -> Result<OcrMatchSummary, Error>`.

- [ ] **Step 1: Write failing manifest and privacy tests**

Test exact file names, safe single-component paths, Apache-2.0 license, pinned 40-hex source revision,
opset 18, runtime 1.22, dynamic detector dimensions divisible by 32, recognizer height 48, all finite
normalization values, valid SHA-256/length pairs, unique dictionary entries, supported languages,
threshold ranges, and the exact resource ceilings.

```rust
#[test]
fn ocr_text_debug_and_json_do_not_expose_content() {
    // Constructed only by the crate-private zeroizing OCR decoder boundary.
    let batch = decode_sensitive_fixture();
    assert_eq!(format!("{batch:?}"), "OcrTextBatch { lines: 1, characters: 24 }");
    fn assert_not_serialize<T>() {}
    assert_not_serialize::<OcrTextBatch>();
}
```

Use a compile-fail doctest to prove `serde_json::to_string(&batch)` does not compile.

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-ai ocr`
Expected: compilation fails because the OCR modules and exports do not exist.

- [ ] **Step 3: Implement strict types and validation**

Use `#[serde(deny_unknown_fields)]` on every manifest struct. Extend `AssetKind` without weakening
existing image validation. `OcrTextBatch` owns `Vec<Zeroizing<String>>`; expose only crate-visible
`line_refs()` and public count methods. Its `Debug` prints counts only.

```rust
pub trait OcrEngine {
    type Error;
    fn classify(
        &mut self,
        frame: &PreparedFrame,
        word_pack: &WordPack,
    ) -> Result<OcrMatchSummary, Self::Error>;
}
```

- [ ] **Step 4: Verify GREEN**

Run: `cargo fmt --all && cargo test -p karma-ai ocr && cargo clippy -p karma-ai --all-targets -- -D warnings`
Expected: all new and existing `karma-ai` tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/karma-ai
git commit -m "feat: define portable OCR contracts"
```

---

### Task 2: Build detector inputs and bounded text geometry

**Files:**
- Create: `crates/karma-ai/src/ocr_geometry.rs`
- Create: `crates/karma-ai/src/ocr_tensor.rs`
- Modify: `crates/karma-ai/src/lib.rs`

**Interfaces:**
- Produces `DetectorTensorBuilder::build`, `DetectionTransform`, `TextQuadrilateral`,
  `sort_and_limit_boxes`, `RecognizerTensorBuilder::build_batch`, and `OcrTensorError`.
- Consumes `PreparedFrame` and manifest contracts.

- [ ] **Step 1: Write detector preprocessing tests**

Cover BGRA→RGB, aspect-preserving resize, padding to 32, NCHW order, exact normalization, inverse
coordinate mapping, checked allocation, zero dimensions, NaN parameters, and redacted `Debug`.
For a 640×360 frame assert the detector tensor is `[1, 3, 384, 640]`.

- [ ] **Step 2: Run detector RED**

Run: `cargo test -p karma-ai ocr_tensor::tests::detector`
Expected: compilation fails because `DetectorTensorBuilder` is missing.

- [ ] **Step 3: Implement detector tensor ownership**

Store tensor pixels in `Zeroizing<Vec<f32>>`. Store only scale/padding metadata in
`DetectionTransform`. Use checked multiplication before every allocation and reject any generated
edge above 640 or not divisible by 32.

- [ ] **Step 4: Write geometry RED tests**

Test non-finite coordinates, boundary clamping, minimum short edge 6, minimum area 48, stable
top-to-bottom then left-to-right ordering using half average-height row tolerance, and truncation to
64 boxes.

- [ ] **Step 5: Implement geometry and crop preprocessing**

Normalize point order clockwise from top-left. Implement inverse bilinear perspective sampling into
RGB height 48, width `min(ceil(48 * aspect), 320)`, then pad to the batch maximum width. Reject
degenerate transforms. Batches contain at most eight crops and use zeroizing tensors.

- [ ] **Step 6: Verify and commit**

Run: `cargo fmt --all && cargo test -p karma-ai ocr_tensor && cargo test -p karma-ai ocr_geometry && cargo clippy -p karma-ai --all-targets -- -D warnings`
Expected: preprocessing, geometry, limits, and redaction tests pass.

```bash
git add crates/karma-ai
git commit -m "feat: prepare bounded OCR tensors"
```

---

### Task 3: Implement bounded CTC decoding

**Files:**
- Create: `crates/karma-ai/src/ctc.rs`
- Modify: `crates/karma-ai/src/lib.rs`

**Interfaces:**
- Produces `CtcDictionary::parse`, `CtcDecoder::decode_batch`, `DecodedLine`, and `CtcError`.
- Consumes recognizer logits shaped `[batch, time, classes]`.

- [ ] **Step 1: Write failing decoder tests**

Cover blank removal, adjacent duplicate collapse, the same token separated by blank, simplified
Chinese, traditional Chinese, ASCII, invalid/duplicate dictionary entries, non-finite logits,
class-count mismatch, confidence below `0.5`, 128-character truncation, and 4,096-frame-character
termination.

```rust
let decoded = decoder.decode_line(&logits, &[1, 4, 4]).unwrap();
assert_eq!(decoded.character_count(), 2);
assert!(decoded.confidence() >= 0.5);
assert!(!format!("{decoded:?}").contains("敏感"));
```

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-ai ctc`
Expected: compilation fails because decoder types do not exist.

- [ ] **Step 3: Implement numerically stable decoding**

Compute per-timestep argmax and softmax confidence by subtracting the maximum logit. Reject
non-finite values. Collapse before dictionary lookup, enforce Unicode-scalar rather than byte
limits, and store decoded strings in `Zeroizing<String>`. Do not expose token IDs or probabilities
outside this module.

- [ ] **Step 4: Verify and commit**

Run: `cargo fmt --all && cargo test -p karma-ai ctc && cargo clippy -p karma-ai --all-targets -- -D warnings`
Expected: all decoder boundary and privacy tests pass.

```bash
git add crates/karma-ai
git commit -m "feat: decode bounded OCR text"
```

---

### Task 4: Verify complete OCR bundles from immutable bytes

**Files:**
- Modify: `crates/karma-onnx/Cargo.toml`
- Modify: `crates/karma-onnx/src/lib.rs`
- Modify: `crates/karma-onnx/src/model.rs`
- Create: `crates/karma-onnx/src/ocr_model.rs`

**Interfaces:**
- Produces `VerifiedOcrBundle::load`, `manifest`, `profile`, `create_engine`.
- Extends `InferenceErrorKind` with stable OCR contract/hash/dictionary/reference variants.

- [ ] **Step 1: Write failing bounded-load tests**

Create temporary detector, recognizer, dictionary, license, and manifest files. Test success,
missing file, wrong length/hash, detector or recognizer over 256 MiB, dictionary over 4 MiB,
manifest over 1 MiB, unsafe names, replacement of files after load, and errors that contain no
absolute path.

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-onnx ocr_model`
Expected: compilation fails because `VerifiedOcrBundle` does not exist.

- [ ] **Step 3: Implement one-pass verification**

Open each asset once, compare metadata length, read within the declared maximum, hash the bytes,
then retain `Arc<[u8]>`. Parse the already-verified dictionary into `Arc<CtcDictionary>`. Never
reopen an asset path when creating sessions.

- [ ] **Step 4: Verify and commit**

Run: `cargo fmt --all && cargo test -p karma-onnx ocr_model && cargo clippy -p karma-onnx --all-targets -- -D warnings`
Expected: all bundle verification and path-replacement tests pass.

```bash
git add crates/karma-onnx
git commit -m "feat: verify OCR model bundles"
```

---

### Task 5: Postprocess DB maps and run both ONNX sessions

**Files:**
- Modify: `crates/karma-onnx/Cargo.toml`
- Create: `crates/karma-onnx/src/db_postprocess.rs`
- Create: `crates/karma-onnx/src/ocr.rs`
- Create: `crates/karma-onnx/src/ocr_health.rs`
- Create: `crates/karma-onnx/tests/fixtures/ocr_detector.onnx`
- Create: `crates/karma-onnx/tests/fixtures/ocr_recognizer.onnx`
- Modify: `crates/karma-onnx/src/lib.rs`

**Interfaces:**
- Produces `DbPostProcessor::extract`, `OnnxOcrEngine`, `OcrInferenceHealth`.
- Implements `karma_ai::OcrEngine` for `OnnxOcrEngine`.

- [ ] **Step 1: Write DB postprocessing RED tests**

Use synthetic maps to test thresholding, connected contours, mean box score, 1.5 unclip,
quadrilateral clipping, small-box rejection, NaN/∞ rejection, stable ordering, and 64-box limit.

- [ ] **Step 2: Implement DB postprocessing**

Use `imageproc::contours` for connected boundaries, calculate a minimum-area rotated rectangle from
convex-hull edge angles, then expand from the centroid by `area * 1.5 / perimeter`. Keep the
algorithm deterministic and return portable `TextQuadrilateral` values.

- [ ] **Step 3: Generate tiny non-sensitive ONNX fixtures**

Add `tools/ocr-export/make_test_fixtures.py` that emits:

- detector: dynamic `[1,3,H,W]` input and `[1,1,H,W]` probability map;
- recognizer: dynamic `[N,3,48,W]` input and fixed-class CTC logits.

Run: `python3 tools/ocr-export/make_test_fixtures.py`
Expected: both checked-in fixtures are below 50 KiB and contain no production model weights.

- [ ] **Step 4: Write session contract and end-to-end RED tests**

Test exact input/output names, `f32`, dynamic dimensions, recognizer class count equal to dictionary
plus blank, reference detector map, reference CTC output, one successful `WordPack` summary, and
health output with no text.

- [ ] **Step 5: Implement CPU-only detector/recognizer execution**

Create sessions with Level3 graph optimization and one intra-op thread. Validate graph contracts
before inference. Process crops in batches of eight, skip only malformed boxes, stop at character
budget, call `word_pack.classify(batch.line_refs())` internally, and drop the batch before return.
Record stable error kinds and counters without runtime error strings.

- [ ] **Step 6: Verify and commit**

Run: `cargo fmt --all && cargo test -p karma-onnx && cargo clippy -p karma-onnx --all-targets -- -D warnings`
Expected: bundle, DB, ONNX, privacy, and existing image-classifier tests pass.

```bash
git add crates/karma-onnx tools/ocr-export/make_test_fixtures.py
git commit -m "feat: run PP-OCR pipelines with ONNX"
```

---

### Task 6: Pin and export official PP-OCRv5 bundles

**Files:**
- Create: `tools/ocr-export/requirements.lock`
- Create: `tools/ocr-export/models.toml`
- Create: `tools/ocr-export/export.py`
- Create: `tools/ocr-export/verify.py`
- Create: `tools/ocr-export/README.md`
- Create: `assets/ocr/pp-ocrv5-mobile/manifest.example.json`
- Create: `assets/ocr/pp-ocrv5-mobile/LICENSE`
- Create: `assets/ocr/pp-ocrv5-mobile/NOTICE.md`
- Modify: `.gitignore`

**Interfaces:**
- Produces a complete local directory accepted by `VerifiedOcrBundle::load`.
- Does not commit production `.onnx`, upstream archives, or generated reference binaries.

- [ ] **Step 1: Add an export-tool contract test**

Run: `python3 -m unittest discover -s tools/ocr-export/tests -v`
Expected RED: tests fail until URL/revision/hash pinning, safe extraction, subprocess checking, and
manifest hashing are implemented.

- [ ] **Step 2: Implement reproducible export**

Pin exact package versions and hashes in `requirements.lock`. `models.toml` must contain immutable
PaddleOCR commit, official URLs, and upstream SHA-256 for all four models. Require an explicit
`--output` directory. Download through standard proxy environment variables, verify upstream hash,
invoke Paddle2ONNX with dynamic shapes/opset 18, run ONNX checker, and generate non-sensitive
Chinese/traditional-Chinese/English reference images and output hashes.

- [ ] **Step 3: Export and verify lightweight locally**

Run:

```bash
python3 -m venv .venv-ocr-export
.venv-ocr-export/bin/pip install --require-hashes -r tools/ocr-export/requirements.lock
.venv-ocr-export/bin/python tools/ocr-export/export.py --profile lightweight --output .local-models/pp-ocrv5-mobile
cargo run -p karma-onnx --example verify_ocr_bundle -- .local-models/pp-ocrv5-mobile/manifest.json
```

Expected: ONNX checker, Paddle-versus-ONNX reference comparison, and Rust reference verification all
pass. If downloads require the user's proxy, run with
`HTTPS_PROXY=http://127.0.0.1:7897 HTTP_PROXY=http://127.0.0.1:7897`.

- [ ] **Step 4: Verify repository hygiene and commit**

Run: `git status --short && git check-ignore .local-models/pp-ocrv5-mobile/detector.onnx`
Expected: generated weights are ignored and only tooling/metadata are staged.

```bash
git add .gitignore tools/ocr-export assets/ocr/pp-ocrv5-mobile
git commit -m "build: pin PP-OCRv5 export pipeline"
```

---

### Task 7: Select profiles with a deterministic benchmark

**Files:**
- Modify: `crates/karma-ai/src/ocr_engine.rs`
- Create: `apps/karma-agent-windows/src/ocr_profile.rs`
- Create: `apps/karma-agent-windows/tests/fixtures/ocr-benchmark-spec.json`

**Interfaces:**
- Produces `OcrProfilePreference::{Auto, Lightweight, Accurate}`, `BenchmarkKey`,
  `BenchmarkResult`, `ProfileSelector::select`.
- Consumes candidate engine factories; never serializes images or text.

- [ ] **Step 1: Write failing policy tests**

Cover lightweight always selected, accurate missing falls back to lightweight with
`download_required`, explicit accurate ignores only performance failure, auto selects accurate only
when 10/10 succeed, no resource limit occurs, reference summary matches, and sorted P95 is at most
800 ms. Test cache invalidation on bundle version, CPU architecture/core count, or display count.

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-agent-windows ocr_profile`
Expected: compilation fails because profile selection is missing.

- [ ] **Step 3: Implement benchmark and cache contract**

Run three unmeasured warmups and ten measured iterations over a programmatically generated 640×360
non-sensitive UI frame. Compute nearest-rank P95 from sorted durations. Persist only the approved
`BenchmarkKey` and latency/result fields using atomic replacement; never persist the frame or
summary categories.

- [ ] **Step 4: Verify and commit**

Run: `cargo fmt --all && cargo test -p karma-agent-windows ocr_profile && cargo clippy -p karma-agent-windows --all-targets -- -D warnings`
Expected: all selection and cache tests pass.

```bash
git add crates/karma-ai apps/karma-agent-windows
git commit -m "feat: benchmark and select OCR profiles"
```

---

### Task 8: Integrate OCR into every Windows frame worker

**Files:**
- Move: `apps/karma-agent-windows/src/image_consumer.rs` to `apps/karma-agent-windows/src/inference_consumer.rs`
- Modify: `apps/karma-agent-windows/src/main.rs`
- Modify: `apps/karma-agent-windows/Cargo.toml`
- Create: `docs/acceptance/windows-ocr-runtime.md`

**Interfaces:**
- Produces `ScheduledInferenceConsumer<I, O, S>` and independent image/OCR health handles.
- `S: OcrSummarySink` receives only `OcrMatchSummary`.

- [ ] **Step 1: Write failing consumer tests**

Test all four `run_image`/`run_ocr` combinations, either engine failing without suppressing the
other, OCR summary sink invocation, three consecutive OCR failures marking only OCR unavailable,
success recovery, and separate state per consumer/monitor.

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-agent-windows inference_consumer`
Expected: compilation fails because the combined consumer is missing.

- [ ] **Step 3: Implement combined consumption**

Borrow one `PreparedFrame` for image then OCR. The default sink counts summaries but stores neither
categories nor text. Create one OCR engine per monitor from the selected verified bundle. Keep
startup alive in degraded mode when OCR fails, provided image inference/frame capture can run.

- [ ] **Step 4: Wire configuration**

Require `KARMA_OCR_LIGHTWEIGHT_MANIFEST`; optionally read `KARMA_OCR_ACCURATE_MANIFEST` and
`KARMA_OCR_PROFILE=auto|lightweight|accurate`. Invalid explicit values produce a stable startup
error. This is a temporary non-UI boundary and must be documented as such.

- [ ] **Step 5: Run full verification**

Run:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo check -p karma-agent-windows --target x86_64-pc-windows-msvc
rg -n 'println!|eprintln!|tracing|log::' crates/karma-ai/src/ocr* crates/karma-onnx/src/ocr* apps/karma-agent-windows/src
```

Expected: tests and Windows cross-check pass; log review shows only stable counters/error kinds and
no OCR buffers, decoded lines, URLs, or local paths.

- [ ] **Step 6: Record Windows acceptance**

Execute the matrix in `docs/acceptance/windows-ocr-runtime.md` on Windows 10 22H2 and Windows 11:
one/two/three displays, simplified/traditional/English/mixed text, video subtitles, browser small
fonts, negative medical/education/news/code/game samples, and lightweight/accurate/auto profiles.
Record P50/P95, CPU, and working set without screenshots or recognized text.

- [ ] **Step 7: Commit**

```bash
git add apps/karma-agent-windows docs/acceptance/windows-ocr-runtime.md
git commit -m "feat: schedule OCR on Windows displays"
```

---

## Final Verification

- [ ] Run every command in Task 8 Step 5 from a clean checkout.
- [ ] Run `rg -n 'TODO|FIXME|placeholder|unimplemented!|todo!' crates/karma-ai crates/karma-onnx apps/karma-agent-windows tools/ocr-export assets/ocr`; expected: no implementation placeholders.
- [ ] Confirm every new public type appears in `lib.rs`, every manifest type uses consistent serde names, and error variants match the design specification.
- [ ] Confirm production model weights, generated crops, reference binaries, `.part` files, and Python environments are ignored.
- [ ] Confirm the runtime succeeds with the bundled lightweight bundle while the accurate bundle is absent.
- [ ] Request code review before merging; address findings with the `receiving-code-review` skill.
