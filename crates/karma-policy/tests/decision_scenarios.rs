use karma_domain::{Action, ModelVersions, MonitorId, OcrRisk, RiskCategory, RiskObservation};
use karma_policy::{
    ApplicationFacts, ApplicationRule, DecisionEngine, EvaluationInput, MinuteRange, RuleEffect,
    WeeklySchedule,
};

fn observation(score: u16) -> RiskObservation {
    RiskObservation {
        monitor_id: MonitorId("display-1".into()),
        captured_at_ms: 10_000,
        image_score_millis: score,
        image_labels: vec![RiskCategory::Suggestive],
        ocr_risk: OcrRisk::None,
        ocr_categories: vec![],
        source_identity: None,
        model_versions: ModelVersions {
            image: "image-v1".into(),
            ocr: "ocr-v1".into(),
            word_pack: "words-v1".into(),
        },
    }
}

fn application() -> ApplicationFacts {
    ApplicationFacts {
        normalized_path: r"c:\browser.exe".into(),
        publisher: None,
        sha256: None,
    }
}

#[test]
fn blocked_schedule_wins_and_audit_is_safe() {
    let schedule = WeeklySchedule::new(
        "bedtime",
        vec![MinuteRange {
            start: 120,
            end: 180,
        }],
    )
    .unwrap();
    let mut engine = DecisionEngine::new(schedule, vec![]);

    let result = engine.evaluate(EvaluationInput {
        minute_of_week: 150,
        user_sid: "S-1-test".into(),
        application: application(),
        observation: observation(950),
    });

    assert_eq!(result.decision.action, Action::CloseGracefully);
    assert_eq!(result.decision.policy_id, "bedtime");

    let json = serde_json::to_string(&result.audit).unwrap();
    assert!(!json.contains("ocr_text"));
    assert!(!json.contains("screenshot"));
}

#[test]
fn allow_rule_does_not_override_high_risk_content() {
    let schedule = WeeklySchedule::new("none", vec![]).unwrap();
    let rules = vec![ApplicationRule::path(
        "allow-browser",
        10,
        "browser.exe",
        RuleEffect::Allow,
    )];
    let mut engine = DecisionEngine::new(schedule, rules);

    let result = engine.evaluate(EvaluationInput {
        minute_of_week: 200,
        user_sid: "S-1-test".into(),
        application: application(),
        observation: observation(950),
    });

    assert_eq!(result.decision.action, Action::CloseGracefully);
}
