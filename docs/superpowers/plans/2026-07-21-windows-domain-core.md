# Karma Windows Domain Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the platform-independent Rust domain core for Karma's Windows MVP: versioned observations, weekly schedules, application rules, image/OCR risk fusion, deterministic decisions, and privacy-safe audit events.

**Architecture:** A Cargo workspace contains two focused libraries. `karma-domain` owns stable serializable values shared by future Agent, Service, storage, and IPC components; `karma-policy` consumes those values and implements pure deterministic policy evaluation with caller-supplied timestamps. This slice has no Windows API, database, ONNX runtime, async runtime, network, or UI dependency.

**Tech Stack:** Rust 1.85+, edition 2024, Cargo, `serde`, `serde_json`, `thiserror`, Rust built-in test framework.

## Global Constraints

- Target platforms remain Windows 10 22H2 and Windows 11, but this pure slice must compile on the development host.
- OCR text is ephemeral; domain and audit types may contain category identifiers but never OCR raw text.
- OCR-only matches never produce `Terminate`; they may produce `Warn`.
- All IPC-facing structures carry schema version 1.
- Time values are Unix milliseconds; weekly schedules use Monday-based minute offsets in `0..10080` at 15-minute granularity.
- No unsafe Rust and no production dependency beyond those named in Tech Stack.
- Every task follows red-green-refactor TDD and ends with a focused commit.

## File Map

- `Cargo.toml`: workspace membership and shared dependencies.
- `rust-toolchain.toml`: Rust channel and quality components.
- `crates/karma-domain/src/`: stable observation, decision, identity, and audit values.
- `crates/karma-policy/src/schedule.rs`: weekly schedule evaluation.
- `crates/karma-policy/src/application.rs`: application rule matching.
- `crates/karma-policy/src/risk.rs`: per-monitor risk state.
- `crates/karma-policy/src/engine.rs`: deterministic composition.
- `crates/karma-policy/tests/decision_scenarios.rs`: cross-module acceptance tests.

---

### Task 1: Bootstrap the workspace and versioned domain envelope

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `crates/karma-domain/Cargo.toml`
- Create: `crates/karma-domain/src/lib.rs`
- Create: `crates/karma-domain/src/observation.rs`

**Interfaces:**
- Produces: `SCHEMA_VERSION`, `Versioned<T>`, and `OcrRisk`.

- [ ] **Step 1: Verify or install Rust**

Run: `cargo --version`

Expected now: command not found. With explicit download approval, install official Rust 1.85+ and run:

```bash
rustup component add rustfmt clippy
cargo --version
```

Expected after installation: Cargo 1.85.0 or newer.

- [ ] **Step 2: Create workspace manifests**

Create root `Cargo.toml`:

```toml
[workspace]
members = ["crates/karma-domain"]
resolver = "2"

[workspace.package]
edition = "2024"
rust-version = "1.85"
license = "MIT"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
```

Create `rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.85"
components = ["rustfmt", "clippy"]
profile = "minimal"
```

Create `crates/karma-domain/Cargo.toml`:

```toml
[package]
name = "karma-domain"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
serde.workspace = true

[dev-dependencies]
serde_json.workspace = true
```

- [ ] **Step 3: Write the failing serialization test**

Create `crates/karma-domain/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

mod observation;
pub use observation::{OcrRisk, Versioned, SCHEMA_VERSION};
```

Create `observation.rs` with this test:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versioned_payload_serializes_schema_version() {
        let json = serde_json::to_value(Versioned::new(OcrRisk::Keyword)).unwrap();
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["payload"], "keyword");
    }
}
```

- [ ] **Step 4: Run RED**

Run: `cargo test -p karma-domain versioned_payload_serializes_schema_version`

Expected: compilation fails because the exported types are undefined.

- [ ] **Step 5: Implement the envelope**

Add above the test in `observation.rs`:

```rust
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Versioned<T> { pub schema_version: u16, pub payload: T }

