# Karma Agent Inference Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the portable Agent inference foundation that validates model metadata, independently schedules work for every monitor, converts ephemeral OCR text into privacy-safe risk categories, and assembles `RiskObservation` values for the existing policy engine.

**Architecture:** A new `karma-ai` crate owns model manifests, frame scheduling, OCR word-pack matching, and observation assembly. It consumes stable types from `karma-domain` but never stores pixels or recognized text. Windows.Graphics.Capture and ONNX Runtime adapters will implement these boundaries in subsequent plans, allowing this slice to remain deterministic and fully testable on macOS.

**Tech Stack:** Rust 1.85+, edition 2024, Cargo, `serde`, `serde_json`, `thiserror`, `regex`, `unicode-normalization`.

## Global Constraints

- Every active monitor has independent image and OCR scheduling state.
- Image inference is capped at 2 FPS; OCR is capped at 1 FPS and only runs after the frame fingerprint changes.
- OCR text exists only as a function input and never appears in `RiskObservation`, logs, errors, or persisted values.
- OCR literal matching uses Unicode NFKC normalization and lowercase conversion.
- Traditional Chinese and simplified Chinese are represented as separate signed word-pack entries; this slice does not perform lossy script conversion.
- OCR-only results never request application termination; the existing policy engine decides action.
- Model and word-pack manifests must have non-empty version, license, and SHA-256 values.
- No unsafe Rust, async runtime, Windows API, ONNX binary, database, network, or UI dependency enters this slice.
- All behavior is developed test-first and each task ends with a focused commit.

## File Map

- `crates/karma-ai/Cargo.toml`: inference-foundation dependencies.
- `crates/karma-ai/src/lib.rs`: public exports.
- `crates/karma-ai/src/manifest.rs`: model metadata and validation.
- `crates/karma-ai/src/scheduler.rs`: per-monitor image/OCR work selection.
- `crates/karma-ai/src/word_pack.rs`: normalized literal/regex matching and exemptions.
- `crates/karma-ai/src/observation.rs`: conversion from non-sensitive inference outputs to `RiskObservation`.

---

### Task 1: Validate signed-asset metadata

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/karma-ai/Cargo.toml`
- Create: `crates/karma-ai/src/lib.rs`
- Create: `crates/karma-ai/src/manifest.rs`

**Interfaces:**
- Produces: `AssetManifest`, `AssetKind`, `ManifestError`, `AssetManifest::validate()`.

- [ ] **Step 1: Add the crate and write failing tests**

Add `crates/karma-ai` to root workspace members and create its manifest:

```toml
[package]
name = "karma-ai"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
karma-domain = { path = "../karma-domain" }
regex = "1"
serde.workspace = true
thiserror.workspace = true
unicode-normalization = "0.1"

[dev-dependencies]
serde_json.workspace = true
```

Create `src/lib.rs`:

```rust
#![forbid(unsafe_code)]
mod manifest;
pub use manifest::{AssetKind, AssetManifest, ManifestError};
```

Create `manifest.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> AssetManifest {
        AssetManifest {
            kind: AssetKind::ImageClassifier,
            version: "image-v1".into(),
            license: "Apache-2.0".into(),
            sha256: "a".repeat(64),
        }
    }

    #[test]
    fn valid_manifest_passes() { assert_eq!(valid().validate(), Ok(())); }

    #[test]
    fn rejects_missing_license() {
        let mut value = valid();
        value.license.clear();
        assert_eq!(value.validate(), Err(ManifestError::MissingLicense));
    }

    #[test]
    fn rejects_non_hex_sha256() {
        let mut value = valid();
        value.sha256 = "z".repeat(64);
        assert_eq!(value.validate(), Err(ManifestError::InvalidSha256));
    }
}
```

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-ai manifest`

Expected: compilation fails because manifest types are undefined.

- [ ] **Step 3: Implement manifest validation**

Add above the tests:

