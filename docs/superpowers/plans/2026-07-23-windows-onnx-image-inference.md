# Windows ONNX Image Inference Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Load a pinned, locally exported `viddexa/nsfw-detection-2-nano` ONNX model and classify scheduled Windows monitor frames with ONNX Runtime CPU.

**Architecture:** `karma-ai` owns the portable model manifest, BGRA-to-NCHW preprocessing, classifier contract, and label-to-risk mapping. A new `karma-onnx` crate verifies the model artifact and owns mutable ONNX Runtime sessions. The Windows Agent constructs one session-backed consumer per monitor; model assets remain external to Git and are produced by a pinned Python export tool.

**Tech Stack:** Rust 1.85, edition 2024, `ort = 2.0.0-rc.10` with ONNX Runtime 1.22, `sha2 = 0.10`, `serde_json`, Python 3.11, PyTorch, Transformers, ONNX.

## Global Constraints

- Model repository is `viddexa/nsfw-detection-2-nano` at revision `913bc502e69fa3edfe2cfce72c98cad4ddc6149b`.
- Runtime inference uses CPU Execution Provider only.
- Input is static `f32` NCHW `[1, 3, 224, 224]`, RGB, with manifest-defined scale, mean, and standard deviation.
- Output labels are exactly `normal`, `hentai`, `porn`, `sexy`, and `drawing`, matched by name rather than assumed index.
- Model and input tensors are never serialized, logged, or persisted.
- Runtime does not download models; missing or invalid local assets fail closed as AI unavailable.
- The existing per-monitor scheduler remains the sole 2 FPS image-inference limit.
- OCR, application termination, GPU providers, signed updates, and production accuracy claims are outside this plan.

## File Map

- `crates/karma-ai/src/image_manifest.rs`: portable model contract and semantic validation.
- `crates/karma-ai/src/image_tensor.rs`: BGRA8 resizing and normalized NCHW tensor ownership.
- `crates/karma-ai/src/image_classifier.rs`: classifier trait, named output, and viddexa risk mapping.
- `crates/karma-onnx/src/model.rs`: manifest parsing, bounded file hashing, and verified model handle.
- `crates/karma-onnx/src/classifier.rs`: ONNX session creation, graph contract validation, and inference.
- `crates/karma-onnx/src/health.rs`: privacy-safe inference counters and latency totals.
- `apps/karma-agent-windows/src/image_consumer.rs`: scheduled-frame consumer backed by `karma-onnx`.
- `tools/model-export/export_viddexa.py`: pinned safetensors-only ONNX export.
- `tools/model-export/requirements.txt`: exact Python export dependencies.
- `assets/image/viddexa-nano/manifest.example.json`: reviewed manifest shape without the binary or final digest.

---

### Task 1: Validate the image-model manifest

**Files:**
- Modify: `crates/karma-ai/src/lib.rs`
- Create: `crates/karma-ai/src/image_manifest.rs`
- Modify: `crates/karma-ai/Cargo.toml`

**Interfaces:**
- Consumes: `AssetKind::ImageClassifier`.
- Produces: `ImageModelManifest`, `TensorLayout`, `ColorOrder`, `ImageManifestError`, `ImageModelManifest::validate()`.

- [ ] **Step 1: Write failing manifest tests**

Add tests that construct a valid manifest with `[1, 3, 224, 224]`, five indexed labels, scale
`1.0 / 255.0`, the pinned EfficientNet processor mean/std, and then independently mutate duplicate labels, a non-static
shape, zero file length, and a non-HTTPS source.

```rust
#[test]
fn rejects_duplicate_or_missing_required_labels() {
    let mut value = valid_manifest();
    value.labels[4].name = "normal".into();
    assert_eq!(value.validate(), Err(ImageManifestError::InvalidLabels));
}

#[test]
fn rejects_non_static_nchw_shape() {
    let mut value = valid_manifest();
    value.input.shape = [2, 3, 224, 224];
    assert_eq!(value.validate(), Err(ImageManifestError::InvalidInputShape));
}
```

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-ai image_manifest`  
Expected: compilation fails because `ImageModelManifest` and related types do not exist.

- [ ] **Step 3: Implement the manifest types and exact validation**

Use serde snake-case enums and reject any contract other than the approved one:

```rust
pub const VIDDEXA_LABELS: [&str; 5] = ["normal", "hentai", "porn", "sexy", "drawing"];

