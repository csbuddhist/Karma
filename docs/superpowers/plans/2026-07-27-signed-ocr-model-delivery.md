# Signed OCR Model Delivery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the Windows Agent securely download, verify, benchmark, activate, and roll back accurate PP-OCRv5 bundles after a parent enables accurate mode and background updates.

**Architecture:** A new platform-neutral `karma-model-delivery` crate owns strict signed-manifest parsing, resumable transfer state, bounded archive extraction, version storage, activation, and rollback. `karma-windows` supplies a WinHTTP transport that inherits Windows proxy settings. The Agent runs delivery on a separate control thread and publishes only verified `PendingActivation` directories to the OCR runtime at worker-rebuild checkpoints.

**Tech Stack:** Rust 1.85, edition 2024, `ed25519-dalek = 2.2`, `sha2 = 0.10`, `serde_json`, `semver = 1`, `url = 2`, `tar = 0.4`, `zstd = 0.13`, Windows WinHTTP through `windows = 0.62.2`.

## Global Constraints

- Begin only after the OCR Runtime plan through Task 7 is green.
- The updater is a model-specific state machine, not a general-purpose downloader.
- Only HTTPS URLs on a compile-time host allowlist are accepted.
- Ed25519 verifies the exact saved UTF-8 manifest bytes before JSON parsing.
- Strict JSON rejects unknown fields and duplicate fields; tests include duplicate-key raw JSON.
- The private signing key never enters the repository, CI logs, Agent, or generated packages.
- Tests inject a fixture public key. Production packaging must inject exactly 32 public-key bytes
  through `KARMA_OCR_UPDATE_ROOT_PUBLIC_KEY_FILE`; release builds fail if it is absent.
- WinHTTP uses system proxy discovery. No custom proxy credentials are logged or persisted.
- Archive maximum is 600 MiB; expanded maximum 1 GiB; maximum 32 files.
- Extraction rejects absolute/parent paths, links, devices, FIFOs, sparse extensions, duplicate
  destinations, case-insensitive collisions, alternate data streams, and undeclared files.
- Keep current plus one previous version. Never switch sessions inside frame processing.
- Three consecutive OCR failures after activation roll back and suppress that version.
- Network requests contain only channel, Agent version, platform architecture, and current package
  version; no monitor, OCR, policy, app, window, file, or hardware-identity data.

## File Map

- `crates/karma-model-delivery/src/manifest.rs`: exact-byte signature verification and update schema.
- `crates/karma-model-delivery/src/transport.rs`: bounded HTTP request/response abstraction.
- `crates/karma-model-delivery/src/resume.rs`: ETag/Range state machine.
- `crates/karma-model-delivery/src/archive.rs`: safe bounded `tar.zst` extraction.
- `crates/karma-model-delivery/src/store.rs`: version directories, atomic JSON, retention.
- `crates/karma-model-delivery/src/coordinator.rs`: download-to-pending state machine.
- `crates/karma-windows/src/winhttp_transport.rs`: system-proxy Windows transport.
- `apps/karma-agent-windows/src/model_delivery.rs`: background task, activation, and rollback wiring.
- `tools/ocr-update/sign_manifest.py`: offline signing tool requiring an explicit private-key path.
- `assets/update/ocr-root-public-key.bin`: generated release input, ignored until secure provisioning.

---

### Task 1: Verify exact signed update manifests before parsing

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Create: `crates/karma-model-delivery/Cargo.toml`
- Create: `crates/karma-model-delivery/src/lib.rs`
- Create: `crates/karma-model-delivery/src/error.rs`
- Create: `crates/karma-model-delivery/src/manifest.rs`
- Create: `crates/karma-model-delivery/tests/fixtures/update-manifest.json`
- Create: `crates/karma-model-delivery/tests/fixtures/update-manifest.sig`

**Interfaces:**
- Produces `UpdateManifestVerifier::verify(raw, signature) -> VerifiedUpdateManifest`.
- Produces stable `DeliveryErrorKind`; error display is only its snake-case identifier.

- [ ] **Step 1: Write failing cryptographic/schema tests**