```rust
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind { ImageClassifier, OcrDetector, OcrRecognizer, WordPack }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetManifest {
    pub kind: AssetKind,
    pub version: String,
    pub license: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ManifestError {
    #[error("asset version is required")] MissingVersion,
    #[error("asset license is required")] MissingLicense,
    #[error("asset SHA-256 must contain 64 lowercase hexadecimal characters")] InvalidSha256,
}

impl AssetManifest {
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.version.trim().is_empty() { return Err(ManifestError::MissingVersion); }
        if self.license.trim().is_empty() { return Err(ManifestError::MissingLicense); }
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|value| value.is_ascii_hexdigit() && !value.is_ascii_uppercase()) {
            return Err(ManifestError::InvalidSha256);
        }
        Ok(())
    }
}
```

- [ ] **Step 4: Verify and commit**

Run: `cargo fmt && cargo test -p karma-ai && cargo clippy -p karma-ai --all-targets -- -D warnings`

Expected: three tests pass and Clippy has no warnings.

```bash
git add Cargo.toml Cargo.lock crates/karma-ai
git commit -m "feat: validate AI asset manifests"
```

---

### Task 2: Schedule image and OCR work independently per monitor

**Files:**
- Modify: `crates/karma-ai/src/lib.rs`
- Create: `crates/karma-ai/src/scheduler.rs`

**Interfaces:**
- Produces: `FrameMetadata`, `FrameWork`, `FrameScheduler::select(FrameMetadata)`.
- Image interval: 500ms. OCR interval: 1000ms and changed fingerprint.

- [ ] **Step 1: Write failing scheduler tests**

