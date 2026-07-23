use karma_domain::{Action, AuditEvent, Decision, ReasonCode, RiskObservation};

use crate::{
    ApplicationFacts, ApplicationRule, RiskState, RuleEffect, WeeklySchedule, resolve_application,
};

pub struct EvaluationInput {
    pub minute_of_week: u16,
    pub user_sid: String,
    pub application: ApplicationFacts,
    pub observation: RiskObservation,
}

pub struct EvaluationResult {
    pub decision: Decision,
    pub audit: AuditEvent,
}

pub struct DecisionEngine {
    schedule: WeeklySchedule,
    rules: Vec<ApplicationRule>,
    risk: RiskState,
}

impl DecisionEngine {
    pub fn new(schedule: WeeklySchedule, rules: Vec<ApplicationRule>) -> Self {
        Self {
            schedule,
            rules,
            risk: RiskState::default(),
        }
    }

    pub fn evaluate(&mut self, input: EvaluationInput) -> EvaluationResult {
        let risk = self.risk.observe(input.observation.clone());
        let matched_rule = resolve_application(&self.rules, &input.application);

        let decision = if self.schedule.is_blocked(input.minute_of_week) {
            Decision {
                action: Action::CloseGracefully,
                reason: ReasonCode::TimeWindowBlocked,
                policy_id: self.schedule.id.clone(),
                expires_at_ms: None,
            }
        } else if let Some(rule) = matched_rule.filter(|rule| rule.effect == RuleEffect::Block) {
            Decision {
                action: Action::CloseGracefully,
                reason: ReasonCode::ApplicationBlocked,
                policy_id: rule.id.clone(),
                expires_at_ms: None,
            }
        } else if risk.action != Action::Allow {
            Decision {
                action: risk.action,
                reason: risk.reason,
                policy_id: "content-risk".into(),
                expires_at_ms: None,
            }
        } else {
            Decision {
                action: Action::Allow,
                reason: ReasonCode::DefaultAllow,
                policy_id: matched_rule.map_or_else(|| "default".into(), |rule| rule.id.clone()),
                expires_at_ms: None,
            }
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