impl ImageModelManifest {
    pub fn validate(&self) -> Result<(), ImageManifestError> {
        self.asset.validate().map_err(|_| ImageManifestError::InvalidAsset)?;
        if self.asset.kind != AssetKind::ImageClassifier {
            return Err(ImageManifestError::InvalidAsset);
        }
        if self.file_name.trim().is_empty() || self.file_bytes == 0 {
            return Err(ImageManifestError::InvalidFile);
        }
        if self.input.shape != [1, 3, 224, 224]
            || self.input.layout != TensorLayout::Nchw
            || self.input.color_order != ColorOrder::Rgb
        {
            return Err(ImageManifestError::InvalidInputShape);
        }
        self.validate_labels()
    }
}
```

Validation also requires finite normalization values, non-zero standard deviations, unique indices
`0..4`, exact label set equality, opset `18`, and the pinned repository/revision.

- [ ] **Step 4: Verify GREEN**

Run: `cargo fmt --all && cargo test -p karma-ai image_manifest && cargo clippy -p karma-ai --all-targets -- -D warnings`  
Expected: all manifest tests pass with no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/karma-ai
git commit -m "feat: validate image model contracts"
```

---

### Task 2: Build image tensors and map named probabilities

**Files:**
- Modify: `crates/karma-ai/src/lib.rs`
- Create: `crates/karma-ai/src/image_tensor.rs`
- Create: `crates/karma-ai/src/image_classifier.rs`

**Interfaces:**
- Consumes: `PreparedFrame`, `ImageInputContract`, `RiskCategory`, `ImageInference`.
- Produces: `ImageTensorBuilder::build(&PreparedFrame, &ImageInputContract)`, `ImageTensor::as_slice()`, `ClassifierOutput::new(labels, probabilities)`, `ViddexaRiskMapper::map(&ClassifierOutput)`, `ImageClassifier`.

- [ ] **Step 1: Write failing tensor tests**

Create a 2×1 BGRA frame containing red and blue pixels, scale it to a 2×1 test contract, and assert
channel-first RGB values. Add a 2×1 to 1×1 interpolation test and a `Debug` redaction assertion.

```rust
assert_eq!(
    tensor.as_slice(),
    &[1.0, 0.0, 0.0, 0.0, 0.0, 1.0]
);
assert!(!format!("{tensor:?}").contains("1.0"));
```

- [ ] **Step 2: Run tensor RED**

Run: `cargo test -p karma-ai image_tensor`  
Expected: compilation fails because tensor types are missing.

- [ ] **Step 3: Implement bounded bilinear preprocessing**

Reuse the existing deterministic axis sampling semantics, write channels into three contiguous
planes, apply `(pixel * scale - mean[channel]) / std[channel]`, and own values in
`Zeroizing<Vec<f32>>`. Checked arithmetic must allocate exactly
`batch * channels * width * height` elements.

- [ ] **Step 4: Verify tensor GREEN**

Run: `cargo test -p karma-ai image_tensor`  
Expected: fixed-pixel, interpolation, and redaction tests pass.

- [ ] **Step 5: Write failing named-output mapping tests**

Test a reordered label vector and verify:

```rust
let output = ClassifierOutput::new(
    vec!["sexy", "drawing", "porn", "normal", "hentai"],
    vec![0.20, 0.05, 0.45, 0.10, 0.20],
).unwrap();
let inference = ViddexaRiskMapper::map(&output).unwrap();
assert_eq!(inference.score_millis, 720);
assert_eq!(inference.categories, vec![RiskCategory::Nudity]);
```

Also reject NaN, negative probabilities, values above one, sums outside `1.0 ± 0.01`, duplicate
labels, and missing required labels.

- [ ] **Step 6: Run mapping RED**

Run: `cargo test -p karma-ai image_classifier`  
Expected: compilation fails because classifier output and mapper types are missing.

- [ ] **Step 7: Implement classifier contract and mapper**

```rust
pub trait ImageClassifier {
    type Error;
    fn classify(&mut self, frame: &PreparedFrame) -> Result<ImageInference, Self::Error>;
}

let explicit = output.probability("porn")? + output.probability("hentai")?;
let suggestive = output.probability("sexy")?;
let score = (explicit + 0.35 * suggestive).clamp(0.0, 1.0);
```

Add `Nudity` when `explicit >= 0.5`, add `Suggestive` when `suggestive >= 0.5`, and round the score
to integer millis.

- [ ] **Step 8: Verify and commit**

Run: `cargo fmt --all && cargo test -p karma-ai && cargo clippy -p karma-ai --all-targets -- -D warnings`  
Expected: all `karma-ai` tests pass.

