# Windows Cloneable Test Bundle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish a Windows x64 development test bundle in the repository so a Windows user can clone `main`, configure no environment variables, and start the current Agent with one PowerShell script.

**Architecture:** Keep source and compiler caches outside the distribution. A checked-in `release/windows-x64-test/` directory contains the verified x64 PE executable, its dynamic DLL, complete verified image and OCR asset trees, a SHA-256 manifest, and `Start-KarmaTest.ps1`. The script derives every path from its own directory, validates all files before launch, and sets process-local environment variables only.

**Tech Stack:** Rust 1.85.1, cargo-xwin 0.23.0, MSVC target `x86_64-pc-windows-msvc`, PowerShell 5.1+, SHA-256, PP-OCRv5, Viddexa ONNX image model.

## Global Constraints

- Commit only runtime outputs and required test assets; never commit `target/debug`, `target/release`, `target/x86_64-pc-windows-msvc`, virtual environments, cargo caches, downloaded archives, or `.part` files.
- Every checked-in binary/model file must remain below GitHub's 100 MiB single-file limit.
- Preserve every model's license, manifest, reference artifact, and complete directory tree.
- The startup script must validate `SHA256SUMS`, `manifest.json`, and required file existence before executing the Agent.
- The script must use process-scoped `KARMA_*` variables and must not write Windows Registry, services, tasks, policies, screenshots, recognized text, or model paths to logs.
- The document must label the bundle as unsigned development/test software and state that automatic process closing, service installation, anti-tamper, time limits, and a final installer are not implemented.

---

### Task 1: Add a deterministic test-bundle verifier and launcher contract

**Files:**
- Create: `release/windows-x64-test/Start-KarmaTest.ps1`
- Create: `release/windows-x64-test/Verify-KarmaTestBundle.ps1`
- Create: `release/windows-x64-test/README.md`
- Create: `tools/package-windows-test/test_bundle_contract.sh`

**Interfaces:**
- `Verify-KarmaTestBundle.ps1` exits `0` only if all SHA-256 entries, the executable, `DirectML.dll`, the image manifest, and the OCR lightweight manifest exist under `$PSScriptRoot`.
- `Start-KarmaTest.ps1 [-OcrProfile auto|lightweight|accurate]` invokes the verifier and starts `karma-agent-windows.exe` with process-local `KARMA_IMAGE_MODEL_MANIFEST`, `KARMA_OCR_LIGHTWEIGHT_MANIFEST`, optional accurate manifest, and `KARMA_OCR_PROFILE`.

- [ ] **Step 1: Write the failing shell contract test**

```bash
#!/usr/bin/env bash
set -euo pipefail
bundle_dir="release/windows-x64-test"
test -f "$bundle_dir/Start-KarmaTest.ps1"
test -f "$bundle_dir/Verify-KarmaTestBundle.ps1"
test -f "$bundle_dir/README.md"
rg -q 'KARMA_IMAGE_MODEL_MANIFEST' "$bundle_dir/Start-KarmaTest.ps1"
rg -q 'Get-FileHash' "$bundle_dir/Verify-KarmaTestBundle.ps1"
```

- [ ] **Step 2: Run RED**

Run: `bash tools/package-windows-test/test_bundle_contract.sh`

Expected: FAIL because the release scripts and README do not exist.

- [ ] **Step 3: Add the minimal PowerShell scripts and README**

`Verify-KarmaTestBundle.ps1` must read UTF-8 `SHA256SUMS` lines formatted as `<hex>  <relative-path>`, reject paths outside `$PSScriptRoot`, calculate each `Get-FileHash -Algorithm SHA256`, and exit non-zero on mismatch. `Start-KarmaTest.ps1` must invoke it before setting environment variables and use `& (Join-Path $PSScriptRoot 'karma-agent-windows.exe')` as its final command.

- [ ] **Step 4: Run GREEN**

Run: `bash tools/package-windows-test/test_bundle_contract.sh`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add release/windows-x64-test tools/package-windows-test/test_bundle_contract.sh
git commit -m "build: add Windows test bundle launcher"
```

### Task 2: Export and validate the required image model

**Files:**
- Create: `release/windows-x64-test/models/image/viddexa-nano/model.onnx`
- Create: `release/windows-x64-test/models/image/viddexa-nano/manifest.json`
- Create: `release/windows-x64-test/models/image/viddexa-nano/reference-output.json`
- Create: `release/windows-x64-test/models/image/viddexa-nano/LICENSE`

**Interfaces:**
- The asset directory is the complete output of `tools/model-export/export_viddexa.py` plus the model license.
- `cargo run -p karma-onnx --example verify-model -- <manifest>` returns exit code `0` before any file is copied to `release/`.

- [ ] **Step 1: Write the failing asset-layout assertion**

Add to `tools/package-windows-test/test_bundle_contract.sh`:

```bash
for artifact in model.onnx manifest.json reference-output.json LICENSE; do
  test -f "release/windows-x64-test/models/image/viddexa-nano/$artifact"
done
```

- [ ] **Step 2: Run RED**

Run: `bash tools/package-windows-test/test_bundle_contract.sh`

Expected: FAIL because the verified image model directory is absent.

- [ ] **Step 3: Export, verify, and copy image assets**

```bash
tools/model-export/.venv/bin/python tools/model-export/export_viddexa.py \
  --output target/model-assets/viddexa-nano
cargo run -p karma-onnx --example verify-model -- \
  target/model-assets/viddexa-nano/manifest.json