Create `scheduler.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use karma_domain::MonitorId;

    fn frame(monitor: &str, at: i64, fingerprint: u64) -> FrameMetadata {
        FrameMetadata { monitor_id: MonitorId(monitor.into()), captured_at_ms: at, fingerprint }
    }

    #[test]
    fn first_frame_runs_image_and_ocr() {
        assert_eq!(FrameScheduler::default().select(frame("a", 1000, 1)), FrameWork { run_image: true, run_ocr: true });
    }

    #[test]
    fn image_is_capped_at_two_fps() {
        let mut value = FrameScheduler::default();
        value.select(frame("a", 1000, 1));
        assert!(!value.select(frame("a", 1499, 2)).run_image);
        assert!(value.select(frame("a", 1500, 3)).run_image);
    }

    #[test]
    fn ocr_requires_one_second_and_changed_frame() {
        let mut value = FrameScheduler::default();
        value.select(frame("a", 1000, 1));
        assert!(!value.select(frame("a", 2000, 1)).run_ocr);
        assert!(value.select(frame("a", 2001, 2)).run_ocr);
    }

    #[test]
    fn monitors_have_independent_state() {
        let mut value = FrameScheduler::default();
        value.select(frame("a", 1000, 1));
        assert_eq!(value.select(frame("b", 1100, 1)), FrameWork { run_image: true, run_ocr: true });
    }
}
```

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-ai scheduler`

Expected: missing scheduler types cause compilation failure.

- [ ] **Step 3: Implement per-monitor scheduling**

Add above the tests:

```rust
use std::collections::HashMap;
use karma_domain::MonitorId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameMetadata { pub monitor_id: MonitorId, pub captured_at_ms: i64, pub fingerprint: u64 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameWork { pub run_image: bool, pub run_ocr: bool }
#[derive(Debug, Clone, Copy)]
struct MonitorSchedule { image_at_ms: i64, ocr_at_ms: i64, ocr_fingerprint: u64 }
#[derive(Debug, Default)]
pub struct FrameScheduler { monitors: HashMap<MonitorId, MonitorSchedule> }

impl FrameScheduler {
    pub fn select(&mut self, frame: FrameMetadata) -> FrameWork {
        let Some(previous) = self.monitors.get_mut(&frame.monitor_id) else {
            self.monitors.insert(frame.monitor_id, MonitorSchedule {
                image_at_ms: frame.captured_at_ms, ocr_at_ms: frame.captured_at_ms,
                ocr_fingerprint: frame.fingerprint,
            });
            return FrameWork { run_image: true, run_ocr: true };
        };
        let run_image = frame.captured_at_ms - previous.image_at_ms >= 500;
        let run_ocr = frame.captured_at_ms - previous.ocr_at_ms >= 1000
            && frame.fingerprint != previous.ocr_fingerprint;
        if run_image { previous.image_at_ms = frame.captured_at_ms; }
        if run_ocr {
            previous.ocr_at_ms = frame.captured_at_ms;
            previous.ocr_fingerprint = frame.fingerprint;
        }
        FrameWork { run_image, run_ocr }
    }
}
```

Add to `lib.rs`:

```rust
mod scheduler;
pub use scheduler::{FrameMetadata, FrameScheduler, FrameWork};
```

- [ ] **Step 4: Verify and commit**

Run: `cargo fmt && cargo test -p karma-ai && cargo clippy -p karma-ai --all-targets -- -D warnings`

Expected: seven cumulative tests pass.

```bash
git add crates/karma-ai
git commit -m "feat: schedule per-monitor inference work"
```

---

### Task 3: Convert ephemeral OCR lines into risk categories

**Files:**
- Modify: `crates/karma-ai/src/lib.rs`
- Create: `crates/karma-ai/src/word_pack.rs`

**Interfaces:**
- Produces: `WordRule`, `WordRuleKind`, `WordPack`, `OcrMatchSummary`, `WordPackError`.
- Produces: `WordPack::compile(rules)` and `classify(&[&str])`.

- [ ] **Step 1: Write failing normalization, regex, and exemption tests**

Create `word_pack.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_matching_normalizes_width_and_case() {
        let pack = WordPack::compile(vec![WordRule::literal("adult_service", "ＡＤＵＬＴ", OcrRisk::Keyword)]).unwrap();
        let result = pack.classify(&["adult"]);
        assert_eq!(result.risk, OcrRisk::Keyword);
        assert_eq!(result.categories, vec!["adult_service"]);
    }

    #[test]
    fn regex_can_mark_high_risk_phrase() {
        let pack = WordPack::compile(vec![WordRule::regex("explicit_term", r"explicit\s+phrase", OcrRisk::HighRiskPhrase)]).unwrap();
        assert_eq!(pack.classify(&["Explicit phrase"]).risk, OcrRisk::HighRiskPhrase);
    }

    #[test]
    fn exemption_is_reported_without_raw_text() {
        let pack = WordPack::compile(vec![WordRule::exemption("medical", "anatomy")]).unwrap();
        let value = pack.classify(&["Anatomy lesson"]);
        assert!(value.exemption_context);
        assert_eq!(value.categories, vec!["medical"]);
        assert!(!serde_json::to_string(&value).unwrap().contains("anatomy lesson"));
    }

    #[test]
    fn invalid_regex_is_rejected() {
        assert!(matches!(WordPack::compile(vec![WordRule::regex("bad", "(", OcrRisk::Keyword)]), Err(WordPackError::InvalidRegex { .. })));
    }
}
```

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-ai word_pack`

Expected: missing word-pack types cause compilation failure.

- [ ] **Step 3: Implement compiled matching**

