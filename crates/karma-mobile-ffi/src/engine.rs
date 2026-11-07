//! The platform-independent decision path. Every input carries its own
//! timestamp, so the same sequence of frames yields the same decisions on
//! desktop tests and on a phone.

use std::collections::BTreeMap;

use karma_ai::{ClassifierOutput, VIDDEXA_LABELS, ViddexaRiskMapper};
use karma_domain::{Action, ModelVersions, MonitorId, OcrRisk, RiskObservation};
use karma_policy::{
    ApplicationEffect, ApplicationPolicy, ContextPolicy, ContextVerdict, NetworkSchedulePolicy,
    RiskState, context_enforcement,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::packages::{
    is_exempt, is_known_browser, is_known_video, normalize_package, with_builtin_applications,
};
use crate::types::{
    ClassifierProbabilities, CoreError, ForegroundContext, MobileAction, MobileDecision,
    MobileReason, PackageClass,
};

pub const IMAGE_MODEL_VERSION: &str = "viddexa-nano-1";
const DEFAULT_COOLDOWN_SECONDS: u64 = 60;
const MIN_COOLDOWN_SECONDS: u64 = 10;
const MAX_COOLDOWN_SECONDS: u64 = 3600;
const MINUTES_PER_WEEK: u32 = 10_080;

/// What a browser window that refuses screenshots leads to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SecureWindowAction {
    /// "If it cannot be seen, it cannot be used" (the default).
    Close,
    Allow,
}

/// A policy document parsed by the shared desktop policy code.
pub struct CompiledPolicy {
    protection_enabled: bool,
    image_enabled: bool,
    applications: ApplicationPolicy,
    context: ContextPolicy,
    schedules: NetworkSchedulePolicy,
    secure_window: SecureWindowAction,
    cooldown_ms: i64,
}

impl CompiledPolicy {
    pub fn compile(policy: &Value) -> Result<Self, CoreError> {
        if !(policy.is_object() || policy.is_null()) {
            return Err(CoreError::InvalidPolicy);
        }
        let merged = with_builtin_applications(policy);
        fn invalid<E>(_: E) -> CoreError {
            CoreError::InvalidPolicy
        }
        let android = merged.get("android");
        let secure_window = match android
            .and_then(|value| value.get("secureWindowBrowserAction"))
            .map(|value| value.as_str())
        {
            None | Some(Some("close")) => SecureWindowAction::Close,
            Some(Some("allow")) => SecureWindowAction::Allow,
            Some(_) => return Err(CoreError::InvalidPolicy),
        };
        let cooldown_seconds = match android.and_then(|value| value.get("cooldownSeconds")) {
            None => DEFAULT_COOLDOWN_SECONDS,
            Some(value) => value
                .as_u64()
                .filter(|seconds| (MIN_COOLDOWN_SECONDS..=MAX_COOLDOWN_SECONDS).contains(seconds))
                .ok_or(CoreError::InvalidPolicy)?,
        };
        Ok(Self {
            protection_enabled: merged
                .get("protectionEnabled")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            image_enabled: merged
                .pointer("/recognition/imageEnabled")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            applications: ApplicationPolicy::from_value(&merged).map_err(invalid)?,
            context: ContextPolicy::from_value(&merged).map_err(invalid)?,
            schedules: NetworkSchedulePolicy::from_value(&schedule_view(&merged))
                .map_err(invalid)?,
            secure_window,
            cooldown_ms: (cooldown_seconds * 1000) as i64,
        })
    }

    pub fn protection_enabled(&self) -> bool {
        self.protection_enabled
    }

    fn is_browser(&self, package: &str) -> bool {
        is_known_browser(package) || self.context.is_browser(package)
    }