```bash
git add crates/karma-ai
git commit -m "feat: prepare and map classifier tensors"
```

---

### Task 3: Verify assets and run ONNX Runtime

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Create: `crates/karma-onnx/Cargo.toml`
- Create: `crates/karma-onnx/src/lib.rs`
- Create: `crates/karma-onnx/src/model.rs`
- Create: `crates/karma-onnx/src/classifier.rs`
- Create: `crates/karma-onnx/src/health.rs`
- Create: `crates/karma-onnx/tests/fixtures/five_class_identity.onnx`

**Interfaces:**
- Consumes: `ImageModelManifest`, `ImageTensorBuilder`, `ClassifierOutput`, `ViddexaRiskMapper`.
- Produces: `VerifiedImageModel::load(manifest_path)`, `VerifiedImageModel::create_classifier()`, `OnnxImageClassifier`, `InferenceErrorKind`, `InferenceHealth`.

- [ ] **Step 1: Write failing asset-verification tests**

Use a temporary directory and small byte files to prove missing model, wrong length, and wrong SHA
fail before session construction. Verify formatted errors contain only stable kinds and never paths.

```rust
assert_eq!(error.kind(), InferenceErrorKind::ModelHashMismatch);
assert!(!error.to_string().contains(temp.path().to_str().unwrap()));
```

- [ ] **Step 2: Run asset RED**

Run: `cargo test -p karma-onnx model`  
Expected: Cargo reports that package `karma-onnx` does not exist.

- [ ] **Step 3: Implement streaming verification**

Add dependencies:

```toml
ort = { version = "=2.0.0-rc.10", default-features = false, features = ["std", "ndarray", "download-binaries", "copy-dylibs"] }
sha2 = "0.10"
serde_json.workspace = true
thiserror.workspace = true
karma-ai = { path = "../karma-ai" }
```

Parse the JSON manifest with a 1 MiB maximum manifest size, validate it, resolve `file_name` only
inside the manifest directory, reject absolute paths and path components, and cap the model at
128 MiB. Read and hash the model through one open file handle, then retain those verified bytes so
every monitor Session loads the exact bytes that passed SHA-256 verification.

- [ ] **Step 4: Verify asset GREEN**

Run: `cargo test -p karma-onnx model`  
Expected: all pre-session verification tests pass.

- [ ] **Step 5: Write failing runtime and health tests**

The committed fixture has one input `[1,3,2,2]` and produces five deterministic logits. Tests load
it through a fixture manifest, assert named probabilities and risk mapping, reject an incorrect
input name, and check health counters:

```rust
assert_eq!(health.inferences(), 1);
assert_eq!(health.failures(), 0);
assert!(health.total_latency_micros() > 0);
```

- [ ] **Step 6: Run runtime RED**

Run: `cargo test -p karma-onnx classifier`  
Expected: tests fail because ONNX session creation and inference are not implemented.

- [ ] **Step 7: Implement the CPU session**

Construct with:

```rust
let session = ort::session::Session::builder()?
    .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
    .with_intra_threads(1)?
    .commit_from_file(model_path)?;
```

Validate session input/output names and tensor shapes before first inference. Run with
`ort::inputs![input_name.as_str() => TensorRef::from_array_view(...)?]`, extract exactly five finite
logits, apply max-subtracted softmax, build `ClassifierOutput`, then use `ViddexaRiskMapper`.
Map all runtime failures to stable `InferenceErrorKind` values without retaining third-party error
strings.

- [ ] **Step 8: Verify and commit**

Run: `cargo fmt --all && cargo test -p karma-onnx && cargo clippy -p karma-onnx --all-targets -- -D warnings`  
Expected: verification, fixture inference, and health tests pass.

```bash
git add Cargo.toml Cargo.lock crates/karma-onnx
git commit -m "feat: run verified ONNX image models"
```

---

### Task 4: Export the pinned viddexa model

**Files:**
- Modify: `.gitignore`
- Create: `tools/model-export/requirements.txt`
- Create: `tools/model-export/export_viddexa.py`
- Create: `assets/image/viddexa-nano/manifest.example.json`
- Create: `assets/image/viddexa-nano/LICENSE`
- Create: `docs/model-assets.md`

**Interfaces:**
- Consumes: pinned Hugging Face repository and revision.
- Produces locally: `target/model-assets/viddexa-nano/model.onnx`, `manifest.json`, `reference-output.json`.

- [ ] **Step 1: Write export-tool validation tests**