Test valid fixture, one-byte mutation, wrong key/signature length, invalid UTF-8, duplicate key,
unknown field, wrong channel, non-semver versions, minimum Agent version, non-HTTPS URL, host not in
allowlist, fragment/userinfo, archive size/hash, duplicate/case-colliding assets, unsafe relative
paths, and missing `manifest.json`.

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-model-delivery manifest`
Expected: crate/types do not exist.

- [ ] **Step 3: Implement verify-then-parse**

Accept `ed25519_dalek::VerifyingKey` in the constructor. Require a 64-byte detached signature and
call strict verification over the raw bytes. Scan JSON tokens for duplicate object keys before
deserializing `#[serde(deny_unknown_fields)]` structs. Validate semver, exact channel, URL, asset
list, sizes, lowercase hashes, and case-folded path uniqueness.

- [ ] **Step 4: Add offline signing fixture tooling**

Create `tools/ocr-update/sign_manifest.py` with required arguments `--private-key`,
`--manifest`, and `--signature-output`. It may read a 32-byte seed only from the explicit path,
refuses paths inside the repository, writes only the detached signature, and never prints key
material.

- [ ] **Step 5: Verify and commit**

Run: `cargo fmt --all && cargo test -p karma-model-delivery manifest && cargo clippy -p karma-model-delivery --all-targets -- -D warnings`
Expected: signature and strict-schema tests pass.

```bash
git add Cargo.toml Cargo.lock crates/karma-model-delivery tools/ocr-update
git commit -m "feat: verify signed OCR update manifests"
```

---

### Task 2: Extract declared tar.zst assets safely and atomically

**Files:**
- Create: `crates/karma-model-delivery/src/archive.rs`
- Create: `crates/karma-model-delivery/src/store.rs`
- Create: `crates/karma-model-delivery/tests/archive_security.rs`

**Interfaces:**
- Produces `ArchiveExtractor::extract_verified`.
- Produces `ModelStore::{install, current, previous, mark_pending, activate, rollback, prune}`.

- [ ] **Step 1: Write malicious-archive RED tests**

Programmatically build archives containing `../`, absolute paths, symlink, hardlink, device/FIFO,
duplicate names, Windows case collisions, colon/ADS, undeclared files, wrong length/hash, 33 files,
declared expansion over 1 GiB, truncated zstd, and a file that expands beyond its declaration.
Also test a valid bundle.

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-model-delivery archive_security`
Expected: archive/store modules are missing.

- [ ] **Step 3: Implement streaming safe extraction**

Stream decompression; never call `Entry::unpack`. Normalize each path component, compare against the
signed declaration, create regular files with `create_new`, count bytes while hashing, reject any
overrun immediately, flush files, and rename the staging directory only after every declared file
matches. Remove failed staging directories using an explicit validated child path.

- [ ] **Step 4: Implement atomic store metadata**

Use version directory names derived from validated semver. Write `current.json.tmp`, flush it and
its parent directory where supported, then same-directory rename to `current.json`. Metadata
contains `current`, `previous`, `pending`, and `failed_versions`, with
`#[serde(deny_unknown_fields)]`. Recovery ignores incomplete staging directories.

- [ ] **Step 5: Verify and commit**

Run: `cargo fmt --all && cargo test -p karma-model-delivery archive_security && cargo clippy -p karma-model-delivery --all-targets -- -D warnings`
Expected: valid install/activate/rollback pass and every malicious archive is rejected.

```bash
git add crates/karma-model-delivery
git commit -m "feat: safely install OCR model archives"
```

---

### Task 3: Implement resumable downloads behind a transport contract

**Files:**
- Create: `crates/karma-model-delivery/src/transport.rs`
- Create: `crates/karma-model-delivery/src/resume.rs`
- Create: `crates/karma-model-delivery/tests/resume.rs`

**Interfaces:**
- Produces `HttpTransport`, `HttpRequest`, `HttpResponse`, `ResumeMetadata`, `ResumableDownload`.
- The transport streams bounded chunks and exposes only status, ETag, Content-Length, and
  Content-Range.

- [ ] **Step 1: Write fake-transport RED tests**

