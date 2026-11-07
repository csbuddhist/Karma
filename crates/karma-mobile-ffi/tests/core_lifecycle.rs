use karma_ai::{ClassifierOutput, VIDDEXA_LABELS, ViddexaRiskMapper};
use karma_domain::{Action, ModelVersions, MonitorId, OcrRisk, RiskObservation};
use karma_mobile_ffi::{
    AuthError, ClassifierProbabilities, CoreError, ForegroundContext, KarmaCore, MobileAction,
    MobileReason,
};
use karma_policy::RiskState;

const KEY: [u8; 32] = [42; 32];
const PASSWORD: &str = "correct horse battery";

fn open(dir: &tempfile::TempDir) -> std::sync::Arc<KarmaCore> {
    KarmaCore::open(dir.path().to_string_lossy().into_owned(), KEY.to_vec()).unwrap()
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

fn frame(porn: f32, sexy: f32) -> ClassifierProbabilities {
    ClassifierProbabilities {
        normal: 1.0 - porn - sexy,
        hentai: 0.0,
        porn,
        sexy,
        drawing: 0.0,
    }
}

#[test]
fn android_decisions_match_the_desktop_risk_state_for_the_same_scores() {
    let sequence: Vec<(i64, f32, f32)> = vec![
        (0, 0.05, 0.10),
        (500, 0.70, 0.20),
        (1_000, 0.80, 0.10),
        (1_500, 0.30, 0.60),
        (2_000, 0.82, 0.05),
        (2_500, 0.84, 0.00),
        (9_000, 0.10, 0.10),
        (20_000, 0.97, 0.00),
    ];
    let dir = tempfile::tempdir().unwrap();
    let core = open(&dir);
    let mut desktop = RiskState::default();
    let mut desktop_closed_until = i64::MIN;

    for (at, porn, sexy) in sequence {
        let probabilities = frame(porn, sexy);
        let mobile = core
            .observe_frame(probabilities, context("com.example.feed", at))
            .unwrap();

        let output = ClassifierOutput::new(
            VIDDEXA_LABELS.iter().map(|l| (*l).to_owned()).collect(),
            vec![1.0 - porn - sexy, 0.0, porn, sexy, 0.0],
        )
        .unwrap();
        let inference = ViddexaRiskMapper::map(&output).unwrap();
        if at < desktop_closed_until {
            // The phone closes again on sight during the cooldown without scoring.
            assert!(mobile.from_cooldown, "frame at {at}");
            continue;
        }
        let score = inference.score_millis;
        let outcome = desktop.observe(RiskObservation {
            monitor_id: MonitorId("android:com.example.feed".into()),
            captured_at_ms: at,
            image_score_millis: score,
            image_labels: inference.categories,
            ocr_risk: OcrRisk::None,
            ocr_categories: vec![],
            source_identity: None,
            model_versions: ModelVersions {
                image: "viddexa-nano-1".into(),
                ocr: String::new(),
                word_pack: String::new(),
            },
        });
        assert_eq!(mobile.action, outcome.action.into(), "frame at {at}");
        assert_eq!(mobile.reason, outcome.reason.into(), "frame at {at}");
        assert_eq!(mobile.image_score_millis, Some(u32::from(score)));
        if outcome.action != Action::Allow {
            desktop_closed_until = mobile.expires_at_ms.unwrap();
        }
    }
}

#[test]
fn enrollment_policy_and_audit_survive_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let core = open(&dir);
    assert!(!core.status().enrolled);

    let enrollment = core.enroll_admin(PASSWORD.into(), 1_000).unwrap();
    assert_eq!(enrollment.recovery_codes.len(), 8);
    assert_eq!(
        core.enroll_admin(PASSWORD.into(), 1_001),
        Err(AuthError::AlreadyEnrolled)
    );

    let policy = r#"{"applications":[{"id":"g","name":"Game","executable":"com.example.game","category":"game","action":"block","enabled":true}]}"#;
    assert_eq!(
        core.set_policy("forged".into(), policy.into(), 1_002),
        Err(CoreError::Unauthorized)
    );
    assert_eq!(
        core.set_policy(enrollment.session.token.clone(), policy.into(), 1_002),
        Ok(1)
    );
    let decision = core.observe_context(context("com.example.game", 2_000));
    assert_eq!(decision.reason, MobileReason::ApplicationBlocked);
    drop(core);

    let reopened = open(&dir);
    let status = reopened.status();
    assert!(status.enrolled);
    assert_eq!(status.policy_revision, 1);
    // The cooldown persisted across the restart.
    let again = reopened.observe_context(context("com.example.game", 3_000));
    assert!(again.from_cooldown);
    let kinds: Vec<String> = reopened
        .query_audit(0, 50)
        .into_iter()
        .map(|event| event.kind)
        .collect();
    assert!(kinds.contains(&"decision_applied".to_string()));
    assert!(kinds.contains(&"policy_changed".to_string()));
    assert!(kinds.contains(&"administrator_enrolled".to_string()));

    assert!(
        reopened
            .verify_admin_password(PASSWORD.into(), 4_000)
            .is_ok()
    );
}

#[test]
fn wrong_passwords_rate_limit_and_recovery_codes_are_single_use() {
    let dir = tempfile::tempdir().unwrap();
    let core = open(&dir);
    let enrollment = core.enroll_admin(PASSWORD.into(), 0).unwrap();

    for _ in 0..5 {
        assert_eq!(
            core.verify_admin_password("wrong password!".into(), 10),
            Err(AuthError::Failed)
        );
    }
    assert!(matches!(
        core.verify_admin_password(PASSWORD.into(), 20),
        Err(AuthError::RateLimited { .. })
    ));
    assert!(core.verify_admin_password(PASSWORD.into(), 40_000).is_ok());

    let code = enrollment.recovery_codes[0].clone();
    let session = core.redeem_recovery_code(code.clone(), 50_000).unwrap();
    assert_eq!(core.status().recovery_codes_remaining, 7);
    assert_eq!(
        core.redeem_recovery_code(code, 50_001),
        Err(AuthError::Failed)
    );
    core.change_admin_password(session.token.clone(), "a brand new password".into(), 50_002)
        .unwrap();
    assert!(
        core.authorize_privileged(session.token, "stop_monitoring".into(), 50_003)
            .is_ok()
    );
    assert!(
        core.verify_admin_password("a brand new password".into(), 50_004)
            .is_ok()
    );
}

#[test]
fn wrong_state_key_does_not_open_the_state() {
    let dir = tempfile::tempdir().unwrap();
    open(&dir).enroll_admin(PASSWORD.into(), 0).unwrap();
    let result = KarmaCore::open(dir.path().to_string_lossy().into_owned(), vec![1; 32]);
    assert!(matches!(result, Err(CoreError::InvalidKey)));
}

#[test]
fn platform_audit_rejects_free_text() {
    let dir = tempfile::tempdir().unwrap();
    let core = open(&dir);
    let event = karma_mobile_ffi::AuditEventRecord {
        occurred_at_ms: 1,
        kind: "settings_watchdog".into(),
        outcome: "blocked".into(),
        package_name: Some("com.android.settings".into()),
        action: Some(MobileAction::Warn),
        reason: None,
    };
    core.record_audit(event.clone()).unwrap();
    let mut bad = event;
    bad.kind = "contains spaces and text".into();
    assert_eq!(core.record_audit(bad), Err(CoreError::InvalidRequest));
    assert_eq!(
        core.query_audit(0, 10)[0].kind,
        "platform:settings_watchdog"
    );
}
