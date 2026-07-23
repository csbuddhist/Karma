use serde::{Deserialize, Serialize};

use crate::{Action, ReasonCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditKind {
    DecisionApplied,
    AuthenticationFailed,
    ComponentHealth,
}

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
    pub fn decision(
        occurred_at_ms: i64,
        user_sid: &str,
        monitor_id: Option<String>,
        application_id: Option<String>,
        reason: ReasonCode,
        action: Action,
    ) -> Self {
        Self {
            occurred_at_ms,
            user_sid: user_sid.into(),
            monitor_id,
            application_id,
            kind: AuditKind::DecisionApplied,
            reason: Some(reason),
            action: Some(action),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_event_has_no_sensitive_payload_fields() {
        let event = AuditEvent::decision(
            42,
            "S-1-test",
            None,
            None,
            ReasonCode::OcrImageCombined,
            Action::Warn,
        );
        let json = serde_json::to_string(&event).unwrap();

        assert!(!json.contains("screenshot"));
        assert!(!json.contains("ocr_text"));
    }
}