```

Copy only the four declared files into `release/windows-x64-test/models/image/viddexa-nano/` using `apply_patch` for text files and a checked packaging command for binaries. Record their hashes in `SHA256SUMS` in Task 4.

- [ ] **Step 4: Run GREEN**

Run: `bash tools/package-windows-test/test_bundle_contract.sh && cargo run -p karma-onnx --example verify-model -- release/windows-x64-test/models/image/viddexa-nano/manifest.json`

Expected: both commands return exit code `0`.

- [ ] **Step 5: Commit**

```bash
git add release/windows-x64-test/models/image
git commit -m "build: package verified image model for Windows tests"
```

### Task 3: Package the verified OCR bundle and Windows runtime

**Files:**
- Create: `release/windows-x64-test/karma-agent-windows.exe`
- Create: `release/windows-x64-test/DirectML.dll`
- Create: `release/windows-x64-test/models/ocr/pp-ocrv5-mobile/`
- Create: `release/windows-x64-test/SHA256SUMS`
- Modify: `tools/package-windows-test/test_bundle_contract.sh`

**Interfaces:**
- The OCR directory must include `manifest.json`, `detector.onnx`, `recognizer.onnx`, `dictionary.txt`, `LICENSE`, `NOTICE.md`, and the four `reference/` files.
- `SHA256SUMS` covers every regular runtime/model file, excludes itself and scripts, uses only forward-slash relative paths, and is sorted bytewise.

- [ ] **Step 1: Write failing runtime and OCR-layout assertions**

Add to `tools/package-windows-test/test_bundle_contract.sh`:

```bash
test -f release/windows-x64-test/karma-agent-windows.exe
test -f release/windows-x64-test/DirectML.dll
test -f release/windows-x64-test/SHA256SUMS
for artifact in manifest.json detector.onnx recognizer.onnx dictionary.txt LICENSE NOTICE.md; do
  test -f "release/windows-x64-test/models/ocr/pp-ocrv5-mobile/$artifact"
done
```

- [ ] **Step 2: Run RED**

Run: `bash tools/package-windows-test/test_bundle_contract.sh`

Expected: FAIL because the checked-in runtime and OCR asset tree is absent.

- [ ] **Step 3: Build and copy only runtime deliverables**

```bash
PATH="/opt/homebrew/opt/llvm/bin:/opt/homebrew/opt/lld/bin:$PATH" \
  cargo xwin build --xwin-version 17 -p karma-agent-windows \
  --target x86_64-pc-windows-msvc --release
```

Copy `karma-agent-windows.exe`, dereferenced `DirectML.dll`, and the complete verified `.local-models/pp-ocrv5-mobile` tree into `release/windows-x64-test`. Use the Python and Rust OCR verifiers against the copied manifest before generating `SHA256SUMS`.

- [ ] **Step 4: Generate and validate hashes**

Generate hashes with sorted relative paths. On macOS, run a deterministic shell loop from the bundle directory; then run `Verify-KarmaTestBundle.ps1` from a Windows host before declaring the bundle ready.

- [ ] **Step 5: Run GREEN**

Run:

```bash
bash tools/package-windows-test/test_bundle_contract.sh
.venv-ocr-export/bin/python tools/ocr-export/verify.py \
  release/windows-x64-test/models/ocr/pp-ocrv5-mobile/manifest.json
cargo run -p karma-onnx --example verify_ocr_bundle -- \
  release/windows-x64-test/models/ocr/pp-ocrv5-mobile/manifest.json
```

Expected: all commands return exit code `0`.

- [ ] **Step 6: Commit**

```bash
git add release/windows-x64-test tools/package-windows-test/test_bundle_contract.sh
git commit -m "build: publish cloneable Windows test bundle"
```

### Task 4: Align user documentation and final verification

**Files:**
- Modify: `docs/windows-installation-guide.md`
- Modify: `README.md`

**Interfaces:**
- Documentation tells a Windows tester to clone `main`, run `release/windows-x64-test/Start-KarmaTest.ps1`, install the VC++ x64 Redistributable, and consult the bundle README.
- Documentation clearly distinguishes the cloneable development bundle from a signed production installer.

- [ ] **Step 1: Write failing documentation assertions**

Add to `tools/package-windows-test/test_bundle_contract.sh`:

```bash
rg -q 'release/windows-x64-test/Start-KarmaTest.ps1' docs/windows-installation-guide.md
rg -q 'cloneable Windows test bundle' README.md
```

- [ ] **Step 2: Run RED**

Run: `bash tools/package-windows-test/test_bundle_contract.sh`

Expected: FAIL because documentation does not name the checked-in bundle entry point.

- [ ] **Step 3: Update documentation**

Document clone, prerequisite, hash verification, one-command launch, supported test scope, known limitations, and physical Windows acceptance matrix. Do not describe this bundle as signed, installed, tamper-resistant, or suitable for child-account enforcement.

- [ ] **Step 4: Run final verification**

Run:

```bash
bash tools/package-windows-test/test_bundle_contract.sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
git diff --check
git status --short
```

Expected: contract checks and Rust verification pass; only intentional release assets and source/documentation changes are staged before commit.

- [ ] **Step 5: Commit**

```bash
git add README.md docs/windows-installation-guide.md tools/package-windows-test
git commit -m "docs: document cloneable Windows test bundle"
```

## Final Delivery Verification

- [ ] On Windows 10 22H2 and Windows 11 x64, run `Set-ExecutionPolicy -Scope Process Bypass` only if required for the local unsigned script, then run `./Start-KarmaTest.ps1` from the bundle directory.
- [ ] Record only counters, status, latency, CPU, working set, and result in `docs/acceptance/windows-ocr-runtime.md`; do not add screenshots, OCR text, terms, categories, model paths, or URLs.
- [ ] Verify one-, two-, and three-display scenarios, OCR degraded mode, and checksum-rejection behavior.
- [ ] Do not claim full family-control protection until source attribution, risk fusion, process enforcement, policy scheduling, authenticated administration, signing, installer, and service work are implemented.