Cover fresh `200`, valid `206`, server ignoring Range, mismatched Content-Range, ETag change,
short/long body, retryable timeout, non-retryable status, three retries with 1/2/4-second injected
backoff, restart after corrupt partial state, final archive hash, and byte counter privacy.

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-model-delivery resume`
Expected: transport/resume types are missing.

- [ ] **Step 3: Implement the state machine**

Store partial data and strict `resume.json` under the model cache. Resume only when URL hash,
declared length/hash, and saved ETag agree. Send `Range: bytes=N-` and `If-Range: ETag`. If the
server returns `200` or changed ETag, discard the explicit partial child and restart. Enforce the
signed length while streaming and verify SHA-256 before returning the archive handle.

- [ ] **Step 4: Verify and commit**

Run: `cargo fmt --all && cargo test -p karma-model-delivery resume && cargo clippy -p karma-model-delivery --all-targets -- -D warnings`
Expected: resume/restart/retry tests pass without real network access.

```bash
git add crates/karma-model-delivery
git commit -m "feat: resume bounded OCR model downloads"
```

---

### Task 4: Add Windows WinHTTP system-proxy transport

**Files:**
- Modify: `crates/karma-windows/Cargo.toml`
- Modify: `crates/karma-windows/src/lib.rs`
- Create: `crates/karma-windows/src/winhttp_transport.rs`
- Create: `apps/karma-agent-windows/tests/winhttp_proxy_server.rs`

**Interfaces:**
- Produces `WinHttpTransport::new(allowlist, timeouts)`.
- Implements `karma_model_delivery::HttpTransport`.

- [ ] **Step 1: Write Windows-only contract tests**

Use a loopback origin and proxy test process to verify system proxy discovery, HTTPS-only rejection,
host allowlist, redirects revalidated against the allowlist, Range/If-Range headers, 10-second
connect and 30-second receive timeouts, bounded streaming, and handle cleanup.

- [ ] **Step 2: Run Windows RED**

Run: `cargo test -p karma-agent-windows --target x86_64-pc-windows-msvc winhttp_proxy_server`
Expected: cross-compilation or Windows CI fails because the adapter is missing.

- [ ] **Step 3: Implement WinHTTP adapter**

Enable only the required `Win32_Networking_WinHttp` Windows feature. Open an automatic-proxy
session, disable cookies and credentials persistence, configure timeouts, revalidate every redirect,
read response data in 64 KiB chunks, and map WinHTTP errors to stable transport categories. Keep
all required `unsafe` blocks narrow and documented with handle/buffer invariants.

- [ ] **Step 4: Verify**

Run:

```bash
cargo fmt --all
cargo check -p karma-windows --target x86_64-pc-windows-msvc
cargo clippy -p karma-windows --all-targets --target x86_64-pc-windows-msvc -- -D warnings
```

Expected: Windows adapter compiles without warnings. Run the loopback proxy tests on Windows 10 and
11 before commit.

- [ ] **Step 5: Commit**

```bash
git add crates/karma-windows apps/karma-agent-windows/tests/winhttp_proxy_server.rs
git commit -m "feat: download models through Windows proxy"
```

---

### Task 5: Coordinate verification, benchmarking, activation, and rollback

**Files:**
- Create: `crates/karma-model-delivery/src/coordinator.rs`
- Modify: `crates/karma-model-delivery/src/lib.rs`
- Create: `crates/karma-model-delivery/tests/coordinator.rs`

**Interfaces:**
- Produces `DeliveryCoordinator`, `DeliveryState`, `PendingActivation`, `ActivationMonitor`.
- Consumes injected bundle verifier and benchmark callbacks from the OCR runtime.

- [ ] **Step 1: Write failing state-machine tests**

Assert exact transitions:

```text
Current -> Downloading -> VerifyingSignature -> VerifyingAssets
-> VerifyingRuntime -> Benchmarking -> PendingActivation -> Active
```

Cover every transition failure returning to Current, minimum Agent rejection, failed-version
suppression, safe-checkpoint activation, session/reference failure before activation, explicit
accurate performance warning without bypassing validity, auto performance rejection, three
post-activation failures rolling back, success resetting the counter, and a higher version clearing
suppression.

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-model-delivery coordinator`
Expected: coordinator types are missing.

- [ ] **Step 3: Implement deterministic coordination**

The coordinator owns no ONNX type. Inject:

```rust
pub trait CandidateValidator {
    type Candidate;
    fn verify_runtime(&self, manifest_path: &Path) -> Result<Self::Candidate, DeliveryError>;
    fn benchmark(&self, candidate: &mut Self::Candidate) -> Result<BenchmarkDecision, DeliveryError>;
}
```

Return `PendingActivation` only after install, runtime/reference validation, and benchmark policy.
Activation requires an explicit control-thread checkpoint. Record the previous version before
publishing current. The monitor rolls back at exactly three consecutive failures from the newly
activated version.

- [ ] **Step 4: Verify and commit**