    /// Whether an active schedule restricts `package`. Android has no network
    /// filter yet, so both launch and network restrictions close the package.
    /// The `all_network` target is not enforced on Android: it would close
    /// every application on the phone.
    fn schedule_blocks(&self, package: &str, minute_of_week: u32) -> Option<String> {
        let minute = (minute_of_week % MINUTES_PER_WEEK) as u16;
        let restriction = self.schedules.restriction_at(minute);
        if !restriction.is_restricted() {
            return None;
        }
        let suffixed = format!("{package}{SCHEDULE_SUFFIX}");
        let blocked = [package, suffixed.as_str()].into_iter().any(|name| {
            restriction.blocks_launch(name)
                || restriction
                    .network_executables
                    .iter()
                    .any(|executable| executable == name)
        });
        blocked.then(|| {
            restriction
                .rule_ids
                .first()
                .cloned()
                .unwrap_or_else(|| "schedule".into())
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cooldown {
    pub until_ms: i64,
    pub action: MobileAction,
    pub reason: MobileReason,
    pub policy_id: String,
}

/// Risk fusion state plus the packages closed recently.
pub struct Engine {
    policy: CompiledPolicy,
    risk: RiskState,
    cooldowns: BTreeMap<String, Cooldown>,
}

pub struct Evaluation {
    pub decision: MobileDecision,
    /// The decision starts a new closure, which is audited and persisted.
    pub new_closure: bool,
}

impl Engine {
    pub fn new(policy: CompiledPolicy, cooldowns: BTreeMap<String, Cooldown>) -> Self {
        Self {
            policy,
            risk: RiskState::default(),
            cooldowns,
        }
    }

    pub fn policy(&self) -> &CompiledPolicy {
        &self.policy
    }

    /// Replaces the policy. Active cooldowns stay; risk history restarts.
    pub fn set_policy(&mut self, policy: CompiledPolicy) {
        self.policy = policy;
        self.risk = RiskState::default();
    }

    pub fn cooldowns(&self) -> &BTreeMap<String, Cooldown> {
        &self.cooldowns
    }

    pub fn classify_package(&self, package_name: &str) -> PackageClass {
        let Some(package) = normalize_package(package_name) else {
            return PackageClass::Monitored;
        };
        if is_exempt(&package) {
            return PackageClass::Exempt;
        }
        match self.policy.applications.effect_for(&package) {
            ApplicationEffect::Block => return PackageClass::Blocked,
            ApplicationEffect::Allow => return PackageClass::Allowed,
            ApplicationEffect::ContentOnly => {}
        }
        if self.policy.is_browser(&package) {
            PackageClass::Browser
        } else if is_known_video(&package) {
            PackageClass::Video
        } else {
            PackageClass::Monitored
        }
    }

    /// Decides from the window context alone (no frame was scored).
    pub fn observe_context(&mut self, context: &ForegroundContext) -> Evaluation {
        match self.context_stage(context) {
            Stage::Decided(evaluation) => evaluation,
            Stage::ContentAllowed(_) | Stage::ContentEligible(_) => Evaluation {
                decision: MobileDecision::allow(MobileReason::DefaultAllow, "default"),
                new_closure: false,
            },
        }
    }

    /// Decides from the window context and one classified frame.
    pub fn observe_frame(
        &mut self,
        probabilities: &ClassifierProbabilities,
        context: &ForegroundContext,
    ) -> Result<Evaluation, CoreError> {
        let output = classifier_output(probabilities)?;
        let package = match self.context_stage(context) {
            Stage::Decided(evaluation) => return Ok(evaluation),
            Stage::ContentAllowed(reason) => {
                return Ok(Evaluation {
                    decision: MobileDecision::allow(reason, "default"),
                    new_closure: false,
                });
            }
            Stage::ContentEligible(package) => package,
        };
        let inference =
            ViddexaRiskMapper::map(&output).map_err(|_| CoreError::InvalidClassifierOutput)?;
        let score = inference.score_millis;
        let outcome = self.risk.observe(RiskObservation {
            // Histories are per package, so switching apps never fuses frames.
            monitor_id: MonitorId(format!("android:{package}")),
            captured_at_ms: context.captured_at_ms,
            image_score_millis: score,
            image_labels: inference.categories,
            ocr_risk: OcrRisk::None,
            ocr_categories: Vec::new(),
            source_identity: None,
            model_versions: ModelVersions {
                image: IMAGE_MODEL_VERSION.into(),
                ocr: String::new(),
                word_pack: String::new(),
            },
        });
        let mut evaluation = if outcome.action == Action::Allow {
            Evaluation {
                decision: MobileDecision::allow(outcome.reason.into(), "default"),
                new_closure: false,
            }
        } else if matches!(outcome.action, Action::CloseGracefully | Action::Terminate) {
            self.close(
                &package,
                context.captured_at_ms,
                outcome.action.into(),
                outcome.reason.into(),
                "content-risk",
            )
        } else {
            Evaluation {
                decision: MobileDecision {
                    action: outcome.action.into(),
                    reason: outcome.reason.into(),
                    policy_id: "content-risk".into(),
                    expires_at_ms: None,
                    image_score_millis: None,
                    from_cooldown: false,
                },
                new_closure: false,
            }
        };
        evaluation.decision.image_score_millis = Some(u32::from(score));
        Ok(evaluation)
    }

    fn context_stage(&mut self, context: &ForegroundContext) -> Stage {
        let now = context.captured_at_ms;
        self.cooldowns.retain(|_, cooldown| cooldown.until_ms > now);
        if !self.policy.protection_enabled {
            return Stage::decided_allow(MobileReason::ProtectionDisabled, "protection");
        }
        let Some(package) = normalize_package(&context.package_name) else {
            // An unreadable source cannot be attributed to an application.
            return Stage::decided_allow(MobileReason::SourceUncertain, "default");
        };
        if is_exempt(&package) {
            return Stage::decided_allow(MobileReason::ExemptApplication, "exempt");
        }
        if let Some(cooldown) = self.cooldowns.get(&package) {
            return Stage::Decided(Evaluation {
                decision: MobileDecision {
                    action: cooldown.action,
                    reason: cooldown.reason,
                    policy_id: cooldown.policy_id.clone(),
                    expires_at_ms: Some(cooldown.until_ms),
                    image_score_millis: None,
                    from_cooldown: true,
                },
                new_closure: false,
            });
        }

        let effect = self.policy.applications.effect_for(&package);
        if effect == ApplicationEffect::Block {
            return Stage::Decided(self.close(
                &package,
                now,
                MobileAction::CloseGracefully,
                MobileReason::ApplicationBlocked,
                "applications",
            ));
        }
        if let Some(rule_id) = self
            .policy
            .schedule_blocks(&package, context.minute_of_week)
        {
            return Stage::Decided(self.close(
                &package,
                now,
                MobileAction::CloseGracefully,
                MobileReason::TimeWindowBlocked,
                &rule_id,
            ));
        }

        let is_browser = self.policy.is_browser(&package);
        let host = if is_browser {
            context.browser_host.as_deref()
        } else {
            None
        };
        let verdict = self.policy.context.evaluate(host, &context.window_title);
        if context_enforcement(verdict, effect) {
            let reason = match verdict {
                ContextVerdict::Blocklisted => MobileReason::BlockedWebsite,
                ContextVerdict::TitleKeyword => MobileReason::TitleKeyword,
                ContextVerdict::Allowlisted | ContextVerdict::None => {
                    MobileReason::ApplicationBlocked
                }
            };
            return Stage::Decided(self.close(
                &package,
                now,
                MobileAction::CloseGracefully,
                reason,
                "context",
            ));
        }
        if effect == ApplicationEffect::Allow {
            return Stage::ContentAllowed(MobileReason::AllowedApplication);
        }
        if verdict == ContextVerdict::Allowlisted {
            return Stage::ContentAllowed(MobileReason::AllowedWebsite);
        }
        if context.secure_window
            && is_browser
            && self.policy.secure_window == SecureWindowAction::Close
        {
            return Stage::Decided(self.close(
                &package,
                now,
                MobileAction::CloseGracefully,
                MobileReason::SecureWindow,
                "secure-window",
            ));
        }
        if !self.policy.image_enabled {
            return Stage::ContentAllowed(MobileReason::DefaultAllow);
        }
        Stage::ContentEligible(package)
    }

    fn close(
        &mut self,
        package: &str,
        now: i64,
        action: MobileAction,
        reason: MobileReason,
        policy_id: &str,
    ) -> Evaluation {
        let until_ms = now.saturating_add(self.policy.cooldown_ms);
        self.cooldowns.insert(
            package.to_owned(),
            Cooldown {
                until_ms,
                action,
                reason,
                policy_id: policy_id.to_owned(),
            },
        );
        Evaluation {
            decision: MobileDecision {
                action,
                reason,
                policy_id: policy_id.to_owned(),
                expires_at_ms: Some(until_ms),
                image_score_millis: None,
                from_cooldown: false,
            },
            new_closure: true,
        }
    }

    /// Ends the cooldown of `package` (an administrator override).
    pub fn clear_cooldown(&mut self, package: &str) -> bool {
        normalize_package(package).is_some_and(|package| self.cooldowns.remove(&package).is_some())
    }
}

enum Stage {
    Decided(Evaluation),
    /// Content enforcement does not apply to this window.
    ContentAllowed(MobileReason),
    /// The (normalized) package is eligible for content enforcement.
    ContentEligible(String),
}

impl Stage {
    fn decided_allow(reason: MobileReason, policy_id: &str) -> Self {
        Self::Decided(Evaluation {
            decision: MobileDecision::allow(reason, policy_id),
            new_closure: false,
        })
    }
}

/// Desktop schedules resolve category members by `.exe` file name only, so a
/// copy of the policy given to the schedule parser names each Android package
/// `<package>.exe`. Nothing else sees this view.
const SCHEDULE_SUFFIX: &str = ".exe";

fn schedule_view(policy: &Value) -> Value {
    let mut view = policy.clone();
    if let Some(applications) = view.get_mut("applications").and_then(Value::as_array_mut) {
        for application in applications {
            let Some(executable) = application.get("executable").and_then(Value::as_str) else {
                continue;
            };
            if executable.to_ascii_lowercase().ends_with(SCHEDULE_SUFFIX) {
                continue;
            }
            if let Some(package) = normalize_package(executable) {
                application["executable"] = Value::String(format!("{package}{SCHEDULE_SUFFIX}"));
            }
        }
    }
    view
}

fn classifier_output(value: &ClassifierProbabilities) -> Result<ClassifierOutput, CoreError> {
    let probabilities = [
        value.normal,
        value.hentai,
        value.porn,
        value.sexy,
        value.drawing,
    ];
    ClassifierOutput::new(
        VIDDEXA_LABELS.iter().map(|label| (*label).into()).collect(),
        probabilities.to_vec(),
    )
    .map_err(|_| CoreError::InvalidClassifierOutput)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn engine(policy: Value) -> Engine {
        Engine::new(CompiledPolicy::compile(&policy).unwrap(), BTreeMap::new())
    }

    fn context(package: &str, at: i64) -> ForegroundContext {
        ForegroundContext {
            package_name: package.into(),
            window_title: String::new(),
            browser_host: None,
            secure_window: false,
            captured_at_ms: at,
            minute_of_week: 600,
        }
    }

    fn probabilities(porn: f32) -> ClassifierProbabilities {
        ClassifierProbabilities {
            normal: 1.0 - porn,
            hentai: 0.0,
            porn,
            sexy: 0.0,
            drawing: 0.0,
        }
    }

    #[test]
    fn immediate_score_closes_and_starts_cooldown() {
        let mut engine = engine(json!({}));
        let first = engine
            .observe_frame(&probabilities(0.97), &context("com.example.gallery", 1_000))
            .unwrap();
        assert_eq!(first.decision.action, MobileAction::CloseGracefully);
        assert_eq!(first.decision.reason, MobileReason::ImageImmediate);
        assert_eq!(first.decision.expires_at_ms, Some(61_000));
        assert!(first.new_closure);

        let again = engine.observe_context(&context("com.example.gallery", 30_000));
        assert!(again.decision.from_cooldown);
        assert!(!again.new_closure);

        let later = engine.observe_context(&context("com.example.gallery", 61_000));
        assert_eq!(later.decision.action, MobileAction::Allow);
    }

    #[test]
    fn three_high_frames_within_five_seconds_close() {
        let mut engine = engine(json!({}));
        for at in [1_000, 2_000] {
            let result = engine
                .observe_frame(&probabilities(0.85), &context("com.example.feed", at))
                .unwrap();
            assert_eq!(result.decision.action, MobileAction::Allow);
        }
        let result = engine
            .observe_frame(&probabilities(0.85), &context("com.example.feed", 3_000))
            .unwrap();
        assert_eq!(result.decision.reason, MobileReason::ImageRepeated);
    }

    #[test]
    fn frames_from_different_packages_are_not_fused() {
        let mut engine = engine(json!({}));
        engine
            .observe_frame(&probabilities(0.85), &context("com.a", 1_000))
            .unwrap();
        engine
            .observe_frame(&probabilities(0.85), &context("com.b", 2_000))
            .unwrap();
        let result = engine
            .observe_frame(&probabilities(0.85), &context("com.c", 3_000))
            .unwrap();
        assert_eq!(result.decision.action, MobileAction::Allow);
    }

    #[test]
    fn exempt_and_allowed_packages_skip_content() {
        let mut engine = engine(json!({"applications": [{
            "id": "reader", "name": "Reader", "executable": "com.example.reader",
            "category": "custom", "action": "allow", "enabled": true
        }]}));
        let karma = engine
            .observe_frame(&probabilities(1.0), &context("com.karma.android", 1))
            .unwrap();
        assert_eq!(karma.decision.reason, MobileReason::ExemptApplication);
        let reader = engine
            .observe_frame(&probabilities(1.0), &context("com.example.reader", 1))
            .unwrap();
        assert_eq!(reader.decision.reason, MobileReason::AllowedApplication);
    }

    #[test]
    fn blocked_application_closes_without_a_frame() {
        let mut engine = engine(json!({"applications": [{
            "id": "game", "name": "Game", "executable": "com.example.game",
            "category": "game", "action": "block", "enabled": true
        }]}));
        assert_eq!(
            engine.classify_package("com.example.game"),
            PackageClass::Blocked
        );
        let result = engine.observe_context(&context("com.example.game", 5));
        assert_eq!(result.decision.reason, MobileReason::ApplicationBlocked);
    }

    #[test]
    fn blocked_website_in_a_browser_closes() {
        let mut engine = engine(json!({"websites": [{
            "id": "bad", "pattern": "bad.example", "action": "block", "enabled": true
        }]}));
        let mut ctx = context("com.android.chrome", 5);
        ctx.browser_host = Some("www.bad.example".into());
        let result = engine.observe_context(&ctx);
        assert_eq!(result.decision.reason, MobileReason::BlockedWebsite);
    }

    #[test]
    fn host_outside_a_browser_is_ignored() {
        let mut engine = engine(json!({"websites": [{
            "id": "bad", "pattern": "bad.example", "action": "block", "enabled": true
        }]}));
        let mut ctx = context("com.example.notes", 5);
        ctx.browser_host = Some("bad.example".into());
        assert_eq!(
            engine.observe_context(&ctx).decision.action,
            MobileAction::Allow
        );
    }

    #[test]
    fn secure_browser_window_closes_by_default_and_can_be_allowed() {
        let mut ctx = context("com.android.chrome", 5);
        ctx.secure_window = true;
        let mut strict = engine(json!({}));
        assert_eq!(
            strict.observe_context(&ctx).decision.reason,
            MobileReason::SecureWindow
        );

        let mut lenient = engine(json!({"android": {"secureWindowBrowserAction": "allow"}}));
        assert_eq!(
            lenient.observe_context(&ctx).decision.action,
            MobileAction::Allow
        );

        let mut banking = engine(json!({}));
        let mut bank = context("com.example.bank", 5);
        bank.secure_window = true;
        assert_eq!(
            banking.observe_context(&bank).decision.action,
            MobileAction::Allow
        );
    }

    #[test]
    fn browser_schedule_closes_android_browsers() {
        let mut engine = engine(json!({"schedules": [{
            "id": "night", "name": "night", "target": "browsers",
            "blocks": [{"day": 0, "start": "21:00", "end": "23:00"}],
            "enabled": true
        }]}));
        let mut ctx = context("org.mozilla.firefox", 5);
        ctx.minute_of_week = 21 * 60 + 30;
        let result = engine.observe_context(&ctx);
        assert_eq!(result.decision.reason, MobileReason::TimeWindowBlocked);
        assert_eq!(result.decision.policy_id, "night");

        let mut notes = context("com.example.notes", 5);
        notes.minute_of_week = 21 * 60 + 30;
        assert_eq!(
            engine.observe_context(&notes).decision.action,
            MobileAction::Allow
        );
    }

    #[test]
    fn protection_disabled_allows_everything() {
        let mut engine = engine(json!({"protectionEnabled": false}));
        let result = engine
            .observe_frame(&probabilities(1.0), &context("com.example", 5))
            .unwrap();
        assert_eq!(result.decision.reason, MobileReason::ProtectionDisabled);
    }

    #[test]
    fn invalid_probabilities_are_rejected() {
        let mut engine = engine(json!({}));
        let bad = ClassifierProbabilities {
            normal: 0.9,
            hentai: 0.9,
            porn: 0.0,
            sexy: 0.0,
            drawing: 0.0,
        };
        assert!(matches!(
            engine.observe_frame(&bad, &context("com.example", 5)),
            Err(CoreError::InvalidClassifierOutput)
        ));
    }

    #[test]
    fn invalid_android_settings_are_rejected() {
        assert!(CompiledPolicy::compile(&json!({"android": {"cooldownSeconds": 1}})).is_err());
        assert!(
            CompiledPolicy::compile(&json!({"android": {"secureWindowBrowserAction": "x"}}))
                .is_err()
        );
        assert!(CompiledPolicy::compile(&json!([])).is_err());
    }
}