impl<T> Versioned<T> {
    pub fn new(payload: T) -> Self { Self { schema_version: SCHEMA_VERSION, payload } }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrRisk { None, Keyword, HighRiskPhrase }
```

- [ ] **Step 6: Verify and commit**

Run: `cargo fmt --check && cargo test -p karma-domain && cargo clippy -p karma-domain --all-targets -- -D warnings`

Expected: one test passes and Clippy has no warnings.

```bash
git add Cargo.toml rust-toolchain.toml crates/karma-domain
git commit -m "feat: bootstrap versioned domain core"
```

---

### Task 2: Add privacy-safe domain values

**Files:**
- Modify: `crates/karma-domain/src/lib.rs`
- Modify: `crates/karma-domain/src/observation.rs`
- Create: `crates/karma-domain/src/identity.rs`
- Create: `crates/karma-domain/src/decision.rs`
- Create: `crates/karma-domain/src/audit.rs`

**Interfaces:**
- Produces: `MonitorId`, `SourceIdentity`, `RiskObservation`, `RiskCategory`, `ModelVersions`.
- Produces: `Action`, `ReasonCode`, `Decision`, `AuditEvent`, `AuditKind`.

- [ ] **Step 1: Write failing privacy-shape tests**

Add to `observation.rs` tests:

```rust
#[test]
fn observation_serializes_categories_without_raw_text() {
    let value = RiskObservation::test_value(640, OcrRisk::HighRiskPhrase);
    let json = serde_json::to_string(&value).unwrap();
    assert!(json.contains("explicit_term"));
    assert!(!json.contains("raw_text"));
    assert!(!json.contains("recognized_text"));
}
```

Create `audit.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_event_has_no_sensitive_payload_fields() {
        let event = AuditEvent::decision(
            42, "S-1-test", None, None, ReasonCode::OcrImageCombined, Action::Warn,
        );
        let json = serde_json::to_string(&event).unwrap();
        assert!(!json.contains("screenshot"));
        assert!(!json.contains("ocr_text"));
    }
}
```

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-domain`

Expected: compilation fails for missing domain values and constructors.

- [ ] **Step 3: Implement identity and decision values**

Create `identity.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MonitorId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceIdentity {
    pub pid: u32,
    pub started_at_ms: i64,
    pub executable_path: String,
    pub publisher: Option<String>,
    pub sha256: Option<String>,
}
```

Create `decision.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action { Allow, Warn, CloseGracefully, Terminate, BlockNetwork }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    DefaultAllow, TimeWindowBlocked, ApplicationBlocked, ImageImmediate,
    ImageRepeated, OcrImageCombined, OcrOnlyWarning, SourceUncertain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub action: Action,
    pub reason: ReasonCode,
    pub policy_id: String,
    pub expires_at_ms: Option<i64>,
}
```

- [ ] **Step 4: Implement observations and audit values**

Add to `observation.rs`:

```rust
use crate::{MonitorId, SourceIdentity};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskCategory { Nudity, Suggestive, ExplicitTerm, AdultService, ExemptionContext }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelVersions { pub image: String, pub ocr: String, pub word_pack: String }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskObservation {
    pub monitor_id: MonitorId,
    pub captured_at_ms: i64,
    pub image_score_millis: u16,
    pub image_labels: Vec<RiskCategory>,
    pub ocr_risk: OcrRisk,
    pub ocr_categories: Vec<RiskCategory>,
    pub source_identity: Option<SourceIdentity>,
    pub model_versions: ModelVersions,
}

#[cfg(test)]
impl RiskObservation {
    fn test_value(score: u16, ocr_risk: OcrRisk) -> Self {
        Self {
            monitor_id: MonitorId("display-1".into()), captured_at_ms: 1,
            image_score_millis: score, image_labels: vec![RiskCategory::Suggestive],
            ocr_risk, ocr_categories: vec![RiskCategory::ExplicitTerm], source_identity: None,
            model_versions: ModelVersions { image: "i1".into(), ocr: "o1".into(), word_pack: "w1".into() },
        }
    }
}
```

Create `audit.rs`:

```rust
use serde::{Deserialize, Serialize};
use crate::{Action, ReasonCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditKind { DecisionApplied, AuthenticationFailed, ComponentHealth }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub occurred_at_ms: i64,
    pub user_sid: String,
    pub monitor_id: Option<String>,
    pub application_id: Option<String>,
    pub kind: AuditKind,
    pub reason: Option<ReasonCode>,
    pub action: Option<Action>,
}

impl AuditEvent {
    pub fn decision(at: i64, sid: &str, monitor_id: Option<String>, application_id: Option<String>,
        reason: ReasonCode, action: Action) -> Self {
        Self { occurred_at_ms: at, user_sid: sid.into(), monitor_id, application_id,
            kind: AuditKind::DecisionApplied,
            reason: Some(reason), action: Some(action) }
    }
}
```

Replace `lib.rs` exports with:

```rust
#![forbid(unsafe_code)]
mod audit; mod decision; mod identity; mod observation;
pub use audit::{AuditEvent, AuditKind};
pub use decision::{Action, Decision, ReasonCode};
pub use identity::{MonitorId, SourceIdentity};
pub use observation::{ModelVersions, OcrRisk, RiskCategory, RiskObservation, Versioned, SCHEMA_VERSION};
```

- [ ] **Step 5: Verify and commit**

Run: `cargo fmt && cargo test -p karma-domain && cargo clippy -p karma-domain --all-targets -- -D warnings`

Expected: all tests pass and production types have no raw OCR or screenshot fields.

```bash
git add crates/karma-domain
git commit -m "feat: add privacy-safe domain values"
```

---

### Task 3: Implement weekly schedules

**Files:**
- Create: `crates/karma-policy/Cargo.toml`
- Create: `crates/karma-policy/src/lib.rs`
- Create: `crates/karma-policy/src/schedule.rs`

**Interfaces:**
- Produces: `WeeklySchedule::new(id, ranges)` and `is_blocked(minute_of_week)`.
- Ranges are inclusive-start/exclusive-end and may wrap across Sunday midnight.

- [ ] **Step 1: Create the policy crate manifest**

First change the root workspace membership to:

```toml
members = ["crates/karma-domain", "crates/karma-policy"]
```

```toml
[package]
name = "karma-policy"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
karma-domain = { path = "../karma-domain" }
serde.workspace = true
thiserror.workspace = true

[dev-dependencies]
serde_json.workspace = true
```

Create `src/lib.rs`:

```rust
#![forbid(unsafe_code)]
mod schedule;
pub use schedule::{MinuteRange, ScheduleError, WeeklySchedule};
```

- [ ] **Step 2: Write failing schedule tests**

Create `schedule.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_is_start_inclusive_and_end_exclusive() {
        let value = WeeklySchedule::new("bedtime", vec![MinuteRange { start: 120, end: 180 }]).unwrap();
        assert!(value.is_blocked(120));
        assert!(value.is_blocked(179));
        assert!(!value.is_blocked(180));
    }

    #[test]
    fn rejects_non_quarter_hour_boundaries() {
        let error = WeeklySchedule::new("bad", vec![MinuteRange { start: 1, end: 30 }]).unwrap_err();
        assert_eq!(error, ScheduleError::NotQuarterHourAligned);
    }

    #[test]
    fn supports_week_boundary_wraparound() {
        let value = WeeklySchedule::new("weekend", vec![MinuteRange { start: 10020, end: 60 }]).unwrap();
        assert!(value.is_blocked(10050));
        assert!(value.is_blocked(30));
        assert!(!value.is_blocked(600));
    }
}
```

- [ ] **Step 3: Run RED**

Run: `cargo test -p karma-policy schedule`

Expected: missing schedule types cause compilation failure.

- [ ] **Step 4: Implement validated schedules**

Add above the tests:

```rust
use serde::{Deserialize, Serialize};
use thiserror::Error;
const MINUTES_PER_WEEK: u16 = 10080;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinuteRange { pub start: u16, pub end: u16 }

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScheduleError {
    #[error("minute is outside the week")] OutOfRange,
    #[error("schedule boundaries must align to 15 minutes")] NotQuarterHourAligned,
    #[error("empty ranges are not allowed")] EmptyRange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeeklySchedule { pub id: String, ranges: Vec<MinuteRange> }

impl WeeklySchedule {
    pub fn new(id: impl Into<String>, ranges: Vec<MinuteRange>) -> Result<Self, ScheduleError> {
        for range in &ranges {
            if range.start >= MINUTES_PER_WEEK || range.end > MINUTES_PER_WEEK { return Err(ScheduleError::OutOfRange); }
            if range.start % 15 != 0 || range.end % 15 != 0 { return Err(ScheduleError::NotQuarterHourAligned); }
            if range.start == range.end { return Err(ScheduleError::EmptyRange); }
        }
        Ok(Self { id: id.into(), ranges })
    }

    pub fn is_blocked(&self, minute: u16) -> bool {
        minute < MINUTES_PER_WEEK && self.ranges.iter().any(|range| {
            if range.start < range.end { (range.start..range.end).contains(&minute) }
            else { minute >= range.start || minute < range.end }
        })
    }
}
```

- [ ] **Step 5: Verify and commit**

Run: `cargo fmt && cargo test -p karma-policy && cargo clippy -p karma-policy --all-targets -- -D warnings`

Expected: three schedule tests pass.

```bash
git add Cargo.toml crates/karma-policy
git commit -m "feat: add weekly schedule policy"
```

---

### Task 4: Implement deterministic application rules

**Files:**
- Modify: `crates/karma-policy/src/lib.rs`
- Create: `crates/karma-policy/src/application.rs`

**Interfaces:**
- Produces: `ApplicationFacts`, `ApplicationRule`, `RuleEffect`, and `resolve_application`.
- Higher priority wins; equal priority uses lexicographically smaller rule ID.

- [ ] **Step 1: Write failing resolution tests**

Create `application.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn facts() -> ApplicationFacts {
        ApplicationFacts { normalized_path: r"c:\browser\browser.exe".into(),
            publisher: Some("Browser Ltd".into()), sha256: Some("abc".into()) }
    }

    #[test]
    fn higher_priority_match_wins() {
        let rules = vec![
            ApplicationRule::path("allow", 10, "browser.exe", RuleEffect::Allow),
            ApplicationRule::publisher("block", 20, "Browser Ltd", RuleEffect::Block),
        ];
        assert_eq!(resolve_application(&rules, &facts()).unwrap().id, "block");
    }

    #[test]
    fn equal_priority_uses_stable_id_order() {
        let rules = vec![
            ApplicationRule::hash("z", 10, "abc", RuleEffect::Block),
            ApplicationRule::hash("a", 10, "abc", RuleEffect::Allow),
        ];
        assert_eq!(resolve_application(&rules, &facts()).unwrap().id, "a");
    }
}
```

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-policy application`

Expected: missing application types cause compilation failure.

- [ ] **Step 3: Implement matching and stable priority**

Add above the tests:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationFacts { pub normalized_path: String, pub publisher: Option<String>, pub sha256: Option<String> }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleEffect { Allow, Block }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApplicationMatcher { PathSuffix(String), Publisher(String), Sha256(String) }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationRule { pub id: String, pub priority: i32, pub matcher: ApplicationMatcher, pub effect: RuleEffect }

impl ApplicationRule {
    pub fn path(id: &str, priority: i32, value: &str, effect: RuleEffect) -> Self {
        Self { id: id.into(), priority, matcher: ApplicationMatcher::PathSuffix(value.into()), effect }
    }
    pub fn publisher(id: &str, priority: i32, value: &str, effect: RuleEffect) -> Self {
        Self { id: id.into(), priority, matcher: ApplicationMatcher::Publisher(value.into()), effect }
    }
    pub fn hash(id: &str, priority: i32, value: &str, effect: RuleEffect) -> Self {
        Self { id: id.into(), priority, matcher: ApplicationMatcher::Sha256(value.into()), effect }
    }
    fn matches(&self, facts: &ApplicationFacts) -> bool {
        match &self.matcher {
            ApplicationMatcher::PathSuffix(value) => facts.normalized_path.ends_with(value),
            ApplicationMatcher::Publisher(value) => facts.publisher.as_ref() == Some(value),
            ApplicationMatcher::Sha256(value) => facts.sha256.as_ref() == Some(value),
        }
    }
}

pub fn resolve_application<'a>(rules: &'a [ApplicationRule], facts: &ApplicationFacts) -> Option<&'a ApplicationRule> {
    rules.iter().filter(|rule| rule.matches(facts)).max_by(|left, right| {
        left.priority.cmp(&right.priority).then_with(|| right.id.cmp(&left.id))
    })
}
```