Use Python `unittest` for pure helpers: reject a changed repository/revision, generate lowercase
SHA-256, and verify the manifest contains the exact five labels and input contract.

Run: `python3 -m unittest tools/model-export/test_export_viddexa.py`  
Expected: import fails because `export_viddexa.py` does not exist.

- [ ] **Step 2: Implement pinned export**

Pin:

```text
huggingface-hub==0.34.4
onnx==1.18.0
onnxruntime==1.22.1
pillow==11.3.0
safetensors==0.5.3
torch==2.7.1
transformers==4.56.1
```

Load with `revision=REVISION`, `use_safetensors=True`, `trust_remote_code=False`; export static
input `[1,3,224,224]` at opset 18. Validate the upstream 1280-kernel pooler, replace it with the
equivalent `AdaptiveAvgPool2d(1)`, and compare logits before and after replacement. This avoids
the legacy exporter changing the oversized pooling denominator. Run `onnx.checker.check_model`; compare PyTorch and
ONNX Runtime logits for a deterministic generated RGB gradient with absolute tolerance `1e-4`;
write the final byte length and SHA-256 into `manifest.json`.

- [ ] **Step 3: Verify pure tests and perform one real export**

Run: `python3 -m unittest tools/model-export/test_export_viddexa.py`  
Expected: all helper tests pass.

Run: `python3 tools/model-export/export_viddexa.py --output target/model-assets/viddexa-nano`  
Expected: model, manifest, and reference output are created; no pickle checkpoint is loaded.

- [ ] **Step 4: Validate the real artifact through Rust**

Run: `cargo run -p karma-onnx --example verify-model -- target/model-assets/viddexa-nano/manifest.json`  
Expected: prints only model version, byte length, and `status=verified`.

- [ ] **Step 5: Commit source and metadata only**

Confirm: `git status --short` does not list `model.onnx`, downloaded weights, or virtual environments.

```bash
git add .gitignore tools/model-export assets/image/viddexa-nano docs/model-assets.md
git commit -m "build: export pinned viddexa ONNX model"
```

---

### Task 5: Connect scheduled Windows frames to ONNX

**Files:**
- Modify: `apps/karma-agent-windows/Cargo.toml`
- Modify: `apps/karma-agent-windows/src/main.rs`
- Create: `apps/karma-agent-windows/src/image_consumer.rs`
- Modify: `README.md`
- Create: `docs/windows-onnx-acceptance.md`

**Interfaces:**
- Consumes: `PreparedFrameConsumer`, `FrameWork`, `OnnxImageClassifier`, `InferenceHealth`.
- Produces: `OnnxFrameConsumer::new(classifier, health)`, `OnnxFrameConsumer::consume`, `KARMA_IMAGE_MODEL_MANIFEST`.

- [ ] **Step 1: Write failing consumer tests**

Extract a generic consumer core testable on macOS. A fake classifier increments calls and returns a
fixed `ImageInference`; prove `run_image=false` skips classification, success increments inference
health, and an error increments failure health without panicking.

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-agent-windows image_consumer`  
Expected: compilation fails because `image_consumer` is missing.

- [ ] **Step 3: Implement the consumer and startup wiring**

Read `KARMA_IMAGE_MODEL_MANIFEST` once at startup. Verify the model before enumerating monitors.
For every monitor, create an independent classifier session and shared health handle, then pass
`OnnxFrameConsumer` to `WindowsFrameWorker::start`. If model loading fails, print only:

```text
status=unavailable component=image_inference error=<stable-kind>
```

The consumer must drop each `PreparedFrame` after inference and must not print the output scores.

- [ ] **Step 4: Verify Windows compilation and workspace quality**

Run:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace --target x86_64-pc-windows-msvc
```

Expected: all commands exit zero.

- [ ] **Step 5: Document true acceptance boundary**

Document environment configuration, offline model placement, expected health-only logs, P50/P95
latency collection, multi-monitor CPU/memory checks, and the required internal content test set.
State explicitly that macOS compilation does not prove Windows runtime performance or model
accuracy.

- [ ] **Step 6: Commit**

```bash
git add apps/karma-agent-windows README.md docs/windows-onnx-acceptance.md
git commit -m "feat: classify scheduled Windows frames"
```

---

## Final Verification

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo test --workspace`.
- [ ] Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Run `cargo check --workspace --target x86_64-pc-windows-msvc`.
- [ ] Run Python export helper tests.
- [ ] Verify the locally exported real model through `karma-onnx`.
- [ ] Run `git diff --check` and confirm the worktree is clean after final commit.