Run: `cargo fmt --all && cargo test -p karma-model-delivery && cargo clippy -p karma-model-delivery --all-targets -- -D warnings`
Expected: all manifest, archive, resume, coordination, activation, and rollback tests pass.

```bash
git add crates/karma-model-delivery
git commit -m "feat: activate and roll back OCR model updates"
```

---

### Task 6: Wire parent authorization and background delivery into the Agent

**Files:**
- Modify: `apps/karma-agent-windows/Cargo.toml`
- Modify: `apps/karma-agent-windows/src/main.rs`
- Create: `apps/karma-agent-windows/src/model_delivery.rs`
- Create: `apps/karma-agent-windows/src/update_key.rs`
- Create: `apps/karma-agent-windows/build.rs`
- Modify: `.gitignore`
- Create: `docs/acceptance/windows-ocr-model-delivery.md`
- Create: `docs/security/ocr-update-key-provisioning.md`

**Interfaces:**
- Produces `ModelDeliveryController::{start, request_check, take_pending, report_ocr_result, stop}`.
- Build script embeds only the production public key.

- [ ] **Step 1: Write failing Agent control tests**

Cover disabled accurate mode causing zero network calls; initial parent enable starting one
background task; updates disabled after initial install; concurrent checks coalesced; shutdown
joining the thread; pending activation consumed only while workers rebuild; new engine creation
before old worker disposal; and rollback reconstructing workers from previous bundle.

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-agent-windows model_delivery`
Expected: Agent controller is missing.

- [ ] **Step 3: Embed the release public key safely**

`build.rs` requires `KARMA_OCR_UPDATE_ROOT_PUBLIC_KEY_FILE` for non-debug builds, verifies exactly
32 bytes, copies it to `OUT_DIR`, and recompiles when the source changes. Debug/tests use an
explicit injected key and never silently use a production-looking default. Ignore
`assets/update/ocr-root-public-key.bin`.

- [ ] **Step 4: Implement the background controller**

Start the thread only when parent-controlled settings enable accurate mode. Send only approved
query fields. The frame-worker thread performs no HTTP, signature, archive, or disk-store work.
At a control-loop checkpoint: stop affected workers, create and reference-verify all replacement
engines, start all replacements, then mark active. If replacement construction fails, restart the
old version and leave current unchanged.

- [ ] **Step 5: Add stable health output**

Report state, downloaded bytes, stable error kind, current/pending version, and rollback outcome.
Redact URL, host path, ETag, tokens, proxy credentials, absolute paths, OCR summary, and model
runtime error strings.

- [ ] **Step 6: Run full verification**

Run:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo check -p karma-agent-windows --target x86_64-pc-windows-msvc
rg -n 'println!|eprintln!|tracing|log::' crates/karma-model-delivery crates/karma-windows/src/winhttp_transport.rs apps/karma-agent-windows/src
```

Expected: workspace tests pass; Windows cross-check passes; manual log inspection confirms the
privacy allowlist.

- [ ] **Step 7: Execute Windows acceptance**

On Windows 10 22H2 and Windows 11 verify direct connection, system proxy, optional local proxy at
`127.0.0.1:7897`, interrupted resume, ETag change, bad signature/hash, incompatible Agent version,
disk full, malicious archives, successful pending activation, session validation failure, three
failure rollback, restart recovery, and update-disabled behavior.

- [ ] **Step 8: Commit**

```bash
git add .gitignore apps/karma-agent-windows docs/acceptance/windows-ocr-model-delivery.md docs/security/ocr-update-key-provisioning.md
git commit -m "feat: deliver signed OCR models in background"
```

---

## Final Verification

- [ ] Run Task 6 Step 6 from a clean checkout with no model cache.
- [ ] Run `rg -n 'TODO|FIXME|placeholder|unimplemented!|todo!' crates/karma-model-delivery crates/karma-windows apps/karma-agent-windows tools/ocr-update`; expected: no implementation placeholders.
- [ ] Confirm the release build fails without the injected public key and the private key is absent from Git history and CI artifacts.
- [ ] Confirm the host allowlist, channel, update root, size limits, timeouts, and retry count cannot be changed by a downloaded manifest.
- [ ] Confirm a failed update leaves current inference operational and restart recovery selects only a previously activated version.
- [ ] Confirm no updater test or health record contains screen/OCR data.
- [ ] Request security-focused code review before merging; address findings with the `receiving-code-review` skill.