Add to `lib.rs`:

```rust
mod application;
pub use application::{resolve_application, ApplicationFacts, ApplicationMatcher, ApplicationRule, RuleEffect};
```

- [ ] **Step 4: Verify and commit**

Run: `cargo fmt && cargo test -p karma-policy && cargo clippy -p karma-policy --all-targets -- -D warnings`

Expected: application and schedule tests pass.

```bash
git add crates/karma-policy
git commit -m "feat: resolve application policy rules"
```

---

### Task 5: Implement per-monitor image and OCR risk fusion

**Files:**
- Modify: `crates/karma-policy/src/lib.rs`
- Create: `crates/karma-policy/src/risk.rs`

**Interfaces:**
- Produces: `RiskState::observe(RiskObservation) -> RiskOutcome`.
- Histories are isolated by `MonitorId` and pruned after 10 seconds.

- [ ] **Step 1: Write failing risk tests**

Create `risk.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use karma_domain::{ModelVersions, RiskCategory};

    fn obs(monitor: &str, at: i64, score: u16, ocr: OcrRisk) -> RiskObservation {
        RiskObservation {
            monitor_id: MonitorId(monitor.into()), captured_at_ms: at, image_score_millis: score,
            image_labels: vec![RiskCategory::Suggestive], ocr_risk: ocr, ocr_categories: vec![],
            source_identity: None,
            model_versions: ModelVersions { image: "i1".into(), ocr: "o1".into(), word_pack: "w1".into() },
        }
    }

    #[test]
    fn immediate_image_threshold_closes() {
        let result = RiskState::default().observe(obs("a", 1000, 950, OcrRisk::None));
        assert_eq!(result, RiskOutcome { action: Action::CloseGracefully, reason: ReasonCode::ImageImmediate });
    }

    #[test]
    fn three_image_hits_inside_five_seconds_close() {
        let mut state = RiskState::default();
        state.observe(obs("a", 1000, 820, OcrRisk::None));
        state.observe(obs("a", 3000, 830, OcrRisk::None));
        assert_eq!(state.observe(obs("a", 5000, 840, OcrRisk::None)).reason, ReasonCode::ImageRepeated);
    }

    #[test]
    fn ocr_only_warns_but_two_combined_hits_close() {
        let mut state = RiskState::default();
        assert_eq!(state.observe(obs("a", 1000, 200, OcrRisk::HighRiskPhrase)).action, Action::Warn);
        state.observe(obs("b", 2000, 650, OcrRisk::HighRiskPhrase));
        assert_eq!(state.observe(obs("b", 3000, 660, OcrRisk::HighRiskPhrase)).reason, ReasonCode::OcrImageCombined);
    }

    #[test]
    fn monitors_do_not_share_history() {
        let mut state = RiskState::default();
        state.observe(obs("a", 1000, 900, OcrRisk::None));
        state.observe(obs("a", 2000, 900, OcrRisk::None));
        assert_eq!(state.observe(obs("b", 3000, 900, OcrRisk::None)).action, Action::Allow);
    }

    #[test]
    fn exemption_context_suppresses_combined_ocr_rule() {
        let mut state = RiskState::default();
        let mut first = obs("a", 1000, 650, OcrRisk::HighRiskPhrase);
        first.ocr_categories.push(RiskCategory::ExemptionContext);
        let mut second = obs("a", 2000, 660, OcrRisk::HighRiskPhrase);
        second.ocr_categories.push(RiskCategory::ExemptionContext);
        state.observe(first);
        assert_eq!(state.observe(second).action, Action::Warn);
    }
}
```

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-policy risk`

Expected: missing risk types cause compilation failure.

- [ ] **Step 3: Implement the sliding window**

Add above the tests:

```rust
use std::collections::{HashMap, VecDeque};
use karma_domain::{Action, MonitorId, OcrRisk, ReasonCode, RiskCategory, RiskObservation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RiskOutcome { pub action: Action, pub reason: ReasonCode }
#[derive(Debug, Default)]
pub struct RiskState { histories: HashMap<MonitorId, VecDeque<RiskObservation>> }

impl RiskState {
    pub fn observe(&mut self, observation: RiskObservation) -> RiskOutcome {
        let now = observation.captured_at_ms;
        let history = self.histories.entry(observation.monitor_id.clone()).or_default();
        history.retain(|item| now - item.captured_at_ms <= 10_000);
        history.push_back(observation);
        let current = history.back().expect("inserted observation exists");
        if current.image_score_millis >= 950 {
            return RiskOutcome { action: Action::CloseGracefully, reason: ReasonCode::ImageImmediate };
        }
        if history.iter().filter(|item| now - item.captured_at_ms <= 5_000 && item.image_score_millis >= 820).count() >= 3 {
            return RiskOutcome { action: Action::CloseGracefully, reason: ReasonCode::ImageRepeated };
        }
        if history.iter().filter(|item| now - item.captured_at_ms <= 5_000
            && item.image_score_millis >= 650 && item.ocr_risk == OcrRisk::HighRiskPhrase
            && !item.ocr_categories.contains(&RiskCategory::ExemptionContext)).count() >= 2 {
            return RiskOutcome { action: Action::CloseGracefully, reason: ReasonCode::OcrImageCombined };
        }
        if current.ocr_risk != OcrRisk::None {
            return RiskOutcome { action: Action::Warn, reason: ReasonCode::OcrOnlyWarning };
        }
        RiskOutcome { action: Action::Allow, reason: ReasonCode::DefaultAllow }
    }
}
```

Add to `lib.rs`:

```rust
mod risk;
pub use risk::{RiskOutcome, RiskState};
```

- [ ] **Step 4: Verify and commit**

Run: `cargo fmt && cargo test -p karma-policy && cargo clippy -p karma-policy --all-targets -- -D warnings`

Expected: all five risk tests pass, OCR-only produces `Warn`, and exemption context suppresses the combined OCR rule.

```bash
git add crates/karma-policy
git commit -m "feat: fuse image and OCR risk signals"
```

---

### Task 6: Compose decisions and privacy-safe audit events

**Files:**
- Modify: `crates/karma-policy/src/lib.rs`
- Create: `crates/karma-policy/src/engine.rs`
- Create: `crates/karma-policy/tests/decision_scenarios.rs`

**Interfaces:**
- Produces: `DecisionEngine::evaluate(EvaluationInput) -> EvaluationResult`.
- Priority: blocked schedule, blocked application, content risk, explicit allow, default allow.

- [ ] **Step 1: Write failing acceptance scenarios**

Create `tests/decision_scenarios.rs`:

```rust
use karma_domain::{Action, ModelVersions, MonitorId, OcrRisk, RiskCategory, RiskObservation};
use karma_policy::{ApplicationFacts, ApplicationRule, DecisionEngine, EvaluationInput,
    MinuteRange, RuleEffect, WeeklySchedule};

fn obs(score: u16) -> RiskObservation {
    RiskObservation { monitor_id: MonitorId("display-1".into()), captured_at_ms: 10_000,
        image_score_millis: score, image_labels: vec![RiskCategory::Suggestive],
        ocr_risk: OcrRisk::None, ocr_categories: vec![], source_identity: None,
        model_versions: ModelVersions { image: "i1".into(), ocr: "o1".into(), word_pack: "w1".into() } }
}

fn app() -> ApplicationFacts {
    ApplicationFacts { normalized_path: r"c:\browser.exe".into(), publisher: None, sha256: None }
}

#[test]
fn blocked_schedule_wins_and_audit_is_safe() {
    let schedule = WeeklySchedule::new("bedtime", vec![MinuteRange { start: 120, end: 180 }]).unwrap();
    let mut engine = DecisionEngine::new(schedule, vec![]);
    let result = engine.evaluate(EvaluationInput { minute_of_week: 150, user_sid: "S-1-test".into(),
        application: app(), observation: obs(950) });
    assert_eq!(result.decision.action, Action::CloseGracefully);
    assert_eq!(result.decision.policy_id, "bedtime");
    let json = serde_json::to_string(&result.audit).unwrap();
    assert!(!json.contains("ocr_text"));
    assert!(!json.contains("screenshot"));
}

#[test]
fn allow_rule_does_not_override_high_risk_content() {
    let schedule = WeeklySchedule::new("none", vec![]).unwrap();
    let rules = vec![ApplicationRule::path("allow-browser", 10, "browser.exe", RuleEffect::Allow)];
    let mut engine = DecisionEngine::new(schedule, rules);
    let result = engine.evaluate(EvaluationInput { minute_of_week: 200, user_sid: "S-1-test".into(),
        application: app(), observation: obs(950) });
    assert_eq!(result.decision.action, Action::CloseGracefully);
}
```

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-policy --test decision_scenarios`

Expected: missing engine types cause compilation failure.

- [ ] **Step 3: Implement deterministic composition**

Create `engine.rs`:

```rust
use karma_domain::{Action, AuditEvent, Decision, ReasonCode, RiskObservation};
use crate::{resolve_application, ApplicationFacts, ApplicationRule, RiskState, RuleEffect, WeeklySchedule};

pub struct EvaluationInput { pub minute_of_week: u16, pub user_sid: String,
    pub application: ApplicationFacts, pub observation: RiskObservation }
pub struct EvaluationResult { pub decision: Decision, pub audit: AuditEvent }
pub struct DecisionEngine { schedule: WeeklySchedule, rules: Vec<ApplicationRule>, risk: RiskState }

impl DecisionEngine {
    pub fn new(schedule: WeeklySchedule, rules: Vec<ApplicationRule>) -> Self {
        Self { schedule, rules, risk: RiskState::default() }
    }

    pub fn evaluate(&mut self, input: EvaluationInput) -> EvaluationResult {
        let risk = self.risk.observe(input.observation.clone());
        let matched = resolve_application(&self.rules, &input.application);
        let decision = if self.schedule.is_blocked(input.minute_of_week) {
            Decision { action: Action::CloseGracefully, reason: ReasonCode::TimeWindowBlocked,
                policy_id: self.schedule.id.clone(), expires_at_ms: None }
        } else if let Some(rule) = matched.filter(|rule| rule.effect == RuleEffect::Block) {
            Decision { action: Action::CloseGracefully, reason: ReasonCode::ApplicationBlocked,
                policy_id: rule.id.clone(), expires_at_ms: None }
        } else if risk.action != Action::Allow {
            Decision { action: risk.action, reason: risk.reason, policy_id: "content-risk".into(), expires_at_ms: None }
        } else {
            Decision { action: Action::Allow, reason: ReasonCode::DefaultAllow,
                policy_id: matched.map_or_else(|| "default".into(), |rule| rule.id.clone()), expires_at_ms: None }
        };
        let audit = AuditEvent::decision(
            input.observation.captured_at_ms,
            &input.user_sid,
            Some(input.observation.monitor_id.0.clone()),
            Some(input.application.normalized_path.clone()),
            decision.reason,
            decision.action,
        );
        EvaluationResult { decision, audit }
    }
}
```

Add to `lib.rs`:

```rust
mod engine;
pub use engine::{DecisionEngine, EvaluationInput, EvaluationResult};
```

- [ ] **Step 4: Run complete quality gates**

Run:

```bash
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
rg -n "raw_text|recognized_text|ocr_text|screenshot" crates
```

Expected: tests pass, Clippy is clean, and sensitive-field matches occur only in negative assertions.

- [ ] **Step 5: Commit the completed slice**

```bash
git add crates/karma-policy Cargo.lock
git commit -m "feat: compose deterministic policy engine"
```

## Slice 1 Completion Check

Run:

```bash
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
git status --short
```

Expected: formatting, tests, and Clippy pass; Git is clean; no Windows API, ONNX binary, model asset, database, async runtime, network dependency, or UI code has entered the workspace.

The next implementation plan begins with the Windows Agent capture boundary and consumes the stable types produced here.