```rust
use regex::{Regex, RegexBuilder};
use serde::Serialize;
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;
use karma_domain::OcrRisk;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordRuleKind { Literal, Regex, Exemption }

pub struct WordRule { pub category: String, pub pattern: String, pub kind: WordRuleKind, pub risk: OcrRisk }

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OcrMatchSummary { pub risk: OcrRisk, pub categories: Vec<String>, pub exemption_context: bool }

enum CompiledRule {
    Literal { category: String, value: String, risk: OcrRisk },
    Regex { category: String, value: Regex, risk: OcrRisk },
    Exemption { category: String, value: String },
}

pub struct WordPack { rules: Vec<CompiledRule> }

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WordPackError {
    #[error("invalid regex in category {category}")]
    InvalidRegex { category: String },
}

impl WordRule {
    pub fn literal(category: &str, pattern: &str, risk: OcrRisk) -> Self {
        Self { category: category.into(), pattern: pattern.into(), kind: WordRuleKind::Literal, risk }
    }
    pub fn regex(category: &str, pattern: &str, risk: OcrRisk) -> Self {
        Self { category: category.into(), pattern: pattern.into(), kind: WordRuleKind::Regex, risk }
    }
    pub fn exemption(category: &str, pattern: &str) -> Self {
        Self { category: category.into(), pattern: pattern.into(), kind: WordRuleKind::Exemption, risk: OcrRisk::None }
    }
}

fn normalize(value: &str) -> String {
    value.nfkc().flat_map(char::to_lowercase).collect()
}

fn risk_rank(value: OcrRisk) -> u8 {
    match value { OcrRisk::None => 0, OcrRisk::Keyword => 1, OcrRisk::HighRiskPhrase => 2 }
}

impl WordPack {
    pub fn compile(rules: Vec<WordRule>) -> Result<Self, WordPackError> {
        let rules = rules.into_iter().map(|rule| match rule.kind {
            WordRuleKind::Literal => Ok(CompiledRule::Literal {
                category: rule.category, value: normalize(&rule.pattern), risk: rule.risk,
            }),
            WordRuleKind::Exemption => Ok(CompiledRule::Exemption {
                category: rule.category, value: normalize(&rule.pattern),
            }),
            WordRuleKind::Regex => RegexBuilder::new(&rule.pattern)
                .case_insensitive(true).unicode(true).build()
                .map(|value| CompiledRule::Regex { category: rule.category.clone(), value, risk: rule.risk })
                .map_err(|_| WordPackError::InvalidRegex { category: rule.category }),
        }).collect::<Result<Vec<_>, _>>()?;
        Ok(Self { rules })
    }

    pub fn classify(&self, lines: &[&str]) -> OcrMatchSummary {
        let mut risk = OcrRisk::None;
        let mut categories = Vec::new();
        let mut exemption_context = false;
        for line in lines {
            let normalized = normalize(line);
            for rule in &self.rules {
                let (matched, category, rule_risk, exemption) = match rule {
                    CompiledRule::Literal { category, value, risk } =>
                        (normalized.contains(value), category, *risk, false),
                    CompiledRule::Regex { category, value, risk } =>
                        (value.is_match(&normalized), category, *risk, false),
                    CompiledRule::Exemption { category, value } =>
                        (normalized.contains(value), category, OcrRisk::None, true),
                };
                if matched {
                    categories.push(category.clone());
                    exemption_context |= exemption;
                    if risk_rank(rule_risk) > risk_rank(risk) { risk = rule_risk; }
                }
            }
        }
        categories.sort();
        categories.dedup();
        OcrMatchSummary { risk, categories, exemption_context }
    }
}
```

Add to `lib.rs`:

```rust
mod word_pack;
pub use word_pack::{OcrMatchSummary, WordPack, WordPackError, WordRule, WordRuleKind};
```

- [ ] **Step 4: Verify and commit**

Run: `cargo fmt && cargo test -p karma-ai && cargo clippy -p karma-ai --all-targets -- -D warnings`

Expected: eleven cumulative tests pass and no production type stores OCR text.

```bash
git add crates/karma-ai Cargo.lock
git commit -m "feat: classify OCR keyword risk"
```

---

### Task 4: Assemble privacy-safe risk observations

**Files:**
- Modify: `crates/karma-ai/src/lib.rs`
- Create: `crates/karma-ai/src/observation.rs`
- Create: `crates/karma-ai/tests/observation_pipeline.rs`

**Interfaces:**
- Produces: `ImageInference`, `ObservationInput`, `ObservationAssembler::assemble`.
- Consumes: `OcrMatchSummary` and stable `karma-domain` types.

- [ ] **Step 1: Write failing end-to-end assembly tests**

Create `tests/observation_pipeline.rs`:

```rust
use karma_ai::{ImageInference, ObservationAssembler, ObservationInput, OcrMatchSummary};
use karma_domain::{MonitorId, OcrRisk, RiskCategory};

#[test]
fn assembly_maps_categories_without_text() {
    let value = ObservationAssembler::assemble(ObservationInput {
        monitor_id: MonitorId("display-1".into()), captured_at_ms: 42,
        image: ImageInference { score_millis: 700, categories: vec![RiskCategory::Suggestive] },
        ocr: OcrMatchSummary { risk: OcrRisk::HighRiskPhrase,
            categories: vec!["explicit_term".into()], exemption_context: false },
        image_model_version: "i1".into(), ocr_model_version: "o1".into(), word_pack_version: "w1".into(),
    });
    assert_eq!(value.ocr_categories, vec![RiskCategory::ExplicitTerm]);
    let json = serde_json::to_string(&value).unwrap();
    assert!(!json.contains("recognized_text"));
    assert!(!json.contains("raw_text"));
}

#[test]
fn exemption_context_is_preserved_as_category() {
    let value = ObservationAssembler::assemble(ObservationInput {
        monitor_id: MonitorId("display-1".into()), captured_at_ms: 42,
        image: ImageInference { score_millis: 0, categories: vec![] },
        ocr: OcrMatchSummary { risk: OcrRisk::None, categories: vec!["medical".into()], exemption_context: true },
        image_model_version: "i1".into(), ocr_model_version: "o1".into(), word_pack_version: "w1".into(),
    });
    assert!(value.ocr_categories.contains(&RiskCategory::ExemptionContext));
}
```

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-ai --test observation_pipeline`

Expected: observation assembler types are undefined.

- [ ] **Step 3: Implement observation assembly**

```rust
use karma_domain::{ModelVersions, MonitorId, RiskCategory, RiskObservation};
use crate::OcrMatchSummary;

pub struct ImageInference { pub score_millis: u16, pub categories: Vec<RiskCategory> }
pub struct ObservationInput {
    pub monitor_id: MonitorId, pub captured_at_ms: i64, pub image: ImageInference,
    pub ocr: OcrMatchSummary, pub image_model_version: String,
    pub ocr_model_version: String, pub word_pack_version: String,
}
pub struct ObservationAssembler;

fn category_rank(value: &RiskCategory) -> u8 {
    match value {
        RiskCategory::Nudity => 0,
        RiskCategory::Suggestive => 1,
        RiskCategory::ExplicitTerm => 2,
        RiskCategory::AdultService => 3,
        RiskCategory::ExemptionContext => 4,
    }
}

impl ObservationAssembler {
    pub fn assemble(input: ObservationInput) -> RiskObservation {
        let mut ocr_categories = input.ocr.categories.iter().filter_map(|value| match value.as_str() {
            "explicit_term" => Some(RiskCategory::ExplicitTerm),
            "adult_service" => Some(RiskCategory::AdultService),
            _ => None,
        }).collect::<Vec<_>>();
        if input.ocr.exemption_context {
            ocr_categories.push(RiskCategory::ExemptionContext);
        }
        ocr_categories.sort_by_key(category_rank);
        ocr_categories.dedup();

        RiskObservation {
            monitor_id: input.monitor_id,
            captured_at_ms: input.captured_at_ms,
            image_score_millis: input.image.score_millis,
            image_labels: input.image.categories,
            ocr_risk: input.ocr.risk,
            ocr_categories,
            source_identity: None,
            model_versions: ModelVersions {
                image: input.image_model_version,
                ocr: input.ocr_model_version,
                word_pack: input.word_pack_version,
            },
        }
    }
}
```

Add to `lib.rs`:

```rust
mod observation;
pub use observation::{ImageInference, ObservationAssembler, ObservationInput};
```

- [ ] **Step 4: Run full quality gates and privacy scan**

Run:

```bash
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
rg -n "raw_text|recognized_text|ocr_text|screenshot" crates
```

Expected: all tests pass, Clippy is clean, and sensitive terms occur only in negative assertions or test names.

- [ ] **Step 5: Commit**

```bash
git add crates/karma-ai Cargo.lock
git commit -m "feat: assemble privacy-safe risk observations"
```

## Completion Check

Run `cargo fmt --check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `git diff --check`, and `git status --short`.

The next plan adds the Windows Agent executable, WGC capture session abstraction, foreground-window attribution, and Windows-target compile checks. Actual screen-capture runtime verification remains a Windows test-machine requirement.
